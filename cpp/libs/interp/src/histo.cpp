#include "adl2/interp/histo.hpp"

#include "adl2/interp/cutflow.hpp"

#include <algorithm>
#include <cctype>
#include <charconv>
#include <cstdio>
#include <cstring>
#include <map>
#include <optional>
#include <sstream>

namespace adl2::interp {
namespace {

using adl2::sema::Hir;
using adl2::sema::HirHisto;
using adl2::sema::HirRegionStmt;
using adl2::sema::HirWeightValueKind;
using adl2::sema::HistoSpecKind;

bool is_histolist(const Hir& hir, std::size_t idx) {
  return idx < hir.histolist_regions.size() && hir.histolist_regions[idx];
}

bool ieq(const std::string& a, const std::string& b) {
  if (a.size() != b.size()) return false;
  for (std::size_t i = 0; i < a.size(); ++i) {
    if (std::tolower(static_cast<unsigned char>(a[i])) !=
        std::tolower(static_cast<unsigned char>(b[i])))
      return false;
  }
  return true;
}

bool parse_f64(const std::string& text, double& out) {
  auto r = std::from_chars(text.data(), text.data() + text.size(), out);
  return r.ec == std::errc{} && r.ptr == text.data() + text.size();
}

std::string json_escape(const std::string& s) {
  std::string out;
  out.reserve(s.size() + 2);
  out.push_back('"');
  for (unsigned char c : s) {
    switch (c) {
      case '"': out += "\\\""; break;
      case '\\': out += "\\\\"; break;
      case '\n': out += "\\n"; break;
      case '\r': out += "\\r"; break;
      case '\t': out += "\\t"; break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out += buf;
        } else {
          out.push_back(static_cast<char>(c));
        }
    }
  }
  out.push_back('"');
  return out;
}

struct JsonWriter {
  std::string out;
  bool pretty = false;
  std::size_t depth = 0;
  std::vector<char> has_item;
  bool pending_value = false;
  explicit JsonWriter(bool p) : pretty(p) {}
  void newline_indent() {
    if (!pretty) return;
    out.push_back('\n');
    out.append(depth * 2, ' ');
  }
  void item() {
    if (pending_value) {
      pending_value = false;
      return;
    }
    if (!has_item.empty()) {
      if (has_item.back()) out.push_back(',');
      has_item.back() = 1;
      newline_indent();
    }
  }
  void open(char c) {
    item();
    out.push_back(c);
    ++depth;
    has_item.push_back(0);
  }
  void close(char c) {
    --depth;
    bool had = !has_item.empty() && has_item.back();
    if (!has_item.empty()) has_item.pop_back();
    if (had) newline_indent();
    out.push_back(c);
  }
  void key(const char* k) {
    item();
    out.push_back('"');
    out += k;
    out += "\":";
    if (pretty) out.push_back(' ');
    pending_value = true;
  }
  void raw(const std::string& v) {
    item();
    out += v;
  }
  void str_val(const std::string& s) {
    item();
    out += json_escape(s);
  }
  void num(double v) {
    item();
    out += json_f64(v);
  }
  void num_array(const std::vector<double>& vs) {
    item();
    out.push_back('[');
    for (std::size_t i = 0; i < vs.size(); ++i) {
      if (i) {
        out.push_back(',');
        if (pretty) out.push_back(' ');
      }
      out += json_f64(vs[i]);
    }
    out.push_back(']');
  }
  void flow(double w, double w2) {
    item();
    const char* sp = pretty ? " " : "";
    out += "{\"w\":";
    out += sp;
    out += json_f64(w);
    out += ",";
    out += sp;
    out += "\"w2\":";
    out += sp;
    out += json_f64(w2);
    out += "}";
  }
  std::string finish() {
    if (pretty) out.push_back('\n');
    return out;
  }
};

void h1_tail_json(JsonWriter& w, const std::vector<double>& sumw, const std::vector<double>& sumw2,
                  double under_w, double under_w2, double over_w, double over_w2,
                  std::uint64_t entries, double tsumw, double tsumw2, double tsumwx,
                  double tsumwx2) {
  w.key("sumw");
  w.num_array(sumw);
  w.key("sumw2");
  w.num_array(sumw2);
  w.key("underflow");
  w.flow(under_w, under_w2);
  w.key("overflow");
  w.flow(over_w, over_w2);
  w.key("entries");
  w.raw(std::to_string(entries));
  w.key("tsumw");
  w.num(tsumw);
  w.key("tsumw2");
  w.num(tsumw2);
  w.key("tsumwx");
  w.num(tsumwx);
  w.key("tsumwx2");
  w.num(tsumwx2);
}

void weight_diags(const Hir& hir, std::vector<std::string>& diags) {
  for (const auto& w : hir.weights) {
    std::string rname = w.region < hir.region_name_order.size()
                            ? hir.symbols.display(hir.region_name_order[w.region])
                            : std::string("?");
    if (w.value.kind == HirWeightValueKind::Num) {
      double v;
      if (!parse_f64(w.value.text, v)) {
        diags.push_back("weight `" + w.name + "` in region `" + rname +
                        "`: malformed numeric literal `" + w.value.text + "`; treated as 1.0");
      }
    } else {
      diags.push_back("weight `" + w.name + "` in region `" + rname +
                      "`: non-numeric argument (" + w.value.text + "); treated as 1.0");
    }
  }
}

std::optional<HistoFill> instantiate(const HirHisto& h, std::size_t region_idx,
                                     const std::string& region_name, std::pair<double, bool> eff,
                                     std::vector<std::string>& diags) {
  auto skip = [&](const std::string& reason) -> std::optional<HistoFill> {
    diags.push_back("histo `" + h.name + "` in region `" + region_name + "`: " + reason +
                    "; histogram skipped");
    return std::nullopt;
  };
  auto mk = [&](const adl2::sema::HNode* expr, const adl2::sema::HNode* expr_y,
                HistAcc hist) -> HistoFill {
    HistoFill f;
    f.name = h.name;
    f.title = h.title;
    f.region = region_name;
    f.region_idx = region_idx;
    f.expr = expr;
    f.expr_y = expr_y;
    f.factor = eff.first;
    f.weighted_incomplete = eff.second;
    f.hist = std::move(hist);
    return f;
  };
  const auto& spec = h.spec;
  switch (spec.kind) {
    case HistoSpecKind::Unsupported:
      return skip(spec.reason);
    case HistoSpecKind::Uniform1D: {
      if (spec.expr.has_unsupported()) return skip("fill expression is outside the checked fragment");
      double lo = 0, hi = 0;
      if (!parse_f64(spec.lo, lo) || !parse_f64(spec.hi, hi)) return skip("malformed axis bound");
      if (lo >= hi) {
        std::ostringstream o;
        o << "empty axis range [" << lo << ", " << hi << ")";
        return skip(o.str());
      }
      HistAcc acc;
      acc.kind = HistAccKind::H1;
      acc.h1 = Hist1D::make(spec.nbins, lo, hi);
      return mk(&spec.expr, nullptr, std::move(acc));
    }
    case HistoSpecKind::Var1D: {
      if (spec.expr.has_unsupported()) return skip("fill expression is outside the checked fragment");
      std::vector<double> parsed;
      parsed.reserve(spec.edges.size());
      for (const auto& e : spec.edges) {
        double v = 0;
        if (!parse_f64(e, v)) return skip("malformed bin edge `" + e + "`");
        parsed.push_back(v);
      }
      if (parsed.size() < 2) return skip("bin edges must be strictly increasing");
      for (std::size_t i = 1; i < parsed.size(); ++i) {
        if (parsed[i - 1] >= parsed[i]) return skip("bin edges must be strictly increasing");
      }
      HistAcc acc;
      acc.kind = HistAccKind::H1Var;
      acc.h1var = Hist1DVar::make(std::move(parsed));
      return mk(&spec.expr, nullptr, std::move(acc));
    }
    case HistoSpecKind::Uniform2D: {
      if (spec.xexpr.has_unsupported() || spec.yexpr.has_unsupported())
        return skip("fill expression is outside the checked fragment");
      double xlo = 0, xhi = 0, ylo = 0, yhi = 0;
      if (!parse_f64(spec.xlo, xlo) || !parse_f64(spec.xhi, xhi) || !parse_f64(spec.ylo, ylo) ||
          !parse_f64(spec.yhi, yhi))
        return skip("malformed axis bound");
      if (xlo >= xhi) {
        std::ostringstream o;
        o << "empty x axis range [" << xlo << ", " << xhi << ")";
        return skip(o.str());
      }
      if (ylo >= yhi) {
        std::ostringstream o;
        o << "empty y axis range [" << ylo << ", " << yhi << ")";
        return skip(o.str());
      }
      HistAcc acc;
      acc.kind = HistAccKind::H2;
      acc.h2 = Hist2D::make(spec.nx, xlo, xhi, spec.ny, ylo, yhi);
      return mk(&spec.xexpr, &spec.yexpr, std::move(acc));
    }
  }
  return skip("unsupported histogram form");
}

void fill_from(HistoFill& f, const Interp& interp, const Event& event) {
  double w = f.factor * event.weight;
  EvalError err;
  auto xo = interp.eval_num(*f.expr, event, err);
  if (!xo) {
    f.error_skips += 1;
    if (!f.first_error) f.first_error = err.reason;
    return;
  }
  if (xo->kind == NumOutcomeKind::NonValue) {
    f.nonvalue_skips += 1;
    return;
  }
  double x = xo->value;
  std::optional<double> y;
  if (f.expr_y) {
    EvalError ey;
    auto yo = interp.eval_num(*f.expr_y, event, ey);
    if (!yo) {
      f.error_skips += 1;
      if (!f.first_error) f.first_error = ey.reason;
      return;
    }
    if (yo->kind == NumOutcomeKind::NonValue) {
      f.nonvalue_skips += 1;
      return;
    }
    y = yo->value;
  }
  if (f.hist.kind == HistAccKind::H1) {
    f.hist.h1.fill(x, w);
  } else if (f.hist.kind == HistAccKind::H1Var) {
    f.hist.h1var.fill(x, w);
  } else if (y) {
    f.hist.h2.fill(x, *y, w);
  } else {
    f.error_skips += 1;
    if (!f.first_error) f.first_error = "internal: 2-D fill without y expression";
  }
}

}  // namespace

