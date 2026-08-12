#pragma once

/// `adl2_sema` — Resolve + HIR (Rust adl-sema). Downstream of syntax only.
///
/// P1 status: **stub**. API surface will grow in a dedicated phase.
/// Headers live under `include/adl2/sema/` so seams stay obvious.
///
/// Dependency spine (do not invert):
///   syntax → sema → {interp ∥ formula} → axioms → solver → analysis → certify
///   viz → sema

namespace adl2::sema {

/// Linkable anchor for the stub static library (not a public API).
int module_anchor();

}  // namespace adl2::sema
