//! The portable `--combine` certificate bundle (schema `smash2-combine/1`).
//!
//! A bundle is the machine-checkable artifact behind one certified
//! PROVEN DISJOINT pair: the exact formula set that was certified (cuts,
//! axiom instances, reconciliation facts — in replay order) plus the
//! Farkas certificate tree. [`CombineBundle::replay`] re-checks it with
//! the same trusted kernel as [`Certificate::replay`] — no solver, no
//! search, no smash2 analysis run.
//!
//! Scope, stated honestly: a successful replay proves the *listed
//! formulas* are (real-)unsatisfiable together. That those formulas
//! faithfully encode the named regions — encoder, polarity projection,
//! axiom catalog — is smash2's claim, audited by its testing nets, not
//! established by the replay. The fixed [`SCOPE_NOTE`] rides inside
//! every bundle so the artifact says so itself.

use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::QuantityId;
use serde::{Deserialize, Serialize};

use crate::{Certificate, QRat};

/// Schema tag every bundle carries; the recheck tool refuses anything else.
pub const BUNDLE_SCHEMA: &str = "smash2-combine/1";

/// The honest-scope sentence embedded in every bundle.
pub const SCOPE_NOTE: &str = "Replaying this bundle proves the listed formulas are \
    (real-)unsatisfiable together. That the formulas faithfully encode the named \
    regions (encoder, polarity projection, axiom catalog) is smash2's claim, \
    audited by its testing nets - not established by this replay.";

/// One linear term: `coeff * q<id>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleTerm {
    /// The exact rational coefficient.
    pub coeff: QRat,
    /// The abstract quantity id (`q<id>` across the whole bundle).
    pub q: u32,
}

/// A comparison relation, serialized as its ADL spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[allow(missing_docs)] // the variants are the six relations, named for themselves
pub enum BundleRel {
    #[serde(rename = "<")]
    Lt,
    #[serde(rename = "<=")]
    Le,
    #[serde(rename = ">")]
    Gt,
    #[serde(rename = ">=")]
    Ge,
    #[serde(rename = "==")]
    Eq,
    #[serde(rename = "!=")]
    Ne,
}

/// A portable [`QFormula`]: linear atoms over abstract quantities `q<id>`,
/// combined with `and`/`or`. Tagged so the JSON is self-describing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum BundleFormula {
    /// The trivially true formula.
    True,
    /// The trivially false formula.
    False,
    /// A linear atom `Σ terms ⋈ k`.
    Atom {
        /// The linear left-hand side.
        terms: Vec<BundleTerm>,
        /// The relation `⋈`.
        rel: BundleRel,
        /// The exact rational right-hand constant.
        k: QRat,
    },
    /// Conjunction of `args`.
    And {
        /// The conjuncts.
        args: Vec<BundleFormula>,
    },
    /// Disjunction of `args`.
    Or {
        /// The disjuncts.
        args: Vec<BundleFormula>,
    },
}

impl BundleFormula {
    fn from_qformula(f: &QFormula) -> Self {
        match f {
            QFormula::True => BundleFormula::True,
            QFormula::False => BundleFormula::False,
            QFormula::Atom(a) => BundleFormula::Atom {
                terms: a
                    .terms()
                    .iter()
                    .map(|(c, q)| BundleTerm {
                        coeff: QRat(c.clone()),
                        q: q.0,
                    })
                    .collect(),
                rel: match a.rel() {
                    Rel::Lt => BundleRel::Lt,
                    Rel::Le => BundleRel::Le,
                    Rel::Gt => BundleRel::Gt,
                    Rel::Ge => BundleRel::Ge,
                    Rel::Eq => BundleRel::Eq,
                    Rel::Ne => BundleRel::Ne,
                },
                k: QRat(a.constant().clone()),
            },
            QFormula::And(v) => BundleFormula::And {
                args: v.iter().map(Self::from_qformula).collect(),
            },
            QFormula::Or(v) => BundleFormula::Or {
                args: v.iter().map(Self::from_qformula).collect(),
            },
        }
    }

    fn to_qformula(&self) -> QFormula {
        match self {
            BundleFormula::True => QFormula::True,
            BundleFormula::False => QFormula::False,
            BundleFormula::Atom { terms, rel, k } => {
                let ts: Vec<(adl_sema::Rat, QuantityId)> = terms
                    .iter()
                    .map(|t| (t.coeff.0.clone(), QuantityId(t.q)))
                    .collect();
                let rel = match rel {
                    BundleRel::Lt => Rel::Lt,
                    BundleRel::Le => Rel::Le,
                    BundleRel::Gt => Rel::Gt,
                    BundleRel::Ge => Rel::Ge,
                    BundleRel::Eq => Rel::Eq,
                    BundleRel::Ne => Rel::Ne,
                };
                QFormula::Atom(LinAtom::new(ts, rel, k.0.clone()))
            }
            BundleFormula::And { args } => {
                QFormula::And(args.iter().map(Self::to_qformula).collect())
            }
            BundleFormula::Or { args } => {
                QFormula::Or(args.iter().map(Self::to_qformula).collect())
            }
        }
    }
}

