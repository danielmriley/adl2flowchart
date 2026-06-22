//! `adl-formula` — polarity-aware formula IR + projections, and the
//! HIR → Formula region encoder (SPEC_ARCHITECTURE §5, SPEC_ANALYSIS §1).
//! No solver dependency.
//!
//! - [`Formula`] is the **exact** region encoding; it may contain
//!   [`Formula::Unknown`] (explicit ignorance, with its diagnostic) and
//!   [`Formula::Dual`] (convention hedge with `minus ⊆ R ⊆ plus`).
//! - [`Formula::not`] is exact NNF negation; `Dual` swaps branches.
//! - [`Over`] / [`Under`] are the only solver-facing forms; they wrap a
//!   [`QFormula`] that is Unknown/Dual-free **by type**, and they can only
//!   be constructed by [`Formula::over`] / [`Formula::under`] — soundness
//!   direction is a type (ADR-004).
//! - [`LinAtom`] construction rejects non-finite constants (audit Bug 5).
//! - [`encode_region`] compiles `adl-sema` HIR regions per the
//!   SPEC_ANALYSIS §1 table, including the PHASE0 OPEN-1 `Dual` bounded
//!   expansion (`k = 3`, empty-collection case in the plus branch).

pub mod encode;
pub mod formula;
pub mod lin;

pub use encode::{EncodedRegion, OPEN1_BOUND, encode_region, encode_regions};
pub use formula::{DiagId, DiagTable, Formula, FormulaDiag, Over, QFormula, Under};
pub use lin::{LinAtom, Rel};

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-formula";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-formula");
    }
}
