//! Encoder per-row tests against tiny HIR fixtures: one test (at least)
//! per row of the SPEC_ANALYSIS §1 table.

use adl_formula::{EncodedRegion, Formula, LinAtom, QFormula, Rel, encode_region, encode_regions};
use adl_sema::{
    ElemIndex, ExtDecls, Hir, HirRegion, HirRegionStmt, Quantity, QuantityId, QuantityTable,
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
    Formula::Atom(LinAtom::new(terms.iter().copied(), rel, k).unwrap())
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
        assert_eq!((a.rel(), a.constant()), (Rel::Eq, 1.0));
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
fn constant_denominator_folds_into_coefficients() {
    let (enc, hir) = encode("region SR\n  select MET / 2 > 50\n", 0);
    assert_eq!(enc.formula, atom(&[(0.5, met_q(&hir))], Rel::Gt, 50.0));
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
fn constant_arithmetic_overflow_fails_the_cut() {
    // ~f64::MAX as a literal is finite; ×10 overflows during folding, so
    // the enclosing comparison is false (SPEC_LANGUAGE §4.4).
    let max = format!("17976931348623157{}", "0".repeat(292));
    let (enc, _) = encode(&format!("region SR\n  select MET > {max} * 10\n"), 0);
    assert_eq!(enc.formula, Formula::False);
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
