#include "adl2/interp/cutflow.hpp"

#include "adl2/interp/provenance.hpp"
#include "json_writer.hpp"

#include <algorithm>
#include <charconv>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <iomanip>
#include <sstream>
#include <string_view>
#include <utility>

namespace adl2::interp {

using adl2::sema::Hir;
using adl2::sema::HirRegionStmt;
using adl2::sema::HirWeightValueKind;
using adl2::sema::Span;

StmtWeights stmt_weights(const Hir& hir, std::size_t ridx) {
  const auto& region = hir.regions[ridx];
  std::map<std::pair<std::size_t, std::size_t>, const adl2::sema::HirWeightValue*> by_span;
  for (const auto& w : hir.weights) {
    if (w.region == ridx) by_span[{w.span.start, w.span.end}] = &w.value;
  }
  double factor = 1.0;
  bool incomplete = false;
  StmtWeights out;
  out.eff.reserve(region.stmts.size());
  for (const auto& stmt : region.stmts) {
    out.eff.push_back({factor, incomplete});
    if (stmt.kind == HirRegionStmt::Kind::NonMembership &&
        std::strcmp(stmt.nm_kind, "weight") == 0) {
      auto it = by_span.find({stmt.span.start, stmt.span.end});
      if (it == by_span.end() || it->second->kind != HirWeightValueKind::Num) {
        incomplete = true;
      } else {
        const std::string& text = it->second->text;
        double v = 0;
        auto r = std::from_chars(text.data(), text.data() + text.size(), v);
        if (r.ec != std::errc{} || r.ptr != text.data() + text.size()) {
          incomplete = true;
        } else {
          factor *= v;
        }
      }
    }
  }
  return out;
}

namespace {

std::optional<std::string> snippet(const std::string& src, Span span) {
  if (span.end < span.start || span.end > src.size()) return std::nullopt;
  std::string_view s(src.data() + span.start, span.end - span.start);
  auto b = s.find_first_not_of(" \t\n\r\f\v");
  if (b == std::string_view::npos) return std::nullopt;
  auto e = s.find_last_not_of(" \t\n\r\f\v");
  return std::string(s.substr(b, e - b + 1));
}

std::string labeled(const std::string& src, const char* kw, Span span, std::size_t stmt_idx) {
  if (auto t = snippet(src, span)) return std::string(kw) + " " + *t;
  return std::string(kw) + " <statement " + std::to_string(stmt_idx) + ">";
}

std::optional<std::string> bin_label(const std::string& src, const std::optional<std::string>& label,
                                     Span span) {
  if (label) return label;
  return snippet(src, span);
}

bool is_histolist(const Hir& hir, std::size_t idx) {
  return idx < hir.histolist_regions.size() && hir.histolist_regions[idx];
}

RegionFlow region_flow(const Hir& hir, const std::string& src, std::size_t ridx, std::string name) {
  const auto& region = hir.regions[ridx];
  auto weights = stmt_weights(hir, ridx);
  RegionFlow flow;
  flow.name = std::move(name);
  flow.region_idx = ridx;
  CutStep all;
  all.kind = "all";
  all.label = "all";
  all.factor = 1.0;
  flow.steps.push_back(std::move(all));
  for (std::size_t i = 0; i < region.stmts.size(); ++i) {
    const auto& stmt = region.stmts[i];
    auto [factor, weighted_incomplete] = weights.at(i);
    const char* kind = nullptr;
    std::string label;
    switch (stmt.kind) {
      case HirRegionStmt::Kind::Select:
        kind = "select";
        label = labeled(src, "select", stmt.node.span, i);
        break;
      case HirRegionStmt::Kind::Reject:
        kind = "reject";
        label = labeled(src, "reject", stmt.node.span, i);
        break;
      case HirRegionStmt::Kind::Trigger:
        kind = "trigger";
        label = labeled(src, "trigger", stmt.node.span, i);
        break;
      case HirRegionStmt::Kind::Inherit: {
        if (is_histolist(hir, stmt.region)) continue;
        if (auto t = snippet(src, stmt.span)) {
          label = *t;
        } else if (stmt.region < hir.region_name_order.size()) {
          label = hir.symbols.display(hir.region_name_order[stmt.region]);
        } else {
          label = "<region " + std::to_string(stmt.region) + ">";
        }
        kind = "inherit";
        break;
      }
      case HirRegionStmt::Kind::Bin: {
        BinFlow b;
        b.kind = BinFlowKind::Boundary;
        b.label = bin_label(src, stmt.label, stmt.span);
        b.edges = stmt.edges;
        b.factor = factor;
        b.weighted_incomplete = weighted_incomplete;
        b.bins.assign(stmt.edges.size(), Counts{});
        flow.bins.push_back(std::move(b));
        continue;
      }
      case HirRegionStmt::Kind::BinCond: {
        BinFlow b;
        b.kind = BinFlowKind::Cond;
        b.label = bin_label(src, stmt.label, stmt.span);
        b.factor = factor;
        b.weighted_incomplete = weighted_incomplete;
        flow.bins.push_back(std::move(b));
        continue;
      }
      case HirRegionStmt::Kind::NonMembership:
        continue;
    }
    flow.step_of_stmt[i] = flow.steps.size();
    CutStep step;
    step.kind = kind;
    step.label = std::move(label);
    step.factor = factor;
    step.weighted_incomplete = weighted_incomplete;
    flow.steps.push_back(std::move(step));
  }
  return flow;
}

std::string pct(std::uint64_t num, std::uint64_t den) {
  if (den == 0) return "-";
  double v = 100.0 * static_cast<double>(num) / static_cast<double>(den);
  std::ostringstream o;
  o << std::fixed << std::setprecision(2) << v << '%';
  return o.str();
}

void record_bin(BinFlow& acc, const BinOutcome& outcome, double w_in) {
  if (outcome.kind == BinOutcomeKind::Failed) {
    acc.failed += 1;
    return;
  }
  if (acc.kind == BinFlowKind::Boundary && outcome.kind == BinOutcomeKind::Boundary) {
    double w = w_in * acc.factor;
    if (outcome.bin && *outcome.bin < acc.bins.size()) {
      acc.bins[*outcome.bin].add(w);
    } else {
      acc.out.add(w);
    }
    return;
  }
  if (acc.kind == BinFlowKind::Cond && outcome.kind == BinOutcomeKind::Cond) {
    double w = w_in * acc.factor;
    if (outcome.member) acc.yes.add(w);
    else acc.no.add(w);
    return;
  }
  acc.failed += 1;
}

void counts_json(JsonWriter& w, Counts c) {
  w.open('{');
  w.key("raw");
  w.raw(std::to_string(c.raw));
  w.key("sumw");
  w.num(c.sumw);
  w.key("sumw2");
  w.num(c.sumw2);
  w.close('}');
}

void bin_json(JsonWriter& w, const BinFlow& bin) {
  w.open('{');
  if (bin.kind == BinFlowKind::Boundary) {
    w.key("kind");
    w.str_val("boundary");
    w.key("label");
    if (bin.label) w.str_val(*bin.label);
    else w.null();
    w.key("edges");
    w.open('[');
    for (const auto& e : bin.edges) w.raw(e);
    w.close(']');
    w.key("bins");
    w.open('[');
    for (const auto& c : bin.bins) counts_json(w, c);
    w.close(']');
    w.key("out");
    counts_json(w, bin.out);
    w.key("failed");
    w.raw(std::to_string(bin.failed));
    if (bin.weighted_incomplete) {
      w.key("weighted_incomplete");
      w.raw("true");
    }
  } else {
    w.key("kind");
    w.str_val("cond");
    w.key("label");
    if (bin.label) w.str_val(*bin.label);
    else w.null();
    w.key("true");
    counts_json(w, bin.yes);
    w.key("false");
    counts_json(w, bin.no);
    w.key("failed");
    w.raw(std::to_string(bin.failed));
    if (bin.weighted_incomplete) {
      w.key("weighted_incomplete");
      w.raw("true");
    }
  }
  w.close('}');
}

}  // namespace

CutflowSet CutflowSet::make(const Hir& hir, const std::string& src) {
  CutflowSet set;
  for (std::size_t ridx = 0; ridx < hir.regions.size(); ++ridx) {
    if (is_histolist(hir, ridx)) continue;
    std::string name = hir.symbols.display(hir.regions[ridx].name);
    bool skip = false;
    std::string reason;
    for (const auto& s : hir.regions[ridx].stmts) {
      if (s.kind == HirRegionStmt::Kind::NonMembership && !s.tag.in_fragment) {
        skip = true;
        reason = s.tag.reason;
        break;
      }
    }
    if (skip) {
      set.setup_diags_.push_back("region `" + name + "`: cannot evaluate (" + reason +
                                 "); cutflow skipped");
      continue;
    }
    set.regions_.push_back(region_flow(hir, src, ridx, std::move(name)));
  }
  return set;
}

void CutflowSet::record_event(const Event& event, const std::vector<RegionResult>& results,
                              const std::vector<std::vector<StepEval>>& traces) {
  double w_in = event.weight;
  total_.add(w_in);
  for (auto& flow : regions_) {
    double f0 = flow.steps[0].factor;
    flow.steps[0].counts.add(w_in * f0);
    if (flow.region_idx >= traces.size()) continue;
    for (const auto& se : traces[flow.region_idx]) {
      auto it = flow.step_of_stmt.find(se.stmt);
      if (it == flow.step_of_stmt.end()) continue;
      CutStep& step = flow.steps[it->second];
      if (!se.pass) {
        step.errors += 1;
      } else if (*se.pass) {
        step.counts.add(w_in * step.factor);
      }
    }
    if (flow.region_idx >= results.size()) continue;
    const auto& result = results[flow.region_idx];
    if (result.pass && *result.pass) {
      std::size_t n = std::min(flow.bins.size(), result.bins.size());
      for (std::size_t i = 0; i < n; ++i) record_bin(flow.bins[i], result.bins[i], w_in);
    }
  }
}

std::string CutflowSet::to_json(bool pretty) const {
  return to_json_with(pretty, nullptr);
}

std::string CutflowSet::to_json_with(bool pretty, const Provenance* provenance) const {
  JsonWriter w(pretty);
  w.open('{');
  w.key("version");
  w.raw("1");
  if (provenance) {
    w.key("provenance");
    provenance->write(w);
  }
  w.key("total");
  counts_json(w, total_);
  w.key("regions");
  w.open('[');
  for (const auto& flow : regions_) {
    w.open('{');
    w.key("name");
    w.str_val(flow.name);
    w.key("steps");
    w.open('[');
    for (const auto& step : flow.steps) {
      w.open('{');
      w.key("kind");
      w.str_val(step.kind);
      w.key("label");
      w.str_val(step.label);
      w.key("raw");
      w.raw(std::to_string(step.counts.raw));
      w.key("sumw");
      w.num(step.counts.sumw);
      w.key("sumw2");
      w.num(step.counts.sumw2);
      w.key("errors");
      w.raw(std::to_string(step.errors));
      if (step.weighted_incomplete) {
        w.key("weighted_incomplete");
        w.raw("true");
      }
      w.close('}');
    }
    w.close(']');
    w.key("bins");
    w.open('[');
    for (const auto& bin : flow.bins) bin_json(w, bin);
    w.close(']');
    w.close('}');
  }
  w.close(']');
  w.close('}');
  return w.finish();
}

std::string CutflowSet::text_table() const {
  std::ostringstream out;
  for (std::size_t ri = 0; ri < regions_.size(); ++ri) {
    const auto& flow = regions_[ri];
    if (ri > 0) out << '\n';
    out << "cutflow: " << flow.name << '\n';
    std::size_t label_w = 4;
    std::size_t raw_w = 3;
    for (const auto& s : flow.steps) {
      label_w = std::max(label_w, s.label.size());
      raw_w = std::max(raw_w, std::to_string(s.counts.raw).size());
    }
    out << "  " << std::left << std::setw(static_cast<int>(label_w)) << "step" << "  "
        << std::right << std::setw(static_cast<int>(raw_w)) << "raw" << "  " << std::setw(8)
        << "abs%" << "  " << std::setw(8) << "rel%" << "  " << std::setw(6) << "errors"
        << "  sumw +- err\n";
    std::uint64_t all_raw = flow.steps.empty() ? 0 : flow.steps[0].counts.raw;
    std::uint64_t prev_raw = all_raw;
    for (std::size_t i = 0; i < flow.steps.size(); ++i) {
      const auto& step = flow.steps[i];
      std::string abs = pct(step.counts.raw, all_raw);
      std::string rel = i == 0 ? std::string("-") : pct(step.counts.raw, prev_raw);
      std::string wcol = json_f64(step.counts.sumw) + " +- " + json_f64(std::sqrt(step.counts.sumw2));
      if (step.weighted_incomplete) wcol += " (weighted incomplete)";
      out << "  " << std::left << std::setw(static_cast<int>(label_w)) << step.label << "  "
          << std::right << std::setw(static_cast<int>(raw_w)) << step.counts.raw << "  "
          << std::setw(8) << abs << "  " << std::setw(8) << rel << "  " << std::setw(6)
          << step.errors << "  " << wcol << '\n';
      prev_raw = step.counts.raw;
    }
  }
  return out.str();
}

}  // namespace adl2::interp
