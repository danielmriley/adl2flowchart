//! The portable `--combine` certificate bundle (schema `smash2-combine/2`).
//!
//! A bundle is the machine-checkable artifact behind one PROVEN DISJOINT
//! pair: the exact formula set that was certified (cuts, axiom instances,
//! reconciliation facts — in replay order), the Farkas certificate tree over
//! it, and — new in `/2` — the **derivation chain** of every reconciliation
//! fact the refutation leans on. [`CombineBundle::replay`] re-checks the whole
//! thing with the same trusted kernel as [`Certificate::replay`]: no solver,
//! no search, no smash2 analysis run.
//!
//! # What replay establishes, and what it does not
//!
//! Replay proves the *listed formulas* are (real-)unsatisfiable together, and
//! that every `XR<k>` reconciliation fact among them is itself refuted-derived
//! from its own listed premises rather than assumed. That the formulas
//! faithfully encode the named regions — encoder, polarity projection, axiom
//! catalog, and the step from "these element predicates are nested" to "these
//! collection sizes are ordered" — is smash2's claim, audited by its testing
//! nets, not established by the replay. The fixed [`SCOPE_NOTE`] rides inside
//! every bundle so the artifact says so itself.
//!
//! # Which fields are checked, and why the rest are not
//!
//! The bundle carries two kinds of content, and it matters which is which:
//!
//! * **Load-bearing** — pinned or arithmetically re-derived by [`CombineBundle::replay`]:
//!   [`schema`](CombineBundle::schema), [`verdict`](CombineBundle::verdict),
//!   [`note`](CombineBundle::note) (pinned to their constants); every
//!   `formula`; every [`Certificate`]; the assert ↔ derived-fact linkage
//!   (an `XR<k>` assert must name a derived fact whose formula is *identical*,
//!   and that fact's own certificates must refute its own premises); and the
//!   *coverage* of the [`quantities`](CombineBundle::quantities) dictionary
//!   (every quantity id appearing anywhere must have an entry).
//! * **Descriptive** — never checked: quantity *labels*, assert
//!   [`source`](BundleAssert::source) records, [`producer`](CombineBundle::producer),
//!   [`inputs`](CombineBundle::inputs), and the region names.
//!
//! The split is not laziness, it is honesty. Those fields are unauthenticated
//! text: there is nothing inside the bundle to check a label or a filename
//! *against*, so "verifying" them would only mean re-reading them. Pinning
//! them would also be a false comfort — a bundle is unsigned, so anyone able
//! to edit a label can edit whatever it was pinned to. What replay can do is
//! guarantee the *math* is real and *complete* (no quantity referred to without
//! being named, no derived fact used without being derived), and then say
//! plainly which parts it is not speaking for. `smash2-recheck` prints exactly
//! that caveat next to every OK line, so a reader never mistakes "replayed"
//! for "these labels are true".

use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::QuantityId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::{Certificate, QRat};

/// Schema tag every bundle carries; the recheck tool refuses anything else.
pub const BUNDLE_SCHEMA: &str = "smash2-combine/2";

/// Schema tags this tool once emitted and no longer accepts. A `/1` bundle
/// carries no derivation chain for its `XR<k>` facts and no quantity
/// dictionary, so it cannot be checked to the `/2` standard; rather than
/// half-verify it, [`crate::bundle::supersession_note`] tells the reader to
/// re-emit it.
pub const SUPERSEDED_SCHEMAS: [&str; 1] = ["smash2-combine/1"];

/// The schema lineage, oldest first — carried in [`Producer::schema_history`]
/// so a bundle explains its own version to someone who has never seen the
/// tool.
pub const SCHEMA_HISTORY: [&str; 2] = [
    "smash2-combine/1: named formula set in replay order + Farkas certificate",
    "smash2-combine/2: + quantity dictionary, assert provenance, embedded \
     derivation chain for reconciliation facts, producer/inputs identity",
];

/// The message for a bundle written by an older smash2.
#[must_use]
pub fn supersession_note(schema: &str) -> String {
    format!(
        "schema {schema:?} is superseded by {BUNDLE_SCHEMA:?}; re-emit it with this \
         smash2 (`verify --combine DIR/`). A {schema:?} bundle carries no derivation \
         chain for its reconciliation facts, so this checker will not half-verify it."
    )
}

/// Canonical verdict string every bundle carries; [`CombineBundle::replay`]
/// refuses anything else so a forged claim cannot ride a valid certificate.
pub const BUNDLE_VERDICT: &str = "PROVEN DISJOINT";

