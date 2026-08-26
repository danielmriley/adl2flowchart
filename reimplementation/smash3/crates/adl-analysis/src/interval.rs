//! Interval fast path (SPEC_ANALYSIS §2): a cheap, sound heuristic over
//! the **unconditional And-spine** of a region's over-projection. Only
//! single-quantity atoms reachable without crossing an `Or` contribute —
//! such atoms are necessary conditions of R⁺ ⊇ R, so an empty
//! intersection proves disjointness without a solver (this is also the
//! no-solver fallback; everything it cannot prove stays POSSIBLY).
//!
//! # Bound provenance (why every bound remembers where it came from)
//!
//! An interval refutation is a Farkas proof in miniature: a lower bound and
//! an upper bound on one quantity, combined with multipliers `1/|c|`. To hand
//! that proof to [`adl_certify`] the engine needs the *atoms*, not just the
//! numbers — which assert carried them, and in what exact form. So each
//! [`Bound`] keeps the [`AssertName`] of the assert whose over-projection
//! carries it and the [`LinAtom`] itself, and every refutation reports the
//! [`RefutingPart`]s that produced it. Without this the fast path could only
//! say "these ranges do not meet" and its PROVEN verdicts would be the one
//! tier of the tool with no machine-checkable receipt.

use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};
use adl_solver::AssertName;
use std::collections::BTreeMap;

/// One side of an interval, with the atom that set it.
#[derive(Debug, Clone, PartialEq)]
pub struct Bound {
    /// The bound value, the exact rational `k/c`.
    pub value: Rat,
    /// `true` for a strict (`<` / `>`) bound.
    pub strict: bool,
    /// The assert whose over-projection carries `atom` on its And-spine.
    pub src: AssertName,
    /// The originating atom `c·q ⋈ k`, a top-level conjunct of that assert.
    pub atom: LinAtom,
}

/// A named formula (or one conjunct of it) participating in an interval
/// refutation — what the engine hands to the certifier.
#[derive(Debug, Clone, PartialEq)]
pub enum RefutingPart {
    /// The whole over-projection of this assert (it carries a constant
    /// `false`, so no single atom is the culprit).
    Whole(AssertName),
    /// One spine conjunct of the named assert.
    Conjunct(AssertName, LinAtom),
}

impl RefutingPart {
    /// The assert this part belongs to.
    #[must_use]
    pub fn src(&self) -> &AssertName {
        match self {
            RefutingPart::Whole(n) | RefutingPart::Conjunct(n, _) => n,
        }
    }
}

/// A (possibly open) interval constraint on one quantity, over exact
/// rationals. Absent bounds are `±∞`. Bounds are EXACT (`k/c` as a rational),
/// so the interval fast path is a precise — not merely sound — summary of the
/// single-quantity atoms on the spine; no f64 rounding can shift a boundary.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Iv {
    pub lo: Option<Bound>,
    pub hi: Option<Bound>,
}

impl Iv {
    fn tighten_lo(&mut self, b: Bound) {
        let better = match &self.lo {
            None => true,
            Some(cur) => b.value > cur.value || (b.value == cur.value && b.strict && !cur.strict),
        };
        if better {
            self.lo = Some(b);
        }
    }

    fn tighten_hi(&mut self, b: Bound) {
        let better = match &self.hi {
            None => true,
            Some(cur) => b.value < cur.value || (b.value == cur.value && b.strict && !cur.strict),
        };
        if better {
            self.hi = Some(b);
        }
    }

    /// The tightest lower and upper bound across `self` and `other` when the
    /// two cannot both hold — i.e. the pair of atoms that refutes. `None` when
    /// some point satisfies both. Ties on value prefer the strict bound (it is
    /// the tighter one, and the one that makes the combination strict).
    #[must_use]
    pub fn refutation<'a>(&'a self, other: &'a Iv) -> Option<(&'a Bound, &'a Bound)> {
        let lo = tightest(self.lo.as_ref(), other.lo.as_ref(), true)?;
        let hi = tightest(self.hi.as_ref(), other.hi.as_ref(), false)?;
        let empty = lo.value > hi.value || (lo.value == hi.value && (lo.strict || hi.strict));
        empty.then_some((lo, hi))
    }

    /// Is the interval itself empty?
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.refutation(self).is_some()
    }

    /// Do `self` and `other` fail to intersect?
    #[must_use]
    pub fn disjoint_from(&self, other: &Iv) -> bool {
        self.refutation(other).is_some()
    }

    #[must_use]
    pub fn human(&self) -> String {
        let lo_b = if self.lo.as_ref().is_some_and(|b| b.strict) { "(" } else { "[" };
        let hi_b = if self.hi.as_ref().is_some_and(|b| b.strict) { ")" } else { "]" };
        let lo = self
            .lo
            .as_ref()
            .map_or("-inf".to_owned(), |b| b.value.to_f64().to_string());
        let hi = self
            .hi
            .as_ref()
            .map_or("inf".to_owned(), |b| b.value.to_f64().to_string());
        format!("{lo_b}{lo}, {hi}{hi_b}")
    }
}

