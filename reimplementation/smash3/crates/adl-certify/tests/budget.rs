//! Budget tests: every resource limit degrades to `Uncertified` gracefully,
//! never a panic, and a generous budget still certifies the same shape.

use adl_certify::{Budget, certify_unsat};
use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};

fn a(qi: u32, rel: Rel, k: i64) -> QFormula {
    QFormula::Atom(LinAtom::single(QuantityId(qi), rel, Rat::from_i64(k)))
}

/// N independent width-2 `Or` obligations over a contradictory hard core:
/// `2^N` leaves, all Farkas-refutable — certifiable only with enough branches.
fn wide_unsat(n: usize) -> Vec<QFormula> {
    let mut forms = vec![a(0, Rel::Gt, 1), a(0, Rel::Lt, 1)]; // hard contradiction
    for i in 0..n {
        let qi = (i as u32) + 1;
        forms.push(QFormula::Or(vec![a(qi, Rel::Gt, 0), a(qi, Rel::Lt, 5)]));
    }
    forms
}

#[test]
fn branch_budget_exhaustion_is_uncertified_not_panic() {
    let forms = wide_unsat(20); // 2^20 leaves
    let tight = Budget {
        max_branches: 8,
        max_atoms: 128,
    };
    let r = certify_unsat(&forms, &tight);
    assert!(!r.is_certified());
    assert!(r.reason().unwrap().starts_with("budget"));
}

#[test]
fn generous_budget_certifies_the_same_shape() {
    let forms = wide_unsat(6); // 64 leaves, easily within default budget
    let r = certify_unsat(&forms, &Budget::default());
    assert!(r.is_certified());
    assert!(r.certificate().unwrap().replay(&forms));
}

#[test]
fn atom_budget_exhaustion_is_uncertified() {
    // A single leaf of 300 atoms over the atom cap.
    let mut forms = Vec::new();
    for i in 0..300u32 {
        forms.push(a(i, Rel::Lt, i as i64));
    }
    let capped = Budget {
        max_branches: 100_000,
        max_atoms: 128,
    };
    let r = certify_unsat(&forms, &capped);
    assert!(!r.is_certified());
    assert!(r.reason().unwrap().contains("atoms"));
}

#[test]
fn deep_or_nesting_hits_depth_cap_without_panic() {
    // A right-nested chain of width-1 Ors, past MAX_DEPTH. The internal depth
    // cap must fire and return Uncertified, never overflow the search/replay
    // stack. (Depth is kept ~MAX_DEPTH: far deeper inputs would overflow
    // QFormula's own derived Clone/Drop before this crate ever runs — a shared
    // adl-formula property, out of this crate's control; real cores are
    // shallow.)
    let mut f = a(0, Rel::Gt, 0); // satisfiable tail
    for _ in 0..1200 {
        f = QFormula::Or(vec![f]);
    }
    // Conjoin a hard contradiction so leaves are genuinely refutable and the
    // search actually descends the whole chain.
    let forms = [a(0, Rel::Lt, -1), f];
    let r = certify_unsat(&forms, &Budget::default());
    // 1200 > MAX_DEPTH: the depth cap fires. Must not panic and must not
    // falsely certify beyond what it can re-check.
    assert!(!r.is_certified());
    assert!(r.reason().unwrap().starts_with("budget"));
}