/// The honest-scope sentence embedded in every bundle.
pub const SCOPE_NOTE: &str = "Replaying this bundle proves the listed formulas are \
    (real-)unsatisfiable together, and that every reconciliation fact among them is \
    refuted-derived from its own listed premises rather than assumed. That the formulas \
    faithfully encode the named regions (encoder, polarity projection, axiom catalog, and \
    the step from nested element predicates to ordered collection sizes) is smash2's \
    claim, audited by its testing nets - not established by this replay. Quantity labels, \
    assert sources, producer and input identity are descriptive and unchecked.";

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
    /// Port a [`QFormula`] into the serializable form.
    #[must_use]
    pub fn from_qformula(f: &QFormula) -> Self {
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

    /// Rebuild the [`QFormula`] this entry denotes.
    #[must_use]
    pub fn to_qformula(&self) -> QFormula {
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

    fn collect_quantities(&self, out: &mut BTreeSet<u32>) {
        match self {
            BundleFormula::True | BundleFormula::False => {}
            BundleFormula::Atom { terms, .. } => out.extend(terms.iter().map(|t| t.q)),
            BundleFormula::And { args } | BundleFormula::Or { args } => {
                for a in args {
                    a.collect_quantities(out);
                }
            }
        }
    }
}

/// Where an assert came from. Descriptive: [`CombineBundle::replay`] reads the
/// [`AssertSource::Derived`] link (that one carries a *checkable* obligation)
/// and otherwise does not verify these records — see the module docs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssertSource {
    /// A cut written in an analysis file.
    Cut {
        /// Region the cut belongs to (`<unit>::<region>` in cross runs).
        region: String,
        /// 1-based source line.
        line: u32,
        /// The cut as written.
        text: String,
        /// `true`: the formula is the cut's whole over-projection. `false`:
        /// it is one conjunct of it — the single-quantity bound the interval
        /// fast path used (see [`crate::certify_bounds`]).
        whole: bool,
    },
    /// A background-fact instance from the axiom catalog.
    Axiom {
        /// Catalog id (`ORD`, `SUB`, …).
        id: String,
        /// What the axiom states.
        statement: String,
        /// The physical assumption that makes it true of every event.
        assumption: String,
    },
    /// A reconciliation fact, derived rather than assumed: names the
    /// [`DerivedFact`] in this bundle that carries its proof.
    Derived {
        /// The `derived_facts[].name` this assert refers to.
        fact: String,
    },
    /// A formula the analyzer constructed for a query (not source text).
    Query {
        /// What the formula plays in the query.
        role: String,
    },
    /// No provenance was recorded. Mathematically checkable all the same.
    Unattributed,
}

/// One named member of a certified set (a cut, an `AX<i>` axiom instance, an
/// `XR<k>` reconciliation fact, or a query-side formula).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BundleAssert {
    /// The assert's engine name (cut name, `AX<i>`, `XR<k>`, `QSUB<k>`, …).
    pub name: String,
    /// Where it came from (descriptive, except the `derived` link).
    pub source: AssertSource,
    /// The formula as asserted.
    pub formula: BundleFormula,
}

impl BundleAssert {
    /// Build an entry from an engine name, formula, and provenance.
    #[must_use]
    pub fn new(name: String, formula: &QFormula, source: AssertSource) -> Self {
        Self {
            name,
            source,
            formula: BundleFormula::from_qformula(formula),
        }
    }
}

/// One refutation supporting a [`DerivedFact`]: the named premises and the
/// certificate that refutes their conjunction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Derivation {
    /// What this refutation shows, in words (descriptive).
    pub claim: String,
    /// The named formulas the certificate refutes, in replay order.
    pub premises: Vec<BundleAssert>,
    /// The Farkas proof tree over the conjunction of `premises`.
    pub certificate: Certificate,
}

impl Derivation {
    /// Package a premise set with its certificate.
    #[must_use]
    pub fn new(claim: String, premises: Vec<BundleAssert>, certificate: Certificate) -> Self {
        Self {
            claim,
            premises,
            certificate,
        }
    }

    /// The premise set, reconstructed in replay order.
    #[must_use]
    pub fn formulas(&self) -> Vec<QFormula> {
        self.premises
            .iter()
            .map(|p| p.formula.to_qformula())
            .collect()
    }

    /// Re-check this derivation with the trusted kernel.
    #[must_use]
    pub fn replay(&self) -> bool {
        self.certificate.replay(&self.formulas())
    }
}

/// A reconciliation fact (`XR<k>`) together with the proof that earns it.
///
/// Before schema `/2` such a fact entered a certified core as a **given**: the
/// certificate proved "UNSAT given `XR<k>`" and the reader had to take
/// `XR<k>` on trust. Here the fact carries its own refutations, so replaying
/// the bundle replays the whole chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedFact {
    /// The engine name (`XR<k>`) the asserts refer to.
    pub name: String,
    /// The catalog id the fact was emitted under (`XSUB` / `XEQ`).
    pub axiom: String,
    /// The fact in words, e.g. `size(C3#jets) <= size(C9#jets)` (descriptive).
    pub statement: String,
    /// The fact as asserted — must be identical to the assert that uses it.
    pub formula: BundleFormula,
    /// The refutations it rests on (one per proven subset direction).
    pub derivations: Vec<Derivation>,
}

