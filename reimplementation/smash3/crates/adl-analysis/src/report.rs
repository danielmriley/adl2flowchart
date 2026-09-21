//! Report data model: versioned JSON schema (SPEC_ANALYSIS §6) + the
//! deterministic human rendering. Stable ordering throughout: regions in
//! declaration order, pairs in (i, j) declaration order, values sorted
//! by label.

use serde::Serialize;

/// Bumped on any breaking schema change.
///
/// v2: pairwise `kind` gained `"candidate_overlapping"` (a SAT overlap whose
/// witness the interpreter could not validate). Under v1 such a pair was
/// reported `"proven_overlapping"`, so a consumer summing proven overlaps
/// reads different totals across the change — a meaning change, hence the
/// bump. Treat `kind` as an open set going forward.
///
/// v3 (proof-system v2 Phase 4): `kind` gained `"candidate_disjoint"` (a
/// solver-UNSAT disjointness the independent exact-rational certifier could
/// not verify — only under `--certify`), and pairwise rows gained
/// `certified` (true = an independently replay-checked Farkas certificate
/// backs the disjointness; absent = certification did not run).
///
/// Still v3 after the reconciliation ledger: `reconciliations` and
/// `recon_near_misses` are ADDITIVE and omitted when empty, so no existing
/// field changed meaning and single-file output is byte-identical.
///
/// v4 (trustworthy verify M2): region `empty` gained `"candidate"` (a
/// solver-UNSAT emptiness the independent certifier could not verify —
/// mirrors pairwise `candidate_disjoint`). Consumers that only knew
/// `proven` / `not_proven` / `unknown` must treat the new variant as a
/// non-claim (not Proven).
///
/// Still v4 after the trust-surface work (report layer only — no verdict,
/// encoding, axiom, or solver behaviour changed). Every field below is
/// ADDITIVE; nothing existing changed name, type, or meaning:
/// - top level: `certification` (was the independent certifier enabled),
///   `solver_failures` (`{spawn, errors, first_reason}`, omitted in healthy
///   runs), `diagnostics` (`internal_diagnostics` classified into
///   `fail_closed` / `contradiction`, same messages, same order — the flat
///   `internal_diagnostics` array stays exactly as it was);
/// - pairwise rows: `proof_path` (`interval` | `solver_core`) and
///   `certificate_size` (formulas the replay kernel checked), both omitted
///   when there is no UNSAT-side proof;
/// - region rows: `empty_proof` (same `proof_path` domain), omitted when the
///   region is not proven/candidate empty;
/// - reconciliation rows: `a_units` / `b_units`, the analysis units whose
///   regions mention each collection (file attribution for `C1#name` ids),
///   omitted when empty.
pub const SCHEMA_VERSION: u32 = 4;

/// How an UNSAT-side claim was obtained. Descriptive provenance: it names
/// the route, never the confidence (the certificate does that).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofPath {
    /// The solver-free interval layer refuted the claim on a bound pair.
    Interval,
    /// The solver returned UNSAT and the claim rests on its unsat core.
    SolverCore,
}

impl ProofPath {
    #[must_use]
    pub fn human(self) -> &'static str {
        match self {
            ProofPath::Interval => "interval bounds",
            ProofPath::SolverCore => "solver unsat core",
        }
    }
}

/// Severity of an internal diagnostic. The distinction is the whole point
/// of splitting the section: a fail-closed note is the tool declining to
/// claim something it cannot back (working as designed); a contradiction is
/// the tool refuting its OWN conclusion (a bug).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticClass {
    /// A claim was withheld / capped because its evidence did not hold up.
    /// No conclusion was contradicted; the conservative answer was taken.
    FailClosed,
    /// One part of the engine refuted a conclusion another part had already
    /// reached (a gate refuting a PROVEN, the replay kernel refusing an
    /// interval refutation, an interpreter rejecting a witness for a fully
    /// decidable region). Release-blocking.
    Contradiction,
}

