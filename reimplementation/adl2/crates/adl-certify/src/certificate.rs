//! The serializable certificate and its **trusted replay kernel**.
//!
//! [`Certificate::replay`] is the one function the whole trust story rests on:
//! it re-checks a certificate against the input formulas using exact rational
//! arithmetic and no search. It shares its boolean decomposition with the
//! searcher (the `saturate` module) and its numeric core with
//! `constraint::farkas_refutes`, so a `true` here means a human-auditable,
//! ~few-hundred-line proof accepted, not "the solver said so".

use adl_formula::QFormula;
use adl_sema::Rat;
use serde::{Deserialize, Serialize};

use crate::MAX_DEPTH;
use crate::constraint::farkas_refutes;
use crate::saturate::{
    build_child, collect_constraints, disjuncts, leftmost_or_index, saturate,
};

/// A serializable exact rational: an [`adl_sema::Rat`] that (de)serializes as
/// the string `"[-]numerator[/denominator]"` in lowest terms.
///
/// Serialization goes through [`Rat::to_parts`]; deserialization rebuilds the
/// value with a digit fold over `Rat`'s public exact arithmetic (`*10`,
/// `+digit`, `/denominator`), so arbitrarily large multipliers round-trip
/// exactly without reaching into `BigRational` internals.
#[derive(Debug, Clone, PartialEq)]
pub struct QRat(pub Rat);

impl QRat {
    fn to_repr(&self) -> String {
        let p = self.0.to_parts();
        let core = if p.denominator == "1" {
            p.numerator
        } else {
            format!("{}/{}", p.numerator, p.denominator)
        };
        if p.negative {
            format!("-{core}")
        } else {
            core
        }
    }

    fn from_repr(s: &str) -> Option<Rat> {
        let (neg, body) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s),
        };
        let (num_s, den_s) = body.split_once('/').unwrap_or((body, "1"));
        let num = parse_uint(num_s)?;
        let den = parse_uint(den_s)?;
        let mut v = num.checked_div(&den)?; // None if denominator is zero
        if neg {
            v = -&v;
        }
        Some(v)
    }
}

/// Parse a non-empty decimal digit string into an exact [`Rat`] using only
/// `Rat`'s public arithmetic — no dependency on `BigRational` internals, and
/// exact for arbitrarily large integers.
fn parse_uint(s: &str) -> Option<Rat> {
    if s.is_empty() {
        return None;
    }
    let ten = Rat::from_i64(10);
    let mut acc = Rat::zero();
    for ch in s.chars() {
        let d = ch.to_digit(10)?;
        acc = &(&acc * &ten) + &Rat::from_i64(i64::from(d));
    }
    Some(acc)
}

impl Serialize for QRat {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_repr())
    }
}

impl<'de> Deserialize<'de> for QRat {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        QRat::from_repr(&s)
            .map(QRat)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid rational literal: {s:?}")))
    }
}

/// A node of the certificate proof tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CertNode {
    /// The conjunction contains an explicit `false` (or an empty disjunction)
    /// — unsatisfiable outright, no multipliers needed.
    Contradiction,
    /// A conjunctive leaf refuted by Farkas multipliers, one per hard atom in
    /// saturation order.
    Farkas {
        /// Nonnegative multipliers aligned to the leaf's canonical atoms.
        multipliers: Vec<QRat>,
    },
    /// A case split on the leftmost `Or` obligation: one sub-certificate per
    /// disjunct, in disjunct order. **Every** branch must refute.
    Split {
        /// Sub-certificates; `branches[i]` proves the branch that picks
        /// disjunct `i` of the split `Or`.
        branches: Vec<CertNode>,
    },
}

/// A machine-checkable proof that a conjunction of [`QFormula`]s is
/// (real-)unsatisfiable. Serializable; [`Certificate::replay`] re-checks it with
/// exact arithmetic and no search.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Certificate {
    /// Root of the proof tree, checked against the whole input conjunction.
    pub root: CertNode,
}

impl Certificate {
    /// Wrap a proof-tree root.
    #[must_use]
    pub fn new(root: CertNode) -> Self {
        Self { root }
    }

    /// Re-check this certificate against `formulas` (their conjunction) using
    /// exact rational arithmetic only — **no search, no solver**. Returns `true`
    /// iff the certificate is a valid refutation.
    ///
    /// This is the trusted kernel: a `true` here is the entire meaning of
    /// "PROVEN DISJOINT". It fails closed — a malformed, tampered, over-deep, or
    /// shape-mismatched certificate returns `false`, never panics, and never
    /// accepts a satisfiable set.
    #[must_use]
    pub fn replay(&self, formulas: &[QFormula]) -> bool {
        replay_node(&self.root, formulas, 0)
    }
}

/// Check one certificate node against the conjunction `conj`. Mirrors
/// [`crate::search::Searcher::refute`] exactly, but only *verifies* the recorded
/// decisions instead of searching for them.
fn replay_node(cert: &CertNode, conj: &[QFormula], depth: usize) -> bool {
    if depth > MAX_DEPTH {
        return false; // fail closed on adversarially deep certificates
    }

    let sat = saturate(conj);
    if sat.has_false {
        // A `false` conjunct makes any refutation valid; we still require the
        // certificate to *claim* the contradiction, so tampering cannot relabel
        // a genuine Farkas/Split leaf as a free win.
        return matches!(cert, CertNode::Contradiction);
    }

    match leftmost_or_index(&sat.items) {
        None => {
            // Leaf: the certificate must be Farkas multipliers over exactly the
            // leaf's canonical atoms, in order.
            let CertNode::Farkas { multipliers } = cert else {
                return false;
            };
            let cons = collect_constraints(&sat.items);
            if multipliers.len() != cons.len() {
                return false;
            }
            let lambdas: Vec<Rat> = multipliers.iter().map(|m| m.0.clone()).collect();
            farkas_refutes(&cons, &lambdas)
        }
        Some(j) => {
            // Split: the certificate must cover every disjunct, and each branch
            // must refute the corresponding child conjunction.
            let CertNode::Split { branches } = cert else {
                return false;
            };
            let ds = disjuncts(&sat.items[j]);
            if branches.len() != ds.len() {
                return false;
            }
            branches.iter().zip(ds).all(|(branch, d)| {
                let child = build_child(&sat.items, j, d);
                replay_node(branch, &child, depth + 1)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use adl_sema::Rat;

    #[test]
    fn qrat_roundtrips_large_values() {
        // A value beyond i64 range must survive the string round-trip exactly.
        let big = Rat::from_decimal_f64(f64::MAX).unwrap();
        let q = QRat(big.clone());
        let repr = q.to_repr();
        assert_eq!(QRat::from_repr(&repr), Some(big));
    }

    #[test]
    fn qrat_roundtrips_fraction_and_sign() {
        let v = Rat::from_i64(2).checked_div(&Rat::from_i64(7)).unwrap();
        let neg = -&v;
        assert_eq!(QRat::from_repr(&QRat(v.clone()).to_repr()), Some(v));
        assert_eq!(QRat::from_repr(&QRat(neg.clone()).to_repr()), Some(neg));
    }

    #[test]
    fn qrat_rejects_garbage() {
        assert!(QRat::from_repr("1/0").is_none());
        assert!(QRat::from_repr("").is_none());
        assert!(QRat::from_repr("x").is_none());
    }
}
