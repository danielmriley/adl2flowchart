//! Interval fast path (SPEC_ANALYSIS §2): a cheap, sound heuristic over
//! the **unconditional And-spine** of a region's over-projection. Only
//! single-quantity atoms reachable without crossing an `Or` contribute —
//! such atoms are necessary conditions of R⁺ ⊇ R, so an empty
//! intersection proves disjointness without a solver (this is also the
//! no-solver fallback; everything it cannot prove stays POSSIBLY).

use adl_formula::{QFormula, Rel};
use adl_sema::{QuantityId, Rat};
use std::collections::BTreeMap;

/// A (possibly open) interval constraint on one quantity, over exact
/// rationals. `None` bounds are `±∞`. Bounds are EXACT (`k/c` as a rational),
/// so the interval fast path is a precise — not merely sound — summary of the
/// single-quantity atoms on the spine; no f64 rounding can shift a boundary.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Iv {
    pub lo: Option<Rat>,
    pub lo_strict: bool,
    pub hi: Option<Rat>,
    pub hi_strict: bool,
}

impl Iv {
    fn tighten_lo(&mut self, v: Rat, strict: bool) {
        let better = match &self.lo {
            None => true,
            Some(cur) => v > *cur || (v == *cur && strict && !self.lo_strict),
        };
        if better {
            self.lo_strict = strict;
            self.lo = Some(v);
        }
    }

    fn tighten_hi(&mut self, v: Rat, strict: bool) {
        let better = match &self.hi {
            None => true,
            Some(cur) => v < *cur || (v == *cur && strict && !self.hi_strict),
        };
        if better {
            self.hi_strict = strict;
            self.hi = Some(v);
        }
    }

    /// Is the interval itself empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match (&self.lo, &self.hi) {
            (Some(lo), Some(hi)) => lo > hi || (lo == hi && (self.lo_strict || self.hi_strict)),
            _ => false,
        }
    }

    /// Do `self` and `other` fail to intersect?
    #[must_use]
    pub fn disjoint_from(&self, other: &Iv) -> bool {
        // Tighter lower bound: the GREATER value (None = −∞ is weakest).
        let (lo, lo_strict): (Option<&Rat>, bool) = match (&self.lo, &other.lo) {
            (None, None) => (None, false),
            (Some(a), None) => (Some(a), self.lo_strict),
            (None, Some(b)) => (Some(b), other.lo_strict),
            (Some(a), Some(b)) => {
                if a > b {
                    (Some(a), self.lo_strict)
                } else if b > a {
                    (Some(b), other.lo_strict)
                } else {
                    (Some(a), self.lo_strict || other.lo_strict)
                }
            }
        };
        // Tighter upper bound: the LESSER value (None = +∞ is weakest).
        let (hi, hi_strict): (Option<&Rat>, bool) = match (&self.hi, &other.hi) {
            (None, None) => (None, false),
            (Some(a), None) => (Some(a), self.hi_strict),
            (None, Some(b)) => (Some(b), other.hi_strict),
            (Some(a), Some(b)) => {
                if a < b {
                    (Some(a), self.hi_strict)
                } else if b < a {
                    (Some(b), other.hi_strict)
                } else {
                    (Some(a), self.hi_strict || other.hi_strict)
                }
            }
        };
        match (lo, hi) {
            (Some(lo), Some(hi)) => lo > hi || (lo == hi && (lo_strict || hi_strict)),
            _ => false,
        }
    }

    #[must_use]
    pub fn human(&self) -> String {
        let lo_b = if self.lo_strict { "(" } else { "[" };
        let hi_b = if self.hi_strict { ")" } else { "]" };
        let lo = self.lo.as_ref().map_or("-inf".to_owned(), |r| r.to_f64().to_string());
        let hi = self.hi.as_ref().map_or("inf".to_owned(), |r| r.to_f64().to_string());
        format!("{lo_b}{lo}, {hi}{hi_b}")
    }
}

/// Per-region interval summary from the And-spine of the
/// over-projections of its statements.
#[derive(Debug, Clone, Default)]
pub struct IntervalMap {
    pub by_quantity: BTreeMap<QuantityId, Iv>,
    /// `False` sat directly on the And-spine: the over-projection is
    /// unsatisfiable outright.
    pub falsified: bool,
}

impl IntervalMap {
    pub fn add_over(&mut self, f: &QFormula) {
        self.spine(f);
    }