/// One internal diagnostic with its severity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub class: DiagnosticClass,
    pub message: String,
}

/// Solver checks that could not run to a usable answer. Present only when
/// something actually failed, so a healthy run's JSON is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SolverFailures {
    /// Checks whose solver process could not run at all (spawn/IO).
    pub spawn: usize,
    /// Checks answered `Unknown("solver reported an error: …")`.
    pub errors: usize,
    /// The first failure reason verbatim, so a reader can act on it
    /// without re-running under `--verbose`.
    pub first_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VerdictKind {
    ProvenDisjoint,
    ProvenOverlapping,
    /// A joint SAT model exists, but the overlap rests on an opaque
    /// quantity the interpreter cannot decide, so the witness could not be
    /// re-validated — a candidate overlap, NOT a proof. Distinct from
    /// `ProvenOverlapping` so the "never emit a false PROVEN" contract is
    /// never overclaimed; conservative for combination (a candidate that is
    /// really empty blocks a merge rather than allowing a double-count).
    CandidateOverlapping,
    /// The solver reported UNSAT for the disjointness query, but the
    /// independent exact-rational certifier could not verify the proof
    /// (budget, shape, or an integrality-only refutation under the real
    /// relaxation) — a candidate, NOT a proof. Only produced under
    /// `--certify` (proof-system v2 Phase 4): with certification off,
    /// solver-UNSAT still reports PROVEN DISJOINT as before.
    CandidateDisjoint,
    PossiblyOverlapping,
    Unknown,
}

impl VerdictKind {
    #[must_use]
    pub fn human(self) -> &'static str {
        match self {
            VerdictKind::ProvenDisjoint => "PROVEN DISJOINT",
            VerdictKind::ProvenOverlapping => "PROVEN OVERLAPPING",
            VerdictKind::CandidateOverlapping => "CANDIDATE OVERLAPPING",
            VerdictKind::CandidateDisjoint => "CANDIDATE DISJOINT",
            VerdictKind::PossiblyOverlapping => "POSSIBLY OVERLAPPING",
            VerdictKind::Unknown => "UNKNOWN",
        }
    }
}

/// A source location rendered for the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceRef {
    pub line: u32,
    pub text: String,
}

/// One dropped (Unknown) leaf of a region encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DroppedLeaf {
    pub line: u32,
    pub reason: String,
}

/// One unsat-core item, mapped back to its origin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "origin")]
pub enum CoreItem {
    Cut {
        region: String,
        line: u32,
        text: String,
    },
    Axiom {
        id: String,
        statement: String,
    },
}

impl CoreItem {
    #[must_use]
    pub fn human(&self) -> String {
        match self {
            CoreItem::Cut { region, line, text } => {
                format!("`{region} line {line}: {text}`")
            }
            CoreItem::Axiom { id, statement } => format!("axiom {id} ({statement})"),
        }
    }
}

/// One witness value (quantity in source notation). `derived` marks
/// Sampling-gate accounting (see [`Report::sampling`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SamplingInfo {
    /// Battery size every UNSAT-side PROVEN verdict was checked against.
    pub events: usize,
    /// Verdicts the gate demoted (each is a filed internal contradiction).
    pub refutations: usize,
}

/// Adversarial refute-gate accounting (see [`Report::refute`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RefuteInfo {
    /// Probe events searched for each UNSAT-side PROVEN claim.
    pub probes: usize,
    /// Verdicts the adversarial search demoted.
    pub refutations: usize,
}

