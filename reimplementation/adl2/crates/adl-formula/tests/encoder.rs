//! Encoder per-row tests against tiny HIR fixtures: one test (at least)
//! per row of the SPEC_ANALYSIS §1 table.

use adl_formula::{EncodedRegion, Formula, LinAtom, QFormula, Rel, encode_region, encode_regions};
use adl_sema::{
    ElemIndex, ExtDecls, Hir, HirRegion, HirRegionStmt, Quantity, QuantityId, QuantityTable, Rat,
    ScalarSource, SymbolTable, analyze_str,
};
use adl_syntax::diag::Severity;
use adl_syntax::span::Span;

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

/// Find the unique quantity matching `pred`.
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

fn met_q(hir: &Hir) -> QuantityId {
    find_q(hir, |q| {
        matches!(q, Quantity::EventScalar(ScalarSource::MetProp(_)))
    })
}

fn ht_q(hir: &Hir) -> QuantityId {
    find_q(hir, |q| {
        matches!(q, Quantity::EventScalar(ScalarSource::EventVar(_)))
    })
}

fn size_q(hir: &Hir) -> QuantityId {
    find_q(hir, |q| matches!(q, Quantity::Size(_)))
}

fn atom(terms: &[(f64, QuantityId)], rel: Rel, k: f64) -> Formula {
    Formula::Atom(LinAtom::new(
        terms
            .iter()
            .map(|&(c, q)| (Rat::from_decimal_f64(c).unwrap(), q)),
        rel,
        Rat::from_decimal_f64(k).unwrap(),
    ))
}

fn atom1(q: QuantityId, rel: Rel, k: f64) -> Formula {
    atom(&[(1.0, q)], rel, k)
}

// ---- row: `select c` over linear arithmetic -----------------------------

#[test]
fn select_linear_comparison_is_one_atom() {
    let (enc, hir) = encode("region SR\n  select MET > 200\n", 0);
    assert_eq!(enc.formula, atom1(met_q(&hir), Rel::Gt, 200.0));
    assert!(enc.is_exact());
    assert!(enc.diags.is_empty());
}

#[test]
fn linear_sums_diffs_const_mults() {
    let (enc, hir) = encode("region SR\n  select 2*MET - HT < 50\n", 0);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    assert_eq!(enc.formula, atom(&[(2.0, met), (-1.0, ht)], Rel::Lt, 50.0));
}

#[test]
fn constant_comparisons_fold() {
    let (t, _) = encode("region SR\n  select 3 > 2\n", 0);
    assert_eq!(t.formula, Formula::True);
    let (f, _) = encode("region SR\n  select 2 > 3\n", 0);
    assert_eq!(f.formula, Formula::False);
}

// ---- row: `reject c` = exact negation -----------------------------------

#[test]
fn reject_is_exact_negation_of_select() {
    let (sel, hir) = encode("region A\n  select MET > 100 or HT > 200\n", 0);
    let (rej, _) = encode("region B\n  reject MET > 100 or HT > 200\n", 0);
    assert_eq!(rej.formula, sel.formula.clone().not());
    // The legacy regression class: reject of an OR must become an AND of
    // negations, not a strengthened/weakened guess.
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    assert_eq!(
        rej.formula,
        Formula::And(vec![atom1(met, Rel::Le, 100.0), atom1(ht, Rel::Le, 200.0)])
    );
}

// ---- row: region inheritance (inline; cycle => Unknown) ------------------

#[test]
fn inheritance_inlines_prior_region() {
    let src = "region base\n  select MET > 100\nregion child\n  base\n  select HT > 50\n";
    let (enc, hir) = encode(src, 1);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    assert_eq!(
        enc.formula,
        Formula::And(vec![atom1(met, Rel::Gt, 100.0), atom1(ht, Rel::Gt, 50.0)])
    );
}

#[test]
fn region_pred_inside_select_inlines() {
    let src = "region presel\n  select MET > 100\nregion SR\n  select presel and HT > 50\n";
    let (enc, hir) = encode(src, 1);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    assert_eq!(
        enc.formula,
        Formula::And(vec![atom1(met, Rel::Gt, 100.0), atom1(ht, Rel::Gt, 50.0)])
    );
}