impl DerivedFact {
    /// Package a fact with its derivations.
    #[must_use]
    pub fn new(
        name: String,
        axiom: String,
        statement: String,
        formula: &QFormula,
        derivations: Vec<Derivation>,
    ) -> Self {
        Self {
            name,
            axiom,
            statement,
            formula: BundleFormula::from_qformula(formula),
            derivations,
        }
    }

    /// Re-check every supporting derivation. A fact with no derivation fails
    /// closed — an unsupported fact is exactly what schema `/2` exists to
    /// prevent.
    #[must_use]
    pub fn replay(&self) -> bool {
        !self.derivations.is_empty() && self.derivations.iter().all(Derivation::replay)
    }
}

/// Which tool wrote this bundle (descriptive).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Producer {
    /// Always `"smash2"` for bundles this workspace writes.
    pub tool: String,
    /// The emitting crate version.
    pub version: String,
    /// The schema lineage ([`SCHEMA_HISTORY`]).
    pub schema_history: Vec<String>,
}

impl Default for Producer {
    fn default() -> Self {
        Self {
            tool: "smash2".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            schema_history: SCHEMA_HISTORY.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

/// One analyzed source unit, identified by content digest (descriptive: a
/// reader holding the same `.adl` can confirm the bundle describes their copy).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleInput {
    /// Unit name as reported by smash2 (usually the file stem).
    pub name: String,
    /// Lowercase hex SHA-256 of the file's bytes.
    pub sha256: String,
}

/// The pieces of a bundle the engine assembles.
#[derive(Debug, Clone)]
pub struct BundleParts {
    /// First region of the pair (`<unit>::<region>` in cross runs).
    pub region_a: String,
    /// Second region of the pair.
    pub region_b: String,
    /// The certified formula set, in replay order.
    pub asserts: Vec<BundleAssert>,
    /// Derivation chains for the reconciliation facts among `asserts`.
    pub derived_facts: Vec<DerivedFact>,
    /// The Farkas proof tree over the conjunction of `asserts`.
    pub certificate: Certificate,
}

/// The whole portable artifact for one PROVEN DISJOINT pair.
///
/// `asserts` is in **replay order**: the certificate's structure refers to
/// the conjunction of these formulas exactly as listed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombineBundle {
    /// Always [`BUNDLE_SCHEMA`]; replay fails closed on anything else.
    pub schema: String,
    /// Which tool wrote this (descriptive).
    pub producer: Producer,
    /// The analyzed source units and their digests (descriptive). Empty when
    /// the bundle came from the library API rather than the CLI, which is what
    /// owns the file bytes.
    pub inputs: Vec<BundleInput>,
    /// First region of the pair (`<unit>::<region>` in cross runs).
    pub region_a: String,
    /// Second region of the pair.
    pub region_b: String,
    /// The claimed verdict this bundle backs (`PROVEN DISJOINT`).
    pub verdict: String,
    /// The honest-scope sentence ([`SCOPE_NOTE`]).
    pub note: String,
    /// Every quantity id appearing anywhere in this bundle → its human label.
    /// Replay checks the *coverage* of this map, never the label text.
    pub quantities: BTreeMap<u32, String>,
    /// The certified formula set, in replay order.
    pub asserts: Vec<BundleAssert>,
    /// The derivation chain of every reconciliation fact used above.
    pub derived_facts: Vec<DerivedFact>,
    /// The Farkas proof tree over the conjunction of `asserts`.
    pub certificate: Certificate,
}

impl CombineBundle {
    /// Package a certified pair. `label` names every quantity the bundle
    /// mentions, so the dictionary is complete by construction.
    #[must_use]
    pub fn new(parts: BundleParts, label: impl Fn(u32) -> String) -> Self {
        let BundleParts {
            region_a,
            region_b,
            asserts,
            derived_facts,
            certificate,
        } = parts;
        let mut ids = BTreeSet::new();
        collect_ids(&asserts, &derived_facts, &mut ids);
        Self {
            schema: BUNDLE_SCHEMA.to_owned(),
            producer: Producer::default(),
            inputs: Vec::new(),
            region_a,
            region_b,
            verdict: BUNDLE_VERDICT.to_owned(),
            note: SCOPE_NOTE.to_owned(),
            quantities: ids.into_iter().map(|q| (q, label(q))).collect(),
            asserts,
            derived_facts,
            certificate,
        }
    }

    /// The certified formula set, reconstructed in replay order.
    #[must_use]
    pub fn formulas(&self) -> Vec<QFormula> {
        self.asserts
            .iter()
            .map(|a| a.formula.to_qformula())
            .collect()
    }

