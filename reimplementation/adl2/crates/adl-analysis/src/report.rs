//! Report data model: versioned JSON schema (SPEC_ANALYSIS §6) + the
//! deterministic human rendering. Stable ordering throughout: regions in
//! declaration order, pairs in (i, j) declaration order, values sorted
//! by label.

use serde::Serialize;

/// Bumped on any breaking schema change.
pub const SCHEMA_VERSION: u32 = 1;

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
    Proven,
    NotProven,
    /// Solver inconclusive / unavailable for this check.
    Unknown,
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
    /// Explanation when `empty == Proven`.
    pub empty_core: Vec<CoreItem>,
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
    pub core: Vec<CoreItem>,
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
    pub regions: Vec<RegionReport>,
    pub pairwise: Vec<PairReport>,
    pub bin_checks: Vec<BinCheckReport>,
    pub axioms_used: Vec<AxiomUse>,
    /// Internal-error diagnostics (e.g. a witness the interpreter
    /// rejected — TESTING §3; each one is a bug report, not user error).
    pub internal_diagnostics: Vec<String>,
}

/// CI gating flags (SPEC_ANALYSIS §6): verdicts never fail the run by
/// default; `--fail-on=overlap|gap|empty|non-exact` opts in explicitly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FailOn {
    pub overlap: bool,
    pub gap: bool,
    pub empty: bool,
    pub non_exact: bool,
}

impl FailOn {
    /// Parse a `--fail-on` value: comma-separated
    /// `overlap|gap|empty|non-exact`.
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
                other => return Err(format!("unknown --fail-on value `{other}`")),
            }
        }
        Ok(out)
    }
}

impl Report {
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
                if b.coverage == CoverageStatus::NotProven {
                    out.push(format!(
                        "gap: {} [{}] bin coverage not proven",
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

    /// The default human report: findings first, aligned region table,
    /// verdict matrix (3–20 regions), pairwise verdicts grouped by
    /// identical (verdict, reason-signature). Deterministic; `color`
    /// adds ANSI styling (callers must pass `false` off-tty / under
    /// `NO_COLOR`). Full per-pair detail stays in [`Report::human`]
    /// (`--explain`).
    #[must_use]
    pub fn human_default(&self, color: bool) -> String {
        crate::render::render_default(self, color)
    }

    /// Deterministic human report with full per-pair detail (complete
    /// unsat cores, witnesses, per-axiom statements) — the `--explain`
    /// rendering.
    #[must_use]
    pub fn human(&self) -> String {
        use std::fmt::Write as _;
        let mut s = String::new();
        let _ = writeln!(s, "ADL2 analysis report — {}", self.unit);
        let _ = writeln!(s, "solver: {}", self.solver);
        let _ = writeln!(s, "\n== regions ==");
        for r in &self.regions {
            let mut line = format!(
                "{}: encoded leaves {}/{}",
                r.name, r.leaves_encoded, r.leaves_total
            );
            if r.exact {
                line.push_str(" (exact)");
            }
            if r.or_clauses > 0 {
                let _ = write!(line, " ({} OR)", r.or_clauses);
            }
            if r.dual_hedges > 0 {
                let _ = write!(line, " ({} dual)", r.dual_hedges);
            }
            let _ = writeln!(s, "{line}");
            for d in &r.dropped {
                let _ = writeln!(s, "  dropped (line {}): {}", d.line, d.reason);
            }
            if r.empty == EmptyStatus::Proven {
                let core = r
                    .empty_core
                    .iter()
                    .map(CoreItem::human)
                    .collect::<Vec<_>>()
                    .join(" with ");
                let _ = writeln!(
                    s,
                    "  region {} provably selects no events — UNSAT: {core}",
                    r.name
                );
            }
        }
        if !self.bin_checks.is_empty() {
            let _ = writeln!(s, "\n== bins ==");
            for b in &self.bin_checks {
                let coverage = match b.coverage {
                    CoverageStatus::Proven => "proven".to_owned(),
                    CoverageStatus::NotProven => {
                        let mut t = "not proven".to_owned();
                        if !b.gap_witness.is_empty() {
                            let vals = b
                                .gap_witness
                                .iter()
                                .map(|w| format!("{} = {}", w.quantity, w.value))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let _ = write!(t, " (gap witness: {vals})");
                        }
                        t
                    }
                    CoverageStatus::Unknown => "unknown".to_owned(),
                };
                let _ = writeln!(
                    s,
                    "{} [{}]: {} bins; disjoint {}/{} pairs; coverage: {}",
                    b.region,
                    b.variable,
                    b.n_bins,
                    b.disjoint_pairs_proven,
                    b.disjoint_pairs_total,
                    coverage
                );
            }
        }
        let _ = writeln!(s, "\n== pairwise ==");
        for p in &self.pairwise {
            let _ = writeln!(s, "{} vs {}: {} — {}", p.a, p.b, p.kind.human(), p.reason);
            if !p.witness.is_empty() {
                let vals = p
                    .witness
                    .iter()
                    .map(|w| {
                        if w.derived {
                            format!("{} = {} (axiom-derived)", w.quantity, w.value)
                        } else {
                            format!("{} = {}", w.quantity, w.value)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let validated = match p.witness_validated {
                    Some(true) => " [witness validated by interpreter]",
                    Some(false) => " [witness is a candidate (not interpreter-checkable)]",
                    None => "",
                };
                let _ = writeln!(s, "  witness: {vals}{validated}");
            }
            if p.subset_a_in_b {
                let _ = writeln!(s, "  PROVEN SUBSET: {} within {}", p.a, p.b);
            }
            if p.subset_b_in_a {
                let _ = writeln!(s, "  PROVEN SUBSET: {} within {}", p.b, p.a);
            }
        }
        let _ = writeln!(s, "\n== axioms used ==");
        for a in &self.axioms_used {
            let _ = writeln!(
                s,
                "{} ({} instances; assumes: {})",
                a.id, a.instances, a.assumption
            );
        }
        if !self.internal_diagnostics.is_empty() {
            let _ = writeln!(s, "\n== INTERNAL DIAGNOSTICS (bugs, please report) ==");
            for d in &self.internal_diagnostics {
                let _ = writeln!(s, "{d}");
            }
        }
        let mut counts = (0usize, 0usize, 0usize, 0usize, 0usize);
        for p in &self.pairwise {
            match p.kind {
                VerdictKind::ProvenDisjoint => counts.0 += 1,
                VerdictKind::ProvenOverlapping => counts.1 += 1,
                VerdictKind::CandidateOverlapping => counts.2 += 1,
                VerdictKind::PossiblyOverlapping => counts.3 += 1,
                VerdictKind::Unknown => counts.4 += 1,
            }
        }
        let _ = writeln!(
            s,
            "\n== summary ==\npairs: {}; proven disjoint: {}; proven overlapping: {}; candidate overlapping: {}; possibly overlapping: {}; unknown: {}",
            self.pairwise.len(),
            counts.0,
            counts.1,
            counts.2,
            counts.3,
            counts.4
        );
        crate::render::fix_negative_zero(&s)
    }
}

/// The model caveat printed with every PROVEN OVERLAPPING
/// (SPEC_ANALYSIS §2).
pub const OVERLAP_CAVEAT: &str = "a model exists in the per-event scalar fragment; opaque \
     external-function values and padded out-of-range element variables are free — the witness \
     is a candidate, not a simulated event";
