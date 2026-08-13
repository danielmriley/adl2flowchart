#include "adl2/viz/viz.hpp"

#include "label.hpp"

#include "adl2/sema/ops.hpp"
#include "adl2/sema/quantity.hpp"

#include <algorithm>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::viz {
namespace {

using adl2::sema::Collection;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::Hir;
using adl2::sema::HirRegionStmt;
using adl2::sema::HNode;
using adl2::sema::ParticleKind;
using adl2::sema::ParticleRef;
using adl2::sema::Quantity;
using adl2::sema::QuantityKind;

void writeln(std::string& s, const std::string& line) {
  s += line;
  s += '\n';
}

std::string esc(const std::string& in) {
  std::string out;
  out.reserve(in.size() + 4);
  for (char c : in) {
    switch (c) {
      case '"':
        out += "\\\"";
        break;
      case '\\':
        out += "\\\\";
        break;
      case '\n':
        out += "\\n";
        break;
      default:
        out.push_back(c);
        break;
    }
  }
  return out;
}

bool collection_is_named(const Hir& hir, CollectionId id) {
  return id.id < hir.coll_names.size() && !hir.coll_names[id.id].empty();
}

bool collection_has_node(const Hir& hir, CollectionId id) {
  return collection_is_named(hir, id) ||
         hir.table.collection(id).kind == CollectionKind::Base;
}

const char* collection_kind(const Hir& hir, CollectionId id) {
  switch (hir.table.collection(id).kind) {
    case CollectionKind::Base:
      return "base";
    case CollectionKind::Filtered:
      return "filtered";
    case CollectionKind::Union:
      return "union";
    case CollectionKind::Combination:
      return "comb";
    case CollectionKind::Sorted:
      return "sorted";
    case CollectionKind::Slice:
      return "slice";
    case CollectionKind::CombProject:
      return "projection";
  }
  return "?";
}

bool collection_unsupported(const Hir& hir, CollectionId id) {
  for (const auto& o : hir.objects) {
    if (o.coll == id) return !o.tag.is_in_fragment();
  }
  return false;
}

void edge_if_node(std::string& s, const Hir& hir, CollectionId parent, std::size_t child_idx,
                  const char* kw) {
  if (collection_has_node(hir, parent)) {
    writeln(s, "  coll" + std::to_string(parent.id) + " -> coll" + std::to_string(child_idx) +
                   " [label=\"" + kw + "\"];");
  }
}

std::optional<std::size_t> region_index_of(const Hir& hir, std::size_t order_idx) {
  if (order_idx >= hir.region_name_order.size()) return std::nullopt;
  const auto sym = hir.region_name_order[order_idx];
  for (std::size_t i = 0; i < hir.regions.size(); ++i) {
    if (hir.regions[i].name == sym) return i;
  }
  return std::nullopt;
}

std::optional<CollectionId> particle_coll(const ParticleRef& p) {
  switch (p.kind) {
    case ParticleKind::Elem:
    case ParticleKind::Whole:
    case ParticleKind::Binder:
      return p.coll;
    case ParticleKind::Met:
    case ParticleKind::ThisElem:
    case ParticleKind::ReduceElem:
    case ParticleKind::Sum:
      return std::nullopt;
  }
  return std::nullopt;
}

void collect_used_collections(const Hir& hir, const HNode& node, std::vector<std::size_t>& out) {
  auto push = [&](CollectionId id) {
    if (collection_has_node(hir, id)) out.push_back(id.id);
  };
  switch (node.kind) {
    case HNode::Kind::Quantity: {
      const Quantity& q = hir.table.quantity(node.qid);
      switch (q.kind) {
        case QuantityKind::Size:
        case QuantityKind::ElemProp:
          push(q.coll);
          break;
        case QuantityKind::AngularSep: {
          if (auto c = particle_coll(q.a)) push(*c);
          if (auto c = particle_coll(q.b)) push(*c);
          break;
        }
        case QuantityKind::Present: {
          const Quantity& inner = hir.table.quantity(q.inner);
          if (inner.kind == QuantityKind::Size || inner.kind == QuantityKind::ElemProp) {
            push(inner.coll);
          }
          break;
        }
        case QuantityKind::EventScalar:
        case QuantityKind::ExternalFn:
          break;
      }
      break;
    }
    case HNode::Kind::CollProp:
    case HNode::Kind::CollValue:
      push(node.coll);
      break;
    case HNode::Kind::Reduce:
      push(node.coll);
      if (node.a) collect_used_collections(hir, *node.a, out);
      break;
    case HNode::Kind::Neg:
    case HNode::Kind::Not:
    case HNode::Kind::Abs:
    case HNode::Kind::Band:
      if (node.a) collect_used_collections(hir, *node.a, out);
      break;
    case HNode::Kind::Binary:
    case HNode::Kind::Cmp:
      if (node.a) collect_used_collections(hir, *node.a, out);
      if (node.b) collect_used_collections(hir, *node.b, out);
      break;
    case HNode::Kind::And:
    case HNode::Kind::Or:
      for (const auto& c : node.items) collect_used_collections(hir, c, out);
      break;
    case HNode::Kind::Ternary:
      if (node.a) collect_used_collections(hir, *node.a, out);
      if (node.b) collect_used_collections(hir, *node.b, out);
      if (node.c) collect_used_collections(hir, *node.c, out);
      break;
    default:
      break;
  }
}

void collect_region_preds(const HNode& node, std::vector<std::size_t>& out) {
  switch (node.kind) {
    case HNode::Kind::RegionPred:
      out.push_back(node.region_index);
      break;
    case HNode::Kind::Neg:
    case HNode::Kind::Not:
    case HNode::Kind::Abs:
      if (node.a) collect_region_preds(*node.a, out);
      break;
    case HNode::Kind::Binary:
    case HNode::Kind::Cmp:
      if (node.a) collect_region_preds(*node.a, out);
      if (node.b) collect_region_preds(*node.b, out);
      break;
    case HNode::Kind::And:
    case HNode::Kind::Or:
      for (const auto& n : node.items) collect_region_preds(n, out);
      break;
    case HNode::Kind::Band:
      if (node.a) collect_region_preds(*node.a, out);
      break;
    case HNode::Kind::Ternary:
      if (node.a) collect_region_preds(*node.a, out);
      if (node.b) collect_region_preds(*node.b, out);
      if (node.c) collect_region_preds(*node.c, out);
      break;
    default:
      break;
  }
}

std::vector<const HNode*> stmt_nodes(const HirRegionStmt& stmt) {
  switch (stmt.kind) {
    case HirRegionStmt::Kind::Select:
    case HirRegionStmt::Kind::Reject:
    case HirRegionStmt::Kind::Trigger:
    case HirRegionStmt::Kind::Bin:
    case HirRegionStmt::Kind::BinCond:
      return {&stmt.node};
    case HirRegionStmt::Kind::Inherit:
    case HirRegionStmt::Kind::NonMembership:
      return {};
  }
  return {};
}

std::string render_stmt_short(const Labeler& lbl, const HirRegionStmt& stmt, const Hir& hir) {
  switch (stmt.kind) {
    case HirRegionStmt::Kind::Select:
      return "select " + lbl.node(stmt.node);
    case HirRegionStmt::Kind::Reject:
      return "reject " + lbl.node(stmt.node);
    case HirRegionStmt::Kind::Trigger:
      return "trigger " + lbl.node(stmt.node);
    case HirRegionStmt::Kind::Inherit: {
      std::string name = "?";
      if (stmt.region < hir.region_name_order.size()) {
        name = hir.symbols.display(hir.region_name_order[stmt.region]);
      }
      return "inherit " + name;
    }
    case HirRegionStmt::Kind::Bin: {
      std::string l = stmt.label ? (*stmt.label + " ") : std::string();
      std::string edges;
      for (std::size_t i = 0; i < stmt.edges.size(); ++i) {
        if (i) edges += " ";
        edges += stmt.edges[i];
      }
      return "bin " + l + lbl.node(stmt.node) + " " + edges;
    }
    case HirRegionStmt::Kind::BinCond: {
      std::string l = stmt.label ? (*stmt.label + " ") : std::string();
      return "bin " + l + lbl.node(stmt.node);
    }
    case HirRegionStmt::Kind::NonMembership:
      return std::string(stmt.nm_kind) + " (no membership)";
  }
  return "?";
}

const char* stmt_keyword(const HirRegionStmt& stmt) {
  switch (stmt.kind) {
    case HirRegionStmt::Kind::Select:
      return "select";
    case HirRegionStmt::Kind::Reject:
      return "reject";
    case HirRegionStmt::Kind::Trigger:
      return "trigger";
    case HirRegionStmt::Kind::Bin:
    case HirRegionStmt::Kind::BinCond:
      return "bin";
    case HirRegionStmt::Kind::Inherit:
    case HirRegionStmt::Kind::NonMembership:
      return "";
  }
  return "";
}

std::string next_id(std::size_t& ctr) {
  std::string id = "n" + std::to_string(ctr);
  ++ctr;
  return id;
}

struct LabelKids {
  std::string label;
  std::vector<const HNode*> children;
};

LabelKids node_label_and_children(const Labeler& lbl, const HNode& n) {
  using Kind = HNode::Kind;
  LabelKids out;
  switch (n.kind) {
    case Kind::Num:
    case Kind::Bool:
    case Kind::Quantity:
    case Kind::ElemSelfProp:
    case Kind::ReduceProp:
    case Kind::CollProp:
    case Kind::Particle:
    case Kind::CollValue:
    case Kind::RegionPred:
    case Kind::Unsupported:
      out.label = lbl.node(n);
      break;
    case Kind::Reduce:
      out.label = adl2::sema::reduce_kind_str(n.reduce);
      if (n.a) out.children.push_back(n.a.get());
      break;
    case Kind::ScalarMinMax:
      out.label = adl2::sema::reduce_kind_str(n.reduce);
      for (const auto& a : n.items) out.children.push_back(&a);
      break;
    case Kind::Neg:
      out.label = "neg";
      if (n.a) out.children.push_back(n.a.get());
      break;
    case Kind::Not:
      out.label = "not";
      if (n.a) out.children.push_back(n.a.get());
      break;
    case Kind::Abs:
      out.label = "abs";
      if (n.a) out.children.push_back(n.a.get());
      break;
    case Kind::Binary:
      out.label = adl2::sema::arith_op_str(n.arith);
      if (n.a) out.children.push_back(n.a.get());
      if (n.b) out.children.push_back(n.b.get());
      break;
    case Kind::Cmp:
      out.label = adl2::sema::cmp_op_str(n.cmp);
      if (n.a) out.children.push_back(n.a.get());
      if (n.b) out.children.push_back(n.b.get());
      break;
    case Kind::And:
      out.label = "and";
      for (const auto& c : n.items) out.children.push_back(&c);
      break;
    case Kind::Or:
      out.label = "or";
      for (const auto& c : n.items) out.children.push_back(&c);
      break;
    case Kind::Band: {
      const char* op = n.band == adl2::sema::BandKind::In ? "[]" : "][";
      out.label = std::string(op) + " " + n.lo + " " + n.hi;
      if (n.a) out.children.push_back(n.a.get());
      break;
    }
    case Kind::Ternary:
      out.label = "ternary ?:";
      if (n.a) out.children.push_back(n.a.get());
      if (n.b) out.children.push_back(n.b.get());
      if (n.c) out.children.push_back(n.c.get());
      break;
  }
  return out;
}

std::pair<std::string, std::size_t> emit_node(std::string& s, const Labeler& lbl, const HNode& n,
                                              std::size_t& ctr) {
  const std::string id = next_id(ctr);
  auto lk = node_label_and_children(lbl, n);
  const char* fill = !n.tag.is_in_fragment() ? ", style=filled, fillcolor=\"#ffe0e0\"" : "";
  writeln(s, "  " + id + " [label=\"" + esc(lk.label) + "\"" + fill + "];");
  std::size_t depth = 0;
  for (const HNode* child : lk.children) {
    auto cd = emit_node(s, lbl, *child, ctr);
    writeln(s, "  " + id + " -> " + cd.first + ";");
    if (cd.second > depth) depth = cd.second;
  }
  return {id, depth + 1};
}

}  // namespace