Hist1D Hist1D::make(std::uint32_t nbins, double lo, double hi) {
  Hist1D h;
  h.nbins = nbins;
  h.lo = lo;
  h.hi = hi;
  h.sumw.assign(nbins, 0.0);
  h.sumw2.assign(nbins, 0.0);
  return h;
}

void Hist1D::fill(double x, double w) {
  entries += 1;
  if (x < lo) {
    underflow_w += w;
    underflow_w2 += w * w;
    return;
  }
  if (x >= hi) {
    overflow_w += w;
    overflow_w2 += w * w;
    return;
  }
  double frac = (x - lo) / (hi - lo);
  auto idx = static_cast<std::size_t>(frac * static_cast<double>(nbins));
  if (idx >= nbins) idx = nbins - 1;
  sumw[idx] += w;
  sumw2[idx] += w * w;
  tsumw += w;
  tsumw2 += w * w;
  tsumwx += w * x;
  tsumwx2 += w * x * x;
}

Hist1DVar Hist1DVar::make(std::vector<double> edges) {
  Hist1DVar h;
  auto n = edges.size() < 1 ? 0 : edges.size() - 1;
  h.edges = std::move(edges);
  h.sumw.assign(n, 0.0);
  h.sumw2.assign(n, 0.0);
  return h;
}

