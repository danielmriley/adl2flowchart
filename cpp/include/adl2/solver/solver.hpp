#pragma once

/// `adl2_solver` — SMT backend facade (Rust adl-solver).
///
/// P1 status: **stub**. API surface will grow in a dedicated phase.
/// Headers live under `include/adl2/solver/` so seams stay obvious.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ‖ formula} → axioms → solver → analysis → certify
///   viz reads HIR only; cli wires modules.

namespace adl2::solver {

/// Linkable anchor for the stub static library (not a public API).
int module_anchor();

}  // namespace adl2::solver
