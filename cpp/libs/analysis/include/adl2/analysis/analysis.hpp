#pragma once

/// `adl2_analysis` — Verify / pairwise analysis (Rust adl-analysis). Must not parse.
///
/// P4 fills interval fast path, statement-granularity encode, report enums,
/// and a subprocess-solver pairwise engine. P5 fills SAT-side witness
/// realization: PROVEN OVERLAPPING only after Kleene `region3` accepts the
/// realized event in both regions. P6 wires `adl2_certify`: solver-UNSAT
/// whose core cannot be independently replayed is CANDIDATE DISJOINT, never
/// PROVEN. Library default `certify=false` keeps encode/interval unit pins
/// stable; CLI `verify` passes `certify=true` (Rust default). Sampling /
/// refute gates are off in the library (`sample_gate=0`, `refute_gate=false`);
/// CLI `verify` turns them on (`64` / true; `--no-refute-gate` disables the
/// adversarial search). `opts.reconcile` (CLI `--cross`) proves same-base
/// collection refinements and asserts derived XSUB/XEQ size facts.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ‖ formula} → axioms → solver
///                                    ↘ certify ↗ analysis
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
/// that pin POSSIBLY stay stable. CLI `verify` passes `Auto` and
/// `certify=true`.
struct AnalysisOptions {
  SolverChoice solver = SolverChoice::NoSolver;
  std::chrono::milliseconds timeout{10000};
  FailOn fail_on;
  bool reconcile = false;
  /// Independent Farkas replay of UNSAT-side claims. Off by default in the
  /// library (encode/interval pins); CLI `verify` turns it on.
  bool certify = false;
  std::size_t sample_gate = 0;
  bool refute_gate = false;
  bool combine = false;
};

/// Encode `hir` and run interval + optional solver pairwise. Does not parse.
/// When `opts.reconcile`, same-base filtered collections are related via
/// derived size facts (XSUB/XEQ) before pairwise. When `opts.sample_gate > 0` / `opts.refute_gate`, UNSAT-side
/// PROVEN claims are checked through the interpreter and demoted on hit.
/// When `opts.certify` is true, solver-UNSAT
/// disjointness/emptiness is independently replayed; `Some(false)` demotes
/// the claim to CANDIDATE. Interval-path disagreements are diagnostics, not
/// demotions. `src` is the unit text used for cut line-text.
Report analyze_hir(adl2::sema::Hir& hir, const std::string& src,
                   const adl2::sema::ExtDecls& ext, const AnalysisOptions& opts);

inline Report analyze_hir(adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext,
                          const AnalysisOptions& opts) {
  return analyze_hir(hir, std::string{}, ext, opts);
}

/// Linkable anchor (kept so existing stub-graph checks still resolve).
int module_anchor();

}  // namespace adl2::analysis