std::string flowchart_dot(const Hir& hir) {
  Labeler lbl(hir);
  std::string s;
  writeln(s, "digraph flowchart {");
  writeln(s, "  rankdir=LR;");
  writeln(s, "  node [shape=box, fontname=\"monospace\"];");
  writeln(s, "  label=\"" + esc(hir.unit) + "\";");
  writeln(s, "  labelloc=t;");

  writeln(s, "  subgraph cluster_objects {");
  writeln(s, "    label=\"objects\";");
  writeln(s, "    color=gray70;");
  const auto& colls = hir.table.collections();
  for (std::size_t i = 0; i < colls.size(); ++i) {
    CollectionId id{static_cast<std::uint32_t>(i)};
    if (!collection_has_node(hir, id)) continue;
    const char* kind = collection_kind(hir, id);
    const std::string label = esc(lbl.collection(id)) + "\\n[" + kind + "]";
    const char* style = collection_unsupported(hir, id)
                            ? ", style=filled, fillcolor=\"#ffe0e0\""
                            : ", style=filled, fillcolor=\"#e0f0ff\"";
    writeln(s, "    coll" + std::to_string(i) + " [label=\"" + label + "\"" + style + "];");
  }
  writeln(s, "  }");

  for (std::size_t i = 0; i < colls.size(); ++i) {
    CollectionId id{static_cast<std::uint32_t>(i)};
    if (!collection_has_node(hir, id)) continue;
    const Collection& coll = colls[i];
    switch (coll.kind) {
      case CollectionKind::Filtered:
        edge_if_node(s, hir, coll.parent, i, "take");
        break;
      case CollectionKind::Union:
        for (auto p : coll.parts) edge_if_node(s, hir, p, i, "union");
        break;
      case CollectionKind::Combination:
        for (auto p : coll.parts) edge_if_node(s, hir, p, i, "comb");
        break;
      case CollectionKind::Sorted:
        edge_if_node(s, hir, coll.parent, i, "sort");
        break;
      case CollectionKind::Slice:
        edge_if_node(s, hir, coll.parent, i, "slice");
        break;
      case CollectionKind::CombProject:
        edge_if_node(s, hir, coll.parent, i, "->");
        break;
      case CollectionKind::Base:
        break;
    }
  }

  writeln(s, "  subgraph cluster_regions {");
  writeln(s, "    label=\"regions\";");
  writeln(s, "    color=gray70;");
  for (std::size_t ri = 0; ri < hir.regions.size(); ++ri) {
    const auto& region = hir.regions[ri];
    std::vector<std::string> lines;
    lines.push_back(esc(hir.symbols.display(region.name)));
    for (const auto& stmt : region.stmts) {
      lines.push_back(esc(render_stmt_short(lbl, stmt, hir)));
    }
    std::string label;
    for (std::size_t i = 0; i < lines.size(); ++i) {
      if (i) label += "\\l";
      label += lines[i];
    }
    writeln(s, "    region" + std::to_string(ri) + " [label=\"" + label +
                   "\\l\", style=filled, fillcolor=\"#f0fff0\"];");
  }
  writeln(s, "  }");

  for (std::size_t ri = 0; ri < hir.regions.size(); ++ri) {
    const auto& region = hir.regions[ri];
    std::vector<std::size_t> parents;
    for (const auto& stmt : region.stmts) {
      if (stmt.kind == HirRegionStmt::Kind::Inherit) {
        parents.push_back(stmt.region);
      } else if (stmt.kind == HirRegionStmt::Kind::Select) {
        collect_region_preds(stmt.node, parents);
      }
    }
    std::sort(parents.begin(), parents.end());
    parents.erase(std::unique(parents.begin(), parents.end()), parents.end());
    for (std::size_t parent : parents) {
      if (auto pi = region_index_of(hir, parent)) {
        writeln(s, "  region" + std::to_string(*pi) + " -> region" + std::to_string(ri) +
                       " [label=\"inherit\", style=dashed];");
      }
    }
  }

  for (std::size_t ri = 0; ri < hir.regions.size(); ++ri) {
    const auto& region = hir.regions[ri];
    std::vector<std::size_t> seen;
    for (const auto& stmt : region.stmts) {
      for (const HNode* node : stmt_nodes(stmt)) {
        collect_used_collections(hir, *node, seen);
      }
    }
    std::sort(seen.begin(), seen.end());
    seen.erase(std::unique(seen.begin(), seen.end()), seen.end());
    for (std::size_t ci : seen) {
      writeln(s, "  coll" + std::to_string(ci) + " -> region" + std::to_string(ri) +
                     " [style=dotted, color=gray60];");
    }
  }

  writeln(s, "}");
  return s;
}

