#pragma once

/// `adl2_interp` — Cutflow / membership interpreter (Rust adl-interp). Parallel to formula after sema.
///
/// P1 status: **stub**. API surface will grow in a dedicated phase.
/// Headers live under `libs/interp/include/adl2/interp/` so seams stay obvious.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ‖ formula} → axioms → solver → analysis → certify
///   viz reads HIR only; cli wires modules.

namespace adl2::interp {

/// Linkable anchor for the stub static library (not a public API).
int module_anchor();

}  // namespace adl2::interp