void Hist1DVar::fill(double x, double w) {
  entries += 1;
  if (edges.empty()) return;
  if (x < edges.front()) {
    underflow_w += w;
    underflow_w2 += w * w;
    return;
  }
  if (x >= edges.back()) {
    overflow_w += w;
    overflow_w2 += w * w;
    return;
  }
  // partition_point: first edge > x, then idx = that - 1. Rust: #edges <= x, minus one.
  auto it = std::upper_bound(edges.begin(), edges.end(), x);
  // upper_bound is first > x. For edges <= x count = distance(begin, upper_bound).
  // idx = count - 1. If x == edges[i], upper_bound is past equals... 
  // Rust partition_point(|e| *e <= x) is first edge where NOT (e <= x) i.e. first e > x
  // that's upper_bound. idx = that - 1.
  auto idx = static_cast<std::size_t>(it - edges.begin());
  if (idx == 0) idx = 1;
  idx -= 1;
  if (idx >= sumw.size()) idx = sumw.size() - 1;
  sumw[idx] += w;
  sumw2[idx] += w * w;
  tsumw += w;
  tsumw2 += w * w;
  tsumwx += w * x;
  tsumwx2 += w * x * x;
}

Hist2D Hist2D::make(std::uint32_t nx, double xlo, double xhi, std::uint32_t ny, double ylo,
                    double yhi) {
  Hist2D h;
  h.nx = nx;
  h.xlo = xlo;
  h.xhi = xhi;
  h.ny = ny;
  h.ylo = ylo;
  h.yhi = yhi;
  auto cells = (static_cast<std::size_t>(nx) + 2) * (static_cast<std::size_t>(ny) + 2);
  h.sumw.assign(cells, 0.0);
  h.sumw2.assign(cells, 0.0);
  return h;
}

