//! The polarity-aware formula IR (SPEC_ARCHITECTURE §5).
//!
//! [`Formula`] is the *exact* region encoding: it may contain explicit
//! ignorance ([`Formula::Unknown`]) and convention hedges
//! ([`Formula::Dual`]). Solver-facing code never sees those: it consumes
//! [`Over`] / [`Under`] projections wrapping a [`QFormula`], which is
//! Unknown/Dual-free **by type**.
//!
//! Soundness direction is a type (ADR-004): only [`Formula::over`] /
//! [`Formula::under`] can construct the projection wrappers, so a
//! disjointness proof cannot be fed an under-approximation.

use crate::lin::LinAtom;
use adl_syntax::span::Span;

/// Identifier of a diagnostic in the [`DiagTable`] that accompanies a
/// formula (region-local; an `Unknown`/`Dual` leaf carries the id of the
/// reason the user sees).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiagId(pub u32);

impl std::fmt::Display for DiagId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "D{}", self.0)
    }
}

/// The diagnostic behind an `Unknown` or `Dual` leaf: source span +
/// human-readable reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaDiag {
    pub span: Span,
    pub reason: String,
}

/// Append-only table of formula diagnostics; [`DiagId`]s index into it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagTable {
    entries: Vec<FormulaDiag>,
}

impl DiagTable {
    pub fn push(&mut self, span: Span, reason: impl Into<String>) -> DiagId {
        let id = DiagId(u32::try_from(self.entries.len()).expect("diag table overflow"));
        self.entries.push(FormulaDiag {
            span,
            reason: reason.into(),
        });
        id
    }

    #[must_use]
    pub fn get(&self, id: DiagId) -> Option<&FormulaDiag> {
        self.entries.get(id.0 as usize)
    }