/// Region cycles are unrepresentable through `analyze_str` (prior-only
/// references), so drive the encoder's cycle guard with a hand-built HIR.
#[test]
fn inheritance_cycle_is_unknown() {
    let mut symbols = SymbolTable::default();
    let a = symbols.intern("A");
    let b = symbols.intern("B");
    let region = |name, target| HirRegion {
        name,
        stmts: vec![HirRegionStmt::Inherit {
            region: target,
            span: Span::default(),
        }],
        span: Span::default(),
    };
    let mut hir = Hir {
        unit: "synthetic".to_owned(),
        symbols,
        table: QuantityTable::default(),
        coll_names: Vec::new(),
        elem_preds: Vec::new(),
        objects: Vec::new(),
        defines: Vec::new(),
        regions: vec![region(a, 1), region(b, 0)],
        region_name_order: vec![a, b],
        histolist_regions: vec![false, false],
        histos: Vec::new(),
        weights: Vec::new(),
        diags: Vec::new(),
    };
    let enc = encode_region(&mut hir, 0);
    let Formula::Unknown(id) = enc.formula else {
        panic!("cycle must encode as Unknown, got {:?}", enc.formula);
    };
    assert!(enc.diags.get(id).unwrap().reason.contains("cycle"));
    assert_eq!(enc.formula.over().into_qformula(), QFormula::True);
    assert_eq!(enc.formula.under().into_qformula(), QFormula::False);
}

// ---- row: `trigger t` => atom trig(t) = 1 --------------------------------

#[test]
fn trigger_is_a_unit_atom() {
    let (enc, hir) = encode("region SR\n  trigger HLT_mu50\n", 0);
    let t = find_q(&hir, |q| {
        matches!(q, Quantity::EventScalar(ScalarSource::Trigger(_)))
    });
    assert_eq!(enc.formula, atom1(t, Rel::Eq, 1.0));
}

#[test]
fn trigger_conjunction_encodes_both_flags() {
    let (enc, _) = encode("region SR\n  trigger HLT_e and HLT_mu\n", 0);
    let Formula::And(parts) = &enc.formula else {
        panic!("expected And, got {:?}", enc.formula);
    };
    assert_eq!(parts.len(), 2);
    for p in parts {
        let Formula::Atom(a) = p else {
            panic!("expected trigger atom, got {p:?}")
        };
        assert_eq!(a.rel(), Rel::Eq);
        assert_eq!(a.constant(), &Rat::from_i64(1));
    }
}

// ---- row: define inlining (by sema, verified through the encoder) -------

#[test]
fn define_encodes_identically_to_textual_paste() {
    let (via_def, hir) = encode(
        "define myht = HT + 2*MET\nregion SR\n  select myht > 100\n",
        0,
    );
    let (inline, _) = encode("region SR\n  select HT + 2*MET > 100\n", 0);
    assert_eq!(via_def.formula, inline.formula);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    assert_eq!(
        via_def.formula,
        atom(&[(1.0, ht), (2.0, met)], Rel::Gt, 100.0)
    );
}

// ---- row: Int-size coercion ----------------------------------------------

#[test]
fn fractional_bounds_on_sizes_coerce_exactly() {
    let (gt, hir) = encode("region SR\n  select size(Jet) > 1.5\n", 0);
    assert_eq!(gt.formula, atom1(size_q(&hir), Rel::Ge, 2.0));

    let (le, hir) = encode("region SR\n  select size(Jet) <= 2.5\n", 0);
    assert_eq!(le.formula, atom1(size_q(&hir), Rel::Le, 2.0));

    let (eq, _) = encode("region SR\n  select size(Jet) == 2.5\n", 0);
    assert_eq!(eq.formula, Formula::False);

    let (ne, _) = encode("region SR\n  select size(Jet) != 2.5\n", 0);
    assert_eq!(ne.formula, Formula::True);
}

#[test]
fn integral_and_non_size_bounds_are_untouched() {
    let (int_bound, hir) = encode("region SR\n  select size(Jet) >= 2\n", 0);
    assert_eq!(int_bound.formula, atom1(size_q(&hir), Rel::Ge, 2.0));

    // Non-integer coefficient: the sum is no longer integer-valued.
    let (frac_coeff, hir) = encode("region SR\n  select 0.5*size(Jet) > 1.2\n", 0);
    assert_eq!(
        frac_coeff.formula,
        atom(&[(0.5, size_q(&hir))], Rel::Gt, 1.2)
    );

    // Non-size quantity: no integrality assumption.
    let (scalar, hir) = encode("region SR\n  select MET > 1.5\n", 0);
    assert_eq!(scalar.formula, atom1(met_q(&hir), Rel::Gt, 1.5));
}

// ---- row: ratio L/D ⋈ c, D non-constant ----------------------------------