namespace {
std::size_t axis_cell(double x, double lo, double hi, std::uint32_t n) {
  if (x < lo) return 0;
  if (x >= hi) return static_cast<std::size_t>(n) + 1;
  double frac = (x - lo) / (hi - lo);
  auto idx = static_cast<std::size_t>(frac * static_cast<double>(n));
  if (idx >= n) idx = n - 1;
  return 1 + idx;
}
}  // namespace

void Hist2D::fill(double x, double y, double w) {
  entries += 1;
  auto bx = axis_cell(x, xlo, xhi, nx);
  auto by = axis_cell(y, ylo, yhi, ny);
  auto gbin = bx + (static_cast<std::size_t>(nx) + 2) * by;
  if (gbin < sumw.size()) {
    sumw[gbin] += w;
    sumw2[gbin] += w * w;
  }
  if (bx >= 1 && bx <= nx && by >= 1 && by <= ny) {
    tsumw += w;
    tsumw2 += w * w;
    tsumwx += w * x;
    tsumwx2 += w * x * x;
    tsumwy += w * y;
    tsumwy2 += w * y * y;
    tsumwxy += w * x * y;
  }
}

HistoSet HistoSet::make(const Hir& hir) {
  HistoSet set;
  weight_diags(hir, set.setup_diags_);
  for (std::size_t ridx = 0; ridx < hir.regions.size(); ++ridx) {
    if (is_histolist(hir, ridx)) continue;
    const auto& region = hir.regions[ridx];
    std::string region_name = hir.symbols.display(region.name);
    auto weights = stmt_weights(hir, ridx);
    std::map<std::pair<std::size_t, std::size_t>, std::size_t> own_pos;
    for (std::size_t i = 0; i < region.stmts.size(); ++i) {
      const auto& s = region.stmts[i];
      if (s.kind == HirRegionStmt::Kind::NonMembership && std::strcmp(s.nm_kind, "histo") == 0) {
        own_pos[{s.span.start, s.span.end}] = i;
      }
    }
    std::vector<std::size_t> seen_lists;
    std::vector<std::pair<const HirHisto*, std::pair<double, bool>>> candidates;
    for (const auto& h : hir.histos) {
      if (h.region != ridx) continue;
      std::pair<double, bool> eff{1.0, false};
      auto it = own_pos.find({h.span.start, h.span.end});
      if (it != own_pos.end()) eff = weights.at(it->second);
      candidates.push_back({&h, eff});
    }
    std::optional<std::size_t> first_fill_stmt;
    for (const auto& kv : own_pos) {
      if (!first_fill_stmt || kv.second < *first_fill_stmt) first_fill_stmt = kv.second;
    }
    for (std::size_t i = 0; i < region.stmts.size(); ++i) {
      const auto& stmt = region.stmts[i];
      if (stmt.kind != HirRegionStmt::Kind::Inherit) continue;
      if (!is_histolist(hir, stmt.region)) continue;
      first_fill_stmt = first_fill_stmt ? std::min(*first_fill_stmt, i) : i;
      bool seen = false;
      for (auto t : seen_lists)
        if (t == stmt.region) seen = true;
      if (seen) {
        std::string list = stmt.region < hir.region_name_order.size()
                               ? hir.symbols.display(hir.region_name_order[stmt.region])
                               : std::string("?");
        set.setup_diags_.push_back(
            "region `" + region_name + "`: histoList `" + list +
            "` referenced more than once; mid-selection fill points are not supported — its "
            "histograms fill once on full region acceptance");
        continue;
      }
      seen_lists.push_back(stmt.region);
      auto eff = weights.at(i);
      for (const auto& h : hir.histos) {
        if (h.region == stmt.region) candidates.push_back({&h, eff});
      }
    }
    if (first_fill_stmt) {
      bool later_weight = false;
      for (std::size_t i = *first_fill_stmt + 1; i < region.stmts.size(); ++i) {
        const auto& s = region.stmts[i];
        if (s.kind == HirRegionStmt::Kind::NonMembership && std::strcmp(s.nm_kind, "weight") == 0)
          later_weight = true;
      }
      if (later_weight) {
        set.setup_diags_.push_back(
            "region `" + region_name +
            "`: a `weight` statement follows a histogram fill point; weights compose "
            "positionally ([DECIDE-W1]) — earlier fill points exclude the later weight");
      }
    }
    for (auto [hp, eff] : candidates) {
      bool dup = false;
      for (const auto& f : set.histos) {
        if (f.region_idx == ridx && ieq(f.name, hp->name)) {
          dup = true;
          break;
        }
      }
      if (dup) {
        set.setup_diags_.push_back("histo `" + hp->name + "` in region `" + region_name +
                                   "`: duplicate histogram name; only the first declaration fills");
        continue;
      }
      if (auto fill = instantiate(*hp, ridx, region_name, eff, set.setup_diags_)) {
        set.histos.push_back(std::move(*fill));
      }
    }
  }
  return set;
}

