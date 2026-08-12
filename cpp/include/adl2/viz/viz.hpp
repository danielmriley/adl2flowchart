#pragma once

/// `adl2_viz` — Flowchart/DOT from HIR (Rust adl-viz). Depends on sema, not AST-only meaning.
///
/// P1 status: **stub**. API surface will grow in a dedicated phase.
/// Headers live under `include/adl2/viz/` so seams stay obvious.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ∥ formula} → axioms → solver → analysis → certify
///   viz → sema

namespace adl2::viz {

/// Linkable anchor for the stub static library (not a public API).
int module_anchor();

}  // namespace adl2::viz
