//! Exact-by-construction numeral conversion shared by both backends.
//!
//! A finite `f64` is rendered through Rust's shortest round-trip decimal
//! (`format!("{v}")` — never scientific notation for floats) and that
//! decimal is converted to a rational numerator/denominator pair. Both
//! backends consume the SAME decimal, so their arithmetic semantics are
//! identical; for source literals (`0.3`, `100`, `1.5`) the round-trip
//! decimal recovers the literal the physicist wrote.

/// A sign + numerator/denominator decimal-string rational. The numerator
/// and denominator are non-negative integer digit strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rational {
    pub negative: bool,
    pub numerator: String,
    pub denominator: String,
}

/// Convert a **finite** `f64` to a decimal rational.
///
/// # Panics
/// Panics on NaN/infinite input — non-finite constants cannot construct
/// atoms upstream (`LinAtom`), so reaching here with one is a bug.
#[must_use]
pub fn rational_of(v: f64) -> Rational {
    assert!(v.is_finite(), "non-finite constant reached the solver");
    let s = format!("{v}");
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.as_str()),
    };
    debug_assert!(
        !digits.contains(['e', 'E']),
        "Rust f64 Display never uses scientific notation"
    );
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((i, f)) => (i, f),
        None => (digits, ""),
    };
    let mut numerator: String = format!("{int_part}{frac_part}");
    // Strip leading zeros but keep at least one digit.
    let stripped = numerator.trim_start_matches('0');
    numerator = if stripped.is_empty() {
        "0".to_owned()
    } else {
        stripped.to_owned()
    };
    let mut denominator = String::with_capacity(frac_part.len() + 1);
    denominator.push('1');
    for _ in 0..frac_part.len() {
        denominator.push('0');
    }
    Rational {
        negative: negative && numerator != "0",
        numerator,
        denominator,
    }
}

impl Rational {
    /// Render as a closed SMT-LIB2 `Real` term.
    #[must_use]
    pub fn smt_real(&self) -> String {
        let mag = if self.denominator == "1" {
            format!("{}.0", self.numerator)
        } else {
            format!("(/ {}.0 {}.0)", self.numerator, self.denominator)
        };
        if self.negative {
            format!("(- {mag})")
        } else {
            mag
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_and_decimals() {
        let r = rational_of(100.0);
        assert_eq!((r.negative, r.numerator.as_str()), (false, "100"));
        assert_eq!(r.denominator, "1");
        assert_eq!(r.smt_real(), "100.0");

        let r = rational_of(0.3);
        assert_eq!(r.numerator, "3");
        assert_eq!(r.denominator, "10");
        assert_eq!(r.smt_real(), "(/ 3.0 10.0)");

        let r = rational_of(-1.5);
        assert!(r.negative);
        assert_eq!(r.smt_real(), "(- (/ 15.0 10.0))");

        let r = rational_of(0.0);
        assert!(!r.negative);
        assert_eq!(r.smt_real(), "0.0");

        let r = rational_of(-0.0);
        assert!(!r.negative, "-0.0 must not render as a negative zero");
    }

    #[test]
    #[should_panic(expected = "non-finite")]
    fn non_finite_panics() {
        let _ = rational_of(f64::NAN);
    }
}