void HistoSet::fill_event(const Interp& interp, const Event& event,
                          const std::vector<RegionResult>& results) {
  for (auto& f : histos) {
    bool accepted = f.region_idx < results.size() && results[f.region_idx].pass &&
                    *results[f.region_idx].pass;
    if (!accepted) continue;
    fill_from(f, interp, event);
  }
}

std::vector<std::string> HistoSet::diagnostics() const {
  auto out = setup_diags_;
  for (const auto& f : histos) {
    if (f.nonvalue_skips > 0) {
      out.push_back("histo `" + f.name + "` (region `" + f.region + "`): " +
                    std::to_string(f.nonvalue_skips) +
                    " fill(s) skipped: expression had no value");
    }
    if (f.error_skips > 0) {
      std::string reason = f.first_error.value_or("evaluation error");
      out.push_back("histo `" + f.name + "` (region `" + f.region + "`): " +
                    std::to_string(f.error_skips) + " fill(s) skipped: " + reason);
    }
  }
  return out;
}

std::string HistoSet::to_json(bool pretty) const {
  JsonWriter w(pretty);
  w.open('{');
  w.key("version");
  w.raw("2");
  w.key("histograms");
  w.open('[');
  for (const auto& f : histos) {
    w.open('{');
    w.key("name");
    w.str_val(f.name);
    w.key("title");
    w.str_val(f.title);
    w.key("region");
    w.str_val(f.region);
    w.key("type");
    if (f.hist.kind == HistAccKind::H1) {
      const auto& h = f.hist.h1;
      w.str_val("h1");
      w.key("nbins");
      w.raw(std::to_string(h.nbins));
      w.key("lo");
      w.num(h.lo);
      w.key("hi");
      w.num(h.hi);
      h1_tail_json(w, h.sumw, h.sumw2, h.underflow_w, h.underflow_w2, h.overflow_w, h.overflow_w2,
                   h.entries, h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2);
    } else if (f.hist.kind == HistAccKind::H1Var) {
      const auto& h = f.hist.h1var;
      w.str_val("h1var");
      w.key("nbins");
      w.raw(std::to_string(h.sumw.size()));
      w.key("edges");
      w.num_array(h.edges);
      h1_tail_json(w, h.sumw, h.sumw2, h.underflow_w, h.underflow_w2, h.overflow_w, h.overflow_w2,
                   h.entries, h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2);
    } else {
      const auto& h = f.hist.h2;
      w.str_val("h2");
      w.key("nx");
      w.raw(std::to_string(h.nx));
      w.key("xlo");
      w.num(h.xlo);
      w.key("xhi");
      w.num(h.xhi);
      w.key("ny");
      w.raw(std::to_string(h.ny));
      w.key("ylo");
      w.num(h.ylo);
      w.key("yhi");
      w.num(h.yhi);
      w.key("contents");
      w.num_array(h.sumw);
      w.key("sumw2");
      w.num_array(h.sumw2);
      w.key("entries");
      w.raw(std::to_string(h.entries));
      w.key("tsumw");
      w.num(h.tsumw);
      w.key("tsumw2");
      w.num(h.tsumw2);
      w.key("tsumwx");
      w.num(h.tsumwx);
      w.key("tsumwx2");
      w.num(h.tsumwx2);
      w.key("tsumwy");
      w.num(h.tsumwy);
      w.key("tsumwy2");
      w.num(h.tsumwy2);
      w.key("tsumwxy");
      w.num(h.tsumwxy);
    }
    if (f.weighted_incomplete) {
      w.key("weighted_incomplete");
      w.raw("true");
    }
    w.close('}');
  }
  w.close(']');
  w.close('}');
  return w.finish();
}

}  // namespace adl2::interp
