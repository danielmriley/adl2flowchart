//! Exact rational arithmetic for the cut/atom numeric core.
//!
//! A numeric literal or event value denotes its **shortest round-trip
//! decimal** as an exact rational — `0.3` is `3/10`, exactly the value the
//! physicist wrote and exactly what the solver consumes. Folding cut
//! arithmetic (`+ - * /`) over [`Rat`] is therefore exact, so the analyzer
//! (which encodes to `Rat` atoms) and the interpreter (which evaluates the
//! rational fragment over `Rat`) agree to the bit — no f64 rounding can open
//! a boundary gap that fabricates a false PROVEN.
//!
//! Irrational operations (`sqrt`, angular separations, opaque functions) have
//! no rational value; callers keep those in `f64` (the analyzer already treats
//! them as opaque, so no PROVEN verdict rests on them).

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

/// An exact rational. Decimal-literal semantics: [`Rat::from_decimal_f64`]
/// reads the shortest round-trip decimal of the `f64`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Rat(BigRational);

impl Default for Rat {
    fn default() -> Self {
        Rat::zero()
    }
}

impl Rat {
    #[must_use]
    pub fn zero() -> Self {
        Rat(BigRational::zero())
    }

    #[must_use]
    pub fn one() -> Self {
        Rat(BigRational::one())
    }

    #[must_use]
    pub fn from_i64(n: i64) -> Self {
        Rat(BigRational::from_integer(BigInt::from(n)))
    }

    /// The exact rational of a finite `f64`, read as its **shortest
    /// round-trip decimal** (`0.3 → 3/10`, `100.0 → 100`, `-1.5 → -3/2`).
    /// Returns `None` for a non-finite `f64` — non-finite values cannot
    /// construct atoms (SPEC_ANALYSIS §1 / §4.4).
    #[must_use]
    pub fn from_decimal_f64(v: f64) -> Option<Self> {
        if !v.is_finite() {
            return None;
        }
        // Rust's `Display` for f64 is the shortest round-trip decimal and
        // never uses scientific notation, so `int[.frac]` parsing is total.
        let s = format!("{v}");
        let (negative, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.as_str()),
        };
        let (int_part, frac_part) = digits.split_once('.').unwrap_or((digits, ""));
        let mut numer: BigInt = format!("{int_part}{frac_part}").parse().ok()?;
        if negative {
            numer = -numer;
        }
        let denom: BigInt = num_traits::pow(BigInt::from(10), frac_part.len());
        Some(Rat(BigRational::new(numer, denom)))
    }

    #[must_use]
    pub fn checked_div(&self, other: &Rat) -> Option<Rat> {
        if other.0.is_zero() {
            return None;
        }
        Some(Rat(&self.0 / &other.0))
    }

    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }

    #[must_use]
    pub fn is_negative(&self) -> bool {
        self.0.is_negative()
    }

    #[must_use]
    pub fn is_positive(&self) -> bool {
        self.0.is_positive()
    }

    /// Absolute value.
    #[must_use]
    pub fn abs(&self) -> Rat {
        Rat(self.0.abs())
    }

    /// `-1 | 0 | 1` — the sign, for relation evaluation.
    #[must_use]
    pub fn signum(&self) -> i32 {
        if self.0.is_positive() {
            1
        } else if self.0.is_negative() {
            -1
        } else {
            0
        }
    }

    /// Nearest `f64` (for witness display / JSON / histogram values). Lossy by
    /// design — never used on a soundness path.
    #[must_use]
    pub fn to_f64(&self) -> f64 {
        self.0.to_f64().unwrap_or(f64::NAN)
    }

    /// Is this rational exactly an integer? (Sizes, counts, tag bits.)
    #[must_use]
    pub fn is_integer(&self) -> bool {
        self.0.is_integer()
    }

    /// Greatest integer `<= self`, as a `Rat`.
    #[must_use]
    pub fn floor(&self) -> Rat {
        Rat(self.0.floor())
    }

    /// Least integer `>= self`, as a `Rat`.
    #[must_use]
    pub fn ceil(&self) -> Rat {
        Rat(self.0.ceil())
    }

    /// Exact integer value, if this rational is an integer in `i64` range.
    #[must_use]
    pub fn to_i64(&self) -> Option<i64> {
        if self.0.is_integer() {
            self.0.to_i64()
        } else {
            None
        }
    }

    /// Exact integer power `self^n`. `None` for `0^(negative)` (the only
    /// non-finite case). A non-integer exponent is not representable as a
    /// rational and is handled by the caller (the power stays symbolic).
    #[must_use]
    pub fn powi(&self, n: i32) -> Option<Rat> {
        if n >= 0 {
            #[allow(clippy::cast_sign_loss)]
            Some(Rat(num_traits::pow(self.0.clone(), n as usize)))
        } else if self.0.is_zero() {
            None
        } else {
            #[allow(clippy::cast_sign_loss)]
            let p: BigRational = num_traits::pow(self.0.clone(), (-n) as usize);
            Some(Rat(p.recip()))
        }
    }

    /// Is this exactly `1`? (Common coefficient, emitted bare.)
    #[must_use]
    pub fn is_one(&self) -> bool {
        self.0.is_one()
    }

    /// Render as a closed SMT-LIB2 `Real` term (exact: `(/ n.0 d.0)`).
    #[must_use]
    pub fn smt_real(&self) -> String {
        let p = self.to_parts();
        let mag = if p.denominator == "1" {
            format!("{}.0", p.numerator)
        } else {
            format!("(/ {}.0 {}.0)", p.numerator, p.denominator)
        };
        if p.negative {
            format!("(- {mag})")
        } else {
            mag
        }
    }

    /// The numerator / denominator as decimal digit strings (non-negative)
    /// plus a sign, for emitting an SMT-LIB `Real` or a z3 numeral. The
    /// fraction is already in lowest terms (`BigRational` normalizes).
    #[must_use]
    pub fn to_parts(&self) -> RatParts {
        RatParts {
            negative: self.0.numer().is_negative(),
            numerator: self.0.numer().magnitude().to_string(),
            denominator: self.0.denom().to_string(),
        }
    }
}