    pub fn iter(&self) -> impl Iterator<Item = (DiagId, &FormulaDiag)> {
        self.entries
            .iter()
            .enumerate()
            .map(|(i, d)| (DiagId(u32::try_from(i).expect("diag table overflow")), d))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The exact formula IR; may contain `Unknown` / `Dual`
/// (SPEC_ARCHITECTURE §5).
#[derive(Debug, Clone, PartialEq)]
pub enum Formula {
    True,
    False,
    /// `Σ cᵢ·Quantityᵢ ⋈ k`.
    Atom(LinAtom),
    And(Vec<Formula>),
    Or(Vec<Formula>),
    /// Explicit ignorance, with its diagnostic.
    Unknown(DiagId),
    /// Convention hedge: the real semantics is *between* the two branches
    /// (`minus ⊆ R ⊆ plus`); `why` names the unresolved question.
    Dual {
        plus: Box<Formula>,
        minus: Box<Formula>,
        why: DiagId,
    },
}

impl Formula {
    /// Exact negation in negation normal form.
    ///
    /// `Unknown` stays `Unknown` (¬unknown is unknown); `Dual` **swaps
    /// branches**: if `minus ⊆ R ⊆ plus` then `¬plus ⊆ ¬R ⊆ ¬minus`, so
    /// the new plus is `¬minus` and the new minus is `¬plus`.
    /// Involutive: `f.not().not() == f`.
    ///
    /// (`std::ops::Not` is also implemented and delegates here; the
    /// inherent name is the SPEC_ARCHITECTURE §5 API.)
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> Formula {
        match self {
            Formula::True => Formula::False,
            Formula::False => Formula::True,
            Formula::Atom(a) => Formula::Atom(a.negated()),
            Formula::And(v) => Formula::Or(v.into_iter().map(Formula::not).collect()),
            Formula::Or(v) => Formula::And(v.into_iter().map(Formula::not).collect()),
            Formula::Unknown(d) => Formula::Unknown(d),
            Formula::Dual { plus, minus, why } => Formula::Dual {
                plus: Box::new(minus.not()),
                minus: Box::new(plus.not()),
                why,
            },
        }
    }

    /// Over-projection `R⁺ ⊇ R`: `Unknown → true`, `Dual → plus`.
    #[must_use]
    pub fn over(&self) -> Over {
        Over(self.project(Polarity::Over))
    }

    /// Under-projection `R⁻ ⊆ R`: `Unknown → false`, `Dual → minus`.
    #[must_use]
    pub fn under(&self) -> Under {
        Under(self.project(Polarity::Under))
    }

    /// Does this formula contain no `Unknown` and no `Dual`? (Then both
    /// projections are the same formula and the encoding is exact.)
    #[must_use]
    pub fn is_exact(&self) -> bool {
        match self {
            Formula::True | Formula::False | Formula::Atom(_) => true,
            Formula::And(v) | Formula::Or(v) => v.iter().all(Formula::is_exact),
            Formula::Unknown(_) | Formula::Dual { .. } => false,
        }
    }

    fn project(&self, polarity: Polarity) -> QFormula {
        match self {
            Formula::True => QFormula::True,
            Formula::False => QFormula::False,
            Formula::Atom(a) => QFormula::Atom(a.clone()),
            Formula::And(v) => QFormula::And(v.iter().map(|f| f.project(polarity)).collect()),
            Formula::Or(v) => QFormula::Or(v.iter().map(|f| f.project(polarity)).collect()),
            Formula::Unknown(_) => match polarity {
                Polarity::Over => QFormula::True,
                Polarity::Under => QFormula::False,
            },
            Formula::Dual { plus, minus, .. } => match polarity {
                Polarity::Over => plus.project(polarity),
                Polarity::Under => minus.project(polarity),
            },
        }
    }
}

impl std::ops::Not for Formula {
    type Output = Formula;

    fn not(self) -> Formula {
        Formula::not(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    Over,
    Under,
}

/// A projected formula: Unknown/Dual-free **by type**, directly
/// emittable to a solver backend.
#[derive(Debug, Clone, PartialEq)]
pub enum QFormula {
    True,
    False,
    Atom(LinAtom),
    And(Vec<QFormula>),
    Or(Vec<QFormula>),
}

impl QFormula {
    /// Exact NNF negation (total on `QFormula`: there is no ignorance to
    /// approximate). `std::ops::Not` delegates here.
    #[must_use]
    #[allow(clippy::should_implement_trait)]
    pub fn not(self) -> QFormula {
        match self {
            QFormula::True => QFormula::False,
            QFormula::False => QFormula::True,
            QFormula::Atom(a) => QFormula::Atom(a.negated()),
            QFormula::And(v) => QFormula::Or(v.into_iter().map(QFormula::not).collect()),
            QFormula::Or(v) => QFormula::And(v.into_iter().map(QFormula::not).collect()),
        }
    }
}

impl std::ops::Not for QFormula {
    type Output = QFormula;

    fn not(self) -> QFormula {
        QFormula::not(self)
    }
}

/// An over-approximation `R⁺ ⊇ R` (`Unknown → true`, `Dual → plus`).
///
/// The **only** constructor is [`Formula::over`]; proof code that
/// requires over-approximations (disjointness, region-empty, the `A⁺`
/// side of subset) takes `&Over` and cannot be handed an
/// under-approximation:
///
/// ```compile_fail,E0308
/// use adl_formula::{Formula, Over};
/// fn prove_disjoint(_a: &Over, _b: &Over) { /* solver call */ }
/// let a = Formula::True;
/// let b = Formula::False;
/// // WRONG direction: under-approximations prove overlap, not disjointness.
/// prove_disjoint(&a.under(), &b.under());
/// ```
///
/// Nor can a polarity be forged outside this crate — the wrapped field is
/// private:
///
/// ```compile_fail,E0603
/// let forged = adl_formula::Over(adl_formula::QFormula::True);
/// ```
///
/// Correct use compiles:
///
/// ```
/// use adl_formula::{Formula, Over};
/// fn prove_disjoint(_a: &Over, _b: &Over) {}
/// let a = Formula::True;
/// let b = Formula::False;
/// prove_disjoint(&a.over(), &b.over());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Over(QFormula);

impl Over {
    /// The projected, emittable formula.
    #[must_use]
    pub fn qformula(&self) -> &QFormula {
        &self.0
    }

    #[must_use]
    pub fn into_qformula(self) -> QFormula {
        self.0
    }
}

/// An under-approximation `R⁻ ⊆ R` (`Unknown → false`, `Dual → minus`).
///
/// The **only** constructor is [`Formula::under`]; see [`Over`] for the
/// compile-time misuse demonstrations (the same applies symmetrically:
/// overlap proofs take `&Under` and reject `&Over`).
///
/// ```compile_fail,E0308
/// use adl_formula::{Formula, Under};
/// fn prove_overlap(_a: &Under, _b: &Under) { /* solver call */ }
/// let a = Formula::True;
/// let b = Formula::False;
/// // WRONG direction: over-approximations prove disjointness, not overlap.
/// prove_overlap(&a.over(), &b.over());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Under(QFormula);

impl Under {
    /// The projected, emittable formula.
    #[must_use]
    pub fn qformula(&self) -> &QFormula {
        &self.0
    }

    #[must_use]
    pub fn into_qformula(self) -> QFormula {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lin::Rel;
    use adl_sema::{QuantityId, Rat};

    fn atom(q: u32, rel: Rel, k: f64) -> Formula {
        Formula::Atom(LinAtom::single(
            QuantityId(q),
            rel,
            Rat::from_decimal_f64(k).unwrap(),
        ))
    }

    #[test]
    fn dual_negation_swaps_branches() {
        let plus = atom(0, Rel::Gt, 1.0);
        let minus = atom(0, Rel::Gt, 2.0);
        let d = Formula::Dual {
            plus: Box::new(plus.clone()),
            minus: Box::new(minus.clone()),
            why: DiagId(0),
        };
        let n = d.clone().not();
        assert_eq!(
            n,
            Formula::Dual {
                plus: Box::new(minus.not()),
                minus: Box::new(plus.not()),
                why: DiagId(0),
            }
        );
        assert_eq!(d.clone().not().not(), d);
    }

    #[test]
    fn projections_resolve_unknown_and_dual() {
        let d = Formula::And(vec![
            Formula::Unknown(DiagId(0)),
            Formula::Dual {
                plus: Box::new(atom(1, Rel::Lt, 5.0)),
                minus: Box::new(Formula::False),
                why: DiagId(1),
            },
        ]);
        assert_eq!(
            d.over().qformula(),
            &QFormula::And(vec![
                QFormula::True,
                QFormula::Atom(LinAtom::single(QuantityId(1), Rel::Lt, Rat::from_decimal_f64(5.0).unwrap())),
            ])
        );
        assert_eq!(
            d.under().qformula(),
            &QFormula::And(vec![QFormula::False, QFormula::False])
        );
    }

    #[test]
    fn exactness_flags() {
        assert!(Formula::And(vec![Formula::True, atom(0, Rel::Ne, 0.0)]).is_exact());
        assert!(!Formula::Or(vec![Formula::Unknown(DiagId(0))]).is_exact());
    }
}
