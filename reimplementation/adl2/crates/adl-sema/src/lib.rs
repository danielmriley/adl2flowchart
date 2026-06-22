//! `adl-sema` — name resolution, the typed Quantity/Collection identity
//! model, define resolution, fragment tagging, and HIR construction
//! (SPEC_ARCHITECTURE §4).
//!
//! The core idea: event quantities are **typed, interned values** whose
//! identity is structural. Identity facts established by construction:
//!
//! - a pure rename (`object MHT take MissingET`, no cuts) binds the SAME
//!   `CollectionId` as its source, transitively;
//! - a filtered collection is a *different* `CollectionId` than its
//!   parent, forever;
//! - `jets[0].pt` and `jets[1].pt` cannot alias; `dPhi(a,b)` and
//!   `dPhi(b,a)` are distinct (oriented), while `dR(a,b)` ≡ `dR(b,a)`
//!   (unoriented, canonically ordered at interning);
//! - numeric defines resolve to their body HIR (inlined by construction);
//!   boolean defines to their predicate HIR; cycles are errors;
//! - every HIR node carries `InFragment` or `Unsupported(reason)`.
//!
//! Entry points: [`analyze`] (AST in), [`analyze_str`] (source in),
//! [`ExtDecls::legacy`] (the ingested legacy standard library),
//! [`quantity_table_dump`] / [`hir_dump`] (deterministic dumps).

pub mod dump;
pub mod ext;
pub mod hir;
pub mod intern;
pub mod merge;
pub mod objects;
pub mod quantity;
pub mod rat;
pub mod resolve;

pub use dump::{hir_dump, quantity_table_dump, render_node};
pub use rat::{Rat, RatParts};
pub use merge::merge_hirs;
pub use objects::object_table;
pub use ext::ExtDecls;
pub use hir::{
    ArithOp, DefineKind, ElemPred, Fragment, HKind, HNode, Hir, HirDefine, HirHisto, HirObject,
    HirRegion, HirRegionStmt, HirWeight, HirWeightValue, HistoSpec,
};
pub use intern::{Symbol, SymbolTable};
pub use quantity::{
    AngKind, Collection, CollectionId, ElemIndex, ElemPredId, ParticleRef, PropId, Quantity,
    QuantityArg, QuantityId, QuantityTable, ScalarSource,
};
pub use resolve::{analyze, analyze_str};

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-sema";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-sema");
    }
}
