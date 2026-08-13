#include "adl2/analysis/encode.hpp"

#include "adl2/sema/dump.hpp"
#include "adl2/sema/ops.hpp"

#include <algorithm>
#include <cctype>
#include <memory>
#include <utility>

namespace adl2::analysis {
namespace {

using adl2::formula::DiagTable;
using adl2::formula::EncodedRegion;
using adl2::formula::Formula;
using adl2::sema::CmpOp;
using adl2::sema::Fragment;
using adl2::sema::HNode;
using adl2::sema::Hir;
using adl2::sema::HirRegion;
using adl2::sema::HirRegionStmt;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::QuantityTable;
using adl2::sema::Span;

/// Byte-offset → line map. Local copy of syntax LineMap so analysis never
/// includes parser headers.
struct LineMap {
  std::vector<std::size_t> line_starts;

  explicit LineMap(const std::string& src) {
    line_starts.push_back(0);
    for (std::size_t i = 0; i < src.size(); ++i) {
      if (src[i] == '\n') line_starts.push_back(i + 1);
    }
  }

  std::pair<std::uint32_t, std::uint32_t> line_col(std::size_t offset) const {
    auto it = std::upper_bound(line_starts.begin(), line_starts.end(), offset);
    std::size_t line = static_cast<std::size_t>(it - line_starts.begin());
    if (line == 0) line = 1;
    --line;
    std::uint32_t col = static_cast<std::uint32_t>(offset - line_starts[line] + 1);
    return {static_cast<std::uint32_t>(line + 1), col};
  }