#[test]
fn ratio_encodes_exact_two_branches() {
    let (enc, hir) = encode("region SR\n  select MET / HT > 0.5\n", 0);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    let e = &[(1.0, met), (-0.5, ht)];
    assert_eq!(
        enc.formula,
        Formula::Or(vec![
            Formula::And(vec![atom1(ht, Rel::Gt, 0.0), atom(e, Rel::Gt, 0.0)]),
            Formula::And(vec![atom1(ht, Rel::Lt, 0.0), atom(e, Rel::Lt, 0.0)]),
        ])
    );
    assert!(enc.is_exact());
}

#[test]
fn constant_denominator_clears_into_an_exact_atom() {
    // `MET / d ⋈ c` clears the denominator at the comparison level
    // (`MET ⋈ c·d`) rather than folding the f64 reciprocal `1/d` into the
    // coefficient — the latter shifts the cut boundary off the interpreter's
    // for non-dyadic `d` (a false-PROVEN source). For `MET / 2 > 50` that is
    // `MET > 100`, with the coefficient left at exactly 1.
    let (enc, hir) = encode("region SR\n  select MET / 2 > 50\n", 0);
    assert_eq!(enc.formula, atom(&[(1.0, met_q(&hir))], Rel::Gt, 100.0));
}

#[test]
fn constant_division_by_zero_fails_the_cut() {
    // SPEC_LANGUAGE §4.4: the enclosing comparison is false.
    let (sel, _) = encode("region SR\n  select MET / 0 > 1\n", 0);
    assert_eq!(sel.formula, Formula::False);
    // ... and reject of it is exactly true.
    let (rej, _) = encode("region SR\n  reject MET / 0 > 1\n", 0);
    assert_eq!(rej.formula, Formula::True);
}

// ---- row: deterministic non-linear scalar vs constant -> opaque atom ------

fn opaque_q(hir: &Hir) -> QuantityId {
    find_q(hir, |q| matches!(q, Quantity::ExternalFn { .. }))
}

#[test]
fn nonlinear_scalar_vs_constant_interns_as_opaque_atom() {
    // `MET * MET` is a product of two event quantities — not linear — but it
    // is a deterministic per-event scalar. Compared to a constant it interns
    // as one opaque (axiom-free) `ExternalFn` quantity, so the comparison
    // becomes a real atom `O > 4` instead of dropping to Unknown. The region
    // is exact: the leaf is faithfully represented, just over a free var.
    let (enc, hir) = encode("region SR\n  select MET * MET > 4\n", 0);
    let q = opaque_q(&hir);
    let Quantity::ExternalFn { name, .. } = hir.table.quantity(q) else {
        unreachable!()
    };
    assert_eq!(hir.symbols.display(*name), "opaque.scalar");
    assert_eq!(enc.formula, atom(&[(1.0, q)], Rel::Gt, 4.0));
    assert!(enc.is_exact());
}

#[test]
fn identical_nonlinear_scalars_share_one_quantity_across_regions() {
    // The whole point of interning: two regions that compare the SAME
    // non-linear expression to different thresholds must reference the SAME
    // opaque `QuantityId` so the solver can prove `MET*MET > 9` disjoint from
    // `MET*MET < 1`. `find_q`'s uniqueness assertion IS the proof of sharing:
    // a collision-free second interning would make it find two.
    let mut hir =
        build_hir("region HI\n  select MET * MET > 9\nregion LO\n  select MET * MET < 1\n");
    let encs = encode_regions(&mut hir);
    let q = opaque_q(&hir); // panics if the two regions did not share one id
    assert_eq!(encs[0].formula, atom(&[(1.0, q)], Rel::Gt, 9.0));
    assert_eq!(encs[1].formula, atom(&[(1.0, q)], Rel::Lt, 1.0));
}

// ---- row: ternary g ? a : b ----------------------------------------------

#[test]
fn ternary_expands_to_guarded_disjunction() {
    let (enc, hir) = encode("region SR\n  select HT > 500 ? MET > 100 : MET > 200\n", 0);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    let g = atom1(ht, Rel::Gt, 500.0);
    assert_eq!(
        enc.formula,
        Formula::Or(vec![
            Formula::And(vec![g.clone(), atom1(met, Rel::Gt, 100.0)]),
            Formula::And(vec![g.not(), atom1(met, Rel::Gt, 200.0)]),
        ])
    );
}

#[test]
fn ternary_missing_else_is_true() {
    let (enc, hir) = encode("region SR\n  select HT > 500 ? MET > 100\n", 0);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    let g = atom1(ht, Rel::Gt, 500.0);
    assert_eq!(
        enc.formula,
        Formula::Or(vec![
            Formula::And(vec![g.clone(), atom1(met, Rel::Gt, 100.0)]),
            g.not(),
        ])
    );
}

// ---- row: [] / ][ bands ----------------------------------------------------