    /// Re-check the bundle with the trusted kernel: `true` iff the schema,
    /// verdict, and scope note match their canonical constants; the quantity
    /// dictionary covers every quantity mentioned; every reconciliation fact
    /// used as an assert is backed by a derived fact with an *identical*
    /// formula whose own certificates replay; and the certificate refutes the
    /// listed formulas. No solver, no search. Fails closed on any mismatch.
    ///
    /// Region names, quantity labels, assert sources, producer, and inputs are
    /// descriptive and intentionally unchecked — see the module docs.
    #[must_use]
    pub fn replay(&self) -> bool {
        self.schema == BUNDLE_SCHEMA
            && self.verdict == BUNDLE_VERDICT
            && self.note == SCOPE_NOTE
            && self.quantities_cover_everything()
            && self.chain_is_complete()
            && self.derived_facts.iter().all(DerivedFact::replay)
            && self.certificate.replay(&self.formulas())
    }

    /// Every quantity id used anywhere must be named. This is the one thing
    /// about the dictionary that *can* be checked — that it leaves nothing
    /// out. What the names say is not checkable and not claimed.
    fn quantities_cover_everything(&self) -> bool {
        let mut ids = BTreeSet::new();
        collect_ids(&self.asserts, &self.derived_facts, &mut ids);
        ids.iter().all(|q| self.quantities.contains_key(q))
    }

    /// Every derived fact is uniquely named; every assert that *uses* one
    /// names a fact that exists and carries the identical formula; and no
    /// `XR<k>` assert slips in without a `derived` link (stripping the link
    /// must not turn a fact back into a free given).
    fn chain_is_complete(&self) -> bool {
        let mut facts: BTreeMap<&str, &DerivedFact> = BTreeMap::new();
        for f in &self.derived_facts {
            if facts.insert(f.name.as_str(), f).is_some() {
                return false; // duplicate name: the link would be ambiguous
            }
        }
        self.asserts.iter().all(|a| match &a.source {
            AssertSource::Derived { fact } => {
                facts.get(fact.as_str()).is_some_and(|f| f.formula == a.formula)
            }
            _ => !a.name.starts_with("XR"),
        })
    }
}

fn collect_ids(asserts: &[BundleAssert], facts: &[DerivedFact], out: &mut BTreeSet<u32>) {
    for a in asserts {
        a.formula.collect_quantities(out);
    }
    for f in facts {
        f.formula.collect_quantities(out);
        for d in &f.derivations {
            for p in &d.premises {
                p.formula.collect_quantities(out);
            }
        }
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

    fn cut(name: &str, f: &QFormula) -> BundleAssert {
        BundleAssert::new(
            name.to_owned(),
            f,
            AssertSource::Cut {
                region: "SR".to_owned(),
                line: 1,
                text: name.to_owned(),
                whole: true,
            },
        )
    }

    fn label(q: u32) -> String {
        format!("q{q}")
    }

    fn bundle(asserts: Vec<BundleAssert>, cert: Certificate) -> CombineBundle {
        CombineBundle::new(
            BundleParts {
                region_a: "A.SR".to_owned(),
                region_b: "B.CR".to_owned(),
                asserts,
                derived_facts: Vec::new(),
                certificate: cert,
            },
            label,
        )
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
        let b = bundle(
            vec![cut("cut_a", &forms[0]), cut("cut_b", &forms[1])],
            cert,
        );
        assert!(b.replay());
        assert_eq!(b.quantities.len(), 1, "the one quantity is named");

        // JSON round-trip preserves replayability and exact formulas.
        let js = serde_json::to_string_pretty(&b).unwrap();
        let back: CombineBundle = serde_json::from_str(&js).unwrap();
        assert_eq!(back, b);
        assert!(back.replay());
        assert_eq!(back.formulas(), forms);
    }

    #[test]
    fn tampered_bundle_fails_replay() {
        let forms = vec![atom(0, Rel::Gt, 2), atom(0, Rel::Lt, 1)];
        let CertifyResult::Certified(cert) = certify_unsat(&forms, &Budget::default()) else {
            panic!("expected certification");
        };
        let b = bundle(vec![cut("a", &forms[0]), cut("b", &forms[1])], cert);
        let js = serde_json::to_string(&b).unwrap();

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

        // Dropping a quantity from the dictionary fails closed (coverage is
        // the one dictionary property replay can and does check).
        let mut gutted = b.clone();
        gutted.quantities.clear();
        assert!(!gutted.replay(), "uncovered quantity was accepted");

        // Renaming a quantity does NOT fail — labels are descriptive.
        let mut relabelled = b.clone();
        relabelled
            .quantities
            .insert(0, "definitely not what this is".to_owned());
        assert!(relabelled.replay(), "labels are not part of the claim");
    }
}
