//! Tamper tests: corrupting any part of a valid certificate must make
//! [`Certificate::replay`] return `false`. The kernel fails closed.
//! Bundle metadata (verdict / scope note) is authenticated the same way.

use adl_certify::bundle::{
    AssertSource, BUNDLE_VERDICT, BundleAssert, BundleFormula, BundleInput, BundleParts,
    DerivedFact, Derivation, SCOPE_NOTE,
};
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

// ---------------------------------------------------------------------------
// Bundle tamper matrix (schema smash2-combine/2)
//
// A bundle carries two kinds of content and the tests below pin the boundary
// between them, because that boundary IS the claim:
//
//   * load-bearing — the pinned constants, every formula, every certificate,
//     the quantity dictionary's COVERAGE, and the assert -> derived-fact link.
//     Tampering any of these must fail replay.
//   * descriptive — quantity LABELS, assert sources, region names, producer,
//     inputs. Tampering these must NOT fail replay, and the tests assert that
//     deliberately.
//
// Why labels are not pinned: there is nothing inside the bundle to check a
// label against. Pinning them would authenticate nothing (the bundle is
// unsigned — an editor who can change a label can change whatever it was
// pinned to) while implying the checker vouches for the prose. What replay CAN
// do is guarantee the math is real and that the description leaves nothing out
// — hence the coverage check — and then say plainly what it is not speaking
// for. `smash2-recheck` prints that caveat with every result.
// ---------------------------------------------------------------------------

fn cut_src(name: &str) -> AssertSource {
    AssertSource::Cut {
        region: "SR".into(),
        line: 7,
        text: format!("select {name}"),
        whole: true,
    }
}

fn certified_bundle() -> CombineBundle {
    let forms = vec![a(0, Rel::Gt, 2), a(0, Rel::Lt, 1)];
    let cert = certified(&forms);
    CombineBundle::new(
        BundleParts {
            region_a: "A".into(),
            region_b: "B".into(),
            asserts: vec![
                BundleAssert::new("a".into(), &forms[0], cut_src("a")),
                BundleAssert::new("b".into(), &forms[1], cut_src("b")),
            ],
            derived_facts: Vec::new(),
            certificate: cert,
        },
        |q| format!("size(c{q})"),
    )
}

/// A bundle whose refutation LEANS on a reconciliation fact: `size(A) <=
/// size(B)` (q0 - q1 <= 0) with `size(A) >= 3` and `size(B) <= 2`. The fact
/// carries its own derivation — the element-predicate subset refutation
/// `pt > 30 AND pt <= 25` over the shared generic element.
fn chained_bundle() -> CombineBundle {
    let fact = QFormula::Atom(LinAtom::new(
        vec![(Rat::from_i64(1), q(0)), (Rat::from_i64(-1), q(1))],
        Rel::Le,
        Rat::zero(),
    ));
    let main = vec![fact.clone(), a(0, Rel::Ge, 3), a(1, Rel::Le, 2)];
    let cert = certified(&main);

    let premises = vec![a(2, Rel::Gt, 30), a(2, Rel::Le, 25)];
    let pcert = certified(&premises);
    let derivation = Derivation::new(
        "every element of A passes B's cuts".into(),
        vec![
            BundleAssert::new(
                "QSUB0".into(),
                &premises[0],
                AssertSource::Query {
                    role: "over-projection of the A element predicate".into(),
                },
            ),
            BundleAssert::new(
                "QSUBNEG".into(),
                &premises[1],
                AssertSource::Query {
                    role: "negated under-projection of the B element predicate".into(),
                },
            ),
        ],
        pcert,
    );

    CombineBundle::new(
        BundleParts {
            region_a: "A::SR".into(),
            region_b: "B::CR".into(),
            asserts: vec![
                BundleAssert::new(
                    "XR0".into(),
                    &fact,
                    AssertSource::Derived {
                        fact: "XR0".into(),
                    },
                ),
                BundleAssert::new("sa".into(), &main[1], cut_src("size(A) >= 3")),
                BundleAssert::new("sb".into(), &main[2], cut_src("size(B) <= 2")),
            ],
            derived_facts: vec![DerivedFact::new(
                "XR0".into(),
                "XSUB".into(),
                "size(A) <= size(B)".into(),
                &fact,
                vec![derivation],
            )],
            certificate: cert,
        },
        |q| format!("size(c{q})"),
    )
}

#[test]
fn tampered_verdict_breaks_bundle_replay() {
    let mut bundle = certified_bundle();
    assert!(bundle.replay());
    assert_eq!(bundle.verdict, BUNDLE_VERDICT);
    bundle.verdict = "PROVEN OVERLAPPING".into();
    assert!(!bundle.replay(), "forged verdict string still replayed");
}