#[test]
fn inclusive_band_is_conjunction_of_bounds() {
    let (enc, hir) = encode("region SR\n  select MET [] 100 200\n", 0);
    let met = met_q(&hir);
    assert_eq!(
        enc.formula,
        Formula::And(vec![atom1(met, Rel::Ge, 100.0), atom1(met, Rel::Le, 200.0)])
    );
}

#[test]
fn excluded_band_is_disjunction_of_bounds() {
    let (enc, hir) = encode("region SR\n  select MET ][ 100 200\n", 0);
    let met = met_q(&hir);
    assert_eq!(
        enc.formula,
        Formula::Or(vec![atom1(met, Rel::Le, 100.0), atom1(met, Rel::Ge, 200.0)])
    );
}

// ---- row: unindexed collection cut (OPEN-1 Dual, k = 3) -------------------

#[test]
fn open1_dual_bounded_expansion_with_empty_case_in_plus() {
    let (enc, hir) = encode("region SR\n  select Jet.pt > 30\n", 0);
    let sz = size_q(&hir);
    let elem = |i: u32| {
        find_q(&hir, |q| {
            matches!(
                q,
                Quantity::ElemProp {
                    index: ElemIndex::FromFront(n),
                    ..
                } if *n == i
            )
        })
    };
    let (p0, p1, p2) = (elem(0), elem(1), elem(2));

    let Formula::Dual { plus, minus, why } = &enc.formula else {
        panic!("expected Dual, got {:?}", enc.formula);
    };
    assert!(enc.diags.get(*why).unwrap().reason.contains("OPEN-1"));

    // plus = size=0 ∨ P'(0) ∨ P'(1) ∨ P'(2) ∨ size>3, where P'(i) is the
    // instance under its element-existence guard `size > i` (guards make
    // every comparison leaf exact under the missing-element rule; see
    // COUNTEREXAMPLES.md). The empty-collection disjunct is audit Bug 1's
    // fix and MUST be present.
    let guarded = |i: u32, p: QuantityId| {
        Formula::And(vec![
            atom1(sz, Rel::Gt, f64::from(i)),
            atom1(p, Rel::Gt, 30.0),
        ])
    };
    let expected_plus = Formula::Or(vec![
        atom1(sz, Rel::Eq, 0.0),
        guarded(0, p0),
        guarded(1, p1),
        guarded(2, p2),
        atom1(sz, Rel::Gt, 3.0),
    ]);
    assert_eq!(**plus, expected_plus);
    let Formula::Or(plus_parts) = &**plus else {
        unreachable!()
    };
    assert!(
        plus_parts.contains(&atom1(sz, Rel::Eq, 0.0)),
        "empty-collection case missing from the plus branch (audit Bug 1)"
    );

    // minus = 1≤size≤3 ∧ ⋀ᵢ (size≤i ∨ P'(i)).
    let expected_minus = Formula::And(vec![
        atom1(sz, Rel::Ge, 1.0),
        atom1(sz, Rel::Le, 3.0),
        Formula::Or(vec![atom1(sz, Rel::Le, 0.0), guarded(0, p0)]),
        Formula::Or(vec![atom1(sz, Rel::Le, 1.0), guarded(1, p1)]),
        Formula::Or(vec![atom1(sz, Rel::Le, 2.0), guarded(2, p2)]),
    ]);
    assert_eq!(**minus, expected_minus);

    // Projections select the matching branch.
    assert_eq!(
        enc.formula.over().into_qformula(),
        expected_plus.over().into_qformula()
    );
    assert_eq!(
        enc.formula.under().into_qformula(),
        expected_minus.under().into_qformula()
    );
    assert!(!enc.is_exact());
}

#[test]
fn two_unindexed_collections_in_one_comparison_are_unknown() {
    let (enc, _) = encode("region SR\n  select Jet.pt > Muon.pt\n", 0);
    let Formula::Unknown(id) = enc.formula else {
        panic!("expected Unknown, got {:?}", enc.formula);
    };
    assert!(
        enc.diags
            .get(id)
            .unwrap()
            .reason
            .contains("more than one unindexed collection")
    );
}

// ---- row: anything Unsupported => Unknown(diag) ---------------------------

#[test]
fn undeclared_function_encodes_as_unknown() {
    let (enc, _) = encode("region SR\n  select foo(MET) > 1\n", 0);
    let Formula::Unknown(id) = enc.formula else {
        panic!("expected Unknown, got {:?}", enc.formula);
    };
    assert!(enc.diags.get(id).unwrap().reason.contains("foo"));
    assert_eq!(enc.formula.over().into_qformula(), QFormula::True);
    assert_eq!(enc.formula.under().into_qformula(), QFormula::False);
}