/// values for quantities introduced by axioms rather than the regions'
/// own cuts.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WitnessValue {
    pub quantity: String,
    pub value: f64,
    pub derived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyStatus {
    /// UNSAT(Ax ∧ R⁺): no physical event can satisfy a superset of R.
    /// Solver-path proofs additionally require independent Farkas
    /// certification when `--certify` is on; interval-only emptiness
    /// (empty unsat core) remains Proven without a certificate.
    Proven,
    /// The solver reported UNSAT for the emptiness query, but the
    /// independent exact-rational certifier could not verify the proof
    /// — a candidate, NOT a proof. Only produced under `--certify`
    /// (mirrors [`VerdictKind::CandidateDisjoint`]); with certification
    /// off, solver-UNSAT still reports [`EmptyStatus::Proven`].
    Candidate,
    NotProven,
    /// Solver inconclusive / unavailable for this check.
    Unknown,
}

impl EmptyStatus {
    #[must_use]
    pub fn human(self) -> &'static str {
        match self {
            EmptyStatus::Proven => "PROVEN EMPTY",
            EmptyStatus::Candidate => "CANDIDATE EMPTY",
            EmptyStatus::NotProven => "NOT PROVEN EMPTY",
            EmptyStatus::Unknown => "UNKNOWN EMPTY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegionReport {
    pub name: String,
    pub leaves_encoded: usize,
    pub leaves_total: usize,
    pub exact: bool,
    pub or_clauses: usize,
    pub dual_hedges: usize,
    pub dropped: Vec<DroppedLeaf>,
    pub empty: EmptyStatus,
    /// Explanation when `empty` is [`EmptyStatus::Proven`] or
    /// [`EmptyStatus::Candidate`] (solver unsat core mapped to origins).
    pub empty_core: Vec<CoreItem>,
    /// Route the emptiness claim took. `None` when there is no claim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_proof: Option<ProofPath>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PairReport {
    pub a: String,
    pub b: String,
    pub kind: VerdictKind,
    pub reason: String,
    pub exact: bool,
    pub shared_dimensions: Vec<String>,
    pub subset_a_in_b: bool,
    pub subset_b_in_a: bool,
    pub witness: Vec<WitnessValue>,
    /// `Some(true)`: the interpreter accepted the synthetic witness event
    /// in both regions; `Some(false)`: validation could not run to a
    /// verdict (opaque quantities) — the witness is a candidate only.
    /// `None`: no witness.
    pub witness_validated: Option<bool>,
    /// UNSAT-side mirror of `witness_validated`: `Some(true)` = the
    /// disjointness proof was verified by the independent exact-rational
    /// certifier (a replay-checked Farkas certificate); `Some(false)` =
    /// certification ran and could not verify it (the pair reports CANDIDATE
    /// DISJOINT); `None` = **certification is disabled** (`--no-certify`).
    ///
    /// Since M2 this covers every PROVEN DISJOINT route, not just solver
    /// cores: an interval fast-path verdict runs no solver but its refutation
    /// is a two-bound Farkas proof, constructed directly and replayed through
    /// the same kernel. So with `--certify` on (the default), a PROVEN
    /// DISJOINT pair carries `certified: true`.
    ///
    /// One flagged exception: if the kernel ever refuses an interval
    /// refutation, the verdict is left alone (that disagreement is a bug in
    /// one of the two, not evidence about the regions) and `certified` stays
    /// `None` — always accompanied by an `internal_diagnostics` entry, so it
    /// is never a silent `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certified: Option<bool>,
    pub core: Vec<CoreItem>,
    /// Route an UNSAT-side verdict (PROVEN / CANDIDATE DISJOINT) took.
    /// `None` for every other kind — there is no UNSAT-side proof to place.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_path: Option<ProofPath>,
    /// How many formulas the replay kernel was handed for this claim (the
    /// unsat core's size on the solver path, the refuting bound set's on the
    /// interval path). `None` when nothing was certified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub certificate_size: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageStatus {
    Proven,
    NotProven,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BinCheckReport {
    pub region: String,
    pub variable: String,
    pub n_bins: usize,
    pub disjoint_pairs_proven: usize,
    pub disjoint_pairs_total: usize,
    pub coverage: CoverageStatus,
    pub gap_witness: Vec<WitnessValue>,
}

/// What the reconciliation prover concluded about one candidate pair of
/// collections (the ledger row). Purely descriptive — the verdicts these
/// enable are reported per-pair as usual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconOutcome {
    /// Both refinement directions proven: the collections hold the same
    /// elements, so their sizes are equal (XEQ).
    Equivalent,
    /// `a`'s cuts imply `b`'s: every element of `a` is in `b` (XSUB).
    ARefinesB,
    /// `b`'s cuts imply `a`'s (XSUB, other direction).
    BRefinesA,
    /// Neither direction could be proven — no size fact was derived.
    Unrelated,
    /// The pair was dropped before proving; `note` says why.
    Skipped,
}

impl ReconOutcome {
    /// The relation symbol shown in the ledger.
    #[must_use]
    pub fn symbol(&self) -> &'static str {
        match self {
            ReconOutcome::Equivalent => "≡",
            ReconOutcome::ARefinesB => "⊆",
            ReconOutcome::BRefinesA => "⊇",
            ReconOutcome::Unrelated => "?",
            ReconOutcome::Skipped => "⊘",
        }
    }

