#pragma once

/// Per-region cutflows (SPEC_EVENT_PIPELINE §2). Faithful port of
/// `adl-interp::cutflow`. Step 0 is `all`; then one step per membership
/// statement. `bin`/`bincond` are appendix-only. Labels are keyword +
/// the expression's source span.

#include "adl2/interp/eval.hpp"
#include "adl2/sema/hir.hpp"

#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::interp {

struct Counts {
  std::uint64_t raw = 0;
  double sumw = 0;
  double sumw2 = 0;

  void add(double w) {
    raw += 1;
    sumw += w;
    sumw2 += w * w;
  }
  void merge(const Counts& o) {
    raw += o.raw;
    sumw += o.sumw;
    sumw2 += o.sumw2;
  }
  bool operator==(const Counts& o) const {
    return raw == o.raw && sumw == o.sumw && sumw2 == o.sumw2;
  }
  bool operator!=(const Counts& o) const { return !(*this == o); }
};

struct CutStep {
  /// `"all"`, `"select"`, `"reject"`, `"inherit"`, or `"trigger"`.
  const char* kind = "all";
  std::string label;
  double factor = 1.0;
  bool weighted_incomplete = false;
  Counts counts;
  std::uint64_t errors = 0;
};

enum class BinFlowKind { Boundary, Cond };

struct BinFlow {
  BinFlowKind kind = BinFlowKind::Boundary;
  std::optional<std::string> label;
  std::vector<std::string> edges;
  double factor = 1.0;
  bool weighted_incomplete = false;
  std::vector<Counts> bins;
  Counts out;
  Counts yes;
  Counts no;
  std::uint64_t failed = 0;
};

struct RegionFlow {
  std::string name;
  std::size_t region_idx = 0;
  std::vector<CutStep> steps;
  std::map<std::size_t, std::size_t> step_of_stmt;
  std::vector<BinFlow> bins;
};

/// Positional ADL `weight` product in effect **before** each statement
/// (SPEC_EVENT_PIPELINE §4). Shared by cutflow and histogram fills.
struct StmtWeights {
  std::vector<std::pair<double, bool>> eff;
  std::pair<double, bool> at(std::size_t i) const {
    if (i >= eff.size()) return {1.0, false};
    return eff[i];
  }
};
StmtWeights stmt_weights(const adl2::sema::Hir& hir, std::size_t ridx);

class CutflowSet {
 public:
  /// Build the step structure from `hir`; `src` is the unit's source text
  /// (labels are verbatim source slices). Unevaluable regions become setup
  /// diagnostics, never silent drops.
  static CutflowSet make(const adl2::sema::Hir& hir, const std::string& src);

  void record_event(const Event& event, const std::vector<RegionResult>& results,
                    const std::vector<std::vector<StepEval>>& traces);

  const std::vector<std::string>& diagnostics() const { return setup_diags_; }
  bool empty() const { return regions_.empty(); }
  const std::vector<RegionFlow>& regions() const { return regions_; }
  Counts total() const { return total_; }

  /// Canonical `cutflow.json` (schema `version: 1`). Compact when
  /// `pretty` is false. No provenance object (CLI `--json` differs from
  /// smash2 by that optional key until provenance is ported).
  std::string to_json(bool pretty) const;

  /// Fixed-width per-region stdout tables matching smash2 `run`.
  std::string text_table() const;

 private:
  std::vector<RegionFlow> regions_;
  Counts total_;
  std::vector<std::string> setup_diags_;
};

}  // namespace adl2::interp
