#pragma once

/// `adl2_analysis` — Verify / pairwise analysis (Rust adl-analysis). Must not parse.
///
/// P4 fills interval fast path, statement-granularity encode, report enums,
/// and a subprocess-solver pairwise engine. Certify / reconcile / refute /
/// witness realization are not claimed. Solver-SAT overlap is
/// CANDIDATE OVERLAPPING (no interpreter re-validation yet).
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ‖ formula} → axioms → solver → analysis → certify
///   viz reads HIR only; cli wires modules.

#include "adl2/analysis/encode.hpp"
#include "adl2/analysis/interval.hpp"
#include "adl2/analysis/report.hpp"
#include "adl2/analysis/witness.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"

#include <chrono>
#include <string>

namespace adl2::analysis {

/// Which solver backend to use. `Native` is accepted but maps to the
/// subprocess backend (C++ has no libz3 link). `Auto` uses z3 on PATH.
enum class SolverChoice {
  Auto,
  Native,
  SubprocessZ3,
  NoSolver,
};

/// Analysis options. Default is interval-only (`NoSolver`) so unit tests
/// that pin POSSIBLY stay stable. CLI `verify` passes `Auto`.
struct AnalysisOptions {
  SolverChoice solver = SolverChoice::NoSolver;
  std::chrono::milliseconds timeout{10000};
  FailOn fail_on;
  bool reconcile = false;
  bool certify = false;
  std::size_t sample_gate = 0;
  bool refute_gate = false;
  bool combine = false;
};

/// Encode `hir` and run interval + optional solver pairwise. Does not parse,
/// certify, reconcile, refute, or sample. `src` is the unit text used for
/// cut line-text; empty falls back to HIR render.
Report analyze_hir(adl2::sema::Hir& hir, const std::string& src,
                   const adl2::sema::ExtDecls& ext, const AnalysisOptions& opts);

inline Report analyze_hir(adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext,
                          const AnalysisOptions& opts) {
  return analyze_hir(hir, std::string{}, ext, opts);
}

/// Linkable anchor (kept so existing stub-graph checks still resolve).
int module_anchor();

}  // namespace adl2::analysis