    /// The axiom family this outcome emitted, if any.
    #[must_use]
    pub fn axiom(&self) -> Option<&'static str> {
        match self {
            ReconOutcome::Equivalent => Some("XEQ"),
            ReconOutcome::ARefinesB | ReconOutcome::BRefinesA => Some("XSUB"),
            ReconOutcome::Unrelated | ReconOutcome::Skipped => None,
        }
    }
}

/// One row of the reconciliation ledger: how two collections from
/// (usually) different analyses were related — or why they were not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconReport {
    pub a: String,
    pub b: String,
    pub outcome: ReconOutcome,
    /// The shared detector base, when the pair had one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// Why a pair was skipped / left unrelated; empty otherwise.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// Analysis units whose regions mention `a` — the file attribution for
    /// its `C<id>#name` label. Usually one; a collection two files define
    /// identically interns to ONE id and legitimately lists both.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub a_units: Vec<String>,
    /// Analysis units whose regions mention `b`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub b_units: Vec<String>,
}

/// An advisory: two collections whose cuts are structurally identical but
/// whose base names differ and cannot be known equal from the ADL text.
/// Derives nothing — it names the assumption a user could supply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconNearMissReport {
    pub a: String,
    pub b: String,
    pub base_a: String,
    pub base_b: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AxiomUse {
    pub id: String,
    pub statement: String,
    pub assumption: String,
    pub instances: usize,
}