/// The tighter of two optional bounds: the greater value for a lower bound
/// (`lower == true`), the lesser for an upper bound. `None` is the weakest
/// (∓∞), so a present bound always wins over an absent one.
fn tightest<'a>(a: Option<&'a Bound>, b: Option<&'a Bound>, lower: bool) -> Option<&'a Bound> {
    match (a, b) {
        (None, None) => None,
        (Some(x), None) | (None, Some(x)) => Some(x),
        (Some(x), Some(y)) => {
            let strictly_tighter = if lower {
                x.value > y.value
            } else {
                x.value < y.value
            };
            // On a tie, the strict bound is the tighter one — and the one that
            // makes the Farkas combination strict, which is what refutes a
            // touching pair like `q > 2` vs `q <= 2`.
            let wins = strictly_tighter || (x.value == y.value && x.strict && !y.strict);
            Some(if wins { x } else { y })
        }
    }
}

/// Why a region's own spine is unsatisfiable.
#[derive(Debug, Clone, PartialEq)]
pub enum SelfEmpty {
    /// A cut is constant-false.
    ConstFalse(AssertName),
    /// One quantity is pinned to an empty interval. Boxed: the interval
    /// carries both bounds' atoms, dwarfing the other variant.
    EmptyInterval(QuantityId, Box<Iv>),
}

impl SelfEmpty {
    /// The reason, as it appears in the report.
    #[must_use]
    pub fn human(&self) -> String {
        match self {
            SelfEmpty::ConstFalse(_) => "a cut is constant-false".to_owned(),
            SelfEmpty::EmptyInterval(q, iv) => format!(
                "quantity {q} constrained to the empty interval {}",
                iv.human()
            ),
        }
    }

    /// The named formulas (or conjuncts) whose conjunction is unsatisfiable.
    #[must_use]
    pub fn parts(&self) -> Vec<RefutingPart> {
        match self {
            SelfEmpty::ConstFalse(n) => vec![RefutingPart::Whole(n.clone())],
            SelfEmpty::EmptyInterval(_, iv) => iv
                .refutation(iv)
                .map(|(lo, hi)| vec![part(lo), part(hi)])
                .unwrap_or_default(),
        }
    }
}

fn part(b: &Bound) -> RefutingPart {
    RefutingPart::Conjunct(b.src.clone(), b.atom.clone())
}

/// Two regions' spines cannot intersect on one quantity.
#[derive(Debug, Clone, PartialEq)]
pub struct IntervalDisjoint {
    /// The quantity carrying the contradiction.
    pub q: QuantityId,
    /// The first region's interval on `q`.
    pub a: Iv,
    /// The second region's interval on `q`.
    pub b: Iv,
    /// The two bound atoms that refute, in (lower, upper) order.
    pub parts: Vec<RefutingPart>,
}

/// What the caller knows about a quantity's definedness, for
/// [`IntervalMap::disjoint_with`]'s belt-and-braces check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Defined at every loader-valid event — no indicator exists or is
    /// needed, so an interval refutation over it stands on its own.
    Total,
    /// Possibly absent; `defined(q)` is this id, and BOTH regions must pin
    /// it present for the refutation to be about a value that exists.
    Indicator(QuantityId),
    /// Possibly absent and no indicator was ever interned — so nothing can
    /// have pinned it present. Fail closed.
    Unpinned,
}

/// Per-region interval summary from the And-spine of the
/// over-projections of its statements.
#[derive(Debug, Clone, Default)]
pub struct IntervalMap {
    pub by_quantity: BTreeMap<QuantityId, Iv>,
    /// `False` sat directly on the And-spine of this assert: its
    /// over-projection is unsatisfiable outright.
    pub falsified: Option<AssertName>,
}

impl IntervalMap {
    /// Fold an over-projection's And-spine into the map, attributing every
    /// bound it sets to `src`.
    ///
    /// Takes [`Over`] — not a raw [`QFormula`] — so the type system, not a
    /// naming convention, guarantees only supersets ever feed the interval
    /// layer. `Under::qformula()` has an identical signature; before this
    /// took `Over`, passing an under here would have compiled and silently
    /// inverted the polarity of the path that decides the majority of
    /// PROVEN DISJOINT verdicts.
    pub fn add_over(&mut self, src: &AssertName, o: &adl_formula::Over) {
        self.spine(src, o.qformula());
    }

