//! Canonical inequality form and the Farkas refutation primitive.
//!
//! This module is part of the **trusted kernel**: [`canonicalize`] and
//! [`farkas_refutes`] are what [`crate::Certificate::replay`] relies on, so
//! they are deliberately tiny, allocation-light, and exact.

use adl_formula::{LinAtom, Rel};
use adl_sema::{QuantityId, Rat};
use std::collections::BTreeMap;

/// A hard inequality in canonical *upper-bound* form `Σ coeffs·q ⋈ b`, where
/// `⋈` is `<` when [`strict`](Self::strict) and `≤` otherwise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Constraint {
    /// Non-zero coefficients, ascending by quantity (canonical order).
    pub coeffs: Vec<(QuantityId, Rat)>,
    /// Right-hand constant `b`.
    pub b: Rat,
    /// `true` ⇒ the relation is strict (`<`); `false` ⇒ `≤`.
    pub strict: bool,
}

/// Rewrite an inequality atom `Σ c·q ⋈ k` (with `⋈ ∈ {<, ≤, >, ≥}`) into the
/// canonical upper-bound form `Σ a·q (< / ≤) b`.
///
/// `≥`/`>` are flipped by negating every coefficient and the constant, which is
/// exact over rationals; `>`/`<` set the strict flag. `=`/`≠` never reach here —
/// the saturation pass splits `=` into two bounds and turns `≠` into a
/// disjunction before any leaf is formed, so a leaf atom is only ever
/// `< | ≤ | > | ≥`.
pub(crate) fn canonicalize(atom: &LinAtom) -> Constraint {
    let (flip, strict) = match atom.rel() {
        Rel::Le => (false, false),
        Rel::Lt => (false, true),
        Rel::Ge => (true, false),
        Rel::Gt => (true, true),
        // Unreachable: saturation removes Eq/Ne before a leaf is canonicalized.
        Rel::Eq | Rel::Ne => (false, false),
    };
    let coeffs = atom
        .terms()
        .iter()
        .map(|(c, q)| (*q, if flip { -c } else { c.clone() }))
        .collect();
    let b = if flip {
        -atom.constant()
    } else {
        atom.constant().clone()
    };
    Constraint { coeffs, b, strict }
}

/// The Farkas refutation check — the heart of the trusted kernel.
///
/// Given canonical [`Constraint`]s and nonnegative multipliers `λ` (one per
/// constraint, same order), returns `true` iff the multipliers witness that the
/// conjunction of the constraints is **real-infeasible**.
///
/// Rule: multiplying constraint `i` (`aᵢ·x ⋈ᵢ bᵢ`, `⋈ᵢ ∈ {<, ≤}`) by
/// `λᵢ ≥ 0` and summing yields `(Σ λᵢ aᵢ)·x ⋈ (Σ λᵢ bᵢ)`, where `⋈` is `<` if
/// some `λᵢ > 0` has a strict `⋈ᵢ`, else `≤`. The combination is a
/// contradiction — hence the system is infeasible — iff every combined
/// coefficient is zero **and** the ground relation `0 ⋈ (Σ λᵢ bᵢ)` is false:
///   * `≤`: `0 ≤ S` is false ⇔ `S < 0`;
///   * `<`: `0 < S` is false ⇔ `S ≤ 0`.
///
/// Nonnegativity is enforced here: a negative multiplier could flip an
/// inequality and "prove" anything, so any `λᵢ < 0` rejects the certificate.
/// All-zero multipliers give `S = 0` with a non-strict combined relation, whose
/// `0 ≤ 0` is not a contradiction — so a degenerate (or tampered-to-zero)
/// multiplier vector correctly fails.
pub(crate) fn farkas_refutes(cons: &[Constraint], lambdas: &[Rat]) -> bool {
    if lambdas.len() != cons.len() {
        return false;
    }
    // Accumulated left-hand coefficients per quantity, and the right-hand sum S.
    let mut acc: BTreeMap<QuantityId, Rat> = BTreeMap::new();
    let mut s = Rat::zero();
    let mut any_strict = false;

    for (con, lam) in cons.iter().zip(lambdas) {
        if lam.is_negative() {
            return false; // nonnegativity is mandatory for soundness
        }
        if lam.is_zero() {
            continue; // contributes nothing; also cannot make the combo strict
        }
        for (q, c) in &con.coeffs {
            let term = lam * c;
            let e = acc.entry(*q).or_insert_with(Rat::zero);
            *e = &*e + &term;
        }
        s = &s + &(lam * &con.b);
        if con.strict {
            // lam > 0 here (it is neither negative nor zero), so a strict
            // participant with a positive multiplier makes the combo strict.
            any_strict = true;
        }
    }

    // The linear parts must cancel: the derived relation has to be ground.
    if acc.values().any(|v| !v.is_zero()) {
        return false;
    }

    if any_strict {
        // Derived `0 < S`; contradictory (unsatisfiable) iff S ≤ 0.
        !s.is_positive()
    } else {
        // Derived `0 ≤ S`; contradictory iff S < 0.
        s.is_negative()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: u32) -> QuantityId {
        QuantityId(n)
    }
    fn r(n: i64) -> Rat {
        Rat::from_i64(n)
    }

    fn con(coeffs: &[(u32, i64)], b: i64, strict: bool) -> Constraint {
        Constraint {
            coeffs: coeffs.iter().map(|(qi, c)| (q(*qi), r(*c))).collect(),
            b: r(b),
            strict,
        }
    }

    #[test]
    fn ge_flips_to_upper_bound() {
        // x >= 5  ==>  -x <= -5
        let atom = LinAtom::single(q(0), Rel::Ge, r(5));
        let c = canonicalize(&atom);
        assert_eq!(c.coeffs, vec![(q(0), r(-1))]);
        assert_eq!(c.b, r(-5));
        assert!(!c.strict);
    }

    #[test]
    fn strict_boundary_refutes() {
        // x >= 5  (-x <= -5) and x < 5 (x < 5) — 1*(-x<=-5) + 1*(x<5) = 0 < 0.
        let a = con(&[(0, -1)], -5, false);
        let b = con(&[(0, 1)], 5, true);
        assert!(farkas_refutes(&[a, b], &[r(1), r(1)]));
    }

    #[test]
    fn nonstrict_equal_bounds_are_feasible() {
        // x <= 1 and x >= 1 (=> -x <= -1): 0 <= 0 is not a contradiction.
        let a = con(&[(0, 1)], 1, false);
        let b = con(&[(0, -1)], -1, false);
        assert!(!farkas_refutes(&[a, b], &[r(1), r(1)]));
    }

    #[test]
    fn negative_multiplier_rejected() {
        let a = con(&[(0, -1)], -5, false);
        let b = con(&[(0, 1)], 5, true);
        assert!(!farkas_refutes(&[a, b], &[r(-1), r(1)]));
    }

    #[test]
    fn uncancelled_coeffs_rejected() {
        let a = con(&[(0, -1)], -5, false);
        let b = con(&[(0, 1)], 5, true);
        // 2*a + 1*b leaves coefficient -1 on x: not ground, not a refutation.
        assert!(!farkas_refutes(&[a, b], &[r(2), r(1)]));
    }
}