/// One named member of the certified set (a cut, an `AX<i>` axiom
/// instance, or an `XR<k>` reconciliation fact).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleAssert {
    /// The assert's engine name (cut name, `AX<i>`, or `XR<k>`).
    pub name: String,
    /// The formula as asserted.
    pub formula: BundleFormula,
}

/// The whole portable artifact for one certified PROVEN DISJOINT pair.
///
/// `asserts` is in **replay order**: the certificate's structure refers to
/// the conjunction of these formulas exactly as listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombineBundle {
    /// Always [`BUNDLE_SCHEMA`]; replay fails closed on anything else.
    pub schema: String,
    /// First region of the pair (`<unit>::<region>` in cross runs).
    pub region_a: String,
    /// Second region of the pair.
    pub region_b: String,
    /// The claimed verdict this bundle backs (`PROVEN DISJOINT`).
    pub verdict: String,
    /// The honest-scope sentence ([`SCOPE_NOTE`]).
    pub note: String,
    /// The certified formula set, in replay order.
    pub asserts: Vec<BundleAssert>,
    /// The Farkas proof tree over the conjunction of `asserts`.
    pub certificate: Certificate,
}

impl CombineBundle {
    /// Package a certified pair. `asserts` must be the named formulas in
    /// the exact order they were handed to the certifier.
    #[must_use]
    pub fn new(
        region_a: String,
        region_b: String,
        asserts: Vec<(String, QFormula)>,
        certificate: Certificate,
    ) -> Self {
        Self {
            schema: BUNDLE_SCHEMA.to_owned(),
            region_a,
            region_b,
            verdict: "PROVEN DISJOINT".to_owned(),
            note: SCOPE_NOTE.to_owned(),
            asserts: asserts
                .into_iter()
                .map(|(name, f)| BundleAssert {
                    name,
                    formula: BundleFormula::from_qformula(&f),
                })
                .collect(),
            certificate,
        }
    }

    /// The certified formula set, reconstructed in replay order.
    #[must_use]
    pub fn formulas(&self) -> Vec<QFormula> {
        self.asserts.iter().map(|a| a.formula.to_qformula()).collect()
    }

    /// Re-check the bundle with the trusted kernel: `true` iff the schema
    /// matches and the certificate is a valid refutation of the listed
    /// formulas. No solver, no search. Fails closed on schema mismatch.
    #[must_use]
    pub fn replay(&self) -> bool {
        self.schema == BUNDLE_SCHEMA && self.certificate.replay(&self.formulas())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Budget, CertifyResult, certify_unsat};
    use adl_sema::Rat;

    fn atom(q: u32, rel: Rel, k: i64) -> QFormula {
        QFormula::Atom(LinAtom::single(QuantityId(q), rel, Rat::from_i64(k)))
    }

    #[test]
    fn bundle_roundtrips_and_replays() {
        // x > 2 AND (x < 1 OR x < 0): unsat, exercises Atom/Or and a Split.
        let forms = vec![
            atom(0, Rel::Gt, 2),
            QFormula::Or(vec![atom(0, Rel::Lt, 1), atom(0, Rel::Lt, 0)]),
        ];
        let CertifyResult::Certified(cert) = certify_unsat(&forms, &Budget::default()) else {
            panic!("expected certification");
        };
        let bundle = CombineBundle::new(
            "A.SR".into(),
            "B.CR".into(),
            vec![("cut_a".into(), forms[0].clone()), ("cut_b".into(), forms[1].clone())],
            cert,
        );
        assert!(bundle.replay());

        // JSON round-trip preserves replayability and exact formulas.
        let js = serde_json::to_string_pretty(&bundle).unwrap();
        let back: CombineBundle = serde_json::from_str(&js).unwrap();
        assert_eq!(back, bundle);
        assert!(back.replay());
        assert_eq!(back.formulas(), forms);
    }

    #[test]
    fn tampered_bundle_fails_replay() {
        let forms = vec![atom(0, Rel::Gt, 2), atom(0, Rel::Lt, 1)];
        let CertifyResult::Certified(cert) = certify_unsat(&forms, &Budget::default()) else {
            panic!("expected certification");
        };
        let bundle = CombineBundle::new(
            "A".into(),
            "B".into(),
            vec![("a".into(), forms[0].clone()), ("b".into(), forms[1].clone())],
            cert,
        );
        let js = serde_json::to_string(&bundle).unwrap();

        // Weaken one constant so the formula set becomes satisfiable: the
        // certificate no longer refutes it.
        let tampered = js.replace("\"k\":\"2\"", "\"k\":\"0\"");
        assert_ne!(js, tampered, "tamper target not found in JSON");
        let t: CombineBundle = serde_json::from_str(&tampered).unwrap();
        assert!(!t.replay(), "tampered formula set still replayed");

        // Wrong schema fails closed.
        let wrong = js.replace(BUNDLE_SCHEMA, "smash2-combine/999");
        let w: CombineBundle = serde_json::from_str(&wrong).unwrap();
        assert!(!w.replay(), "unknown schema was accepted");
    }
}