    fn spine(&mut self, f: &QFormula) {
        match f {
            QFormula::True => {}
            QFormula::False => self.falsified = true,
            QFormula::And(v) => {
                for p in v {
                    self.spine(p);
                }
            }
            // Disjunctive structure leaves the spine; ignoring it is
            // sound (we only ever DROP necessary conditions).
            QFormula::Or(_) => {}
            QFormula::Atom(a) => {
                let [(c, q)] = a.terms() else { return };
                if c.is_zero() {
                    return;
                }
                // `c·q ⋈ k` ⇒ `q (⋈ or ⋈̄) k/c`, EXACT over rationals — the
                // bound is the precise rational `k/c`, so no rounding can
                // exclude a valid point (the old f64 nudge/subnormal guards are
                // gone). The relation flips when `c < 0`.
                let Some(bound) = a.constant().checked_div(c) else {
                    return;
                };
                let rel = if c.is_negative() {
                    a.rel().flipped()
                } else {
                    a.rel()
                };
                let iv = self.by_quantity.entry(*q).or_default();
                match rel {
                    Rel::Lt => iv.tighten_hi(bound, true),
                    Rel::Le => iv.tighten_hi(bound, false),
                    Rel::Gt => iv.tighten_lo(bound, true),
                    Rel::Ge => iv.tighten_lo(bound, false),
                    Rel::Eq => {
                        iv.tighten_lo(bound.clone(), false);
                        iv.tighten_hi(bound, false);
                    }
                    Rel::Ne => {}
                }
            }
        }
    }

    /// Is the region's own And-spine unsatisfiable?
    #[must_use]
    pub fn self_empty(&self) -> Option<String> {
        if self.falsified {
            return Some("a cut is constant-false".to_owned());
        }
        self.by_quantity
            .iter()
            .find(|(_, iv)| iv.is_empty())
            .map(|(q, iv)| {
                format!(
                    "quantity {q} constrained to the empty interval {}",
                    iv.human()
                )
            })
    }

    /// First quantity on which the two regions' spines cannot intersect.
    #[must_use]
    pub fn disjoint_with(&self, other: &IntervalMap) -> Option<(QuantityId, Iv, Iv)> {
        for (q, a) in &self.by_quantity {
            if let Some(b) = other.by_quantity.get(q)
                && a.disjoint_from(b)
            {
                return Some((*q, a.clone(), b.clone()));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adl_formula::LinAtom;

    fn atom(q: u32, rel: Rel, k: f64) -> QFormula {
        QFormula::Atom(LinAtom::single(
            QuantityId(q),
            rel,
            Rat::from_decimal_f64(k).unwrap(),
        ))
    }

    #[test]
    fn spine_intervals_and_disjointness() {
        let mut a = IntervalMap::default();
        a.add_over(&QFormula::And(vec![
            atom(0, Rel::Gt, 100.0),
            atom(0, Rel::Lt, 200.0),
        ]));
        let mut b = IntervalMap::default();
        b.add_over(&atom(0, Rel::Gt, 300.0));
        assert!(a.disjoint_with(&b).is_some());
        assert!(a.self_empty().is_none());

        // Touching closed bounds intersect; strict ones do not.
        let mut c = IntervalMap::default();
        c.add_over(&atom(0, Rel::Ge, 200.0));
        assert!(a.disjoint_with(&c).is_some(), "a is strict at 200");
        let mut d = IntervalMap::default();
        d.add_over(&atom(0, Rel::Le, 100.0));
        assert!(a.disjoint_with(&d).is_some(), "a is strict at 100");
    }

    #[test]
    fn or_branches_are_ignored_soundly() {
        let mut a = IntervalMap::default();
        a.add_over(&QFormula::Or(vec![
            atom(0, Rel::Lt, 100.0),
            atom(0, Rel::Gt, 500.0),
        ]));
        assert!(a.by_quantity.is_empty(), "Or contributes nothing");
    }

    #[test]
    fn negative_coefficient_flips() {
        // -2 q <= -400  ⇔  q >= 200
        let mut a = IntervalMap::default();
        a.add_over(&QFormula::Atom(LinAtom::new(
            [(Rat::from_i64(-2), QuantityId(0))],
            Rel::Le,
            Rat::from_i64(-400),
        )));
        let iv = &a.by_quantity[&QuantityId(0)];
        assert_eq!((iv.lo.clone(), iv.lo_strict), (Some(Rat::from_i64(200)), false));
    }

    #[test]
    fn self_empty_detection() {
        let mut a = IntervalMap::default();
        a.add_over(&QFormula::And(vec![
            atom(0, Rel::Gt, 5.0),
            atom(0, Rel::Lt, 5.0),
        ]));
        assert!(a.self_empty().is_some());
    }
}
