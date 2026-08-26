//! The numeric value model shared by the interpreter and the encoder.
//!
//! A finite numeric value is either an **exact rational** ([`Rat`]) or an
//! **approximate** `f64`. Which one it is follows the source of the value,
//! not the syntax:
//!
//! - `Exact` — numeric literals, event properties, sizes, event scalars,
//!   `MET.pt`, and every `+ - * /` / neg / abs / min / max step whose inputs
//!   are all `Exact`. This is the *rational fragment*: the interpreter
//!   computes it with no rounding at all.
//! - `Approx` — genuinely irrational or trig-derived values (`sqrt`, `dR` /
//!   `dPhi` / `dEta`, Lorentz-vector kinematics), `^` (fractional powers are
//!   irrational and overflow is part of the §4.4 contract), and any step
//!   with an `Approx` input.
//!
//! **Why this lives in `adl-sema` and not in the interpreter.** The analyzer
//! proves a region pair disjoint by showing its encoding is UNSAT. That is
//! only sound if the encoded cut boundary is the boundary the interpreter
//! actually decides membership on. So `adl-formula`'s constant folding must
//! reproduce these rules leaf for leaf — one implementation, used by both,
//! is the only way to keep them from drifting apart (they did drift, and it
//! fabricated a PROVEN DISJOINT: see the M4 entry in COUNTEREXAMPLES.md).

use crate::hir::ArithOp;
use crate::rat::Rat;

/// A finite numeric value: exact rational or approximate `f64`.
#[derive(Debug, Clone, PartialEq)]
pub enum NumVal {
    Exact(Rat),
    Approx(f64),
}

impl NumVal {
    /// The `f64` reading of the value (lossy for `Exact`). This is the
    /// conversion the interpreter applies at a *mixed* comparison edge.
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        match self {
            NumVal::Exact(r) => r.to_f64(),
            NumVal::Approx(v) => *v,
        }
    }

    /// Wrap a finite `f64` as `Approx`; `None` for NaN/±inf (§4.4: a
    /// non-finite intermediate makes the enclosing comparison false).
    #[must_use]
    pub fn from_f64(v: f64) -> Option<Self> {
        v.is_finite().then_some(NumVal::Approx(v))
    }

    #[must_use]
    pub fn is_exact(&self) -> bool {
        matches!(self, NumVal::Exact(_))
    }

    #[must_use]
    pub fn negated(self) -> Self {
        match self {
            NumVal::Exact(r) => NumVal::Exact(-&r),
            NumVal::Approx(v) => NumVal::Approx(-v),
        }
    }

    #[must_use]
    pub fn abs(self) -> Self {
        match self {
            NumVal::Exact(r) => NumVal::Exact(r.abs()),
            NumVal::Approx(v) => NumVal::Approx(v.abs()),
        }
    }

    #[must_use]
    pub fn is_nonzero(&self) -> bool {
        match self {
            NumVal::Exact(r) => !r.is_zero(),
            NumVal::Approx(v) => *v != 0.0,
        }
    }
}

/// `min` of two values: exact when both are, else the `f64` minimum.
#[must_use]
pub fn num_min(a: NumVal, b: NumVal) -> NumVal {
    match (&a, &b) {
        (NumVal::Exact(x), NumVal::Exact(y)) => {
            NumVal::Exact(if x <= y { x.clone() } else { y.clone() })
        }
        _ => NumVal::Approx(a.to_f64().min(b.to_f64())),
    }
}

/// `max` of two values: exact when both are, else the `f64` maximum.
#[must_use]
pub fn num_max(a: NumVal, b: NumVal) -> NumVal {
    match (&a, &b) {
        (NumVal::Exact(x), NumVal::Exact(y)) => {
            NumVal::Exact(if x >= y { x.clone() } else { y.clone() })
        }
        _ => NumVal::Approx(a.to_f64().max(b.to_f64())),
    }
}

/// One binary arithmetic step. `None` is the §4.4 non-value (division by
/// zero, or an `f64` step that went non-finite) — the caller must make the
/// enclosing comparison false, never substitute a default.
///
/// `^` always leaves the rational fragment: fractional exponents are
/// irrational, and `HT ^ HT` overflowing to `inf` is pinned §4.4 behaviour.
#[must_use]
pub fn bin_arith(op: ArithOp, a: NumVal, b: NumVal) -> Option<NumVal> {
    use ArithOp::{Add, Div, Mul, Pow, Sub};
    match op {
        Pow => NumVal::from_f64(a.to_f64().powf(b.to_f64())),
        Add | Sub | Mul | Div => match (&a, &b) {
            (NumVal::Exact(x), NumVal::Exact(y)) => match op {
                Add => Some(NumVal::Exact(x + y)),
                Sub => Some(NumVal::Exact(x - y)),
                Mul => Some(NumVal::Exact(x * y)),
                Div => x.checked_div(y).map(NumVal::Exact),
                Pow => unreachable!("Pow handled above"),
            },
            // Mixed Exact/Approx (or both Approx): the f64 path.
            _ => {
                let (af, bf) = (a.to_f64(), b.to_f64());
                let v = match op {
                    Add => af + bf,
                    Sub => af - bf,
                    Mul => af * bf,
                    Div => af / bf,
                    Pow => unreachable!("Pow handled above"),
                };
                NumVal::from_f64(v)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(v: f64) -> NumVal {
        NumVal::Exact(Rat::from_decimal_f64(v).unwrap())
    }

    #[test]
    fn exact_addition_is_the_decimal_sum_not_the_f64_sum() {
        // The whole point: 0.1 + 0.2 is 3/10, not 0.30000000000000004.
        let s = bin_arith(ArithOp::Add, ex(0.1), ex(0.2)).unwrap();
        assert_eq!(s, NumVal::Exact(Rat::from_ratio(3, 10).unwrap()));
        assert_ne!(s, ex(0.1 + 0.2));
    }

    #[test]
    fn one_approx_input_poisons_the_step_to_f64() {
        let s = bin_arith(ArithOp::Add, NumVal::Approx(0.1), ex(0.2)).unwrap();
        assert_eq!(s, NumVal::Approx(0.1 + 0.2));
    }

    #[test]
    fn pow_always_leaves_the_rational_fragment() {
        let p = bin_arith(ArithOp::Pow, ex(2.0), ex(0.5)).unwrap();
        assert_eq!(p, NumVal::Approx(2.0_f64.sqrt()));
        // Overflow to inf is the §4.4 non-value.
        assert!(bin_arith(ArithOp::Pow, ex(210.0), ex(210.0)).is_none());
    }

    #[test]
    fn division_by_zero_is_a_non_value_on_both_paths() {
        assert!(bin_arith(ArithOp::Div, ex(1.0), ex(0.0)).is_none());
        assert!(bin_arith(ArithOp::Div, NumVal::Approx(1.0), ex(0.0)).is_none());
    }

    #[test]
    fn min_max_stay_exact_only_when_both_sides_are() {
        assert!(num_min(ex(1.0), ex(2.0)).is_exact());
        assert!(!num_min(ex(1.0), NumVal::Approx(2.0)).is_exact());
    }
}
