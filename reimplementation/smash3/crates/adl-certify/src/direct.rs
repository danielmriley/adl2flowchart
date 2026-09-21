//! Direct (search-free) certificate construction for **bound refutations** —
//! the proof shape the analyzer's interval fast path produces.
//!
//! The interval fast path never calls a solver: it folds the unconditional
//! And-spine of a region's over-projection into one `[lo, hi]` per quantity and
//! reports disjointness when two intervals cannot intersect (or when one is
//! itself empty). That refutation is already a Farkas certificate of the
//! smallest possible size: a lower bound `a_L·q ⋈ b_L` (`a_L < 0` in canonical
//! upper-bound form) and an upper bound `a_U·q ⋈ b_U` (`a_U > 0`) combine under
//! `λ = (1/|a_L|, 1/|a_U|)` to the ground relation `0 ⋈ hi − lo`, which is
//! false exactly when the intervals fail to intersect — strictness included,
//! since the combined relation is `<` iff either bound is strict (Motzkin).
//!
//! So instead of running the DPLL(Farkas) search, [`construct`] computes those
//! multipliers in closed form and lets the trusted kernel accept or reject the
//! result. Nothing here is trusted: [`crate::certify_bounds`] replays every
//! certificate this module builds before returning it, exactly as
//! [`crate::certify_unsat`] does for the search.

use adl_formula::QFormula;
use adl_sema::{QuantityId, Rat};

use crate::certificate::{CertNode, Certificate, QRat};
use crate::constraint::{Constraint, farkas_refutes};
use crate::saturate::{collect_constraints, leftmost_or_index, saturate};

/// Cap on the canonical constraints a direct construction will scan. The
/// pairing loop is O(n²) and interval refutations are 1–4 constraints wide;
/// the cap only exists so a caller cannot hand this an unbounded conjunction.
const MAX_CONSTRAINTS: usize = 64;

/// Build a bound-pair (or contradiction) certificate for `formulas`, or `None`
/// if the conjunction is not of that shape. Never panics; the result is
/// **unchecked** — go through [`crate::certify_bounds`], which replays it.
pub(crate) fn construct(formulas: &[QFormula]) -> Option<Certificate> {
    let sat = saturate(formulas);
    if sat.has_false {
        // A constant-false cut on the spine: the region's over-projection is
        // unsatisfiable outright, no multipliers involved.
        return Some(Certificate::new(CertNode::Contradiction));
    }
    if leftmost_or_index(&sat.items).is_some() {
        // A case split is not a bound refutation. The interval layer ignores
        // disjunctive structure, so its winning atoms are always on the spine;
        // a caller that lands here should fall back to the atom-level set.
        return None;
    }
    let cons = collect_constraints(&sat.items);
    if cons.is_empty() || cons.len() > MAX_CONSTRAINTS {
        return None;
    }
    let zeros = vec![Rat::zero(); cons.len()];

    // (a) One constraint that is already ground-false (`0 ≤ b` with b < 0).
    for (i, c) in cons.iter().enumerate() {
        if c.coeffs.iter().any(|(_, a)| !a.is_zero()) {
            continue; // not a ground relation
        }
        let mut lam = zeros.clone();
        lam[i] = Rat::one();
        if farkas_refutes(&cons, &lam) {
            return Some(leaf(&lam));
        }
    }

    // (b) A lower/upper bound pair on one shared quantity.
    for i in 0..cons.len() {
        let Some((qi, ai)) = sole_term(&cons[i]) else {
            continue;
        };
        for j in i + 1..cons.len() {
            let Some((qj, aj)) = sole_term(&cons[j]) else {
                continue;
            };
            if qi != qj || ai.is_negative() == aj.is_negative() {
                continue; // different quantity, or two bounds on the same side
            }
            let (Some(li), Some(lj)) = (recip_abs(ai), recip_abs(aj)) else {
                continue;
            };
            let mut lam = zeros.clone();
            lam[i] = li;
            lam[j] = lj;
            if farkas_refutes(&cons, &lam) {
                return Some(leaf(&lam));
            }
        }
    }
    None
}

fn leaf(lambdas: &[Rat]) -> Certificate {
    Certificate::new(CertNode::Farkas {
        multipliers: lambdas.iter().cloned().map(QRat).collect(),
    })
}

