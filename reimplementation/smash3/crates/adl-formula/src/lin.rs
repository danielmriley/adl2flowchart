//! Linear atoms: `Σ cᵢ·Quantityᵢ ⋈ k` over interned [`QuantityId`]s, with
//! coefficients and the right-hand constant held as **exact rationals**
//! ([`Rat`]). Rationals are always finite, so atom construction is total —
//! there is no non-finite coefficient/constant to reject (the old f64
//! overflow/NaN failure modes vanish), and folding cut arithmetic is exact:
//! the analyzer and interpreter agree on the rational fragment to the bit.

use adl_sema::{QuantityId, Rat};

/// Comparison relation of a [`LinAtom`] (`⋈ ∈ <, ≤, >, ≥, =, ≠`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Rel {
    /// The exact logical negation (`<` ↔ `≥`, `≤` ↔ `>`, `=` ↔ `≠`).
    /// Involutive: `r.negated().negated() == r`.
    #[must_use]
    pub fn negated(self) -> Self {
        match self {
            Rel::Lt => Rel::Ge,
            Rel::Le => Rel::Gt,
            Rel::Gt => Rel::Le,
            Rel::Ge => Rel::Lt,
            Rel::Eq => Rel::Ne,
            Rel::Ne => Rel::Eq,
        }
    }

    /// The relation after swapping sides (or multiplying both sides by a
    /// negative): `<` ↔ `>`, `≤` ↔ `≥`; `=`/`≠` unchanged.
    #[must_use]
    pub fn flipped(self) -> Self {
        match self {
            Rel::Lt => Rel::Gt,
            Rel::Le => Rel::Ge,
            Rel::Gt => Rel::Lt,
            Rel::Ge => Rel::Le,
            Rel::Eq => Rel::Eq,
            Rel::Ne => Rel::Ne,
        }
    }

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Rel::Lt => "<",
            Rel::Le => "<=",
            Rel::Gt => ">",
            Rel::Ge => ">=",
            Rel::Eq => "==",
            Rel::Ne => "!=",
        }
    }

    /// Evaluate `lhs ⋈ rhs` exactly.
    #[must_use]
    pub fn eval(self, lhs: &Rat, rhs: &Rat) -> bool {
        match self {
            Rel::Lt => lhs < rhs,
            Rel::Le => lhs <= rhs,
            Rel::Gt => lhs > rhs,
            Rel::Ge => lhs >= rhs,
            Rel::Eq => lhs == rhs,
            Rel::Ne => lhs != rhs,
        }
    }
}

/// A linear atom `Σ cᵢ·qᵢ ⋈ k` in canonical form: terms sorted by
/// `QuantityId`, duplicate quantities merged, zero coefficients dropped.
/// Coefficients and the constant are exact rationals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinAtom {
    terms: Vec<(Rat, QuantityId)>,
    rel: Rel,
    constant: Rat,
}

impl LinAtom {
    /// Construct a canonical atom. Total: rationals never go non-finite, and
    /// merging duplicate quantities is exact.
    #[must_use]
    pub fn new(
        terms: impl IntoIterator<Item = (Rat, QuantityId)>,
        rel: Rel,
        constant: Rat,
    ) -> Self {
        let mut merged: std::collections::BTreeMap<QuantityId, Rat> =
            std::collections::BTreeMap::new();
        for (c, q) in terms {
            let entry = merged.entry(q).or_insert_with(Rat::zero);
            *entry = &*entry + &c;
        }
        let terms = merged
            .into_iter()
            .filter(|(_, c)| !c.is_zero())
            .map(|(q, c)| (c, q))
            .collect();
        Self {
            terms,
            rel,
            constant,
        }
    }

    /// Convenience: the single-term atom `1·q ⋈ k`.
    #[must_use]
    pub fn single(q: QuantityId, rel: Rel, constant: Rat) -> Self {
        Self::new([(Rat::one(), q)], rel, constant)
    }

    /// Canonical term list `(coefficient, quantity)`, sorted by quantity.
    #[must_use]
    pub fn terms(&self) -> &[(Rat, QuantityId)] {
        &self.terms
    }

    #[must_use]
    pub fn rel(&self) -> Rel {
        self.rel
    }

    #[must_use]
    pub fn constant(&self) -> &Rat {
        &self.constant
    }

    /// The exact negation: same linear form, negated relation.
    /// Involutive: `a.negated().negated() == a`.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            terms: self.terms.clone(),
            rel: self.rel.negated(),
            constant: self.constant.clone(),
        }
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

    #[test]
    fn canonicalizes_merge_sort_and_zero_drop() {
        let a = LinAtom::new(
            [
                (r(2), q(3)),
                (r(1), q(1)),
                (r(3), q(3)),
                (r(4), q(2)),
                (r(-4), q(2)),
            ],
            Rel::Le,
            r(7),
        );
        assert_eq!(a.terms(), &[(r(1), q(1)), (r(5), q(3))]);
        assert_eq!(a.rel(), Rel::Le);
        assert_eq!(a.constant(), &r(7));
    }

    #[test]
    fn merging_huge_coefficients_is_exact_not_overflow() {
        // The old f64 path overflowed `MAX + MAX → inf` and rejected the
        // atom; rationals merge it exactly to `2·MAX`.
        let big = Rat::from_decimal_f64(f64::MAX).unwrap();
        let a = LinAtom::new([(big.clone(), q(1)), (big.clone(), q(1))], Rel::Gt, r(0));
        assert_eq!(a.terms().len(), 1);
        assert_eq!(a.terms()[0].0, &big + &big);
    }

    #[test]
    fn negation_is_involutive() {
        let a = LinAtom::new(
            [(r(1), q(0)), (Rat::from_decimal_f64(-2.5).unwrap(), q(4))],
            Rel::Lt,
            r(30),
        );
        assert_ne!(a.negated(), a);
        assert_eq!(a.negated().negated(), a);
        for rel in [Rel::Lt, Rel::Le, Rel::Gt, Rel::Ge, Rel::Eq, Rel::Ne] {
            assert_eq!(rel.negated().negated(), rel);
            assert_eq!(rel.flipped().flipped(), rel);
        }
    }
}
