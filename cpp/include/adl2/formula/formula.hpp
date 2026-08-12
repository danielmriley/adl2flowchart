#pragma once

/// `adl2_formula` — Region formula IR / encoder (Rust adl-formula). Parallel to interp after sema.
///
/// P1 status: **stub**. API surface will grow in a dedicated phase.
/// Headers live under `include/adl2/formula/` so seams stay obvious.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ‖ formula} → axioms → solver → analysis → certify
///   viz reads HIR only; cli wires modules.

namespace adl2::formula {

/// Linkable anchor for the stub static library (not a public API).
int module_anchor();

}  // namespace adl2::formula
