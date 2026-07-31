//! Fold faithfulness: the encoder's decision boundary must be the
//! interpreter's decision boundary, leaf for leaf.
//!
//! Two regimes, and the whole file is about keeping them apart:
//!
//! - **rational fragment** (event properties, sizes, event scalars, `MET.pt`,
//!   literals, and `+ - * /` over them) — the interpreter is exact there, so
//!   the encoder folds exactly. `0.1 + 0.2` is `3/10` on both sides.
//! - **approximate** (`dR`, external kinematics, `^`) — the interpreter works
//!   in f64, so the encoder may only flatten IEEE-exact steps, and a
//!   threshold compared against such a value is `fl(k)`, not `k`.
//!
//! History: audit 2026-07-28 C1–C6 / CE-14 were all *one* bug — the encoder
//! folding exactly while the interpreter stepped in f64. M3 moved the
//! interpreter onto exact rationals, which killed those counterexamples at
//! the source; M4 (this file) realigned the encoder so the two agree again,
//! and closed the mirror-image seam the realignment exposed (`0.1 + 0.2`
//! folding to the f64 sum while the interpreter had gone exact).

use adl_formula::{
    EncodedRegion, Formula, LinAtom, MAX_STATIC_SLICE_REDUCE, Rel, encode_region, encode_regions,
};
use adl_sema::{ExtDecls, Hir, Quantity, QuantityId, Rat, ScalarSource, analyze_str};
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
    assert_eq!(
        hits.len(),
        1,
        "expected exactly one matching quantity, got {hits:?}"
    );
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

fn has_opaque(hir: &Hir) -> bool {
    hir.table.quantities().iter().any(|q| {
        matches!(q, Quantity::ExternalFn { name, .. }
            if hir.symbols.display(*name) == "opaque.scalar")
    })
}

fn atom1(q: QuantityId, rel: Rel, k: f64) -> Formula {
    Formula::Atom(LinAtom::new(
        [(Rat::one(), q)],
        rel,
        Rat::from_decimal_f64(k).unwrap(),
    ))
}

fn collect_atoms(f: &Formula, out: &mut Vec<LinAtom>) {
    match f {
        Formula::Atom(a) => out.push(a.clone()),
        Formula::And(v) | Formula::Or(v) => v.iter().for_each(|p| collect_atoms(p, out)),
        Formula::Dual { plus, minus, .. } => {
            collect_atoms(plus, out);
            collect_atoms(minus, out);
        }
        Formula::True | Formula::False | Formula::Unknown(_) => {}
    }
}

fn only_atom(f: &Formula) -> LinAtom {
    let mut out = Vec::new();
    collect_atoms(f, &mut out);
    assert_eq!(out.len(), 1, "expected a single atom, got {f:?}");
    out.remove(0)
}

/// Atoms of the cut itself, dropping the element-existence guards
/// (`size(jets) > 1`) an indexed reference adds.
fn cut_atoms(f: &Formula, hir: &Hir) -> Vec<LinAtom> {
    let mut out = Vec::new();
    collect_atoms(f, &mut out);
    out.retain(|a| {
        !a.terms()
            .iter()
            .all(|(_, q)| matches!(hir.table.quantity(*q), Quantity::Size(_)))
    });
    out
}

fn only_cut_atom(f: &Formula, hir: &Hir) -> LinAtom {
    let mut out = cut_atoms(f, hir);
    assert_eq!(out.len(), 1, "expected a single cut atom, got {f:?}");
    out.remove(0)
}

/// The exact rational boundary of a single-cut region — the value the
/// analyzer will cut at.
fn boundary(src: &str) -> Rat {
    let (enc, hir) = encode(src, 0);
    only_cut_atom(&enc.formula, &hir).constant().clone()
}

fn rat(v: f64) -> Rat {
    Rat::from_decimal_f64(v).unwrap()
}

/// A jets collection, for the `dR` (approximate) fixtures.
const JETS: &str = "object jets\n  take Jet\n";

// ---- the rational fragment folds exactly ----------------------------------