/// The full analysis report (one analysis unit).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub unit: String,
    pub solver: String,
    /// Set when a solver WAS selected but checks failed to run usefully:
    /// spawn/IO failure (binary vanished after the probe) **or** a
    /// spawnable-but-broken solver that answers `-version` then errors on
    /// every script (`Unknown("solver reported an error: …")`, G7).
    /// Timeouts / plain `unknown` answers do NOT set this. Verdicts
    /// degraded to UNKNOWN/POSSIBLY; the CLI warns as loudly as for
    /// no-solver-found. Absent in healthy runs (omitted from JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solver_degraded: Option<String>,
    /// Structured twin of [`Self::solver_degraded`]: the two failure counts
    /// plus the first reason verbatim. Present exactly when
    /// `solver_degraded` is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solver_failures: Option<SolverFailures>,
    /// Was the independent exact-rational certifier enabled for this run
    /// (`--certify`, the default)? Without it, `certified` is `None`
    /// everywhere and no tier carries a receipt — a fact about the RUN that
    /// no per-pair field can express.
    pub certification: bool,
    /// Sampling-gate accounting (proof-system v2 Phase 1): how many synthetic
    /// events every UNSAT-side PROVEN verdict was refuted against, and how
    /// many verdicts the gate demoted (each demotion also files an
    /// internal-contradiction diagnostic — a refutation is an encoder/axiom
    /// bug, not a user error). Absent when the gate is disabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling: Option<SamplingInfo>,
    /// Adversarial refute-gate accounting (trustworthy-verify M1): cut-
    /// anchored + flat-spot probe count and demotions. Absent when
    /// `--no-refute-gate` / `refute_gate: false`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refute: Option<RefuteInfo>,
    pub regions: Vec<RegionReport>,
    pub pairwise: Vec<PairReport>,
    pub bin_checks: Vec<BinCheckReport>,
    /// The reconciliation ledger (cross-file runs): how same-base
    /// collections from different analyses were related, including the
    /// pairs that could not be. Empty when reconciliation did not run.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reconciliations: Vec<ReconReport>,
    /// Advisories: structurally-identical collections blocked only by
    /// differing base names. Derive nothing; name a missing assumption.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub recon_near_misses: Vec<ReconNearMissReport>,
    pub axioms_used: Vec<AxiomUse>,
    /// Every internal diagnostic as a flat message list, in emission order.
    /// Kept verbatim for existing consumers; [`Self::diagnostics`] carries
    /// the same messages with their severity.
    pub internal_diagnostics: Vec<String>,
    /// The same diagnostics, classified: `fail_closed` (a claim withheld or
    /// capped — working as designed) vs `contradiction` (the engine refuting
    /// its own conclusion — a bug). Same order as
    /// [`Self::internal_diagnostics`], one entry each.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<Diagnostic>,
    /// Portable certificate bundles (`--combine`), one per certified
    /// PROVEN DISJOINT pair that survived to the final report. Not part
    /// of the versioned `--json` output — the CLI writes each bundle to
    /// its own file, re-checkable offline with `smash2-recheck`.
    #[serde(skip)]
    pub combine_bundles: Vec<adl_certify::CombineBundle>,
}

/// CI gating flags (SPEC_ANALYSIS §6): verdicts never fail the run by
/// default; `--fail-on=overlap|gap|empty|non-exact` opts in explicitly.
///
/// `gap` fires on bin coverage holes (`CoverageStatus::NotProven`) **and**
/// on unproven bin-pair disjointness (`disjoint_pairs_proven < total`) —
/// fail-closed so CI can gate on bins that may double-count (G6).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FailOn {
    pub overlap: bool,
    pub gap: bool,
    pub empty: bool,
    pub non_exact: bool,
    /// Gate on the analysis having been unable to answer: any UNKNOWN pair,
    /// or a run whose solver failed to produce usable answers. A systematically
    /// broken solver otherwise yields an all-UNKNOWN report at exit 0.
    pub unknown: bool,
}

impl FailOn {
    /// Parse a `--fail-on` value: comma-separated
    /// `overlap|gap|empty|non-exact|unknown`.
    ///
    /// # Errors
    /// Returns the offending token.
    pub fn parse(s: &str) -> Result<FailOn, String> {
        let mut out = FailOn::default();
        for tok in s.split(',').map(str::trim).filter(|t| !t.is_empty()) {
            match tok {
                "overlap" => out.overlap = true,
                "gap" => out.gap = true,
                "empty" => out.empty = true,
                "non-exact" | "non_exact" => out.non_exact = true,
                "unknown" => out.unknown = true,
                other => return Err(format!("unknown --fail-on value `{other}`")),
            }
        }
        Ok(out)
    }
}

/// Which reconciliation ledger rows to show (`--recon`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReconFilter {
    /// Every enumerated candidate pair, related or not (the default).
    #[default]
    All,
    /// Only the pairs a refinement was proven for (`≡` / `⊆` / `⊇`).
    Related,
}

impl ReconFilter {
    /// Parse a `--recon` value.
    ///
    /// # Errors
    /// Returns the offending token.
    pub fn parse(s: &str) -> Result<ReconFilter, String> {
        match s.trim() {
            "all" => Ok(ReconFilter::All),
            "related" => Ok(ReconFilter::Related),
            other => Err(format!("unknown --recon value `{other}` (all|related)")),
        }
    }
}

