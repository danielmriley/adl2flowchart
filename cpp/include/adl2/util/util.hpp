#pragma once

/// `adl2_util` — Optional shared helpers. Prefer module-local types (Rust keeps span/diag in adl-syntax).
///
/// P1 status: **stub**. API surface will grow in a dedicated phase.
/// Headers live under `include/adl2/util/` so seams stay obvious.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ‖ formula} → axioms → solver → analysis → certify
///   viz reads HIR only; cli wires modules.

namespace adl2::util {

/// Linkable anchor for the stub static library (not a public API).
int module_anchor();

}  // namespace adl2::util