/// The M4 kill case, at the encoder level: `0.1 + 0.2` must fold to `3/10`,
/// which is what the interpreter computes, NOT to the f64 sum
/// `0.30000000000000004`. While the two disagreed, `MET > 0.1 + 0.2` and a
/// band pinned at `0.30000000000000004` were "proven disjoint" although the
/// event `MET = 0.30000000000000004` passes both.
#[test]
fn constant_addition_folds_to_the_exact_decimal_sum() {
    let k = boundary("region SR\n  select MET > 0.1 + 0.2\n");
    assert_eq!(k, Rat::from_ratio(3, 10).unwrap());
    assert_ne!(
        k,
        rat(0.1 + 0.2),
        "folding in f64 is the bug this test exists for"
    );
}

#[test]
fn constant_multiplication_folds_to_the_exact_decimal_product() {
    // CE-13/C6: `0.1 * 3` is 3/10, not fl(fl(0.1)*3).
    let k = boundary("region SR\n  select MET <= 0.1 * 3\n");
    assert_eq!(k, Rat::from_ratio(3, 10).unwrap());
    assert_ne!(k, rat(0.1 * 3.0));
}

/// The boundary is *strictly* between the two f64 neighbours of 0.3 — so an
/// event at either neighbour is decided the same way by the atom and by the
/// interpreter (which compares the same exact rationals).
#[test]
fn the_folded_boundary_separates_the_ulp_neighbours() {
    let k = boundary("region SR\n  select MET > 0.1 + 0.2\n");
    let below = rat(0.3_f64.next_down());
    let above = rat(0.3_f64.next_up());
    assert!(below < k, "{below:?} must be below the cut");
    assert!(above > k, "{above:?} must be above the cut");
    // The literal itself sits exactly on it.
    assert_eq!(rat(0.3), k);
}

