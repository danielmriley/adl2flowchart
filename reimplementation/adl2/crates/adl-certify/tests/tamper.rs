//! Tamper tests: corrupting any part of a valid certificate must make
//! [`Certificate::replay`] return `false`. The kernel fails closed.
//! Bundle metadata (verdict / scope note) is authenticated the same way.

use adl_certify::bundle::{BUNDLE_VERDICT, SCOPE_NOTE};
use adl_certify::{
    Budget, CertNode, Certificate, CertifyResult, CombineBundle, QRat, certify_unsat,
};
use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};

fn q(n: u32) -> QuantityId {
    QuantityId(n)
}
fn a(qi: u32, rel: Rel, k: i64) -> QFormula {
    QFormula::Atom(LinAtom::single(q(qi), rel, Rat::from_i64(k)))
}

fn certified(forms: &[QFormula]) -> Certificate {
    match certify_unsat(forms, &Budget::default()) {
        CertifyResult::Certified(c) => c,
        other => panic!("expected Certified, got {:?}", other.reason()),
    }
}

#[test]
fn zeroing_a_multiplier_breaks_replay() {
    let forms = [a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let cert = certified(&forms);
    assert!(cert.replay(&forms));

    let CertNode::Farkas { multipliers } = &cert.root else {
        panic!("expected a Farkas leaf, got {:?}", cert.root);
    };
    assert_eq!(multipliers.len(), 2);

    // Zero out the first multiplier: the linear parts no longer cancel.
    let tampered = Certificate::new(CertNode::Farkas {
        multipliers: vec![QRat(Rat::zero()), multipliers[1].clone()],
    });
    assert!(!tampered.replay(&forms), "zeroed multiplier still replayed");
}

#[test]
fn negating_a_multiplier_breaks_replay() {
    let forms = [a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let cert = certified(&forms);
    let CertNode::Farkas { multipliers } = &cert.root else {
        panic!("expected a Farkas leaf");
    };
    let neg0 = QRat(-&multipliers[0].0);
    let tampered = Certificate::new(CertNode::Farkas {
        multipliers: vec![neg0, multipliers[1].clone()],
    });
    assert!(!tampered.replay(&forms), "negative multiplier still replayed");
}

#[test]
fn scaling_one_multiplier_breaks_cancellation() {
    let forms = [a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let cert = certified(&forms);
    let CertNode::Farkas { multipliers } = &cert.root else {
        panic!("expected a Farkas leaf");
    };
    let doubled = QRat(&multipliers[0].0 + &multipliers[0].0);
    let tampered = Certificate::new(CertNode::Farkas {
        multipliers: vec![doubled, multipliers[1].clone()],
    });
    assert!(!tampered.replay(&forms), "unbalanced multipliers still replayed");
}

#[test]
fn dropping_a_split_branch_breaks_replay() {
    // (x < 0 OR x > 10) AND x == 5
    let or = QFormula::Or(vec![a(0, Rel::Lt, 0), a(0, Rel::Gt, 10)]);
    let forms = [or, a(0, Rel::Eq, 5)];
    let cert = certified(&forms);

    let CertNode::Split { branches } = &cert.root else {
        panic!("expected a Split, got {:?}", cert.root);
    };
    assert_eq!(branches.len(), 2);

    // Drop one branch: the split no longer covers every disjunct.
    let tampered = Certificate::new(CertNode::Split {
        branches: vec![branches[0].clone()],
    });
    assert!(!tampered.replay(&forms), "under-covered split still replayed");
}

#[test]
fn swapping_a_branch_for_contradiction_breaks_replay() {
    let or = QFormula::Or(vec![a(0, Rel::Lt, 0), a(0, Rel::Gt, 10)]);
    let forms = [or, a(0, Rel::Eq, 5)];
    let cert = certified(&forms);
    let CertNode::Split { branches } = &cert.root else {
        panic!("expected a Split");
    };
    // Replace a genuine Farkas branch with a bogus Contradiction claim.
    let tampered = Certificate::new(CertNode::Split {
        branches: vec![CertNode::Contradiction, branches[1].clone()],
    });
    assert!(!tampered.replay(&forms), "bogus contradiction branch still replayed");
}

#[test]
fn wrong_node_shape_breaks_replay() {
    // A leaf certified by Farkas cannot be replayed as a Split, or vice versa.
    let forms = [a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let as_split = Certificate::new(CertNode::Split { branches: vec![] });
    assert!(!as_split.replay(&forms));

    let or = QFormula::Or(vec![a(0, Rel::Lt, 0), a(0, Rel::Gt, 10)]);
    let forms2 = [or, a(0, Rel::Eq, 5)];
    let as_farkas = Certificate::new(CertNode::Farkas { multipliers: vec![] });
    assert!(!as_farkas.replay(&forms2));
}

#[test]
fn genuine_certificate_fails_against_a_different_system() {
    // A valid certificate is a proof about ONE formula set. Replaying it
    // against a satisfiable system — even one with the same shape and the
    // same atom count — must fail.
    let unsat = [a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let cert = certified(&unsat);
    assert!(cert.replay(&unsat));

    let sat_same_shape = [a(0, Rel::Gt, 1), a(0, Rel::Lt, 2)]; // 1 < x < 2
    assert!(
        !cert.replay(&sat_same_shape),
        "certificate replayed against a satisfiable look-alike system"
    );

    let sat_other_quantity = [a(1, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    assert!(
        !cert.replay(&sat_other_quantity),
        "certificate replayed against a different-quantity system"
    );
}

#[test]
fn contradiction_claim_without_false_breaks_replay() {
    // Claiming Contradiction on a set with no `false` conjunct must fail.
    let forms = [a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let bogus = Certificate::new(CertNode::Contradiction);
    assert!(!bogus.replay(&forms));
}

#[test]
fn permuting_multipliers_on_nonsymmetric_system_breaks_replay() {
    // Coefficient asymmetry forces unequal Farkas multipliers (λ = [2, 1]
    // for x>1 ∧ 2x<1). Swapping them leaves uncancelled linear parts.
    let forms = [
        a(0, Rel::Gt, 1), // x > 1
        QFormula::Atom(LinAtom::new(
            vec![(Rat::from_i64(2), q(0))],
            Rel::Lt,
            Rat::from_i64(1),
        )), // 2x < 1
    ];
    let cert = certified(&forms);
    assert!(cert.replay(&forms));
    let CertNode::Farkas { multipliers } = &cert.root else {
        panic!("expected a Farkas leaf, got {:?}", cert.root);
    };
    assert_eq!(multipliers.len(), 2);
    assert_ne!(
        multipliers[0], multipliers[1],
        "multipliers are equal — permutation would be a no-op: {multipliers:?}"
    );

    let tampered = Certificate::new(CertNode::Farkas {
        multipliers: vec![multipliers[1].clone(), multipliers[0].clone()],
    });
    assert!(
        !tampered.replay(&forms),
        "permuted multipliers on a non-symmetric system still replayed"
    );
}

#[test]
fn swapping_split_branches_breaks_replay() {
    // Branch certificates are ordered to match disjuncts; a swap pairs each
    // sub-proof with the wrong child conjunction.
    let or = QFormula::Or(vec![a(0, Rel::Lt, 0), a(0, Rel::Gt, 10)]);
    let forms = [or, a(0, Rel::Eq, 5)];
    let cert = certified(&forms);
    let CertNode::Split { branches } = &cert.root else {
        panic!("expected a Split, got {:?}", cert.root);
    };
    assert_eq!(branches.len(), 2);
    assert_ne!(
        branches[0], branches[1],
        "branches are identical — a swap would be a no-op"
    );

    let tampered = Certificate::new(CertNode::Split {
        branches: vec![branches[1].clone(), branches[0].clone()],
    });
    assert!(
        !tampered.replay(&forms),
        "swapped Split branches still replayed"
    );
}

fn certified_bundle() -> CombineBundle {
    let forms = vec![a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let cert = certified(&forms);
    CombineBundle::new(
        "A".into(),
        "B".into(),
        vec![("a".into(), forms[0].clone()), ("b".into(), forms[1].clone())],
        cert,
    )
}

#[test]
fn tampered_verdict_breaks_bundle_replay() {
    let mut bundle = certified_bundle();
    assert!(bundle.replay());
    assert_eq!(bundle.verdict, BUNDLE_VERDICT);
    bundle.verdict = "PROVEN OVERLAPPING".into();
    assert!(
        !bundle.replay(),
        "forged verdict string still replayed"
    );
}

#[test]
fn tampered_scope_note_breaks_bundle_replay() {
    let mut bundle = certified_bundle();
    assert!(bundle.replay());
    assert_eq!(bundle.note, SCOPE_NOTE);
    bundle.note = "FORGED: this bundle proves regions are identical.".into();
    assert!(
        !bundle.replay(),
        "forged scope note still replayed"
    );
}
