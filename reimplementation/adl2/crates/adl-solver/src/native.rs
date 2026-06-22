//! PRIMARY backend: native libz3 via the `z3` crate (ADR-006).
//!
//! Typed term construction makes malformed input unrepresentable; models
//! and unsat cores come back through the API, not a text protocol — the
//! whole legacy echo-tag/dropped-assert hazard class (audit Bug 5 layer
//! 2) does not exist here.

use crate::{AssertName, Model, QSort, SatResult, Solver};
use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};
use std::collections::BTreeMap;
use std::time::Duration;
use z3::ast::{Bool, Int, Real};

enum Var {
    R(Real),
    I(Int),
}

impl Var {
    fn as_real(&self) -> Real {
        match self {
            Var::R(r) => r.clone(),
            Var::I(i) => i.to_real(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastCheck {
    None,
    Sat,
    Unsat,
    Unknown,
}

/// Native z3 solver. Uses the crate's thread-local context; one
/// `NativeSolver` per analysis (and per thread).
pub struct NativeSolver {
    solver: z3::Solver,
    vars: BTreeMap<QuantityId, (QSort, Var)>,
    /// Tracker literals for named assertions, per push-frame.
    track_frames: Vec<Vec<(Bool, AssertName)>>,
    tracker_seq: u32,
    last: LastCheck,
}

impl Default for NativeSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeSolver {
    #[must_use]
    pub fn new() -> Self {
        Self {
            solver: z3::Solver::new(),
            vars: BTreeMap::new(),
            track_frames: vec![Vec::new()],
            tracker_seq: 0,
            last: LastCheck::None,
        }
    }

    fn var(&mut self, q: QuantityId, sort: QSort) -> &Var {
        let entry = self.vars.entry(q).or_insert_with(|| {
            let name = format!("q{}", q.0);
            let v = match sort {
                QSort::Real => Var::R(Real::new_const(name)),
                QSort::Int => Var::I(Int::new_const(name)),
            };
            (sort, v)
        });
        &entry.1
    }

    fn real_of(r: &Rat) -> Real {
        let p = r.to_parts();
        let num = if p.negative {
            format!("-{}", p.numerator)
        } else {
            p.numerator
        };
        Real::from_rational_str(&num, &p.denominator)
            .expect("decimal rational strings are valid z3 numerals")
    }

    fn atom_term(&mut self, a: &LinAtom) -> Bool {
        let mut sum: Option<Real> = None;
        for (c, q) in a.terms() {
            let var = self.var(*q, QSort::Real).as_real();
            let term = if c.is_one() {
                var
            } else {
                Real::mul(&[Self::real_of(c), var])
            };
            sum = Some(match sum {
                None => term,
                Some(s) => Real::add(&[s, term]),
            });
        }
        let lhs = sum.unwrap_or_else(|| Self::real_of(&Rat::zero()));
        let rhs = Self::real_of(a.constant());
        match a.rel() {
            Rel::Lt => lhs.lt(&rhs),
            Rel::Le => lhs.le(&rhs),
            Rel::Gt => lhs.gt(&rhs),
            Rel::Ge => lhs.ge(&rhs),
            Rel::Eq => lhs.safe_eq(&rhs).expect("same-sort equality"),
            Rel::Ne => lhs.safe_eq(&rhs).expect("same-sort equality").not(),
        }
    }

    fn term(&mut self, f: &QFormula) -> Bool {
        match f {
            QFormula::True => Bool::from_bool(true),
            QFormula::False => Bool::from_bool(false),
            QFormula::Atom(a) => self.atom_term(a),
            QFormula::And(v) => {
                let parts: Vec<Bool> = v.iter().map(|p| self.term(p)).collect();
                Bool::and(&parts)
            }
            QFormula::Or(v) => {
                let parts: Vec<Bool> = v.iter().map(|p| self.term(p)).collect();
                Bool::or(&parts)
            }
        }
    }
}

impl Solver for NativeSolver {
    fn declare(&mut self, q: QuantityId, sort: QSort) {
        let _ = self.var(q, sort);
    }

    fn push(&mut self) {
        self.solver.push();
        self.track_frames.push(Vec::new());
        self.last = LastCheck::None;
    }

    fn pop(&mut self) {
        self.solver.pop(1);
        if self.track_frames.len() > 1 {
            self.track_frames.pop();
        }
        self.last = LastCheck::None;
    }

    fn assert(&mut self, f: &QFormula, name: Option<AssertName>) {
        let term = self.term(f);
        match name {
            None => self.solver.assert(&term),
            Some(n) => {
                self.tracker_seq += 1;
                let tracker = Bool::new_const(format!("track!{}", self.tracker_seq));
                self.solver.assert_and_track(term, &tracker);
                self.track_frames
                    .last_mut()
                    .expect("base frame always present")
                    .push((tracker, n));
            }
        }
        self.last = LastCheck::None;
    }

    fn check(&mut self, timeout: Duration) -> SatResult {
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        let mut params = z3::Params::new();
        params.set_u32("timeout", ms.max(1));
        self.solver.set_params(&params);
        match self.solver.check() {
            z3::SatResult::Sat => {
                self.last = LastCheck::Sat;
                SatResult::Sat
            }
            z3::SatResult::Unsat => {
                self.last = LastCheck::Unsat;
                SatResult::Unsat
            }
            z3::SatResult::Unknown => {
                self.last = LastCheck::Unknown;
                SatResult::Unknown(
                    self.solver
                        .get_reason_unknown()
                        .unwrap_or_else(|| "unknown".to_owned()),
                )
            }
        }
    }

    fn model(&mut self) -> Option<Model> {
        if self.last != LastCheck::Sat {
            return None;
        }
        let model = self.solver.get_model()?;
        let mut values = BTreeMap::new();
        for (&q, (sort, var)) in &self.vars {
            let v = match (sort, var) {
                // Prefer the exact rational: `approx_f64` goes through a
                // truncated decimal string and can be several ulps off,
                // which breaks bit-exact witness re-evaluation. Integer
                // → f64 conversion is exact below 2^53 and f64 division
                // is correctly rounded, so small rationals round-trip
                // perfectly (dyadics exactly).
                #[allow(clippy::cast_precision_loss)] // fallback below 2^53 is exact
                (QSort::Real, Var::R(r)) => model.eval(r, true).map(|x| match x.as_rational() {
                    Some((n, d)) if d != 0 && n.abs() < (1i64 << 53) && d.abs() < (1i64 << 53) => {
                        n as f64 / d as f64
                    }
                    _ => x.approx_f64(),
                }),
                (QSort::Int, Var::I(i)) => model
                    .eval(i, true)
                    .and_then(|x| x.as_i64())
                    .map(|x| x as f64),
                // Sort/var mismatch cannot happen by construction.
                _ => None,
            };
            if let Some(v) = v {
                values.insert(q, v);
            }
        }
        Some(Model::from_values(values))
    }

    fn unsat_core(&mut self) -> Option<Vec<AssertName>> {
        if self.last != LastCheck::Unsat {
            return None;
        }
        let core = self.solver.get_unsat_core();
        let mut names = Vec::new();
        for tracked in &core {
            for frame in &self.track_frames {
                for (tracker, name) in frame {
                    if tracker == tracked {
                        names.push(name.clone());
                    }
                }
            }
        }
        names.sort();
        names.dedup();
        Some(names)
    }

    fn backend_name(&self) -> &'static str {
        "z3-native"
    }
}
