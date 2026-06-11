//! NNF / projection laws over generated small formulas
//! (SPEC_ARCHITECTURE §5):
//!
//! - `not` is involutive: `not(not f) ≡ f`;
//! - polarity duality: `project(not f, Over) ≡ not(project(f, Under))`
//!   and symmetrically — this is exactly why `reject` stays sound through
//!   the projections;
//! - on Unknown/Dual-free formulas the two projections coincide.

use adl_formula::{DiagId, Formula, LinAtom, QFormula, Rel};
use adl_sema::QuantityId;
use proptest::prelude::*;

fn arb_rel() -> impl Strategy<Value = Rel> {
    prop_oneof![
        Just(Rel::Lt),
        Just(Rel::Le),
        Just(Rel::Gt),
        Just(Rel::Ge),
        Just(Rel::Eq),
        Just(Rel::Ne),
    ]
}

fn arb_atom() -> impl Strategy<Value = LinAtom> {
    let term = (
        (-3i32..=3).prop_map(f64::from),
        (0u32..4).prop_map(QuantityId),
    );
    (
        proptest::collection::vec(term, 0..3),
        arb_rel(),
        (-10i32..=10).prop_map(f64::from),
    )
        .prop_map(|(terms, rel, k)| LinAtom::new(terms, rel, k).expect("finite by construction"))
}

fn arb_formula() -> impl Strategy<Value = Formula> {
    let leaf = prop_oneof![
        Just(Formula::True),
        Just(Formula::False),
        arb_atom().prop_map(Formula::Atom),
        (0u32..4).prop_map(|i| Formula::Unknown(DiagId(i))),
    ];
    leaf.prop_recursive(3, 32, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Formula::And),
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Formula::Or),
            (inner.clone(), inner, 0u32..4).prop_map(|(p, m, w)| Formula::Dual {
                plus: Box::new(p),
                minus: Box::new(m),
                why: DiagId(w),
            }),
        ]
    })
}

proptest! {
    /// `not(not f) ≡ f` — atom negation is involutive and `Dual` swaps
    /// branches twice.
    #[test]
    fn double_negation_is_identity(f in arb_formula()) {
        prop_assert_eq!(f.clone().not().not(), f);
    }

    /// `project(not f, Over) ≡ not(project(f, Under))`: negating first and
    /// over-projecting equals under-projecting first and negating.
    #[test]
    fn over_of_not_equals_not_of_under(f in arb_formula()) {
        let lhs = f.clone().not().over().into_qformula();
        let rhs = f.under().into_qformula().not();
        prop_assert_eq!(lhs, rhs);
    }

    /// The symmetric law: `project(not f, Under) ≡ not(project(f, Over))`.
    #[test]
    fn under_of_not_equals_not_of_over(f in arb_formula()) {
        let lhs = f.clone().not().under().into_qformula();
        let rhs = f.over().into_qformula().not();
        prop_assert_eq!(lhs, rhs);
    }

    /// On Unknown/Dual-free formulas the projections coincide (the
    /// encoding is exact, `R⁺ = R⁻ = R`).
    #[test]
    fn exact_formulas_project_identically(f in arb_formula()) {
        if f.is_exact() {
            prop_assert_eq!(f.over().into_qformula(), f.under().into_qformula());
        }
    }

    /// `QFormula::not` is involutive too (sanity for the law equations).
    #[test]
    fn qformula_double_negation(f in arb_formula()) {
        let q = f.over().into_qformula();
        prop_assert_eq!(q.clone().not().not(), q);
    }
}

#[test]
fn unknown_projects_to_polarity_units() {
    let f = Formula::Unknown(DiagId(0));
    assert_eq!(f.over().into_qformula(), QFormula::True);
    assert_eq!(f.under().into_qformula(), QFormula::False);
}

#[test]
fn dual_projects_to_its_branches() {
    let plus = Formula::Atom(LinAtom::single(QuantityId(0), Rel::Gt, 1.0).unwrap());
    let minus = Formula::Atom(LinAtom::single(QuantityId(0), Rel::Gt, 2.0).unwrap());
    let d = Formula::Dual {
        plus: Box::new(plus.clone()),
        minus: Box::new(minus.clone()),
        why: DiagId(0),
    };
    assert_eq!(d.over().into_qformula(), plus.over().into_qformula());
    assert_eq!(d.under().into_qformula(), minus.under().into_qformula());
}