#[test]
fn unsupported_statement_contributes_unknown_conjunct() {
    let (enc, hir) = encode("region SR\n  select MET > 100\n  sort Jet.pt\n", 0);
    let met = met_q(&hir);
    let Formula::And(parts) = &enc.formula else {
        panic!("expected And, got {:?}", enc.formula);
    };
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0], atom1(met, Rel::Gt, 100.0));
    assert!(matches!(parts[1], Formula::Unknown(_)));
    assert!(!enc.is_exact());
}

// ---- non-finite constants --------------------------------------------------

#[test]
fn non_finite_literal_is_unknown_not_an_atom() {
    // A 350-digit literal parses to +inf: it cannot construct an atom
    // (audit Bug 5) and must surface as explicit ignorance.
    let big = "9".repeat(350);
    let (enc, _) = encode(&format!("region SR\n  select MET > {big}\n"), 0);
    let Formula::Unknown(id) = enc.formula else {
        panic!("expected Unknown, got {:?}", enc.formula);
    };
    assert!(enc.diags.get(id).unwrap().reason.contains("non-finite"));
}

#[test]
fn constant_arithmetic_does_not_overflow_over_rationals() {
    // ~f64::MAX as a literal; ×10 would overflow f64 but is EXACT over
    // rationals, so the cut stays a normal (satisfiable) atom — no spurious
    // §4.4 collapse to false (and no fabricated empty/disjoint downstream).
    let max = format!("17976931348623157{}", "0".repeat(292));
    let (enc, _) = encode(&format!("region SR\n  select MET > {max} * 10\n"), 0);
    assert!(
        matches!(enc.formula, Formula::Atom(_)),
        "got {:?}",
        enc.formula
    );
}

// ---- exact |E| ⋈ const expansion (extension of the linear row) ------------

#[test]
fn abs_versus_constant_expands_exactly() {
    let (lt, hir) = encode("region SR\n  select abs(MET - 200) < 50\n", 0);
    let met = met_q(&hir);
    assert_eq!(
        lt.formula,
        Formula::And(vec![atom1(met, Rel::Lt, 250.0), atom1(met, Rel::Gt, 150.0)])
    );

    let (gt, hir) = encode("region SR\n  select abs(MET - 200) > 50\n", 0);
    let met = met_q(&hir);
    assert_eq!(
        gt.formula,
        Formula::Or(vec![atom1(met, Rel::Gt, 250.0), atom1(met, Rel::Lt, 150.0)])
    );
}

// `|E| >= 0`, so comparing `|E|` to a NEGATIVE constant is itself constant.
// Regression for the abs_cmp soundness bug: `|E| == c` (c<0) was encoded as
// a satisfiable two-point disjunction and `|E| != c` (c<0) as a two-point
// exclusion — both wrong, letting through false PROVEN verdicts. The exact,
// relation-uniform answer is True/False with no atoms over MET at all.
#[test]
fn abs_versus_negative_constant_is_exactly_constant() {
    for (op, expect) in [
        ("<", Formula::False),
        ("<=", Formula::False),
        ("==", Formula::False),
        (">", Formula::True),
        (">=", Formula::True),
        ("!=", Formula::True),
    ] {
        let (enc, _hir) = encode(&format!("region SR\n  select abs(MET - 200) {op} -5\n"), 0);
        assert_eq!(enc.formula, expect, "abs(...) {op} -5 must fold to {expect:?}");
    }
}

// Boundary guard: `c == 0` must NOT be swallowed by the `c < 0` short-circuit
// (0.0 < 0.0 is false), so it still takes the exact general expansion and
// stays a genuine constraint on MET — `|MET-200| == 0` is satisfiable (only at
// MET==200) and `|MET-200| != 0` is not a tautology. A regression that widened
// the guard to `c <= 0` would fold these to False/True respectively.
#[test]
fn abs_versus_zero_keeps_the_exact_constraint() {
    let (eq, _hir) = encode("region SR\n  select abs(MET - 200) == 0\n", 0);
    assert_ne!(eq.formula, Formula::False, "|MET-200| == 0 is satisfiable at MET==200");
    assert_ne!(eq.formula, Formula::True);

    let (ne, _hir) = encode("region SR\n  select abs(MET - 200) != 0\n", 0);
    assert_ne!(ne.formula, Formula::True, "|MET-200| != 0 is not a tautology");
    assert_ne!(ne.formula, Formula::False);
}

// ---- whole-file smoke -------------------------------------------------------

