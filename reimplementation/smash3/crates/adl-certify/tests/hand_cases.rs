//! Hand-picked cases that pin the certifier's behaviour on the shapes the task
//! calls out explicitly, plus serde round-tripping.

use adl_certify::{Budget, CertifyResult, Certificate, certify_unsat};
use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};

fn q(n: u32) -> QuantityId {
    QuantityId(n)
}
fn ri(n: i64) -> Rat {
    Rat::from_i64(n)
}
/// `1·q  rel  k`
fn a(qi: u32, rel: Rel, k: i64) -> QFormula {
    QFormula::Atom(LinAtom::single(q(qi), rel, ri(k)))
}
/// `coeff·q  rel  k`
fn ac(coeff: i64, qi: u32, rel: Rel, k: i64) -> QFormula {
    QFormula::Atom(LinAtom::new([(ri(coeff), q(qi))], rel, ri(k)))
}

fn certify(forms: &[QFormula]) -> CertifyResult {
    certify_unsat(forms, &Budget::default())
}

/// A certified result must (a) really carry a certificate and (b) replay true,
/// and (c) still replay true after a serde JSON round-trip.
fn assert_certified(forms: &[QFormula]) -> Certificate {
    let r = certify(forms);
    let CertifyResult::Certified(cert) = r else {
        panic!("expected Certified, got {:?}", r.reason());
    };
    assert!(cert.replay(forms), "certificate did not replay");
    let js = serde_json::to_string(&cert).unwrap();
    let back: Certificate = serde_json::from_str(&js).unwrap();
    assert!(back.replay(forms), "certificate did not replay after JSON round-trip");
    cert
}

fn assert_uncertified(forms: &[QFormula]) {
    assert!(
        !certify(forms).is_certified(),
        "expected Uncertified but the set was certified"
    );
}

#[test]
fn x_gt_2_and_x_lt_1_is_certified() {
    assert_certified(&[a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)]);
}

#[test]
fn x_ge_1_and_x_le_1_is_satisfiable() {
    // x = 1 satisfies both — not UNSAT.
    assert_uncertified(&[a(0, Rel::Ge, 1), a(0, Rel::Le, 1)]);
}

#[test]
fn x_gt_1_and_x_le_1_is_certified() {
    assert_certified(&[a(0, Rel::Gt, 1), a(0, Rel::Le, 1)]);
}

#[test]
fn strict_boundary_x_ge_5_and_x_lt_5_is_certified() {
    assert_certified(&[a(0, Rel::Ge, 5), a(0, Rel::Lt, 5)]);
}

#[test]
fn eq_ne_interplay_is_certified_via_split() {
    // x == 3  AND  x != 3
    assert_certified(&[a(0, Rel::Eq, 3), a(0, Rel::Ne, 3)]);
}

#[test]
fn or_split_is_certified() {
    // (x < 0 OR x > 10)  AND  x == 5
    let or = QFormula::Or(vec![a(0, Rel::Lt, 0), a(0, Rel::Gt, 10)]);
    assert_certified(&[or, a(0, Rel::Eq, 5)]);
}

#[test]
fn two_x_eq_1_is_real_satisfiable_uncertified() {
    // 2x == 1 is integer-infeasible but real-feasible (x = 1/2); under the real
    // relaxation we (correctly, conservatively) do not certify.
    assert_uncertified(&[ac(2, 0, Rel::Eq, 1)]);
}

#[test]
fn false_literal_is_certified_contradiction() {
    assert_certified(&[QFormula::False]);
    assert_certified(&[a(0, Rel::Gt, 0), QFormula::False]);
}

#[test]
fn multivariable_transitivity_is_certified() {
    // x < y, y < z, z < x  (as x - y < 0 etc.) is a cyclic strict contradiction.
    let xy = QFormula::Atom(LinAtom::new(
        [(ri(1), q(0)), (ri(-1), q(1))],
        Rel::Lt,
        ri(0),
    ));
    let yz = QFormula::Atom(LinAtom::new(
        [(ri(1), q(1)), (ri(-1), q(2))],
        Rel::Lt,
        ri(0),
    ));
    let zx = QFormula::Atom(LinAtom::new(
        [(ri(1), q(2)), (ri(-1), q(0))],
        Rel::Lt,
        ri(0),
    ));
    assert_certified(&[xy, yz, zx]);
}

#[test]
fn nested_and_or_is_certified() {
    // x > 0 AND (x < -1 OR (x < 0 AND y < y+... )) — force And inside Or.
    let inner_and = QFormula::And(vec![a(0, Rel::Lt, 0), a(1, Rel::Lt, -5)]);
    let or = QFormula::Or(vec![a(0, Rel::Lt, -1), inner_and]);
    // Conjoined with x > 0 and y > 0: both Or branches force x < 0 vs x > 0.
    assert_certified(&[a(0, Rel::Gt, 0), a(1, Rel::Gt, 0), or]);
}

#[test]
fn fractional_constants_are_exact() {
    // 10x > 3 and 10x < 3  (i.e. x > 0.3 and x < 0.3) — exact-rational boundary.
    assert_certified(&[ac(10, 0, Rel::Gt, 3), ac(10, 0, Rel::Lt, 3)]);
}
