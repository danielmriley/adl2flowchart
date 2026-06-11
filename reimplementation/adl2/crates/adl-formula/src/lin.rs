//! Linear atoms: `Σ cᵢ·Quantityᵢ ⋈ k` over interned [`QuantityId`]s.
//!
//! Non-finite constants cannot construct atoms (SPEC_ANALYSIS §1, audit
//! Bug 5 layer 1): [`LinAtom::new`] returns `Err` for any NaN/infinite
//! coefficient or right-hand constant, including ones produced by
//! coefficient merging.

use adl_sema::QuantityId;

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

    /// Evaluate `lhs ⋈ rhs` on concrete (finite) values.
    #[must_use]
    pub fn eval(self, lhs: f64, rhs: f64) -> bool {
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

/// Why a [`LinAtom`] could not be constructed.
#[derive(Debug, Clone, PartialEq)]
pub enum LinAtomError {
    /// A coefficient (possibly after merging duplicate quantities) is NaN
    /// or infinite.
    NonFiniteCoefficient(QuantityId),
    /// The right-hand constant is NaN or infinite.
    NonFiniteConstant,
}

impl std::fmt::Display for LinAtomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinAtomError::NonFiniteCoefficient(q) => {
                write!(f, "non-finite coefficient for quantity {q}")
            }
            LinAtomError::NonFiniteConstant => write!(f, "non-finite right-hand constant"),
        }
    }
}

impl std::error::Error for LinAtomError {}

/// A linear atom `Σ cᵢ·qᵢ ⋈ k` in canonical form: terms sorted by
/// `QuantityId`, duplicate quantities merged, zero coefficients dropped,
/// every constant finite **by construction**.
#[derive(Debug, Clone, PartialEq)]
pub struct LinAtom {
    terms: Vec<(f64, QuantityId)>,
    rel: Rel,
    constant: f64,
}

impl LinAtom {
    /// Construct a canonical atom.
    ///
    /// # Errors
    /// Rejects NaN/infinite coefficients and constants — including a
    /// coefficient that becomes non-finite when duplicate quantities are
    /// merged (audit Bug 5 layer 1: no non-finite value may reach a
    /// solver encoding).
    pub fn new(
        terms: impl IntoIterator<Item = (f64, QuantityId)>,
        rel: Rel,
        constant: f64,
    ) -> Result<Self, LinAtomError> {
        if !constant.is_finite() {
            return Err(LinAtomError::NonFiniteConstant);
        }
        let mut merged: std::collections::BTreeMap<QuantityId, f64> =
            std::collections::BTreeMap::new();
        for (c, q) in terms {
            if !c.is_finite() {
                return Err(LinAtomError::NonFiniteCoefficient(q));
            }
            let entry = merged.entry(q).or_insert(0.0);
            *entry += c;
            if !entry.is_finite() {
                return Err(LinAtomError::NonFiniteCoefficient(q));
            }
        }
        let terms = merged
            .into_iter()
            .filter(|&(_, c)| c != 0.0)
            .map(|(q, c)| (c, q))
            .collect();
        Ok(Self {
            terms,
            rel,
            constant,
        })
    }

    /// Convenience: the single-term atom `1·q ⋈ k`.
    ///
    /// # Errors
    /// Same contract as [`LinAtom::new`].
    pub fn single(q: QuantityId, rel: Rel, constant: f64) -> Result<Self, LinAtomError> {
        Self::new([(1.0, q)], rel, constant)
    }

    /// Canonical term list `(coefficient, quantity)`, sorted by quantity.
    #[must_use]
    pub fn terms(&self) -> &[(f64, QuantityId)] {
        &self.terms
    }

    #[must_use]
    pub fn rel(&self) -> Rel {
        self.rel
    }

    #[must_use]
    pub fn constant(&self) -> f64 {
        self.constant
    }

    /// The exact negation: same linear form, negated relation.
    /// Involutive: `a.negated().negated() == a`.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            terms: self.terms.clone(),
            rel: self.rel.negated(),
            constant: self.constant,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: u32) -> QuantityId {
        QuantityId(n)
    }

    #[test]
    fn rejects_non_finite_constant() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                LinAtom::new([(1.0, q(0))], Rel::Lt, bad),
                Err(LinAtomError::NonFiniteConstant)
            );
        }
    }

    #[test]
    fn rejects_non_finite_coefficient() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                LinAtom::new([(bad, q(2))], Rel::Ge, 1.0),
                Err(LinAtomError::NonFiniteCoefficient(q(2)))
            );
        }
    }

    #[test]
    fn rejects_overflow_from_merging() {
        let huge = f64::MAX;
        assert_eq!(
            LinAtom::new([(huge, q(1)), (huge, q(1))], Rel::Gt, 0.0),
            Err(LinAtomError::NonFiniteCoefficient(q(1)))
        );
    }

    #[test]
    fn canonicalizes_merge_sort_and_zero_drop() {
        let a = LinAtom::new(
            [
                (2.0, q(3)),
                (1.0, q(1)),
                (3.0, q(3)),
                (4.0, q(2)),
                (-4.0, q(2)),
            ],
            Rel::Le,
            7.0,
        )
        .unwrap();
        assert_eq!(a.terms(), &[(1.0, q(1)), (5.0, q(3))]);
        assert_eq!(a.rel(), Rel::Le);
        assert_eq!(a.constant(), 7.0);
    }

    #[test]
    fn negation_is_involutive() {
        let a = LinAtom::new([(1.0, q(0)), (-2.5, q(4))], Rel::Lt, 30.0).unwrap();
        assert_ne!(a.negated(), a);
        assert_eq!(a.negated().negated(), a);
        for rel in [Rel::Lt, Rel::Le, Rel::Gt, Rel::Ge, Rel::Eq, Rel::Ne] {
            assert_eq!(rel.negated().negated(), rel);
            assert_eq!(rel.flipped().flipped(), rel);
        }
    }
}