#[test]
fn tampered_scope_note_breaks_bundle_replay() {
    let mut bundle = certified_bundle();
    assert!(bundle.replay());
    assert_eq!(bundle.note, SCOPE_NOTE);
    bundle.note = "FORGED: this bundle proves regions are identical.".into();
    assert!(!bundle.replay(), "forged scope note still replayed");
}

#[test]
fn tampered_schema_breaks_bundle_replay() {
    let mut bundle = certified_bundle();
    bundle.schema = "smash2-combine/1".into();
    assert!(!bundle.replay(), "a superseded schema tag still replayed");
}

#[test]
fn quantity_dictionary_coverage_is_checked_but_labels_are_not() {
    let mut bundle = certified_bundle();
    assert!(bundle.replay());

    // Coverage IS load-bearing: a bundle may not mention a quantity it does
    // not name.
    let mut gutted = bundle.clone();
    gutted.quantities.remove(&0);
    assert!(!gutted.replay(), "an unnamed quantity was accepted");

    // The label text is NOT: nothing in the bundle could check it, and
    // pretending otherwise would overstate what a passing replay means.
    bundle
        .quantities
        .insert(0, "size(something else entirely)".into());
    assert!(
        bundle.replay(),
        "labels must stay descriptive — see the module header"
    );
}

#[test]
fn descriptive_fields_do_not_affect_replay() {
    let mut bundle = certified_bundle();
    assert!(bundle.replay());

    bundle.region_a = "not the region at all".into();
    bundle.region_b = "nor this one".into();
    bundle.producer.tool = "definitely-not-smash2".into();
    bundle.producer.version = "99.99.99".into();
    bundle.producer.schema_history = vec!["invented".into()];
    bundle.inputs = vec![BundleInput {
        name: "someone-elses.adl".into(),
        sha256: "00".repeat(32),
    }];
    bundle.asserts[0].source = AssertSource::Cut {
        region: "a region that does not exist".into(),
        line: 99999,
        text: "select something never written".into(),
        whole: false,
    };
    bundle.asserts[0].name = "renamed".into();
    assert!(
        bundle.replay(),
        "descriptive fields must not be able to break a valid proof — the \
         checker states it does not vouch for them instead of pretending to"
    );
}

#[test]
fn a_derived_fact_backs_its_assert() {
    let bundle = chained_bundle();
    assert!(bundle.replay(), "the chained bundle must replay as built");
    assert_eq!(bundle.derived_facts.len(), 1);
}

#[test]
fn dropping_the_derived_fact_breaks_replay() {
    let mut bundle = chained_bundle();
    bundle.derived_facts.clear();
    assert!(
        !bundle.replay(),
        "a reconciliation fact used as a free given was accepted"
    );
}

#[test]
fn stripping_the_derived_link_breaks_replay() {
    // Rewriting the `XR0` assert's source to look like an ordinary cut must
    // not smuggle it past the chain check.
    let mut bundle = chained_bundle();
    bundle.asserts[0].source = cut_src("XR0");
    assert!(
        !bundle.replay(),
        "an XR assert without a derivation link was accepted"
    );
}

#[test]
fn a_derived_fact_for_a_different_formula_breaks_replay() {
    // The embedded chain must prove the fact that is actually used: swapping
    // in a weaker fact (the derivation still replays!) must fail.
    let mut bundle = chained_bundle();
    bundle.derived_facts[0].formula =
        BundleFormula::from_qformula(&a(0, Rel::Le, 1_000_000));
    assert!(
        !bundle.replay(),
        "a chain for a different formula was accepted"
    );
}

#[test]
fn tampering_the_embedded_certificate_breaks_replay() {
    let mut bundle = chained_bundle();
    bundle.derived_facts[0].derivations[0].certificate =
        Certificate::new(CertNode::Contradiction);
    assert!(!bundle.replay(), "a bogus embedded certificate was accepted");
}

#[test]
fn tampering_a_premise_breaks_replay() {
    // Weaken a premise so the embedded certificate no longer refutes it.
    let mut bundle = chained_bundle();
    bundle.derived_facts[0].derivations[0].premises[1].formula =
        BundleFormula::from_qformula(&a(2, Rel::Le, 1_000));
    assert!(!bundle.replay(), "a tampered premise set was accepted");
}

#[test]
fn a_fact_with_no_derivation_breaks_replay() {
    let mut bundle = chained_bundle();
    bundle.derived_facts[0].derivations.clear();
    assert!(
        !bundle.replay(),
        "a derived fact with an empty chain was accepted"
    );
}

#[test]
fn duplicate_derived_fact_names_break_replay() {
    let mut bundle = chained_bundle();
    let dup = bundle.derived_facts[0].clone();
    bundle.derived_facts.push(dup);
    assert!(
        !bundle.replay(),
        "an ambiguous derivation link was accepted"
    );
}