/// Presentation knobs for the human renderings. Defaults reproduce the
/// pre-existing behaviour exactly.
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderOptions {
    /// ANSI styling (callers must pass `false` off-tty / under `NO_COLOR`).
    pub color: bool,
    /// Print the verdict matrix regardless of region count (`--matrix`).
    pub force_matrix: bool,
    /// Reconciliation ledger filter (`--recon`).
    pub recon: ReconFilter,
}

/// Counts behind the trust summary block. Pure view over the report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TrustStats {
    pub proven_disjoint: usize,
    pub candidate_disjoint: usize,
    pub proven_overlapping: usize,
    pub candidate_overlapping: usize,
    pub possibly: usize,
    pub unknown: usize,
    /// PROVEN DISJOINT pairs carrying a replay-checked certificate.
    pub certified: usize,
    /// PROVEN OVERLAPPING pairs whose witness the interpreter accepted.
    pub witness_validated: usize,
    pub proven_subsets: usize,
    pub proven_empty: usize,
    pub candidate_empty: usize,
}

impl TrustStats {
    /// Percent of PROVEN DISJOINT pairs with a certificate, rounded down.
    /// `None` when there are no such pairs (no denominator to speak of).
    #[must_use]
    pub fn certified_pct(&self) -> Option<usize> {
        (self.proven_disjoint > 0).then(|| self.certified * 100 / self.proven_disjoint)
    }
}

impl Report {
    /// Verdict/evidence counts for the trust summary.
    #[must_use]
    pub fn trust_stats(&self) -> TrustStats {
        let mut t = TrustStats::default();
        for p in &self.pairwise {
            match p.kind {
                VerdictKind::ProvenDisjoint => {
                    t.proven_disjoint += 1;
                    if p.certified == Some(true) {
                        t.certified += 1;
                    }
                }
                VerdictKind::CandidateDisjoint => t.candidate_disjoint += 1,
                VerdictKind::ProvenOverlapping => {
                    t.proven_overlapping += 1;
                    if p.witness_validated == Some(true) {
                        t.witness_validated += 1;
                    }
                }
                VerdictKind::CandidateOverlapping => t.candidate_overlapping += 1,
                VerdictKind::PossiblyOverlapping => t.possibly += 1,
                VerdictKind::Unknown => t.unknown += 1,
            }
            t.proven_subsets += usize::from(p.subset_a_in_b) + usize::from(p.subset_b_in_a);
        }
        for r in &self.regions {
            match r.empty {
                EmptyStatus::Proven => t.proven_empty += 1,
                EmptyStatus::Candidate => t.candidate_empty += 1,
                EmptyStatus::NotProven | EmptyStatus::Unknown => {}
            }
        }
        t
    }

    /// The distinct soundness assumptions in force, as `assumption (IDS)`
    /// clauses in axiom-catalog order. Axioms assuming nothing are omitted.
    #[must_use]
    pub fn assumption_clauses(&self) -> Vec<String> {
        let mut by_assumption: Vec<(&str, Vec<&str>)> = Vec::new();
        for a in &self.axioms_used {
            if a.assumption == "none" {
                continue;
            }
            match by_assumption.iter_mut().find(|(k, _)| *k == a.assumption) {
                Some((_, ids)) => ids.push(&a.id),
                None => by_assumption.push((&a.assumption, vec![&a.id])),
            }
        }
        by_assumption
            .into_iter()
            .map(|(k, ids)| format!("{k} ({})", ids.join(", ")))
            .collect()
    }