std::string ast_dot(const Hir& hir) {
  Labeler lbl(hir);
  std::string s;
  writeln(s, "digraph ast {");
  writeln(s, "  node [shape=box, fontname=\"monospace\"];");
  writeln(s, "  label=\"" + esc(hir.unit) + " (AST)\";");
  writeln(s, "  labelloc=t;");

  std::size_t ctr = 0;
  std::vector<std::pair<std::string, std::size_t>> roots;

  for (const auto& def : hir.defines) {
    const std::string name = hir.symbols.display(def.name);
    const std::string root = next_id(ctr);
    writeln(s, "  " + root + " [label=\"define " + esc(name) + " (" +
                   adl2::sema::define_kind_str(def.kind) +
                   ")\", style=filled, fillcolor=\"#fff0e0\"];");
    auto cd = emit_node(s, lbl, def.body, ctr);
    writeln(s, "  " + root + " -> " + cd.first + ";");
    roots.emplace_back(root, cd.second + 1);
  }

  for (const auto& obj : hir.objects) {
    const Collection& coll = hir.table.collection(obj.coll);
    if (coll.kind != CollectionKind::Filtered) continue;
    const std::string name = hir.symbols.display(obj.name);
    const std::string root = next_id(ctr);
    writeln(s, "  " + root + " [label=\"object " + esc(name) +
                   "\", style=filled, fillcolor=\"#e0f0ff\"];");
    const auto& ep = hir.elem_pred(coll.pred);
    auto cd = emit_node(s, lbl, ep.node, ctr);
    writeln(s, "  " + root + " -> " + cd.first + " [label=\"predicate\"];");
    roots.emplace_back(root, cd.second + 1);
  }

  for (const auto& region : hir.regions) {
    const std::string name = hir.symbols.display(region.name);
    const std::string root = next_id(ctr);
    writeln(s, "  " + root + " [label=\"region " + esc(name) +
                   "\", style=filled, fillcolor=\"#f0fff0\"];");
    std::size_t max_depth = 0;
    for (const auto& stmt : region.stmts) {
      for (const HNode* node : stmt_nodes(stmt)) {
        const char* kw = stmt_keyword(stmt);
        auto cd = emit_node(s, lbl, *node, ctr);
        writeln(s, "  " + root + " -> " + cd.first + " [label=\"" + kw + "\"];");
        if (cd.second > max_depth) max_depth = cd.second;
      }
    }
    roots.emplace_back(root, max_depth + 1);
  }

  for (std::size_t i = 0; i + 1 < roots.size(); ++i) {
    const std::string& prev = roots[i].first;
    const std::size_t depth = roots[i].second;
    const std::string& next = roots[i + 1].first;
    const std::size_t minlen = depth < 1 ? 1 : depth;
    writeln(s, "  " + prev + " -> " + next + " [style=invis, weight=100, minlen=" +
                   std::to_string(minlen) + "];");
  }

  writeln(s, "}");
  return s;
}

int module_anchor() { return 4; }

}  // namespace adl2::viz
