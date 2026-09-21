//! Boolean saturation — the shared, deterministic decomposition that both the
//! search (the `search` module) and the trusted replay
//! ([`crate::Certificate::replay`]) run *identically*, so that Farkas
//! multipliers line up positionally with the atoms they weight.
//!
//! Part of the trusted kernel: the certificate stores multipliers in the order
//! that [`collect_constraints`] produces, and picks case-splits at the index
//! [`leftmost_or_index`] returns, so replay's re-derivation must match the
//! search's byte-for-byte. Keeping this logic in one place is what guarantees
//! that.

use crate::constraint::{Constraint, canonicalize};
use adl_formula::{LinAtom, QFormula, Rel};

/// The result of flattening a conjunction into hard inequality atoms and
/// pending disjunctive obligations.
pub(crate) struct Saturated {
    /// A `False` conjunct collapsed the conjunction to `false` — refuted
    /// outright, no Farkas needed.
    pub has_false: bool,
    /// Flat conjuncts, each either an inequality `QFormula::Atom` (with
    /// `Rel ∈ {<, ≤, >, ≥}`) or a `QFormula::Or` obligation, in left-to-right
    /// traversal order.
    pub items: Vec<QFormula>,
}

/// Flatten a conjunction (the slice is an implicit `And`) into [`Saturated`].
///
/// `And`s are flattened, `True` dropped, `False` recorded; an `=` atom becomes
/// the two bounds `≤` then `≥` (fixed order); a `≠` atom becomes an `Or`
/// (`< ∨ >`); everything else is kept in place. The traversal is an explicit
/// work stack (not recursion) so arbitrarily deep `And` nesting cannot overflow
/// the stack.
pub(crate) fn saturate(conj: &[QFormula]) -> Saturated {
    let mut items = Vec::new();
    let mut has_false = false;
    // Push reversed so the stack pops left-to-right, preserving source order.
    let mut stack: Vec<&QFormula> = conj.iter().rev().collect();

    while let Some(f) = stack.pop() {
        match f {
            QFormula::True => {}
            QFormula::False => has_false = true,
            QFormula::And(v) => stack.extend(v.iter().rev()),
            QFormula::Or(_) => items.push(f.clone()),
            QFormula::Atom(a) => match a.rel() {
                Rel::Eq => {
                    // x = k  ⇔  x ≤ k ∧ x ≥ k
                    items.push(QFormula::Atom(retagged(a, Rel::Le)));
                    items.push(QFormula::Atom(retagged(a, Rel::Ge)));
                }
                Rel::Ne => {
                    // x ≠ k  ⇔  x < k ∨ x > k
                    items.push(QFormula::Or(vec![
                        QFormula::Atom(retagged(a, Rel::Lt)),
                        QFormula::Atom(retagged(a, Rel::Gt)),
                    ]));
                }
                _ => items.push(f.clone()),
            },
        }
    }

    Saturated { has_false, items }
}

/// Same linear form and constant, a different relation.
fn retagged(atom: &LinAtom, rel: Rel) -> LinAtom {
    LinAtom::new(atom.terms().iter().cloned(), rel, atom.constant().clone())
}

/// Index of the leftmost `Or` obligation, if any. This is the canonical
/// case-split choice both search and replay make.
pub(crate) fn leftmost_or_index(items: &[QFormula]) -> Option<usize> {
    items.iter().position(|f| matches!(f, QFormula::Or(_)))
}

/// The disjuncts of an item known (by [`leftmost_or_index`]) to be an `Or`.
pub(crate) fn disjuncts(item: &QFormula) -> &[QFormula] {
    match item {
        QFormula::Or(v) => v,
        // Only ever called on an index returned by `leftmost_or_index`.
        _ => &[],
    }
}

/// Canonicalize the inequality atoms of a saturated item list, in order. At a
/// leaf (no `Or` items) this is every item; the non-atom filter is defensive.
pub(crate) fn collect_constraints(items: &[QFormula]) -> Vec<Constraint> {
    items
        .iter()
        .filter_map(|f| match f {
            QFormula::Atom(a) => Some(canonicalize(a)),
            _ => None,
        })
        .collect()
}

/// Build the child conjunction for one branch of a case split: every item
/// except the split `Or`, plus the chosen disjunct. Search and replay build it
/// the same way so their recursion trees coincide.
pub(crate) fn build_child(items: &[QFormula], or_index: usize, chosen: &QFormula) -> Vec<QFormula> {
    let mut child = Vec::with_capacity(items.len());
    for (i, it) in items.iter().enumerate() {
        if i != or_index {
            child.push(it.clone());
        }
    }
    child.push(chosen.clone());
    child
}

#[cfg(test)]
mod tests {
    use super::*;
    use adl_sema::{QuantityId, Rat};

    fn atom(qi: u32, rel: Rel, k: i64) -> QFormula {
        QFormula::Atom(LinAtom::single(QuantityId(qi), rel, Rat::from_i64(k)))
    }

    #[test]
    fn flattens_and_and_drops_true() {
        let conj = vec![QFormula::And(vec![
            atom(0, Rel::Lt, 1),
            QFormula::True,
            atom(1, Rel::Gt, 2),
        ])];
        let s = saturate(&conj);
        assert!(!s.has_false);
        assert_eq!(s.items.len(), 2);
        assert!(leftmost_or_index(&s.items).is_none());
    }

    #[test]
    fn eq_splits_and_ne_becomes_or() {
        let s = saturate(&[atom(0, Rel::Eq, 3)]);
        assert_eq!(s.items.len(), 2); // two bounds
        let s2 = saturate(&[atom(0, Rel::Ne, 3)]);
        assert_eq!(s2.items.len(), 1);
        assert_eq!(leftmost_or_index(&s2.items), Some(0));
    }

    #[test]
    fn false_is_detected() {
        let s = saturate(&[QFormula::False, atom(0, Rel::Lt, 1)]);
        assert!(s.has_false);
    }
}