/// The single non-zero coefficient of a constraint, if it has exactly one.
fn sole_term(c: &Constraint) -> Option<(QuantityId, &Rat)> {
    let mut it = c.coeffs.iter().filter(|(_, a)| !a.is_zero());
    let (q, a) = it.next()?;
    it.next().is_none().then_some((*q, a))
}

/// `1/|a|` — the multiplier that normalizes a bound to a unit coefficient.
fn recip_abs(a: &Rat) -> Option<Rat> {
    Rat::one().checked_div(&a.abs())
}

#[cfg(test)]
mod tests {
    use crate::certify_bounds;
    use adl_formula::{LinAtom, QFormula, Rel};
    use adl_sema::{QuantityId, Rat};

    fn atom(q: u32, rel: Rel, k: i64) -> QFormula {
        QFormula::Atom(LinAtom::single(QuantityId(q), rel, Rat::from_i64(k)))
    }

    fn scaled(q: u32, c: i64, rel: Rel, k: i64) -> QFormula {
        QFormula::Atom(LinAtom::new(
            [(Rat::from_i64(c), QuantityId(q))],
            rel,
            Rat::from_i64(k),
        ))
    }

    /// The Motzkin strictness rules, one case per row: `lo > hi` refutes with
    /// either strictness, `lo == hi` refutes only when a bound is strict.
    #[test]
    fn strict_and_loose_boundaries() {
        // q > 2 vs q <= 2 — touching, one strict: refuted.
        assert!(certify_bounds(&[atom(0, Rel::Gt, 2), atom(0, Rel::Le, 2)]).is_some());
        // q >= 2 vs q < 2 — touching the other way: refuted.
        assert!(certify_bounds(&[atom(0, Rel::Ge, 2), atom(0, Rel::Lt, 2)]).is_some());
        // q > 2 vs q < 2 — both strict: refuted.
        assert!(certify_bounds(&[atom(0, Rel::Gt, 2), atom(0, Rel::Lt, 2)]).is_some());
        // q >= 2 vs q <= 2 — both loose: q = 2 is a solution, NOT refuted.
        assert!(certify_bounds(&[atom(0, Rel::Ge, 2), atom(0, Rel::Le, 2)]).is_none());
        // Separated bounds refute regardless of strictness.
        assert!(certify_bounds(&[atom(0, Rel::Ge, 5), atom(0, Rel::Le, 2)]).is_some());
    }

    #[test]
    fn coefficients_are_normalized_not_assumed_unit() {
        // 3q >= 12 (q >= 4) vs -2q >= -4 (q <= 2): λ = (1/3, 1/2).
        assert!(certify_bounds(&[scaled(0, 3, Rel::Ge, 12), scaled(0, -2, Rel::Ge, -4)]).is_some());
    }

    #[test]
    fn equality_atom_supplies_both_bounds() {
        // q = 5 saturates to q ≤ 5 ∧ q ≥ 5; with q > 5 the pair refutes.
        assert!(certify_bounds(&[atom(0, Rel::Eq, 5), atom(0, Rel::Gt, 5)]).is_some());
    }

    #[test]
    fn different_quantities_do_not_refute() {
        assert!(certify_bounds(&[atom(0, Rel::Gt, 5), atom(1, Rel::Lt, 2)]).is_none());
    }

    #[test]
    fn constant_false_is_a_contradiction_certificate() {
        let c = certify_bounds(&[QFormula::And(vec![atom(0, Rel::Gt, 1), QFormula::False])]);
        assert!(c.is_some(), "a falsified spine certifies outright");
    }

    #[test]
    fn disjunction_is_out_of_shape() {
        let f = QFormula::Or(vec![atom(0, Rel::Lt, 1), atom(0, Rel::Gt, 5)]);
        assert!(certify_bounds(&[f, atom(0, Rel::Gt, 100)]).is_none());
    }

    #[test]
    fn satisfiable_sets_are_never_certified() {
        assert!(certify_bounds(&[atom(0, Rel::Gt, 1), atom(0, Rel::Lt, 5)]).is_none());
        assert!(certify_bounds(&[]).is_none());
    }
}