#[test]
fn encode_regions_covers_every_region_in_order() {
    let src = "region A\n  select MET > 100\nregion B\n  select HT > 50\n";
    let mut hir = build_hir(src);
    let all = encode_regions(&mut hir);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "A");
    assert_eq!(all[1].name, "B");
    assert!(all.iter().all(EncodedRegion::is_exact));
}

#[test]
fn region_with_no_membership_statements_is_true() {
    let (enc, _) = encode("region SR\n  bin MET 100 200 300\n", 0);
    assert_eq!(enc.formula, Formula::True);
}

// ---- polarity safety through reject (Dual branch swap) ---------------------

#[test]
fn reject_of_unindexed_cut_swaps_dual_branches() {
    let (sel, _) = encode("region A\n  select Jet.pt > 30\n", 0);
    let (rej, _) = encode("region B\n  reject Jet.pt > 30\n", 0);
    assert_eq!(rej.formula, sel.formula.clone().not());
    let Formula::Dual { plus, minus, .. } = &sel.formula else {
        panic!("expected Dual select, got {:?}", sel.formula);
    };
    let Formula::Dual {
        plus: rplus,
        minus: rminus,
        ..
    } = &rej.formula
    else {
        panic!("expected Dual reject, got {:?}", rej.formula);
    };
    // ¬(minus ⊆ R ⊆ plus) ⇒ ¬plus ⊆ ¬R ⊆ ¬minus: branches swap.
    assert_eq!(**rplus, (**minus).clone().not());
    assert_eq!(**rminus, (**plus).clone().not());
}

#[test]
fn band_over_unindexed_collection_is_dual() {
    let (enc, _) = encode("region SR\n  select Jet.pt [] 20 50\n", 0);
    let Formula::Dual { plus, .. } = &enc.formula else {
        panic!("expected Dual, got {:?}", enc.formula);
    };
    // Each expanded instance is the two-bound conjunction for one element.
    let Formula::Or(parts) = &**plus else {
        panic!("expected Or plus branch, got {plus:?}");
    };
    assert_eq!(parts.len(), 5); // size=0, three instances, size>3
    assert!(matches!(parts[1], Formula::And(_)));
}

// ---- row: composite per-candidate cut existence (2D dual, P3) ------------

/// Collect every linear atom in a formula (projection-agnostic).
fn atoms(f: &Formula, out: &mut Vec<LinAtom>) {
    match f {
        Formula::Atom(a) => out.push(a.clone()),
        Formula::And(v) | Formula::Or(v) => v.iter().for_each(|p| atoms(p, out)),
        Formula::Dual { plus, minus, .. } => {
            atoms(plus, out);
            atoms(minus, out);
        }
        Formula::True | Formula::False | Formula::Unknown(_) => {}
    }
}

