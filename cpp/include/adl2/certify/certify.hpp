#pragma once

/// `adl2_certify` — Small trusted certificate kernel (Rust adl-certify). Keep thin.
///
/// P1 status: **stub**. API surface will grow in a dedicated phase.
/// Headers live under `include/adl2/certify/` so seams stay obvious.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ∥ formula} → axioms → solver → analysis → certify
///   viz → sema

namespace adl2::certify {

/// Linkable anchor for the stub static library (not a public API).
int module_anchor();

}  // namespace adl2::certify