/// Sign + numerator/denominator decimal strings (denominator > 0, lowest
/// terms) for solver numeral emission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RatParts {
    pub negative: bool,
    pub numerator: String,
    pub denominator: String,
}

impl std::ops::Add for &Rat {
    type Output = Rat;
    fn add(self, rhs: &Rat) -> Rat {
        Rat(&self.0 + &rhs.0)
    }
}

impl std::ops::Sub for &Rat {
    type Output = Rat;
    fn sub(self, rhs: &Rat) -> Rat {
        Rat(&self.0 - &rhs.0)
    }
}

impl std::ops::Mul for &Rat {
    type Output = Rat;
    fn mul(self, rhs: &Rat) -> Rat {
        Rat(&self.0 * &rhs.0)
    }
}

impl std::ops::Neg for &Rat {
    type Output = Rat;
    fn neg(self) -> Rat {
        Rat(-&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_literals_are_exact() {
        assert_eq!(Rat::from_decimal_f64(0.3).unwrap().to_parts().numerator, "3");
        assert_eq!(Rat::from_decimal_f64(0.3).unwrap().to_parts().denominator, "10");
        assert_eq!(Rat::from_decimal_f64(100.0).unwrap().to_parts().denominator, "1");
        let neg = Rat::from_decimal_f64(-1.5).unwrap();
        assert!(neg.is_negative());
        let p = neg.to_parts();
        assert_eq!((p.negative, p.numerator.as_str(), p.denominator.as_str()), (true, "3", "2"));
    }

    #[test]
    fn the_additive_boundary_folds_exactly() {
        // The round-3 defect: 0.9 - 0.3 must be exactly 6/10, NOT the f64
        // 0.6000000000000001.
        let nine = Rat::from_decimal_f64(0.9).unwrap();
        let three = Rat::from_decimal_f64(0.3).unwrap();
        let six_tenths = &nine - &three;
        assert_eq!(six_tenths, Rat::from_decimal_f64(0.6).unwrap());
        // and is strictly less than the f64 seam literal:
        assert!(six_tenths < Rat::from_decimal_f64(0.6000000000000001).unwrap());
    }

    #[test]
    fn non_finite_has_no_rational() {
        assert!(Rat::from_decimal_f64(f64::NAN).is_none());
        assert!(Rat::from_decimal_f64(f64::INFINITY).is_none());
    }

    #[test]
    fn division_is_exact_and_guards_zero() {
        let one = Rat::one();
        let forty_nine = Rat::from_decimal_f64(49.0).unwrap();
        let inv = one.checked_div(&forty_nine).unwrap();
        // 1/49 is exact, not the f64 reciprocal.
        assert_eq!(inv.to_parts().numerator, "1");
        assert_eq!(inv.to_parts().denominator, "49");
        assert!(one.checked_div(&Rat::zero()).is_none());
    }

    #[test]
    fn huge_magnitudes_stay_plain_decimal() {
        // f64::MAX Display is a ~309-digit integer, no scientific notation.
        let big = Rat::from_decimal_f64(f64::MAX).unwrap();
        assert!(big.is_integer());
        let doubled = &big + &big;
        assert!(doubled > big);
    }
}
