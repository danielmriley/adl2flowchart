//! Regression tests for scalar `min`/`max` comparison encoding: the
//! existence guards must survive an opaque argument (S5, a false PROVEN
//! SUBSET), and an unindexed collection argument must terminate rather than
//! re-expand forever (S6, a stack-overflow crash).

use adl_formula::{EncodedRegion, Formula, LinAtom, QFormula, Rel, encode_region};
use adl_sema::{ElemIndex, ExtDecls, Hir, Quantity, QuantityId, Rat, analyze_str};
use adl_syntax::diag::Severity;

fn build_hir(src: &str) -> Hir {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, "test.adl", &ext);
    assert!(
        !hir.diags.iter().any(|d| d.severity == Severity::Error),
        "fixture has sema/parse errors: {:?}",
        hir.diags
    );
    hir
}

fn encode(src: &str, region: usize) -> (EncodedRegion, Hir) {
    let mut hir = build_hir(src);
    let enc = encode_region(&mut hir, region);
    (enc, hir)
}

fn find_q(hir: &Hir, pred: impl Fn(&Quantity) -> bool) -> QuantityId {
    let hits: Vec<usize> = hir
        .table
        .quantities()
        .iter()
        .enumerate()
        .filter(|(_, q)| pred(q))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(hits.len(), 1, "expected exactly one matching quantity");
    QuantityId(u32::try_from(hits[0]).unwrap())
}

fn size_q(hir: &Hir) -> QuantityId {
    find_q(hir, |q| matches!(q, Quantity::Size(_)))
}

fn front_elem_q(hir: &Hir, idx: u32) -> QuantityId {
    find_q(hir, |q| {
        matches!(q, Quantity::ElemProp { index: ElemIndex::FromFront(n), .. } if *n == idx)
    })
}

fn qatoms(f: &QFormula, out: &mut Vec<LinAtom>) {
    match f {
        QFormula::Atom(a) => out.push(a.clone()),
        QFormula::And(v) | QFormula::Or(v) => v.iter().for_each(|p| qatoms(p, out)),
        QFormula::True | QFormula::False => {}
    }
}

fn qcontains_false(f: &QFormula) -> bool {
    match f {
        QFormula::False => true,
        QFormula::And(v) | QFormula::Or(v) => v.iter().any(qcontains_false),
        _ => false,
    }
}

fn is_guard_atom(a: &LinAtom, size: QuantityId, floor: i64) -> bool {
    a.rel() == Rel::Gt
        && a.constant() == &Rat::from_i64(floor)
        && a.terms().len() == 1
        && a.terms()[0].1 == size
        && a.terms()[0].0 == Rat::one()
}

/// S5: one opaque argument (a value-position ternary) must NOT drop the
/// element-existence guard for the *other*, exactly-encoded argument. Before
/// the fix the under-projection was the bare, satisfiable atom
/// `jets[0].pt < 50` — an under-approximation claiming membership on events
/// with an empty jet collection, fabricating a PROVEN SUBSET.
#[test]
fn opaque_arg_keeps_existence_guard_and_falsifies_under() {
    let src = "\
object jets
  take Jet
region SR
  select min(jets[0].pt, MET > 100 ? MET : 7) < 50
";
    let (enc, hir) = encode(src, 0);
    let sz = size_q(&hir);

    // The opaque ternary arg makes the whole leaf non-exact.
    assert!(!enc.is_exact(), "an opaque arg must leave the leaf non-exact");

    // The under projection carries the size guard (emitted UNCONDITIONALLY,
    // not gated on exactness) ...
    let under = enc.formula.under().into_qformula();
    let mut under_atoms = Vec::new();
    qatoms(&under, &mut under_atoms);
    assert!(
        under_atoms.iter().any(|a| is_guard_atom(a, sz, 0)),
        "under projection must contain the `size(jets) > 0` guard, got {under:?}"
    );

    // ... and is honestly false (a `False` conjunct): an opaque arg's
    // definedness decides a strict min comparison, so we cannot prove
    // membership. The element atom never appears UNGUARDED — every element
    // reference sits under the conjoined size guard.
    assert!(
        qcontains_false(&under),
        "under projection must collapse to false via the opaque arg, got {under:?}"
    );

    // The over projection still requires the element to exist (the guard is a
    // necessary condition on both sides), so it is not the trivial `true`.
    let over = enc.formula.over().into_qformula();
    let mut over_atoms = Vec::new();
    qatoms(&over, &mut over_atoms);
    assert!(
        over_atoms.iter().any(|a| is_guard_atom(a, sz, 0)),
        "over projection must retain the `size(jets) > 0` guard, got {over:?}"
    );
}

/// No regression: with BOTH args exactly encoded the leaf stays exact and
/// still carries the element-existence guard (`min(a,b)` is a value only when
/// both a and b exist ⇒ `size(jets) > 1`).
#[test]
fn both_args_exact_still_guards_and_stays_exact() {
    let src = "\
object jets
  take Jet
region SR
  select min(jets[0].pt, jets[1].pt) < 50
";
    let (enc, hir) = encode(src, 0);
    assert!(enc.is_exact(), "two exact args must keep the leaf exact");

    let (sz, e0, e1) = (size_q(&hir), front_elem_q(&hir, 0), front_elem_q(&hir, 1));
    // Exact ⇒ both projections are the formula itself; collect its atoms.
    let q = enc.formula.under().into_qformula();
    let mut leaf_atoms = Vec::new();
    qatoms(&q, &mut leaf_atoms);

    // `min(jets[0], jets[1])` is a value only when both exist ⇒ `size > 1`
    // (the per-collection max over the two element floors 0 and 1).
    assert!(
        leaf_atoms.iter().any(|a| is_guard_atom(a, sz, 1)),
        "exact min must still carry the `size(jets) > 1` guard, got {q:?}"
    );
    // Both element comparisons survive in the monotone disjunction.
    assert!(
        leaf_atoms.contains(&LinAtom::single(e0, Rel::Lt, Rat::from_i64(50))),
        "jets[0].pt < 50 must appear, got {q:?}"
    );
    assert!(
        leaf_atoms.contains(&LinAtom::single(e1, Rel::Lt, Rat::from_i64(50))),
        "jets[1].pt < 50 must appear, got {q:?}"
    );
    // No unindexed collection escaped to a Dual/Unknown — the leaf is exact.
    assert!(!qcontains_false(&q), "exact leaf must not carry a False conjunct");
}

/// S6: `min` over an UNINDEXED collection property must terminate. Without a
/// `ScalarMinMax` arm in `subst`, the OPEN-1 leaf expansion re-substituted
/// nothing and re-expanded the same `jets.pt` collprop forever (stack
/// overflow). Any sound result is acceptable; the assertion below simply
/// proves the encoder returned.
#[test]
fn min_over_unindexed_collection_terminates() {
    let src = "\
object jets
  take Jet
region SR
  select min(jets.pt, MET) < 50
";
    let (enc, _hir) = encode(src, 0);
    // Reaching this line proves termination. The unindexed collprop drives the
    // OPEN-1 bounded expansion, so the result is a Dual (both projections are
    // well-formed, no infinite recursion).
    assert!(
        matches!(enc.formula, Formula::Dual { .. }),
        "expected an OPEN-1 Dual, got {:?}",
        enc.formula
    );
}
