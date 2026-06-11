//! Interval fast path (SPEC_ANALYSIS §2): a cheap, sound heuristic over
//! the **unconditional And-spine** of a region's over-projection. Only
//! single-quantity atoms reachable without crossing an `Or` contribute —
//! such atoms are necessary conditions of R⁺ ⊇ R, so an empty
//! intersection proves disjointness without a solver (this is also the
//! no-solver fallback; everything it cannot prove stays POSSIBLY).

use adl_formula::{QFormula, Rel};
use adl_sema::QuantityId;
use std::collections::BTreeMap;

/// A (possibly open) interval constraint on one quantity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Iv {
    pub lo: f64,
    pub lo_strict: bool,
    pub hi: f64,
    pub hi_strict: bool,
}

impl Default for Iv {
    fn default() -> Self {
        Self {
            lo: f64::NEG_INFINITY,
            lo_strict: false,
            hi: f64::INFINITY,
            hi_strict: false,
        }
    }
}

impl Iv {
    fn tighten_lo(&mut self, v: f64, strict: bool) {
        if v > self.lo || (v == self.lo && strict && !self.lo_strict) {
            self.lo = v;
            self.lo_strict = strict;
        }
    }

    fn tighten_hi(&mut self, v: f64, strict: bool) {
        if v < self.hi || (v == self.hi && strict && !self.hi_strict) {
            self.hi = v;
            self.hi_strict = strict;
        }
    }

    /// Is the interval itself empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lo > self.hi || (self.lo == self.hi && (self.lo_strict || self.hi_strict))
    }

    /// Do `self` and `other` fail to intersect?
    #[must_use]
    pub fn disjoint_from(&self, other: &Iv) -> bool {
        let lo = if self.lo > other.lo {
            (self.lo, self.lo_strict)
        } else if other.lo > self.lo {
            (other.lo, other.lo_strict)
        } else {
            (self.lo, self.lo_strict || other.lo_strict)
        };
        let hi = if self.hi < other.hi {
            (self.hi, self.hi_strict)
        } else if other.hi < self.hi {
            (other.hi, other.hi_strict)
        } else {
            (self.hi, self.hi_strict || other.hi_strict)
        };
        lo.0 > hi.0 || (lo.0 == hi.0 && (lo.1 || hi.1))
    }

    #[must_use]
    pub fn human(&self) -> String {
        let lo_b = if self.lo_strict { "(" } else { "[" };
        let hi_b = if self.hi_strict { ")" } else { "]" };
        format!("{lo_b}{}, {}{hi_b}", self.lo, self.hi)
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
                if *c == 0.0 {
                    return;
                }
                let bound = a.constant() / c;
                if !bound.is_finite() {
                    return;
                }
                let rel = if *c < 0.0 { a.rel().flipped() } else { a.rel() };
                let iv = self.by_quantity.entry(*q).or_default();
                match rel {
                    Rel::Lt => iv.tighten_hi(bound, true),
                    Rel::Le => iv.tighten_hi(bound, false),
                    Rel::Gt => iv.tighten_lo(bound, true),
                    Rel::Ge => iv.tighten_lo(bound, false),
                    Rel::Eq => {
                        iv.tighten_lo(bound, false);
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
                return Some((*q, *a, *b));
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
        QFormula::Atom(LinAtom::single(QuantityId(q), rel, k).unwrap())
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
        a.add_over(&QFormula::Atom(
            LinAtom::new([(-2.0, QuantityId(0))], Rel::Le, -400.0).unwrap(),
        ));
        let iv = a.by_quantity[&QuantityId(0)];
        assert_eq!((iv.lo, iv.lo_strict), (200.0, false));
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