    /// The findings selected by `fail_on`, as human lines. Empty ⇒ the
    /// run passes the gate.
    #[must_use]
    pub fn findings(&self, fail_on: &FailOn) -> Vec<String> {
        let mut out = Vec::new();
        if fail_on.overlap {
            for p in &self.pairwise {
                match p.kind {
                    VerdictKind::ProvenOverlapping => {
                        out.push(format!("overlap: {} vs {}", p.a, p.b));
                    }
                    VerdictKind::CandidateOverlapping => {
                        out.push(format!("candidate overlap: {} vs {}", p.a, p.b));
                    }
                    _ => {}
                }
            }
        }
        if fail_on.gap {
            for b in &self.bin_checks {
                // Fail-closed (G6): coverage holes AND unproven bin-pair
                // disjointness both fire — bins that may double-count are a
                // physics finding CI must be able to gate on, matching the
                // human findings renderer.
                if b.coverage == CoverageStatus::NotProven {
                    out.push(format!(
                        "gap: {} [{}] bin coverage not proven",
                        b.region, b.variable
                    ));
                }
                if b.disjoint_pairs_proven < b.disjoint_pairs_total {
                    out.push(format!(
                        "gap: {} [{}] bin pair disjointness not proven",
                        b.region, b.variable
                    ));
                }
            }
        }
        if fail_on.empty {
            for r in &self.regions {
                if r.empty == EmptyStatus::Proven {
                    out.push(format!(
                        "empty: region {} provably selects no events",
                        r.name
                    ));
                }
            }
        }
        if fail_on.non_exact {
            for r in &self.regions {
                if !r.exact {
                    out.push(format!(
                        "non-exact: region {} encoding is not exact",
                        r.name
                    ));
                }
            }
        }
        if fail_on.unknown {
            if let Some(why) = &self.solver_degraded {
                out.push(format!("unknown: {why}"));
            }
            for p in &self.pairwise {
                if p.kind == VerdictKind::Unknown {
                    out.push(format!("unknown: {} vs {}", p.a, p.b));
                }
            }
        }
        out
    }

    /// Exit code under `fail_on`: 0 when no selected finding fired,
    /// 4 otherwise (parse/sema errors are the caller's 1/2 territory).
    #[must_use]
    pub fn exit_code(&self, fail_on: &FailOn) -> i32 {
        if self.findings(fail_on).is_empty() {
            0
        } else {
            4
        }
    }

    /// Versioned JSON (stable field and element order; byte-identical
    /// across runs of the same input).
    ///
    /// # Panics
    /// Never in practice: the report contains no non-string keys.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("report serializes")
    }

    /// The default human report: trust summary, findings, aligned region
    /// table, verdict matrix, pairwise verdicts grouped by identical
    /// (verdict, trust annotation, reason-signature). Deterministic;
    /// `color` adds ANSI styling (callers must pass `false` off-tty / under
    /// `NO_COLOR`). Full per-claim evidence stays in [`Report::human`]
    /// (`--explain`).
    #[must_use]
    pub fn human_default(&self, color: bool) -> String {
        self.render_default(&RenderOptions {
            color,
            ..RenderOptions::default()
        })
    }

    /// [`Self::human_default`] with the presentation flags (`--matrix`,
    /// `--recon`) the CLI exposes.
    #[must_use]
    pub fn render_default(&self, opts: &RenderOptions) -> String {
        crate::render::render_default(self, opts)
    }

    /// Deterministic human report with full per-claim evidence (proof route,
    /// certificate size, complete unsat cores with their axiom statements,
    /// gate coverage, witnesses) — the `--explain` rendering.
    #[must_use]
    pub fn human(&self) -> String {
        self.render_explain(&RenderOptions::default())
    }

    /// [`Self::human`] with the presentation flags the CLI exposes.
    #[must_use]
    pub fn render_explain(&self, opts: &RenderOptions) -> String {
        crate::render::render_explain(self, opts)
    }
}

/// The model caveat printed with every PROVEN OVERLAPPING
/// (SPEC_ANALYSIS §2).
pub const OVERLAP_CAVEAT: &str = "a model exists in the per-event scalar fragment; opaque \
     external-function values and padded out-of-range element variables are free — the witness \
     is a candidate, not a simulated event";