#[test]
fn linear_arithmetic_over_event_data_flattens() {
    // Each of these rounded under the pre-M3 interpreter and had to intern
    // opaque; each is exact rational arithmetic now, so the flattened atom is
    // the interpreter's own predicate.
    /// (cut source, expected terms as (coefficient, which quantity), relation, boundary)
    type Case = (&'static str, &'static [(f64, &'static str)], Rel, f64);
    let cases: &[Case] = &[
        ("MET + 0.5 <= 1", &[(1.0, "met")], Rel::Le, 0.5),
        ("MET + -0.1 > 0.3", &[(1.0, "met")], Rel::Gt, 0.4),
        ("MET * 0.3 <= 1", &[(0.3, "met")], Rel::Le, 1.0),
        ("MET / 3 > 50", &[(1.0 / 3.0, "met")], Rel::Gt, 50.0),
        ("2*MET - HT < 50", &[(2.0, "met"), (-1.0, "ht")], Rel::Lt, 50.0),
    ];
    for (cut, terms, rel, k) in cases {
        let src = format!("region SR\n  select {cut}\n");
        let (enc, hir) = encode(&src, 0);
        let a = only_atom(&enc.formula);
        assert_eq!(a.rel(), *rel, "{cut}");
        assert!(!has_opaque(&hir), "{cut} must not intern an opaque scalar");
        let want: Vec<(Rat, QuantityId)> = terms
            .iter()
            .map(|(c, which)| {
                let q = if *which == "met" { met_q(&hir) } else { ht_q(&hir) };
                // 1/3 has no decimal form — build it exactly.
                let c = if (*c - 1.0 / 3.0).abs() < 1e-18 {
                    Rat::one().checked_div(&Rat::from_i64(3)).unwrap()
                } else {
                    rat(*c)
                };
                (c, q)
            })
            .collect();
        let mut want = want;
        want.sort_by_key(|(_, q)| q.0);
        assert_eq!(a.terms(), want.as_slice(), "{cut}");
        assert_eq!(a.constant(), &rat(*k), "{cut}");
    }
}

/// C1–C6 were "false PROVEN DISJOINT" only because the interpreter rounded.
/// With both sides exact these region pairs are *genuine* complements, and
/// the encoder is supposed to say so — the differential proof that they
/// really are disjoint (no event in both) lives in
/// `adl-analysis/tests/f64_fold_regressions.rs`.
#[test]
fn the_c1_to_c6_pairs_are_now_honest_complements() {
    let pairs = [
        (
            "region a\n  select MET*0.2*0.3 + HT > 1\nregion b\n  select 0.06*MET + HT <= 1\n",
            "C1",
        ),
        (
            "region a\n  select MET + -0.1 > 0.3\nregion b\n  select MET <= 0.4\n",
            "C2",
        ),
        (
            "region a\n  select MET + 0.5 <= 1\nregion b\n  select MET > 0.5\n",
            "C4",
        ),
    ];
    for (src, label) in pairs {
        let mut hir = build_hir(src);
        let encs = encode_regions(&mut hir);
        let (x, y) = (only_atom(&encs[0].formula), only_atom(&encs[1].formula));
        assert_eq!(x.terms(), y.terms(), "{label}: same flattened terms");
        assert_eq!(x.constant(), y.constant(), "{label}: same boundary");
        assert!(
            matches!(
                (x.rel(), y.rel()),
                (Rel::Gt, Rel::Le) | (Rel::Le, Rel::Gt) | (Rel::Lt, Rel::Ge) | (Rel::Ge, Rel::Lt)
            ),
            "{label}: complementary relations, got {:?} / {:?}",
            x.rel(),
            y.rel()
        );
    }
}

#[test]
fn abs_of_an_exact_inner_expands_after_cancellation() {
    // C3: `MET + HT - HT` is exactly MET in rational arithmetic.
    let (enc, hir) = encode("region SR\n  select abs(MET + HT - HT) < 50\n", 0);
    let met = met_q(&hir);
    assert_eq!(
        enc.formula,
        Formula::And(vec![
            atom1(met, Rel::Lt, 50.0),
            atom1(met, Rel::Gt, -50.0)
        ])
    );
    assert!(!has_opaque(&hir));
}

#[test]
fn size_integer_arithmetic_stays_exact() {
    let (enc, hir) = encode("region SR\n  select size(Jet) + size(Electron) > 2\n", 0);
    let a = only_atom(&enc.formula);
    assert!(a.terms().iter().all(|(c, q)| {
        c.is_integer() && matches!(hir.table.quantity(*q), Quantity::Size(_))
    }));
}

// ---- approximate values keep the conservative treatment -------------------

/// `dR` is f64 in the interpreter. Arithmetic on it rounds, so an exact fold
/// would sit off its boundary: these must stay opaque (or Unknown).
#[test]
fn arithmetic_over_approximate_values_never_flattens() {
    let cuts = [
        "dR(jets[0], jets[1]) + 0.5 <= 1",
        "dR(jets[0], jets[1]) * 0.3 <= 1",
        "dR(jets[0], jets[1]) / 0.3 <= 0.1",
        "abs(dR(jets[0], jets[1]) - 0.1) < 0.05",
    ];
    for cut in cuts {
        let src = format!("{JETS}region SR\n  select {cut}\n");
        let (enc, hir) = encode(&src, 0);
        assert!(
            has_opaque(&hir) || matches!(enc.formula, Formula::Unknown(_)),
            "{cut} must not flatten: {:?}",
            enc.formula
        );
    }
}

/// Power-of-two scaling is IEEE-exact even in f64, so it may still flatten.
#[test]
fn power_of_two_scaling_of_an_approximate_value_still_flattens() {
    let src = format!("{JETS}region SR\n  select 2 * dR(jets[0], jets[1]) > 0.8\n");
    let (enc, hir) = encode(&src, 0);
    let a = only_cut_atom(&enc.formula, &hir);
    assert!(!has_opaque(&hir), "pow2 scale must not go opaque");
    assert_eq!(a.terms().len(), 1);
    assert_eq!(a.terms()[0].0, Rat::from_i64(2));
    // Threshold still converts: `fl(0.8)`, halved by the exact scale later.
    assert_eq!(a.constant(), &Rat::from_f64_exact(0.8).unwrap());
}

/// The comparison edge: an approximate value is compared **in f64**, so the
/// atom's boundary is `fl(k)`, not the decimal `k`. There is no f64 strictly
/// between the two, which is exactly why an event landing on `fl(k)` is the
/// only witness that separates them — and exactly why a probe finds it.
#[test]
fn an_approximate_comparison_cuts_at_the_f64_threshold() {
    let src = format!("{JETS}region SR\n  select dR(jets[0], jets[1]) < 0.3\n");
    let k = boundary(&src);
    assert_eq!(k, Rat::from_f64_exact(0.3).unwrap());
    assert_ne!(k, Rat::from_ratio(3, 10).unwrap());
    assert!(k < Rat::from_ratio(3, 10).unwrap(), "fl(0.3) < 3/10");

    // An exact quantity keeps the decimal boundary — the two regimes must not
    // bleed into each other.
    assert_eq!(
        boundary("region SR\n  select MET < 0.3\n"),
        Rat::from_ratio(3, 10).unwrap()
    );
}

#[test]
fn approximate_band_bounds_also_move_to_the_f64_threshold() {
    let src = format!("{JETS}region SR\n  select dR(jets[0], jets[1]) [] 0.3 0.7\n");
    let (enc, hir) = encode(&src, 0);
    let atoms = cut_atoms(&enc.formula, &hir);
    assert_eq!(atoms.len(), 2, "a band is its two bounds: {atoms:?}");
    let consts: Vec<Rat> = atoms.iter().map(|a| a.constant().clone()).collect();
    assert!(consts.contains(&Rat::from_f64_exact(0.3).unwrap()));
    assert!(consts.contains(&Rat::from_f64_exact(0.7).unwrap()));
}

/// `min`/`max` is rewritten argument by argument, so a mixed one hides the
/// same rounding: the interpreter takes the minimum *after* folding the
/// exact argument to f64, and a per-argument atom over its exact value
/// cannot say that. A homogeneous min/max still rewrites.
#[test]
fn a_mixed_scalar_min_is_unknown_but_a_homogeneous_one_rewrites() {
    let mixed =
        format!("{JETS}region SR\n  select min(dR(jets[0], jets[1]), pT(jets[0])) < 0.4\n");
    let (enc, _) = encode(&mixed, 0);
    assert!(
        matches!(enc.formula, Formula::Unknown(_)),
        "mixed min must not rewrite: {:?}",
        enc.formula
    );

    let approx = format!(
        "{JETS}region SR\n  select min(dR(jets[0], jets[1]), dR(jets[0], jets[2])) < 0.4\n"
    );
    let (enc, hir) = encode(&approx, 0);
    let atoms = cut_atoms(&enc.formula, &hir);
    assert_eq!(atoms.len(), 2, "min over two approximate args: {atoms:?}");
    for a in &atoms {
        assert_eq!(a.constant(), &Rat::from_f64_exact(0.4).unwrap());
    }

    let exact = "region SR\n  select min(MET, HT) < 0.4\n";
    let (enc, hir) = encode(exact, 0);
    for a in cut_atoms(&enc.formula, &hir) {
        assert_eq!(a.constant(), &Rat::from_ratio(2, 5).unwrap());
    }
}

/// An exact quantity compared against an approximate one is rounded at the
/// edge by the interpreter, and a linear atom over the reals cannot express
/// that rounding — so the comparison must not become an atom at all.
#[test]
fn a_mixed_exact_approximate_comparison_is_unknown() {
    let src = format!("{JETS}region SR\n  select dR(jets[0], jets[1]) > pT(jets[0])\n");
    let (enc, _) = encode(&src, 0);
    let Formula::Unknown(id) = enc.formula else {
        panic!("expected Unknown, got {:?}", enc.formula);
    };
    assert!(
        enc.diags.get(id).unwrap().reason.contains("approximate"),
        "reason was {:?}",
        enc.diags.get(id).unwrap().reason
    );
}

// ---- unchanged positive controls ------------------------------------------

#[test]
fn bare_quantity_vs_const_stays_exact() {
    let (enc, hir) = encode("region SR\n  select MET > 200\n", 0);
    assert_eq!(enc.formula, atom1(met_q(&hir), Rel::Gt, 200.0));
}

#[test]
fn abs_bare_quantity_stays_exact() {
    let (enc, hir) = encode("region SR\n  select abs(MET) < 50\n", 0);
    let met = met_q(&hir);
    assert_eq!(
        enc.formula,
        Formula::And(vec![atom1(met, Rel::Lt, 50.0), atom1(met, Rel::Gt, -50.0)])
    );
}

#[test]
fn node_node_and_pow2_scale_stay_exact() {
    let (enc, hir) = encode("region SR\n  select MET > HT\n", 0);
    let (met, ht) = (met_q(&hir), ht_q(&hir));
    assert_eq!(
        enc.formula,
        Formula::Atom(LinAtom::new(
            [(Rat::one(), met), (Rat::from_i64(-1), ht)],
            Rel::Gt,
            Rat::zero(),
        ))
    );
    let (enc, hir) = encode("region SR\n  select MET / 2 > 50\n", 0);
    assert_eq!(
        enc.formula,
        Formula::Atom(LinAtom::new(
            [(rat(0.5), met_q(&hir))],
            Rel::Gt,
            Rat::from_i64(50),
        ))
    );
    let (enc, hir) = encode("region SR\n  select 0.25 * MET <= 10\n", 0);
    assert_eq!(
        enc.formula,
        Formula::Atom(LinAtom::new(
            [(rat(0.25), met_q(&hir))],
            Rel::Le,
            Rat::from_i64(10),
        ))
    );
}

#[test]
fn opaque_keys_are_deterministic() {
    let src = format!("{JETS}region SR\n  select dR(jets[0], jets[1]) + 0.5 <= 1\n");
    let (e1, h1) = encode(&src, 0);
    let (e2, h2) = encode(&src, 0);
    assert_eq!(e1.formula, e2.formula);
    let key = |h: &Hir| {
        let q = find_q(h, |q| {
            matches!(q, Quantity::ExternalFn { name, .. }
                if h.symbols.display(*name) == "opaque.scalar")
        });
        match h.table.quantity(q) {
            Quantity::ExternalFn { args, .. } => args.clone(),
            _ => panic!("expected opaque"),
        }
    };
    assert_eq!(key(&h1), key(&h2));
}

/// Structurally identical approximate cuts must share one opaque quantity, so
/// complementary thresholds on the same expression still decide.
#[test]
fn same_shape_approximate_cuts_share_one_opaque() {
    let src = format!(
        "{JETS}region a\n  select dR(jets[0], jets[1]) + 0.5 <= 1\n\
         region b\n  select dR(jets[0], jets[1]) + 0.5 > 1\n"
    );
    let mut hir = build_hir(&src);
    let encs = encode_regions(&mut hir);
    let q = find_q(&hir, |q| {
        matches!(q, Quantity::ExternalFn { name, .. }
            if hir.symbols.display(*name) == "opaque.scalar")
    });
    let a = only_cut_atom(&encs[0].formula, &hir);
    let b = only_cut_atom(&encs[1].formula, &hir);
    assert_eq!(a.terms(), &[(Rat::one(), q)]);
    assert_eq!(b.terms(), &[(Rat::one(), q)]);
    assert_eq!((a.rel(), b.rel()), (Rel::Le, Rel::Gt));
    assert_eq!(a.constant(), b.constant());
}

// ---- R2: static-slice reducer width cap -----------------------------------

#[test]
fn static_slice_wider_than_cap_is_unknown() {
    let n = MAX_STATIC_SLICE_REDUCE + 1;
    let src = format!("region SR\n  select any(Jet[0:{n}].pt > 30)\n");
    let (enc, _) = encode(&src, 0);
    let Formula::Unknown(id) = enc.formula else {
        panic!("expected Unknown for oversized slice, got {:?}", enc.formula);
    };
    let why = enc.diags.get(id).unwrap();
    assert!(
        why.reason.contains("exceeds reducer expansion cap"),
        "reason was {:?}",
        why.reason
    );
}

#[test]
fn static_slice_at_cap_still_encodes() {
    let n = MAX_STATIC_SLICE_REDUCE;
    let src = format!("region SR\n  select any(Jet[0:{n}].pt > 30)\n");
    let (enc, _) = encode(&src, 0);
    assert!(
        !matches!(enc.formula, Formula::Unknown(_)),
        "width={n} must still expand, got {:?}",
        enc.formula
    );
}