    fn spine(&mut self, src: &AssertName, f: &QFormula) {
        match f {
            QFormula::True => {}
            QFormula::False => {
                if self.falsified.is_none() {
                    self.falsified = Some(src.clone());
                }
            }
            QFormula::And(v) => {
                for p in v {
                    self.spine(src, p);
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
                let mk = |value: Rat, strict: bool| Bound {
                    value,
                    strict,
                    src: src.clone(),
                    atom: a.clone(),
                };
                let iv = self.by_quantity.entry(*q).or_default();
                match rel {
                    Rel::Lt => iv.tighten_hi(mk(bound, true)),
                    Rel::Le => iv.tighten_hi(mk(bound, false)),
                    Rel::Gt => iv.tighten_lo(mk(bound, true)),
                    Rel::Ge => iv.tighten_lo(mk(bound, false)),
                    Rel::Eq => {
                        iv.tighten_lo(mk(bound.clone(), false));
                        iv.tighten_hi(mk(bound, false));
                    }
                    Rel::Ne => {}
                }
            }
        }
    }

    /// Is the region's own And-spine unsatisfiable?
    #[must_use]
    pub fn self_empty(&self) -> Option<SelfEmpty> {
        if let Some(n) = &self.falsified {
            return Some(SelfEmpty::ConstFalse(n.clone()));
        }
        self.by_quantity
            .iter()
            .find(|(_, iv)| iv.is_empty())
            .map(|(q, iv)| SelfEmpty::EmptyInterval(*q, Box::new(iv.clone())))
    }

    /// Does this map pin `q` PRESENT — i.e. does the spine carry
    /// `defined(q) >= 1`?
    #[must_use]
    fn pins_present(&self, present_id: QuantityId) -> bool {
        self.by_quantity
            .get(&present_id)
            .and_then(|iv| iv.lo.as_ref())
            .is_some_and(|b| b.value >= Rat::one())
    }

    /// First quantity on which the two regions' spines cannot intersect.
    ///
    /// # The presence check (belt and braces)
    ///
    /// This path has the NARROWEST premise set in the engine — no axiom, no
    /// solver, no certificate beyond the two bounds themselves — and it is
    /// where the complementary-reject false PROVEN DISJOINT shipped: two
    /// regions that both accept a property-less event, each recording a
    /// bound on the SAME junk value, "refuting" each other.
    ///
    /// Invariant E-i says that can no longer happen — a `q`-bound reaches the
    /// spine only conjoined with `defined(q) >= 1`, and a region that does
    /// not commit to presence contributes no `q`-bound at all (a negated leaf
    /// is a disjunction, which the spine skips). This turns E-i from a
    /// code-review property into a RUNTIME check on exactly that path: if a
    /// possibly-absent quantity's bounds refute but either side fails to pin
    /// it present, refuse the shortcut and let the solver decide.
    ///
    /// It costs nothing in precision — by the argument above the failing side
    /// has no bound to refute with — and everything it refuses is still
    /// reachable through the solver.
    #[must_use]
    pub fn disjoint_with(
        &self,
        other: &IntervalMap,
        presence: &dyn Fn(QuantityId) -> Presence,
    ) -> Option<IntervalDisjoint> {
        for (q, a) in &self.by_quantity {
            let Some(b) = other.by_quantity.get(q) else {
                continue;
            };
            let Some((lo, hi)) = a.refutation(b) else {
                continue;
            };
            match presence(*q) {
                Presence::Total => {}
                Presence::Indicator(p) => {
                    if !(self.pins_present(p) && other.pins_present(p)) {
                        continue;
                    }
                }
                // Possibly absent with NO interned indicator: nothing in
                // either region can have pinned it present, so E-i is
                // violated somewhere. Refuse — fail closed.
                Presence::Unpinned => continue,
            }
            return Some(IntervalDisjoint {
                q: *q,
                a: a.clone(),
                b: b.clone(),
                parts: vec![part(lo), part(hi)],
            });
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adl_formula::{Formula, Over};

    /// Tests must construct a genuine [`Over`] the way the engine does —
    /// through the projection — since `add_over` no longer takes raw
    /// formulas.
    fn over(f: Formula) -> Over {
        f.over()
    }

    fn name(s: &str) -> AssertName {
        AssertName::new(s.to_owned())
    }

    fn atom(q: u32, rel: Rel, k: f64) -> Formula {
        Formula::Atom(LinAtom::single(
            QuantityId(q),
            rel,
            Rat::from_decimal_f64(k).unwrap(),
        ))
    }

    fn map(src: &str, f: Formula) -> IntervalMap {
        let mut m = IntervalMap::default();
        m.add_over(&name(src), &over(f));
        m
    }

    #[test]
    fn spine_intervals_and_disjointness() {
        let a = map(
            "A",
            Formula::And(vec![atom(0, Rel::Gt, 100.0), atom(0, Rel::Lt, 200.0)]),
        );
        let b = map("B", atom(0, Rel::Gt, 300.0));
        assert!(a.disjoint_with(&b, &|_| Presence::Total).is_some());
        assert!(a.self_empty().is_none());

        // Touching closed bounds intersect; strict ones do not.
        let c = map("C", atom(0, Rel::Ge, 200.0));
        assert!(a.disjoint_with(&c, &|_| Presence::Total).is_some(), "a is strict at 200");
        let d = map("D", atom(0, Rel::Le, 100.0));
        assert!(a.disjoint_with(&d, &|_| Presence::Total).is_some(), "a is strict at 100");
    }

    #[test]
    fn refutation_reports_both_originating_atoms() {
        let a = map(
            "A",
            Formula::And(vec![atom(0, Rel::Gt, 100.0), atom(0, Rel::Lt, 200.0)]),
        );
        let b = map("B", atom(0, Rel::Gt, 300.0));
        let d = a.disjoint_with(&b, &|_| Presence::Total).expect("disjoint");
        assert_eq!(d.parts.len(), 2);
        // Lower bound comes from B (300 beats 100), upper bound from A (200).
        assert_eq!(d.parts[0].src(), &name("B"));
        assert_eq!(d.parts[1].src(), &name("A"));
    }

    #[test]
    fn or_branches_are_ignored_soundly() {
        let a = map(
            "A",
            Formula::Or(vec![atom(0, Rel::Lt, 100.0), atom(0, Rel::Gt, 500.0)]),
        );
        assert!(a.by_quantity.is_empty(), "Or contributes nothing");
    }

    #[test]
    fn negative_coefficient_flips() {
        // -2 q <= -400  ⇔  q >= 200
        let a = map(
            "A",
            Formula::Atom(LinAtom::new(
                [(Rat::from_i64(-2), QuantityId(0))],
                Rel::Le,
                Rat::from_i64(-400),
            )),
        );
        let lo = a.by_quantity[&QuantityId(0)].lo.clone().unwrap();
        assert_eq!((lo.value, lo.strict), (Rat::from_i64(200), false));
    }

    #[test]
    fn self_empty_detection_and_provenance() {
        let a = map(
            "A",
            Formula::And(vec![atom(0, Rel::Gt, 5.0), atom(0, Rel::Lt, 5.0)]),
        );
        let e = a.self_empty().expect("empty");
        assert_eq!(e.parts().len(), 2);
        assert!(e.human().starts_with("quantity "));

        let f = map("F", Formula::False);
        let e = f.self_empty().expect("false spine");
        assert_eq!(e.parts(), vec![RefutingPart::Whole(name("F"))]);
        assert_eq!(e.human(), "a cut is constant-false");
    }
}

#[cfg(test)]
mod presence_guard_tests {
    use super::*;
    use adl_formula::Formula;

    fn atom(q: u32, rel: Rel, k: i64) -> Formula {
        Formula::Atom(LinAtom::single(QuantityId(q), rel, Rat::from_i64(k)))
    }

    fn map(src: &str, f: Formula) -> IntervalMap {
        let mut m = IntervalMap::default();
        m.add_over(&AssertName::new(src.to_owned()), &f.over());
        m
    }

    /// Q0 is a possibly-absent property with indicator Q9. Two bounds that
    /// refute on Q0 are accepted only when BOTH regions pin Q9 present —
    /// the runtime form of invariant E-i on the one path with no
    /// certificate and no axiom behind it.
    #[test]
    fn refutation_over_a_possibly_absent_quantity_needs_both_sides_present() {
        let p = |q: QuantityId| {
            if q == QuantityId(0) {
                Presence::Indicator(QuantityId(9))
            } else {
                Presence::Total
            }
        };
        // Both pin presence: accepted.
        let a = map("A", Formula::And(vec![atom(9, Rel::Ge, 1), atom(0, Rel::Gt, 10)]));
        let b = map("B", Formula::And(vec![atom(9, Rel::Ge, 1), atom(0, Rel::Lt, 5)]));
        assert!(a.disjoint_with(&b, &p).is_some());

        // One side does not commit to presence: refused, even though the
        // numeric bounds still fail to intersect.
        let c = map("C", atom(0, Rel::Lt, 5));
        assert!(
            a.disjoint_with(&c, &p).is_none(),
            "a bound on a quantity nobody proved present must not refute"
        );

        // No indicator interned at all: fail closed.
        assert!(
            a.disjoint_with(&b, &|_| Presence::Unpinned).is_none(),
            "an unpinnable possibly-absent quantity must refuse the shortcut"
        );

        // A total quantity is unaffected.
        let d = map("D", atom(1, Rel::Gt, 10));
        let e = map("E", atom(1, Rel::Lt, 5));
        assert!(d.disjoint_with(&e, &p).is_some());
    }
}