  std::string line_text(const std::string& src, std::size_t offset) const {
    auto lc = line_col(offset);
    std::size_t start = line_starts[lc.first - 1];
    std::size_t end = (lc.first < line_starts.size()) ? line_starts[lc.first] : src.size();
    std::string s = src.substr(start, end - start);
    while (!s.empty() && (s.back() == '\n' || s.back() == '\r')) s.pop_back();
    return s;
  }
};

std::string trim(std::string s) {
  auto not_space = [](unsigned char c) { return !std::isspace(c); };
  s.erase(s.begin(), std::find_if(s.begin(), s.end(), not_space));
  s.erase(std::find_if(s.rbegin(), s.rend(), not_space).base(), s.end());
  return s;
}

void retag_node(const Hir& hir, HNode& node) {
  if (!node.tag.is_in_fragment() &&
      node.tag.reason.find("is not declared in the external library") != std::string::npos &&
      node.kind == HNode::Kind::Quantity &&
      hir.table.quantity(node.qid).kind == QuantityKind::ExternalFn) {
    node.tag = Fragment::ok();
  }
  switch (node.kind) {
    case HNode::Kind::Neg:
    case HNode::Kind::Not:
    case HNode::Kind::Abs:
      if (node.a) retag_node(hir, *node.a);
      break;
    case HNode::Kind::Binary:
    case HNode::Kind::Cmp:
      if (node.a) retag_node(hir, *node.a);
      if (node.b) retag_node(hir, *node.b);
      break;
    case HNode::Kind::And:
    case HNode::Kind::Or:
      for (auto& n : node.items) retag_node(hir, n);
      break;
    case HNode::Kind::Band:
      if (node.a) retag_node(hir, *node.a);
      break;
    case HNode::Kind::Ternary:
      if (node.a) retag_node(hir, *node.a);
      if (node.b) retag_node(hir, *node.b);
      if (node.c) retag_node(hir, *node.c);
      break;
    default:
      break;
  }
}

EncodedRegion encode_synthetic(Hir& hir, std::vector<HirRegionStmt> stmts, Span span) {
  HirRegion syn;
  syn.name = hir.symbols.intern("__adl2_synth__");
  syn.stmts = std::move(stmts);
  syn.span = span;
  hir.regions.push_back(std::move(syn));
  hir.region_name_order.push_back(hir.regions.back().name);
  std::size_t idx = hir.regions.size() - 1;
  EncodedRegion enc = adl2::formula::encode_region(hir, idx);
  hir.regions.pop_back();
  hir.region_name_order.pop_back();
  return enc;
}

HirRegionStmt opaque_stmt(Span span, const char* reason) {
  HirRegionStmt s;
  s.kind = HirRegionStmt::Kind::NonMembership;
  s.nm_kind = "inherit";
  s.tag = Fragment::unsupported(reason);
  s.span = span;
  return s;
}

void flatten_inherits(const Hir& hir, const std::vector<HirRegionStmt>& stmts,
                      std::vector<std::size_t>& stack, std::vector<HirRegionStmt>& out) {
  for (const auto& stmt : stmts) {
    if (stmt.kind != HirRegionStmt::Kind::Inherit) {
      out.push_back(stmt);
      continue;
    }
    if (stmt.region >= hir.regions.size()) {
      out.push_back(opaque_stmt(stmt.span, "reference to an unknown region"));
      continue;
    }
    bool cycle = false;
    for (std::size_t s : stack) {
      if (s == stmt.region) {
        cycle = true;
        break;
      }
    }
    if (cycle) {
      out.push_back(opaque_stmt(stmt.span, "region inheritance cycle"));
      continue;
    }
    std::vector<HirRegionStmt> inherited;
    for (const auto& s : hir.regions[stmt.region].stmts) {
      if (s.kind == HirRegionStmt::Kind::Bin || s.kind == HirRegionStmt::Kind::BinCond) {
        continue;
      }
      inherited.push_back(s);
    }
    stack.push_back(stmt.region);
    flatten_inherits(hir, inherited, stack, out);
    stack.pop_back();
  }
}

Span stmt_span(const HirRegionStmt& stmt) {
  switch (stmt.kind) {
    case HirRegionStmt::Kind::Select:
    case HirRegionStmt::Kind::Reject:
    case HirRegionStmt::Kind::Trigger:
      return stmt.node.span;
    case HirRegionStmt::Kind::Inherit:
    case HirRegionStmt::Kind::Bin:
    case HirRegionStmt::Kind::BinCond:
    case HirRegionStmt::Kind::NonMembership:
      return stmt.span;
  }
  return stmt.span;
}

bool is_membership(const HirRegionStmt& stmt) {
  switch (stmt.kind) {
    case HirRegionStmt::Kind::Select:
    case HirRegionStmt::Kind::Reject:
    case HirRegionStmt::Kind::Inherit:
    case HirRegionStmt::Kind::Trigger:
      return true;
    case HirRegionStmt::Kind::Bin:
    case HirRegionStmt::Kind::BinCond:
      return false;
    case HirRegionStmt::Kind::NonMembership:
      return !stmt.tag.is_in_fragment();
  }
  return false;
}

bool is_presence_atom(const QuantityTable& table, const Formula& f) {
  if (f.kind != Formula::Kind::Atom) return false;
  if (f.atom.terms().size() != 1) return false;
  return table.quantity(f.atom.terms()[0].second).kind == QuantityKind::Present;
}

void coverage(const QuantityTable& table, const Formula& f, const DiagTable& diags,
              RegionEnc& out, const LineMap& map) {
  switch (f.kind) {
    case Formula::Kind::True:
    case Formula::Kind::False:
    case Formula::Kind::Atom:
      if (is_presence_atom(table, f)) return;
      out.leaves_total += 1;
      out.leaves_encoded += 1;
      return;
    case Formula::Kind::And:
      for (const auto& p : f.items) coverage(table, p, diags, out, map);
      return;
    case Formula::Kind::Or:
      out.or_clauses += 1;
      for (const auto& p : f.items) coverage(table, p, diags, out, map);
      return;
    case Formula::Kind::Unknown: {
      out.leaves_total += 1;
      if (const auto* diag = diags.get(f.diag)) {
        auto lc = map.line_col(diag->span.start);
        out.dropped.push_back({lc.first, diag->reason});
      }
      return;
    }
    case Formula::Kind::Dual:
      out.leaves_total += 1;
      out.leaves_encoded += 1;
      out.dual_hedges += 1;
      return;
  }
}

std::optional<BinSetEnc> encode_boundary_bins(Hir& hir, std::size_t region_idx,
                                              const HNode& var,
                                              const std::vector<std::string>& edges,
                                              Span span, const std::string& src) {
  if (edges.empty()) return std::nullopt;
  std::string variable;
  if (var.span.start < src.size() && var.span.end <= src.size() &&
      var.span.end >= var.span.start) {
    variable = trim(src.substr(var.span.start, var.span.end - var.span.start));
  }
  if (variable.empty()) variable = adl2::sema::render_node(hir, var);

  auto num = [&](const std::string& text) {
    HNode n = HNode::make(HNode::Kind::Num, span);
    n.text = text;
    return n;
  };
  auto cmp = [&](CmpOp op, HNode rhs) {
    HNode n = HNode::make(HNode::Kind::Cmp, span);
    n.cmp = op;
    n.a = std::make_unique<HNode>(var);
    n.b = std::make_unique<HNode>(std::move(rhs));
    return n;
  };

  BinSetEnc set;
  set.region_idx = region_idx;
  set.variable = std::move(variable);
  for (std::size_t i = 0; i < edges.size(); ++i) {
    std::vector<HirRegionStmt> stmts;
    HirRegionStmt ge;
    ge.kind = HirRegionStmt::Kind::Select;
    ge.node = cmp(CmpOp::Ge, num(edges[i]));
    ge.span = span;
    stmts.push_back(std::move(ge));
    if (i + 1 < edges.size()) {
      HirRegionStmt lt;
      lt.kind = HirRegionStmt::Kind::Select;
      lt.node = cmp(CmpOp::Lt, num(edges[i + 1]));
      lt.span = span;
      stmts.push_back(std::move(lt));
    }
    EncodedRegion e = encode_synthetic(hir, std::move(stmts), span);
    set.bins.push_back(std::move(e.formula));
  }
  return set;
}

}  // namespace

void retag_opaque_externals(Hir& hir) {
  for (auto& region : hir.regions) {
    for (auto& stmt : region.stmts) {
      switch (stmt.kind) {
        case HirRegionStmt::Kind::Select:
        case HirRegionStmt::Kind::Reject:
        case HirRegionStmt::Kind::Trigger:
        case HirRegionStmt::Kind::Bin:
        case HirRegionStmt::Kind::BinCond:
          retag_node(hir, stmt.node);
          break;
        default:
          break;
      }
    }
  }
}

void formula_quantities(const Formula& f, std::set<QuantityId>& out) {
  switch (f.kind) {
    case Formula::Kind::True:
    case Formula::Kind::False:
    case Formula::Kind::Unknown:
      return;
    case Formula::Kind::Atom:
      for (const auto& t : f.atom.terms()) out.insert(t.second);
      return;
    case Formula::Kind::And:
    case Formula::Kind::Or:
      for (const auto& p : f.items) formula_quantities(p, out);
      return;
    case Formula::Kind::Dual:
      if (f.plus) formula_quantities(*f.plus, out);
      if (f.minus) formula_quantities(*f.minus, out);
      return;
  }
}

UnitEnc encode_unit(Hir& hir, const std::string& src) {
  LineMap map(src);
  UnitEnc unit;

  for (std::size_t ridx = 0; ridx < hir.regions.size(); ++ridx) {
    std::string name = hir.symbols.display(hir.regions[ridx].name);
    std::vector<HirRegionStmt> own = hir.regions[ridx].stmts;
    std::vector<HirRegionStmt> stmt_list;
    stmt_list.reserve(own.size());
    std::vector<std::size_t> stack{ridx};
    flatten_inherits(hir, own, stack, stmt_list);

    RegionEnc enc;
    enc.idx = ridx;
    enc.name = std::move(name);

    std::vector<Formula> cond_bins;
    for (std::size_t sidx = 0; sidx < stmt_list.size(); ++sidx) {
      const HirRegionStmt& stmt = stmt_list[sidx];
      Span span = stmt_span(stmt);
      if (is_membership(stmt)) {
        EncodedRegion e = encode_synthetic(hir, {stmt}, span);
        auto lc = map.line_col(span.start);
        std::uint32_t line = span.line != 0 ? span.line : lc.first;
        std::string text;
        if (src.empty()) {
          switch (stmt.kind) {
            case HirRegionStmt::Kind::Select:
            case HirRegionStmt::Kind::Trigger:
              text = adl2::sema::render_node(hir, stmt.node);
              break;
            case HirRegionStmt::Kind::Reject:
              text = "reject " + adl2::sema::render_node(hir, stmt.node);
              break;
            default:
              break;
          }
        } else {
          text = trim(map.line_text(src, span.start));
        }
        coverage(hir.table, e.formula, e.diags, enc, map);
        formula_quantities(e.formula, enc.quantities);
        StmtEnc se;
        se.name = adl2::solver::AssertName::make("R" + std::to_string(ridx) + "S" +
                                                 std::to_string(sidx));
        se.span = span;
        se.line = line;
        se.text = std::move(text);
        se.formula = std::move(e.formula);
        se.diags = std::move(e.diags);
        enc.stmts.push_back(std::move(se));
      } else if (stmt.kind == HirRegionStmt::Kind::Bin) {
        if (auto set = encode_boundary_bins(hir, ridx, stmt.node, stmt.edges, span, src)) {
          unit.bin_sets.push_back(std::move(*set));
        }
      } else if (stmt.kind == HirRegionStmt::Kind::BinCond) {
        HirRegionStmt sel;
        sel.kind = HirRegionStmt::Kind::Select;
        sel.node = stmt.node;
        sel.span = span;
        EncodedRegion e = encode_synthetic(hir, {std::move(sel)}, span);
        cond_bins.push_back(std::move(e.formula));
      }
    }
    if (!cond_bins.empty()) {
      BinSetEnc set;
      set.region_idx = ridx;
      set.variable = "boolean bins";
      set.bins = std::move(cond_bins);
      unit.bin_sets.push_back(std::move(set));
    }
    unit.regions.push_back(std::move(enc));
  }
  return unit;
}

}  // namespace adl2::analysis
