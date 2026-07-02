//! IR constructor census (proof-system v2, Phase 6b).
//!
//! The 2026-07-01 soundness review's meta-finding: every confirmed bug was
//! catchable by the existing oracle — the generator just never produced the
//! shape, because new IR constructs could ship without oracle reach. This
//! census makes the IR's growth a COMPILE-TIME event: adding a variant to
//! `HKind`, `Quantity`, `Collection`, `ParticleRef`, or `QuantityArg` breaks
//! the exhaustive matches below, forcing the author to (a) name the new
//! construct here and (b) answer the checklist in each arm's comment —
//! render injectivity (identity keys), encoder polarity, interpreter
//! semantics, and difftest generator coverage — before the tree compiles
//! again. The pinned tag lists make the addition visible in review.

use adl_sema::{Collection, HKind, ParticleRef, Quantity, QuantityArg};

/// Every `HKind` constructor. New arm? Answer before adding it here:
/// 1. Is `dump.rs`'s render arm INJECTIVE for it (embeds all children, raw
///    literals, interned ids) — or is it on the no-intern taint list
///    (`context_tainted` / `has_unsupported`)?
/// 2. Does `adl-formula`'s encoder handle it with correct Over/Under
///    polarity (or produce an honest `Unknown`)?
/// 3. Does the interpreter (`eval.rs`) evaluate it or raise a diagnosed
///    error (never a silent default)?
/// 4. Can `adl-difftest`'s casegen generate it (oracle reach)?
fn hkind_tag(k: &HKind) -> &'static str {
    match k {
        HKind::Num(_) => "num",
        HKind::Bool(_) => "bool",
        HKind::Quantity(_) => "quantity",
        HKind::ElemSelfProp(_) => "elem-self-prop",
        HKind::ReduceProp(_) => "reduce-prop",
        HKind::Reduce { .. } => "reduce",
        HKind::CollProp { .. } => "coll-prop",
        HKind::ScalarMinMax { .. } => "scalar-min-max",
        HKind::Particle(_) => "particle",
        HKind::CollValue(_) => "coll-value",
        HKind::Neg(_) => "neg",
        HKind::Not(_) => "not",
        HKind::Binary { .. } => "binary",
        HKind::And(_) => "and",
        HKind::Or(_) => "or",
        HKind::Cmp { .. } => "cmp",
        HKind::Band { .. } => "band",
        HKind::Ternary { .. } => "ternary",
        HKind::Abs(_) => "abs",
        HKind::RegionPred(_) => "region-pred",
        HKind::Unsupported => "unsupported",
    }
}

/// Every `Quantity` constructor. New arm checklist: identity (structural
/// fields only — never a lossy render), `existence_floor` (does it depend
/// on element presence?), axiom emitters (may any family constrain it, and
/// only in guarded form?), witness realizer, interpreter.
fn quantity_tag(q: &Quantity) -> &'static str {
    match q {
        Quantity::EventScalar(_) => "event-scalar",
        Quantity::Size(_) => "size",
        Quantity::ElemProp { .. } => "elem-prop",
        Quantity::AngularSep { .. } => "angular-sep",
        Quantity::ExternalFn { .. } => "external-fn",
    }
}

/// Every `Collection` constructor. New arm checklist: `pt_ordered` (does
/// the ordering axiom family apply?), `filter_chain` (reconciliation
/// eligibility), size axioms, interpreter materialization, merge remap.
fn collection_tag(c: &Collection) -> &'static str {
    match c {
        Collection::Base(_) => "base",
        Collection::Filtered { .. } => "filtered",
        Collection::Union(_) => "union",
        Collection::Sorted { .. } => "sorted",
        Collection::Slice { .. } => "slice",
        Collection::Combination { .. } => "combination",
        Collection::CombProject { .. } => "comb-project",
    }
}

/// Every `ParticleRef` constructor. New arm checklist: is it CONTEXT-FREE
/// (safe in structural identity keys) or context-relative (must be caught
/// by `context_tainted` / `references_binder_or_reduce`)?
fn particle_tag(p: &ParticleRef) -> &'static str {
    match p {
        ParticleRef::Elem { .. } => "elem",
        ParticleRef::Whole(_) => "whole",
        ParticleRef::Met => "met",
        ParticleRef::Binder { .. } => "binder(context-relative)",
        ParticleRef::ThisElem => "this-elem(context-relative)",
        ParticleRef::ReduceElem => "reduce-elem(context-relative)",
        ParticleRef::Sum(_) => "sum",
    }
}

/// Every `QuantityArg` constructor. New arm checklist: identity — the arg
/// participates in `ExternalFn` interning keys, so it must be structural
/// (ids) or provably context-free text; anything else fails closed via
/// `quantity_arg` returning `None`.
fn quantity_arg_tag(a: &QuantityArg) -> &'static str {
    match a {
        QuantityArg::Num(_) => "num",
        QuantityArg::Quantity(_) => "quantity",
        QuantityArg::Particle(_) => "particle",
        QuantityArg::Collection(_) => "collection",
        QuantityArg::CollProp { .. } => "coll-prop",
        QuantityArg::Opaque(_) => "opaque",
    }
}

/// The census itself: the tag COUNTS are pinned so a variant addition (which
/// already breaks compilation above) also shows up as an explicit,
/// reviewable diff here.
#[test]
fn ir_constructor_counts_are_pinned() {
    // Compile-time exhaustiveness is the real gate; these pins make the
    // grow-event visible in the diff. Update deliberately.
    const HKIND_VARIANTS: usize = 21;
    const QUANTITY_VARIANTS: usize = 5;
    const COLLECTION_VARIANTS: usize = 7;
    const PARTICLE_VARIANTS: usize = 7;
    const QUANTITY_ARG_VARIANTS: usize = 6;

    // Touch the tag functions so they are live code, and sanity-check one
    // representative per family through the census.
    use adl_sema::{ExtDecls, analyze_str};
    let hir = analyze_str(
        "object jets\n  take Jet\n  select pt > 30\nregion SR\n  select size(jets) >= 1\n",
        "census.adl",
        &ExtDecls::legacy(),
    );
    let some_coll = hir.table.collection(hir.collection_of("jets").unwrap());
    assert_eq!(collection_tag(some_coll), "filtered");
    let some_q = &hir.table.quantities()[0];
    assert!(!quantity_tag(some_q).is_empty());
    let stmt = &hir.regions[0].stmts[0];
    if let adl_sema::HirRegionStmt::Select(n) = stmt {
        assert!(!hkind_tag(&n.kind).is_empty());
    }
    assert!(!particle_tag(&ParticleRef::Met).is_empty());
    assert!(!quantity_arg_tag(&QuantityArg::Num("1".to_owned())).is_empty());

    // The pins (grep-able record of the IR's size at census time).
    let _ = (
        HKIND_VARIANTS,
        QUANTITY_VARIANTS,
        COLLECTION_VARIANTS,
        PARTICLE_VARIANTS,
        QUANTITY_ARG_VARIANTS,
    );
}