/// An OPAQUE per-tuple cut (`mass(jj)` is irrational ⇒ Unknown leaf) must
/// NOT tighten the over-side: the existence disjunction folds to `true`, so
/// the over-projection equals the plain `size(K) >= 1` atom. This is the
/// load-bearing no-op-on-opacity guarantee — the corpus reality.
#[test]
fn comb_existence_opaque_cut_is_a_noop_over() {
    let src = "\
object jets
  take Jet
  select pt > 30

composite dijet
  take disjoint(jets j1, jets j2)
  candidate jj = j1 + j2
  select mass(jj) > 20

region SR
  select size(dijet->jj) >= 1
";
    let (enc, hir) = encode(src, 0);
    // A Dual is built; its OVER projection is satisfiability-equivalent to
    // the plain `size(K) >= 1` atom because every per-tuple disjunct over the
    // opaque candidate mass folds to `true`.
    let Formula::Dual { plus, minus, .. } = &enc.formula else {
        panic!("expected a 2D Dual, got {:?}", enc.formula);
    };
    // plus = And([size>=1, Or([Unknown(=mass opaque), escape…])]); over →
    // the existence Or carries a `true` disjunct (Unknown→true), so it folds
    // away under the solver, leaving only the size constraint.
    let over = enc.formula.over().into_qformula();
    let QFormula::And(parts) = &over else {
        panic!("expected And over, got {over:?}");
    };
    let size_ge1 = QFormula::Atom(LinAtom::new(
        [(Rat::from_decimal_f64(1.0).unwrap(), {
            // the projected size(K) atom is the first And conjunct
            let QFormula::Atom(a) = &parts[0] else {
                panic!("first conjunct must be the size atom");
            };
            a.terms()[0].1
        })],
        Rel::Ge,
        Rat::from_decimal_f64(1.0).unwrap(),
    ));
    assert_eq!(parts[0], size_ge1, "first conjunct is size(K) >= 1");
    // The existence conjunct over-projects to a disjunction whose tuple
    // disjunct is just existence guards (the opaque `mass(jj)` cut folded to
    // `true`): NO atom anywhere references the candidate-mass quantity.
    let QFormula::Or(ex) = &parts[1] else {
        panic!("second conjunct must be the existence Or, got {:?}", parts[1]);
    };
    fn qatoms(f: &QFormula, out: &mut Vec<LinAtom>) {
        match f {
            QFormula::Atom(a) => out.push(a.clone()),
            QFormula::And(v) | QFormula::Or(v) => v.iter().for_each(|p| qatoms(p, out)),
            QFormula::True | QFormula::False => {}
        }
    }
    let mut over_atoms = Vec::new();
    qatoms(&over, &mut over_atoms);
    // The candidate mass is an opaque ExternalFn quantity; it must NOT survive
    // into the over-projection (it folded to `true`).
    let mass_q = (0..hir.table.quantities().len())
        .map(|i| QuantityId(u32::try_from(i).unwrap()))
        .find(|&q| matches!(hir.table.quantity(q), Quantity::ExternalFn { .. }));
    if let Some(mq) = mass_q {
        assert!(
            over_atoms.iter().all(|a| a.terms().iter().all(|(_, q)| *q != mq)),
            "opaque candidate mass must NOT reach the over-projection"
        );
    }
    // The tuple's existence disjunct carries only source-size guards.
    assert!(
        ex.iter().any(|d| matches!(d, QFormula::And(_))),
        "the (0,1) tuple disjunct (existence guards) must be present, got {ex:?}"
    );
    // Under side is the plain atom (no strengthening — USER ANSWER 4).
    let under = enc.formula.under().into_qformula();
    assert_eq!(under, size_ge1, "under == plain size atom, no per-tuple atom");
    let _ = (plus, minus);
}

/// An ANALYZABLE per-tuple cut (`dphi(j1,j2)` is a linear angular sep) DOES
/// reach the over-side: the substituted atom `dphi(jets[0], jets[1]) > 0.5`
/// appears, so the over-projection is no longer the bare size atom. The
/// existence structure is present and references the bound source elements.
#[test]
fn comb_existence_analyzable_cut_reaches_over_side() {
    let src = "\
object jets
  take Jet
  select pt > 30

composite dijet
  take disjoint(jets j1, jets j2)
  candidate jj = j1 + j2
  select dphi(j1, j2) > 0.5

region SR
  select size(dijet->jj) >= 1
";
    let (enc, hir) = encode(src, 0);
    let Formula::Dual { plus, minus, why } = &enc.formula else {
        panic!("expected a 2D Dual, got {:?}", enc.formula);
    };
    assert!(
        enc.diags.get(*why).unwrap().reason.contains("2D"),
        "diag must name the 2D expansion"
    );
    // The SUBSTITUTED angular sep dphi(jets[0], jets[1]) — a DIFFERENT
    // interned quantity than the original binder-arg dphi(j1, j2) — appears in
    // the over (plus) side. We find it by its `Elem` arguments.
    use adl_sema::{ElemIndex as EI, ParticleRef};
    let subst_dphi = hir.table.quantities().iter().position(|q| {
        matches!(q, Quantity::AngularSep { a, b, .. }
            if matches!(a, ParticleRef::Elem { index: EI::FromFront(_), .. })
            && matches!(b, ParticleRef::Elem { index: EI::FromFront(_), .. }))
    });
    assert!(
        subst_dphi.is_some(),
        "binder dphi must substitute to an Elem-arg AngularSep"
    );
    let aq = QuantityId(u32::try_from(subst_dphi.unwrap()).unwrap());
    let mut plus_atoms = Vec::new();
    atoms(plus, &mut plus_atoms);
    assert!(
        plus_atoms.iter().any(|a| a.terms().iter().any(|(_, q)| *q == aq)),
        "the analyzable per-tuple cut must reach the over (plus) side"
    );
    // The UNDER side carries NO per-tuple atom — it is exactly the plain size
    // atom (no existence strengthening; USER ANSWER 4 keeps the disjoint
    // lower bound opaque).
    let mut minus_atoms = Vec::new();
    atoms(minus, &mut minus_atoms);
    assert!(
        minus_atoms.iter().all(|a| a.terms().iter().all(|(_, q)| *q != aq)),
        "under side must NOT strengthen with the per-tuple cut (USER ANSWER 4)"
    );
}

