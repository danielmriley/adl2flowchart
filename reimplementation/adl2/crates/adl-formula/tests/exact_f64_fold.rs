//! Exact-f64 foldability criterion (audit 2026-07-28 C1–C6 / R1–R2).
//!
//! A quantity-bearing operand flattens to a `LinAtom` only when every
//! interpreter arithmetic step is provably IEEE-exact. Otherwise it interns
//! as a structure-keyed opaque scalar — never a silently wrong complementary
//! pair of exact atoms.

use adl_formula::{
    EncodedRegion, Formula, LinAtom, MAX_STATIC_SLICE_REDUCE, Rel, encode_region, encode_regions,
};
use adl_sema::{
    ExtDecls, Hir, Quantity, QuantityId, Rat, ScalarSource, analyze_str,
};
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
    assert_eq!(hits.len(), 1, "expected exactly one matching quantity, got {hits:?}");
    QuantityId(u32::try_from(hits[0]).unwrap())
}

fn met_q(hir: &Hir) -> QuantityId {
    find_q(hir, |q| {
        matches!(q, Quantity::EventScalar(ScalarSource::MetProp(_)))
    })
}

fn opaque_q(hir: &Hir) -> QuantityId {
    find_q(hir, |q| {
        matches!(
            q,
            Quantity::ExternalFn { name, .. }
                if hir.symbols.display(*name) == "opaque.scalar"
        )
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

/// True when two closed/open cuts on the same terms+constant are disjoint
/// (`≤` vs `>`, `<` vs `≥`). `≤` vs `≥` at the same bound *overlap* at the
/// endpoint — that is sound (and is exactly what C6's f64-emulation fix
/// produces for `MET ≤ fl(0.1*3)` vs `MET ≥ 0.300…04`).
fn is_disjoint_complement(a: Rel, b: Rel) -> bool {
    matches!(
        (a, b),
        (Rel::Lt, Rel::Ge)
            | (Rel::Ge, Rel::Lt)
            | (Rel::Le, Rel::Gt)
            | (Rel::Gt, Rel::Le)
            | (Rel::Eq, Rel::Ne)
            | (Rel::Ne, Rel::Eq)
    )
}

/// Two encodings must not be disjoint-complementary exact atoms over the
/// *same* flattened quantities (the false-PROVEN shape).
fn not_complementary_exact_pair(a: &Formula, b: &Formula) {
    let mut aa = Vec::new();
    let mut bb = Vec::new();
    collect_atoms(a, &mut aa);
    collect_atoms(b, &mut bb);
    if aa.len() == 1 && bb.len() == 1 {
        let (x, y) = (&aa[0], &bb[0]);
        assert!(
            !(x.terms() == y.terms()
                && x.constant() == y.constant()
                && is_disjoint_complement(x.rel(), y.rel())),
            "disjoint-complementary exact atoms over identical terms — unsound fold:\n  {x:?}\n  {y:?}"
        );
    }
}

// ---- C1–C6: bug shapes must not produce complementary exact atoms ---------

#[test]
fn c1_multiplicative_regrouping_goes_opaque() {
    let src = "\
region a\n  select MET*0.2*0.3 + HT > 1\n\
region b\n  select 0.06*MET + HT <= 1\n";
    let mut hir = build_hir(src);
    let encs = encode_regions(&mut hir);
    not_complementary_exact_pair(&encs[0].formula, &encs[1].formula);
    // Both sides involve non-exact-f64 arithmetic → opaque scalars.
    assert!(
        hir.table.quantities().iter().any(|q| matches!(
            q,
            Quantity::ExternalFn { name, .. }
                if hir.symbols.display(*name) == "opaque.scalar"
        )),
        "C1 operands must intern opaque"
    );
}

#[test]
fn c2_neg_const_add_goes_opaque() {
    let src = "\
region a\n  select MET + -0.1 > 0.3\n\
region b\n  select MET <= 0.4\n";
    let mut hir = build_hir(src);
    let encs = encode_regions(&mut hir);
    not_complementary_exact_pair(&encs[0].formula, &encs[1].formula);
    // Region a must not be the bare atom MET > 0.4.
    let met = met_q(&hir);
    assert_ne!(encs[0].formula, atom1(met, Rel::Gt, 0.4));
}

#[test]
fn c3_abs_cancellation_inner_goes_opaque() {
    let src = "\
region a\n  select abs(MET + HT - HT) < 50\n\
region b\n  select abs(MET) >= 50\n";
    let mut hir = build_hir(src);
    let encs = encode_regions(&mut hir);
    not_complementary_exact_pair(&encs[0].formula, &encs[1].formula);
    // a is opaque over the whole abs(...); b expands abs(MET) exactly.
    let o = opaque_q(&hir);
    assert_eq!(encs[0].formula, atom1(o, Rel::Lt, 50.0));
}

#[test]
fn c4_dyadic_add_goes_opaque() {
    let src = "\
region a\n  select MET + 0.5 <= 1\n\
region b\n  select MET > 0.5\n";
    let mut hir = build_hir(src);
    let encs = encode_regions(&mut hir);
    not_complementary_exact_pair(&encs[0].formula, &encs[1].formula);
    let met = met_q(&hir);
    assert_ne!(encs[0].formula, atom1(met, Rel::Le, 0.5));
}

#[test]
fn c5_single_non_pow2_mul_goes_opaque() {
    let src = "\
region a\n  select MET*0.3 <= 1\n\
region b\n  select MET >= 3.3333333333333335\n";
    let mut hir = build_hir(src);
    let encs = encode_regions(&mut hir);
    not_complementary_exact_pair(&encs[0].formula, &encs[1].formula);
    let o = opaque_q(&hir);
    assert_eq!(encs[0].formula, atom1(o, Rel::Le, 1.0));
}

#[test]
fn c6_const_mul_emulates_f64() {
    // `0.1 * 3` must fold as fl(fl(0.1)*3) = 0.30000000000000004, not 3/10.
    let (enc, hir) = encode("region SR\n  select MET <= 0.1 * 3\n", 0);
    let met = met_q(&hir);
    let expected = fl_const_mul_0_1_times_3();
    assert_eq!(enc.formula, atom1(met, Rel::Le, expected));
    // Paired with the bare literal at the f64 boundary — not complementary
    // over 0.3 exactly.
    let src = "\
region a\n  select MET <= 0.1 * 3\n\
region b\n  select MET >= 0.30000000000000004\n";
    let mut hir = build_hir(src);
    let encs = encode_regions(&mut hir);
    not_complementary_exact_pair(&encs[0].formula, &encs[1].formula);
}

fn fl_const_mul_0_1_times_3() -> f64 {
    0.1 * 3.0
}

// ---- Positive cases: still exact ------------------------------------------

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
    let (met, ht) = (
        met_q(&hir),
        find_q(&hir, |q| {
            matches!(q, Quantity::EventScalar(ScalarSource::EventVar(_)))
        }),
    );
    assert_eq!(
        enc.formula,
        Formula::Atom(LinAtom::new(
            [
                (Rat::one(), met),
                (Rat::from_i64(-1), ht),
            ],
            Rel::Gt,
            Rat::zero(),
        ))
    );
    let (enc, hir) = encode("region SR\n  select MET / 2 > 50\n", 0);
    assert_eq!(
        enc.formula,
        Formula::Atom(LinAtom::new(
            [(Rat::from_decimal_f64(0.5).unwrap(), met_q(&hir))],
            Rel::Gt,
            Rat::from_i64(50),
        ))
    );
    let (enc, hir) = encode("region SR\n  select 0.25 * MET <= 10\n", 0);
    assert_eq!(
        enc.formula,
        Formula::Atom(LinAtom::new(
            [(Rat::from_decimal_f64(0.25).unwrap(), met_q(&hir))],
            Rel::Le,
            Rat::from_i64(10),
        ))
    );
}

#[test]
fn size_integer_arithmetic_stays_exact() {
    let (enc, hir) = encode(
        "region SR\n  select size(Jet) + size(Electron) > 2\n",
        0,
    );
    let mut atoms = Vec::new();
    collect_atoms(&enc.formula, &mut atoms);
    assert_eq!(atoms.len(), 1, "size±size must stay one exact atom");
    assert!(atoms[0].terms().iter().all(|(c, q)| {
        c.is_integer() && matches!(hir.table.quantity(*q), Quantity::Size(_))
    }));
}

#[test]
fn const_add_subtree_matches_f64_emulation() {
    // `0.1 + 0.2` is opaque under the old dyadic guard; f64 emulation makes
    // it decidable as the interpreter sees it.
    let (enc, hir) = encode("region SR\n  select MET <= 0.1 + 0.2\n", 0);
    let met = met_q(&hir);
    let v = 0.1 + 0.2;
    assert_eq!(enc.formula, atom1(met, Rel::Le, v));
}

// ---- Completeness: same-tree complementary cuts share an opaque -----------

#[test]
fn same_shape_complementary_cuts_share_opaque_atom() {
    let src = "\
region a\n  select MET + 0.5 <= 1\n\
region b\n  select MET + 0.5 > 1\n";
    let mut hir = build_hir(src);
    let encs = encode_regions(&mut hir);
    let q = opaque_q(&hir);
    assert_eq!(encs[0].formula, atom1(q, Rel::Le, 1.0));
    assert_eq!(encs[1].formula, atom1(q, Rel::Gt, 1.0));
}

#[test]
fn opaque_keys_are_deterministic() {
    let src = "region SR\n  select MET + 0.5 <= 1\n";
    let (e1, h1) = encode(src, 0);
    let (e2, h2) = encode(src, 0);
    assert_eq!(e1.formula, e2.formula);
    // Same body key → same display identity for the opaque.
    let k1 = match h1.table.quantity(opaque_q(&h1)) {
        Quantity::ExternalFn { args, .. } => args.clone(),
        _ => panic!("expected opaque"),
    };
    let k2 = match h2.table.quantity(opaque_q(&h2)) {
        Quantity::ExternalFn { args, .. } => args.clone(),
        _ => panic!("expected opaque"),
    };
    assert_eq!(k1, k2);
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