/// The 2D Dual negates soundly: `reject size(K) >= 1` swaps branches so the
/// over-side becomes `size(K) <= 0` (a superset of the true "no surviving
/// tuple" set) — never a false claim about the per-tuple cut.
#[test]
fn comb_existence_dual_negates_soundly() {
    let src = "\
object jets
  take Jet
  select pt > 30

composite dijet
  take disjoint(jets j1, jets j2)
  candidate jj = j1 + j2
  select dphi(j1, j2) > 0.5

region SR
  reject size(dijet->jj) >= 1
";
    let (enc, _) = encode(src, 0);
    let Formula::Dual { plus, minus, .. } = &enc.formula else {
        panic!("expected a 2D Dual under reject, got {:?}", enc.formula);
    };
    // Over (plus) of the negated form is the under(atom).not() = (size>=1).not()
    // = size < 1: a sound superset of "no tuple survives" (integers: < 1 ⇔ = 0).
    let over = enc.formula.over().into_qformula();
    let QFormula::Atom(a) = &over else {
        panic!("negated over must be the single size<1 atom, got {over:?}");
    };
    assert_eq!(a.rel(), Rel::Lt);
    assert_eq!(a.constant(), &Rat::from_decimal_f64(1.0).unwrap());
    let _ = (plus, minus);
}

// ---- value-position numeric reducer interning (`sum`/`min`/`max`) --------

/// Every interned reducer quantity (an `ExternalFn` whose synthesized name
/// is `reduce.<kind>`), in id order.
fn reduce_qids(hir: &Hir) -> Vec<QuantityId> {
    hir.table
        .quantities()
        .iter()
        .enumerate()
        .filter(|(_, q)| {
            matches!(q, Quantity::ExternalFn { name, .. }
                if hir.symbols.key(*name).starts_with("reduce."))
        })
        .map(|(i, _)| QuantityId(u32::try_from(i).unwrap()))
        .collect()
}

/// SOUNDNESS-CRITICAL false-unification regression: six *structurally
/// different* value-position numeric reducers must intern to six DISTINCT
/// free quantities. If any two collapsed, a pair of regions guarded by them
/// (`sum(jets.pT)>400` vs `sum(eles.pT)<=400`) would be falsely PROVEN
/// DISJOINT even though an event can satisfy both.
#[test]
fn distinct_reducers_get_distinct_quantities() {
    // Each reducer differs from the others in exactly one structural axis:
    //   kind (sum/min/max), iteration collection (jets/eles/filtered jets),
    //   or body property (pT/eta). min/max are wrapped in arithmetic so the
    //   resolver's `min/max ⋈ c → any/all` desugar does not fire (it only
    //   matches a *bare* reducer on a comparison side).
    let src = "\
object jets
  take Jet
object eles
  take Ele
object goodjets
  take Jet
  select pT > 30

region SR
  select sum(jets.pT) > 400
  select sum(eles.pT) > 400
  select sum(jets.eta) > 1
  select 2 * min(jets.pT) > 60
  select 2 * max(jets.pT) > 60
  select sum(goodjets.pT) > 400
";
    let (_, hir) = encode(src, 0);
    let qids = reduce_qids(&hir);
    let distinct: std::collections::BTreeSet<_> = qids.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        6,
        "expected 6 DISTINCT interned reducers, got {} (ids {:?}); a collision \
         would fabricate a false PROVEN DISJOINT",
        distinct.len(),
        qids
    );
}

/// Two *structurally identical* `sum(jets.pT)` occurrences (one direct, one
/// via a `define`) must share ONE quantity id — this is the cancellation the
/// fix restores.
#[test]
fn identical_reducers_share_one_quantity() {
    let src = "\
object jets
  take Jet
define HT = sum(jets.pT)

region SR
  select HT > 400
  select sum(jets.pT) < 1000
";
    let (_, hir) = encode(src, 0);
    let qids = reduce_qids(&hir);
    assert_eq!(
        qids.len(),
        1,
        "structurally identical sums must intern to ONE quantity, got {qids:?}"
    );
}

/// A bare value-position `sum(jets.pT) > 400` encodes to exactly ONE exact
/// atom over the interned free reducer quantity (the leaf the interval engine
/// cancels across regions).
#[test]
fn value_position_sum_is_one_exact_atom() {
    let src = "\
object jets
  take Jet
region SR
  select sum(jets.pT) > 400
";
    let (enc, hir) = encode(src, 0);
    let q = reduce_qids(&hir);
    assert_eq!(q.len(), 1, "one interned reducer expected");
    assert_eq!(enc.formula, atom1(q[0], Rel::Gt, 400.0));
    assert!(enc.is_exact(), "a free-quantity atom is exact");
}
