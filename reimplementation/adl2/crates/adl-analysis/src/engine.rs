//! The pairwise verdict engine (SPEC_ANALYSIS §2–§5).
//!
//! Pipeline per pair: interval fast path on the unconditional And-spine
//! of the over-projections (sound; also the no-solver fallback) → solver
//! checks batched in one incremental session (push/pop frames over a
//! base frame holding the axiom set) → witness/core extraction for
//! proven verdicts, with interpreter re-validation of every witness
//! (TESTING §3) and unsat cores mapped back to source spans (§3).
//!
//! Soundness polarity is enforced in the types: the disjoint/empty/
//! superset side of every check consumes [`Over`] projections, the
//! overlap/subset-inner side consumes [`Under`] projections — these are
//! the only verdict constructors (ADR-004).

use crate::encode::{BinSetEnc, RegionEnc, UnitEnc};
use crate::interval::IntervalMap;
use crate::report::{
    AxiomUse, BinCheckReport, CoreItem, CoverageStatus, EmptyStatus, OVERLAP_CAVEAT, PairReport,
    RegionReport, Report, SCHEMA_VERSION, VerdictKind, WitnessValue,
};
use crate::witness::{Validation, validate_witness};
use adl_axioms::{AxiomId, AxiomSet, catalog_entry, derived_size_le, quantity_label, twin_pairs};
use adl_formula::{Over, QFormula, Under};
use adl_interp::Interp;
use adl_sema::{ElemIndex, ExtDecls, Hir, Quantity, QuantityId, Rat};
use adl_solver::{AssertName, Model, QSort, SatResult, Solver};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

pub(crate) struct Engine<'a> {
    pub hir: &'a Hir,
    pub ext: &'a ExtDecls,
    pub unit: &'a UnitEnc,
    pub axioms: &'a AxiomSet,
    pub solver: Option<Box<dyn Solver>>,
    pub solver_label: String,
    pub timeout: Duration,
    pub unit_name: String,
    /// Cross/intra-collection reconciliation encoding, present only in an
    /// explicit `verify --cross` run. Consumed once by [`Self::reconcile`],
    /// which asserts the proven `size(A) <= size(B)` facts at the base frame
    /// before the pairwise loop.
    pub recon: Option<crate::reconcile::ReconEnc>,
    /// Checks whose result was Unknown because the solver process could not
    /// run at all (spawn/IO failure — e.g. the binary vanished after the
    /// probe). Surfaced via [`Report::solver_degraded`] so the CLI warns.
    pub spawn_failures: usize,
    /// The sampling-gate battery (proof-system v2 Phase 1): deterministic
    /// loader-valid events every UNSAT-side PROVEN verdict is refuted against
    /// through the reference interpreter before being reported. Empty = gate
    /// disabled.
    pub gate_events: Vec<adl_interp::Event>,
    /// Certify disjointness proofs through the independent exact-rational
    /// checker (`adl-certify`, proof-system v2 Phase 4): solver-UNSAT pairs
    /// whose core cannot be certified demote to CANDIDATE DISJOINT.
    pub certify: bool,
    /// The reconciliation facts asserted at the persistent frame, retained by
    /// name so a certified core containing an `XR{k}` member can map it back
    /// to its formula.
    pub recon_facts: Vec<(AssertName, QFormula)>,
}

/// Bounded witness retry: how many distinct overlap models to try to
/// realize before downgrading to POSSIBLY.
const MAX_WITNESS_ATTEMPTS: u32 = 6;

/// ε for the interior-model wish: far above any f64 rounding error the
/// interpreter's re-evaluation can accumulate (≤ ~1e-12 for sums of
/// physical magnitudes), far below any physical cut granularity, and
/// **dyadic** (2⁻²⁰) so tightened bounds stay exactly representable —
/// a decimal ε would smear model values off the f64 grid and break
/// equality atoms over sums.
const WITNESS_EPS: f64 = 9.5367431640625e-7; // 2^-20

/// A finite `f64` (axiom/hint/witness constant) as an exact `Rat`.
fn rat(v: f64) -> Rat {
    Rat::from_decimal_f64(v).expect("finite constant")
}

/// Label for a collection-size quantity using the id-disambiguated
/// `size(C3#jets)` form the rendered cut text uses (falls back to the plain
/// quantity label for non-Size quantities).
fn size_label(hir: &Hir, q: QuantityId) -> String {
    match hir.table.quantity(q) {
        Quantity::Size(c) => format!("size({})", adl_sema::collection_ref(hir, *c)),
        _ => quantity_label(hir, q),
    }
}

/// Snap every model value to the dyadic 2⁻²² grid (second-chance
/// realization). A solver vertex can sit at a non-dyadic rational where
/// two quantities share a non-representable fractional part — their
/// exact difference then misses an equality bound after independent f64
/// rounding. Snapping moves equal fractional parts identically (exact
/// differences survive) and stays far inside the ε-interior margins;
/// the interpreter re-validation still decides, so this is pure search.
fn snap_model(model: &Model) -> Model {
    const GRID: f64 = 4_194_304.0; // 2^22
    let snapped = model
        .iter()
        .map(|(q, v)| {
            let s = if v.is_finite() && v.abs() < 1e9 {
                (v * GRID).round() / GRID
            } else {
                v
            };
            (q, s)
        })
        .collect();
    Model::from_values(snapped)
}

/// `¬(⋀ q = v)` over the mentioned quantities of `model`: excludes this
/// assignment so the solver proposes a different overlap model.
fn blocking_clause(model: &Model, mentioned: &BTreeSet<QuantityId>) -> Option<QFormula> {
    let mut parts = Vec::new();
    for &q in mentioned {
        if let Some(v) = model.get(q)
            && v.is_finite()
            && let Some(rv) = Rat::from_decimal_f64(v)
        {
            let atom = adl_formula::LinAtom::single(q, adl_formula::Rel::Ne, rv);
            parts.push(QFormula::Atom(atom));
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(QFormula::Or(parts))
    }
}

/// Whether any mentioned quantity addresses an element from the back
/// (`coll[-k]`), directly or as an angular-separation anchor.
fn mentions_back_index(hir: &Hir, quantities: &BTreeSet<QuantityId>) -> bool {
    use adl_sema::{ElemIndex, ParticleRef};
    let is_back = |i: &ElemIndex| matches!(i, ElemIndex::FromBack(_));
    quantities.iter().any(|&q| match hir.table.quantity(q) {
        Quantity::ElemProp { index, .. } => is_back(index),
        Quantity::AngularSep { a, b, .. } => [a, b].iter().any(|p| {
            matches!(p, ParticleRef::Elem { index, .. } if is_back(index))
        }),
        _ => false,
    })
}

/// Per-region precomputation.
struct RegionCtx {
    overs: Vec<(AssertName, Over)>,
    unders: Vec<Under>,
    intervals: IntervalMap,
}

impl Engine<'_> {
    pub fn run(mut self) -> Report {
        let interp = Interp::new(self.hir, self.ext);
        let mut internal: Vec<String> = Vec::new();

        // Name -> origin map for core explanations.
        let mut origins: BTreeMap<AssertName, CoreItem> = BTreeMap::new();
        for r in &self.unit.regions {
            for s in &r.stmts {
                origins.insert(
                    s.name.clone(),
                    CoreItem::Cut {
                        region: r.name.clone(),
                        line: s.line,
                        text: s.text.clone(),
                    },
                );
            }
        }
        for (i, inst) in self.axioms.instances.iter().enumerate() {
            origins.insert(
                AssertName::new(format!("AX{i}")),
                CoreItem::Axiom {
                    id: inst.id.as_str().to_owned(),
                    statement: inst.description.clone(),
                },
            );
        }

        // Base frame: declare sorts, assert the axiom set (named).
        if let Some(s) = self.solver.as_deref_mut() {
            let mut all_q: BTreeSet<QuantityId> = BTreeSet::new();
            for r in &self.unit.regions {
                all_q.extend(&r.quantities);
            }
            all_q.extend(self.axioms.quantities());
            for set in &self.unit.bin_sets {
                for f in &set.bins {
                    crate::encode::formula_quantities(f, &mut all_q);
                }
            }
            // Reconciliation grounds each candidate predicate onto a shared
            // generic base element; those helper quantities (and the candidate
            // sizes) must be declared before the subset frames assert them.
            if let Some(recon) = self.recon.as_ref() {
                all_q.extend(recon.quantities());
            }
            for &q in &all_q {
                let sort = match self.hir.table.quantity(q) {
                    Quantity::Size(_) => QSort::Int,
                    _ => QSort::Real,
                };
                s.declare(q, sort);
            }
            for (i, inst) in self.axioms.instances.iter().enumerate() {
                s.assert(&inst.formula, Some(AssertName::new(format!("AX{i}"))));
            }
        }

        // Cross/intra reconciliation: prove each candidate pair's refinement
        // and assert the derived size facts at the base frame (persistent for
        // every pairwise push/pop below). Runs after the base axioms so the
        // proofs see them, and before the pairwise loop so the sizes relate.
        let recon_counts = self.reconcile(&mut origins);

        // Per-region projections + interval maps.
        let ctxs: Vec<RegionCtx> = self
            .unit
            .regions
            .iter()
            .map(|r| {
                let overs: Vec<(AssertName, Over)> =
                    r.stmts.iter().map(|s| (s.name.clone(), s.over())).collect();
                let unders: Vec<Under> =
                    r.stmts.iter().map(crate::encode::StmtEnc::under).collect();
                let mut intervals = IntervalMap::default();
                for (_, o) in &overs {
                    intervals.add_over(o.qformula());
                }
                RegionCtx {
                    overs,
                    unders,
                    intervals,
                }
            })
            .collect();

        // -- region reports (coverage + empty) -------------------------------
        let mut gate_refutations = 0usize;
        let mut region_reports = Vec::new();
        for (r, ctx) in self.unit.regions.iter().zip(&ctxs) {
            let (mut empty, mut empty_core) = self.region_empty(ctx, &origins);
            if empty == EmptyStatus::Proven
                && self.gate_empty(r.idx, &interp, &mut internal, &mut gate_refutations)
            {
                empty = EmptyStatus::NotProven;
                empty_core = Vec::new();
            }
            region_reports.push(RegionReport {
                name: r.name.clone(),
                leaves_encoded: r.leaves_encoded,
                leaves_total: r.leaves_total,
                exact: r.exact(),
                or_clauses: r.or_clauses,
                dual_hedges: r.dual_hedges,
                dropped: r
                    .dropped
                    .iter()
                    .map(|(line, reason)| crate::report::DroppedLeaf {
                        line: *line,
                        reason: reason.clone(),
                    })
                    .collect(),
                empty,
                empty_core,
            });
        }

        // -- pairwise ---------------------------------------------------------
        let mut pairwise = Vec::new();
        for i in 0..self.unit.regions.len() {
            for j in i + 1..self.unit.regions.len() {
                let mut pair = self.pair(
                    &self.unit.regions[i],
                    &self.unit.regions[j],
                    &ctxs[i],
                    &ctxs[j],
                    &origins,
                    &interp,
                    &mut internal,
                );
                self.gate_pair(
                    &mut pair,
                    self.unit.regions[i].idx,
                    self.unit.regions[j].idx,
                    &interp,
                    &mut internal,
                    &mut gate_refutations,
                );
                pairwise.push(pair);
            }
        }

        // -- bins --------------------------------------------------------------
        let mut bin_checks = Vec::new();
        for set in &self.unit.bin_sets {
            let report = self.bin_check(set, &ctxs[set.region_idx]);
            bin_checks.push(report);
        }

        // -- axioms used ---------------------------------------------------------
        let mut axiom_counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        for inst in &self.axioms.instances {
            *axiom_counts.entry(inst.id.as_str()).or_insert(0) += 1;
        }
        for (id, n) in recon_counts {
            *axiom_counts.entry(id).or_insert(0) += n;
        }
        let axioms_used = adl_axioms::AxiomId::ALL
            .into_iter()
            .filter_map(|id| {
                axiom_counts.get(id.as_str()).map(|&n| {
                    let e = catalog_entry(id);
                    AxiomUse {
                        id: id.as_str().to_owned(),
                        statement: e.statement.to_owned(),
                        assumption: e.assumption.to_owned(),
                        instances: n,
                    }
                })
            })
            .collect();

        Report {
            schema_version: SCHEMA_VERSION,
            unit: self.unit_name.clone(),
            solver: self.solver_label.clone(),
            sampling: (!self.gate_events.is_empty()).then_some(crate::report::SamplingInfo {
                events: self.gate_events.len(),
                refutations: gate_refutations,
            }),
            solver_degraded: (self.spawn_failures > 0).then(|| {
                format!(
                    "{} solver check(s) could not run (the `{}` process failed to \
                     spawn); affected verdicts degraded to UNKNOWN/POSSIBLY",
                    self.spawn_failures, self.solver_label
                )
            }),
            regions: region_reports,
            pairwise,
            bin_checks,
            axioms_used,
            internal_diagnostics: internal,
        }
    }

    fn check(&mut self, timeout: Duration) -> Option<SatResult> {
        let r = self.solver.as_deref_mut().map(|s| s.check(timeout));
        if let Some(SatResult::Unknown(reason)) = &r
            && reason.contains("spawn")
        {
            self.spawn_failures += 1;
        }
        r
    }

    fn push(&mut self) {
        if let Some(s) = self.solver.as_deref_mut() {
            s.push();
        }
    }

    fn pop(&mut self) {
        if let Some(s) = self.solver.as_deref_mut() {
            s.pop();
        }
    }

    fn assert_overs(&mut self, overs: &[(AssertName, Over)], named: bool) {
        if let Some(s) = self.solver.as_deref_mut() {
            for (name, o) in overs {
                s.assert(o.qformula(), named.then(|| name.clone()));
            }
        }
    }

    fn assert_unders(&mut self, unders: &[Under]) {
        if let Some(s) = self.solver.as_deref_mut() {
            for u in unders {
                s.assert(u.qformula(), None);
            }
        }
    }

    /// `¬(R⁻)` for the subset/coverage checks: the under-projection of a
    /// region is the conjunction of its statement unders, so its exact
    /// negation is the disjunction of their NNF negations.
    fn negated_under(unders: &[Under]) -> QFormula {
        QFormula::Or(unders.iter().map(|u| u.qformula().clone().not()).collect())
    }

    fn core_items(&mut self, origins: &BTreeMap<AssertName, CoreItem>) -> Vec<CoreItem> {
        let Some(s) = self.solver.as_deref_mut() else {
            return Vec::new();
        };
        let names = s.unsat_core().unwrap_or_default();
        names
            .into_iter()
            .filter_map(|n| origins.get(&n).cloned())
            .collect()
    }

    fn core_reason(items: &[CoreItem]) -> String {
        if items.is_empty() {
            return "UNSAT (no core available)".to_owned();
        }
        let cuts: Vec<String> = items
            .iter()
            .filter(|c| matches!(c, CoreItem::Cut { .. }))
            .map(CoreItem::human)
            .collect();
        let axs: Vec<String> = items
            .iter()
            .filter(|c| matches!(c, CoreItem::Axiom { .. }))
            .map(CoreItem::human)
            .collect();
        let mut reason = format!("UNSAT core: {}", cuts.join(" cannot hold together with "));
        if cuts.len() == 1 {
            reason = format!("UNSAT core: {} cannot hold", cuts[0]);
        }
        if !axs.is_empty() {
            reason.push_str(&format!(" (using {})", axs.join(", ")));
        }
        reason
    }

    fn region_empty(
        &mut self,
        ctx: &RegionCtx,
        origins: &BTreeMap<AssertName, CoreItem>,
    ) -> (EmptyStatus, Vec<CoreItem>) {
        if ctx.intervals.self_empty().is_some() {
            return (EmptyStatus::Proven, Vec::new());
        }
        if self.solver.is_none() {
            return (EmptyStatus::Unknown, Vec::new());
        }
        self.push();
        self.assert_overs(&ctx.overs, true);
        let result = self.check(self.timeout);
        let out = match result {
            Some(SatResult::Unsat) => {
                let items = self.core_items(origins);
                (EmptyStatus::Proven, items)
            }
            Some(SatResult::Sat) => (EmptyStatus::NotProven, Vec::new()),
            _ => (EmptyStatus::Unknown, Vec::new()),
        };
        self.pop();
        out
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn pair(
        &mut self,
        ra: &RegionEnc,
        rb: &RegionEnc,
        ca: &RegionCtx,
        cb: &RegionCtx,
        origins: &BTreeMap<AssertName, CoreItem>,
        interp: &Interp<'_>,
        internal: &mut Vec<String>,
    ) -> PairReport {
        let shared: Vec<QuantityId> = ra
            .quantities
            .intersection(&rb.quantities)
            .copied()
            .collect();
        let shared_dimensions: Vec<String> = shared
            .iter()
            .map(|&q| quantity_label(self.hir, q))
            .collect();
        let exact = ra.exact() && rb.exact();
        let mut report = PairReport {
            a: ra.name.clone(),
            b: rb.name.clone(),
            kind: VerdictKind::PossiblyOverlapping,
            reason: String::new(),
            exact,
            shared_dimensions,
            subset_a_in_b: false,
            subset_b_in_a: false,
            witness: Vec::new(),
            witness_validated: None,
            certified: None,
            core: Vec::new(),
        };

        // 1. Interval fast path (also the no-solver fallback).
        if let Some((q, ia, ib)) = ca.intervals.disjoint_with(&cb.intervals) {
            report.kind = VerdictKind::ProvenDisjoint;
            report.reason = format!(
                "intervals cannot intersect on {}: {} requires {}, {} requires {}",
                quantity_label(self.hir, q),
                ra.name,
                ia.human(),
                rb.name,
                ib.human()
            );
            return report;
        }
        for (ctx, enc) in [(ca, ra), (cb, rb)] {
            if let Some(why) = ctx.intervals.self_empty() {
                report.kind = VerdictKind::ProvenDisjoint;
                report.reason = format!(
                    "region {} provably selects no events ({why}), so the pair cannot intersect",
                    enc.name
                );
                return report;
            }
        }

        if self.solver.is_none() {
            report.kind = VerdictKind::PossiblyOverlapping;
            report.reason =
                "no solver available: interval heuristics only, verdict capped at POSSIBLY"
                    .to_owned();
            return report;
        }

        // Canonical solver order (by region name): the solver sees the
        // same query sequence regardless of declaration order, so model
        // selection — and therefore witness validation — is symmetric
        // under swap(A, B) (metamorphic battery).
        let a_first = ra.name <= rb.name;
        let (c1, c2) = if a_first { (ca, cb) } else { (cb, ca) };

        // 2. Disjointness: UNSAT(Ax ∧ A⁺ ∧ B⁺).
        self.push();
        self.assert_overs(&c1.overs, true);
        self.assert_overs(&c2.overs, true);
        let disjoint_result = self.check(self.timeout);
        if matches!(disjoint_result, Some(SatResult::Unsat)) {
            // Fetch the core names ONCE: they feed both the human
            // explanation and, under --certify, the certifier's checked set.
            let core_names = self
                .solver
                .as_deref_mut()
                .and_then(adl_solver::Solver::unsat_core);
            let items: Vec<CoreItem> = core_names
                .iter()
                .flatten()
                .filter_map(|n| origins.get(n).cloned())
                .collect();
            let certified = self.certify_disjoint(core_names.as_deref(), c1, c2);
            self.pop();
            report.certified = certified;
            if certified == Some(false) {
                // Solver said UNSAT but the independent exact-rational
                // certifier could not verify the proof: a candidate, not a
                // claim (proof-system v2 Phase 4 — mirrors the overlap
                // side's candidate tier).
                report.kind = VerdictKind::CandidateDisjoint;
                report.reason = format!(
                    "solver reported UNSAT but the proof could not be independently \
                     certified (budget, shape, or an integrality-only refutation); \
                     candidate, not a claim — {}",
                    Self::core_reason(&items)
                );
            } else {
                report.kind = VerdictKind::ProvenDisjoint;
                report.reason = Self::core_reason(&items);
            }
            report.core = items;
            return report;
        }
        self.pop();

        // Subset checks are UNSAT-direction and unaffected by twin caps
        // (canonical query order; results mapped back to a/b).
        let one_in_two = self.subset(c1.overs.iter().map(|(_, o)| o), &c2.unders);
        let two_in_one = self.subset(c2.overs.iter().map(|(_, o)| o), &c1.unders);
        (report.subset_a_in_b, report.subset_b_in_a) = if a_first {
            (one_in_two, two_in_one)
        } else {
            (two_in_one, one_in_two)
        };

        // 3. SAT-direction caps (SPEC_ANALYSIS §2/§4).
        let mut combined: BTreeSet<QuantityId> = ra.quantities.clone();
        combined.extend(&rb.quantities);
        let twins = twin_pairs(&self.hir.table, &combined);
        if !twins.is_empty() {
            report.kind = VerdictKind::PossiblyOverlapping;
            let (t1, t2) = &twins[0];
            report.reason = format!(
                "convention-ambiguous oriented twin pair present ({} / {}): SAT-direction \
                 verdicts capped at POSSIBLY until OPEN-2 is resolved",
                quantity_label(self.hir, *t1),
                quantity_label(self.hir, *t2)
            );
            return report;
        }

        // 4. Overlap: SAT(Ax ∧ A⁻ ∧ B⁻) + witness re-validation.
        self.push();
        self.assert_unders(&c1.unders);
        self.assert_unders(&c2.unders);
        let overlap_result = self.check(self.timeout);
        match overlap_result {
            Some(SatResult::Sat) => {
                if report.shared_dimensions.is_empty() {
                    self.pop();
                    report.kind = VerdictKind::PossiblyOverlapping;
                    report.reason = "under-approximations intersect but the regions share no \
                                     dimension; capped at POSSIBLY"
                        .to_owned();
                    return report;
                }
                // A back-indexed element (`coll[-k]`) is a sound free leaf for
                // the UNSAT (disjoint/subset) direction, but the witness
                // builder cannot realize it: its value is constrained by the
                // pT-descending input invariant the encoder does not axiomatize
                // on back-indices, so a SAT model may be unrealizable and the
                // interpreter check is model-dependent. Treat it like an opaque
                // quantity — cap the overlap at POSSIBLY rather than chase a
                // model-dependent (and metamorphically unstable) witness.
                if mentions_back_index(self.hir, &combined) {
                    self.pop();
                    report.kind = VerdictKind::PossiblyOverlapping;
                    report.reason = "under-approximations intersect, but a back-indexed element \
                                     (`coll[-k]`) is not realizable by the witness builder; \
                                     capped at POSSIBLY"
                        .to_owned();
                    return report;
                }
                // Witness search with bounded retry: a Rejected
                // validation says THIS model could not be realized, not
                // that the overlap is unreal — block the assignment and
                // ask for a different model before downgrading, so the
                // verdict depends on realizability, not on the solver's
                // arbitrary first model (metamorphic stability).
                let interior: Vec<QFormula> = c1
                    .unders
                    .iter()
                    .chain(c2.unders.iter())
                    .map(|u| self.tightened(u.qformula()))
                    .collect();
                let mut last_reject: Option<String> = None;
                let mut outcome: Option<(Model, Validation)> = None;
                for _attempt in 0..MAX_WITNESS_ATTEMPTS {
                    let Some(model) = self.refined_model(&combined, &interior) else {
                        break;
                    };
                    let validation = validate_witness(
                        self.hir, self.ext, interp, &model, &combined, ra.idx, rb.idx,
                    );
                    let validation = match validation {
                        Validation::Rejected(first_why) => {
                            // Second chance on the dyadic grid before
                            // burning a solver retry.
                            let snapped = snap_model(&model);
                            match validate_witness(
                                self.hir, self.ext, interp, &snapped, &combined, ra.idx, rb.idx,
                            ) {
                                Validation::Rejected(_) => Validation::Rejected(first_why),
                                ok => {
                                    outcome = Some((snapped, ok));
                                    break;
                                }
                            }
                        }
                        ok => ok,
                    };
                    match validation {
                        Validation::Rejected(why) => {
                            last_reject = Some(why);
                            let Some(block) = blocking_clause(&model, &combined) else {
                                break;
                            };
                            let timeout = self.timeout;
                            let Some(s) = self.solver.as_deref_mut() else {
                                break;
                            };
                            s.assert(&block, None);
                            if !matches!(s.check(timeout), SatResult::Sat) {
                                break;
                            }
                        }
                        ok => {
                            outcome = Some((model, ok));
                            break;
                        }
                    }
                }
                self.pop();
                match outcome {
                    Some((model, Validation::Validated(json))) => {
                        report.witness = validated_witness_values(
                            self.hir, self.ext, interp, &json, &model, &combined,
                        );
                        report.kind = VerdictKind::ProvenOverlapping;
                        report.reason = format!(
                            "both region cut sets are satisfiable together ({OVERLAP_CAVEAT})"
                        );
                        report.witness_validated = Some(true);
                    }
                    Some((model, Validation::Candidate(why))) => {
                        report.witness = witness_values(self.hir, &model, &combined);
                        report.kind = VerdictKind::CandidateOverlapping;
                        report.reason = format!(
                            "a joint model exists but rests on an opaque quantity the \
                             interpreter cannot decide, so the witness is a candidate, not \
                             a proof ({OVERLAP_CAVEAT}); {why}"
                        );
                        report.witness_validated = Some(false);
                    }
                    Some((_, Validation::Rejected(_))) | None => {
                        report.kind = VerdictKind::PossiblyOverlapping;
                        match last_reject {
                            Some(why) => {
                                // A witness the interpreter rejects is only a
                                // genuine encoder/interpreter contradiction
                                // (release-blocking) when the interpreter could
                                // FULLY decide the region. If the rejection
                                // co-occurs with something the interpreter
                                // cannot decide, the region is not fully
                                // interpreter-checkable, so a rejected witness is
                                // expected: downgrade quietly, no internal-bug
                                // diagnostic. That covers (a) either region being
                                // inexact — any out-of-fragment construct
                                // (unresolved identifier, sorted/sliced/composite
                                // collection, member access) resolves to Unknown,
                                // so its witness need not realize; and (b) an
                                // opaque quantity / OPEN-1 leaf that is encodable
                                // but has no reference interpretation.
                                if !exact
                                    || why.contains("no reference interpretation")
                                    || why.contains("OPEN-1 unresolved")
                                    || why.contains("cannot evaluate")
                                    || why.contains("unresolved identifier")
                                {
                                    report.reason = format!(
                                        "under-approximations intersect, but no witness could \
                                         be realized through the interpreter (the region depends \
                                         on an opaque quantity); capped at POSSIBLY ({why})"
                                    );
                                } else {
                                    report.reason = format!(
                                        "overlap model found, but witness re-validation failed; \
                                         downgraded to POSSIBLY ({why})"
                                    );
                                    internal.push(format!(
                                        "INTERNAL: witness validation failed for {} vs {}: {why}",
                                        ra.name, rb.name
                                    ));
                                }
                            }
                            None => {
                                report.reason = "solver returned SAT but no model; capped at \
                                                 POSSIBLY"
                                    .to_owned();
                            }
                        }
                    }
                }
            }
            Some(SatResult::Unsat) => {
                self.pop();
                report.kind = VerdictKind::PossiblyOverlapping;
                report.reason = "over-approximations may intersect but under-approximations \
                                 cannot: an encoding gap blocks both a disjointness and an \
                                 overlap proof"
                    .to_owned();
            }
            Some(SatResult::Unknown(why)) => {
                self.pop();
                if let Some(SatResult::Unknown(dwhy)) = &disjoint_result {
                    report.kind = VerdictKind::Unknown;
                    report.reason =
                        format!("solver inconclusive in both directions ({dwhy}; {why})");
                } else {
                    report.kind = VerdictKind::PossiblyOverlapping;
                    report.reason = format!("solver inconclusive in the SAT direction ({why})");
                }
            }
            None => {
                self.pop();
                report.kind = VerdictKind::PossiblyOverlapping;
                report.reason = "no solver".to_owned();
            }
        }
        report
    }

    /// After a SAT overlap check, try to strengthen the model toward a
    /// realizable event: prefer ε-interior models of the under-formulas
    /// (z3's boundary vertices are exactly where exact-rational sums and
    /// the interpreter's f64 sums disagree by one ulp), require every
    /// mentioned element to actually exist (`size(C) > max mentioned
    /// index`, incl. angular-pair anchors), and keep every mentioned
    /// collection size within the witness realizer's cap. Sound: any
    /// model of the strengthened set is a model of the original; on
    /// UNSAT/Unknown the original model is used.
    fn refined_model(
        &mut self,
        mentioned: &BTreeSet<QuantityId>,
        interior: &[QFormula],
    ) -> Option<Model> {
        let mut lo_hints: BTreeMap<QuantityId, f64> = BTreeMap::new();
        let mut hi_hints: BTreeMap<QuantityId, f64> = BTreeMap::new();
        // dPhi wish: keep models inside the f64 wrap range [−π, π) the
        // interpreter can actually produce (the DPHI axiom's upper bound
        // is π + 1 ulp, an unrealizable sliver).
        let mut dphi_hints: Vec<QuantityId> = Vec::new();
        let mut need_elem = |hir: &Hir, coll: adl_sema::CollectionId, i: u32| {
            // The size quantity was interned eagerly before the engine
            // ran (lib.rs); a miss just skips the hint.
            if let Some(sq) = lookup_size(hir, coll) {
                let need = f64::from(i);
                let e = lo_hints.entry(sq).or_insert(need);
                *e = e.max(need);
            }
        };
        for &q in mentioned {
            match self.hir.table.quantity(q) {
                Quantity::ElemProp {
                    coll,
                    index: ElemIndex::FromFront(i),
                    ..
                } => need_elem(self.hir, *coll, *i),
                Quantity::AngularSep { kind, a, b, .. } => {
                    if *kind == adl_sema::AngKind::DPhi {
                        dphi_hints.push(q);
                    }
                    for p in [a, b] {
                        if let adl_sema::ParticleRef::Elem {
                            coll,
                            index: ElemIndex::FromFront(i),
                        } = p
                        {
                            need_elem(self.hir, *coll, *i);
                        }
                    }
                }
                Quantity::Size(_) => {
                    hi_hints.insert(q, crate::witness::MAX_REALIZED_F);
                }
                _ => {}
            }
        }
        let timeout = self.timeout;
        let s = self.solver.as_deref_mut()?;
        let base = s.model();
        let lo_atoms: Vec<QFormula> = lo_hints
            .iter()
            .map(|(&sq, &min_idx)| {
                QFormula::Atom(adl_formula::LinAtom::single(
                    sq,
                    adl_formula::Rel::Gt,
                    rat(min_idx),
                ))
            })
            .collect();
        let mut hi_atoms: Vec<QFormula> = hi_hints
            .iter()
            .map(|(&sq, &cap)| {
                QFormula::Atom(adl_formula::LinAtom::single(sq, adl_formula::Rel::Le, rat(cap)))
            })
            .collect();
        // Top wish: dPhi = 0 outright. Zero is dyadic (f64-exact in any
        // sum), so equality-shaped constraints over `… ± dPhi` — which
        // have no ε-interior — realize bit-exactly whenever the regions
        // tolerate a vanishing separation. π-flavored boundary values
        // are the one non-dyadic source in the model space.
        let zero_atoms: Vec<QFormula> = dphi_hints
            .iter()
            .map(|&q| {
                QFormula::Atom(adl_formula::LinAtom::single(
                    q,
                    adl_formula::Rel::Eq,
                    Rat::zero(),
                ))
            })
            .collect();
        // Dyadic dPhi wish bounds, strictly inside [−π, π): (a) keeps
        // boundary picks away from the wrap discontinuity (at
        // v = next_down(π), `v + π` rounds to exactly 2π and the
        // interpreter's wrap flips the sign — a 2π realization error);
        // (b) being dyadic, a vertex pick AT the bound stays on the
        // f64-exact grid, so sums involving dPhi re-evaluate exactly.
        // π itself is the one non-dyadic constant in the model space.
        const DPHI_WISH_BOUND: f64 = 3.140625; // dyadic, < π
        for q in &dphi_hints {
            let q = *q;
            hi_atoms.push(QFormula::Atom(adl_formula::LinAtom::single(
                q,
                adl_formula::Rel::Ge,
                rat(-DPHI_WISH_BOUND),
            )));
            hi_atoms.push(QFormula::Atom(adl_formula::LinAtom::single(
                q,
                adl_formula::Rel::Le,
                rat(DPHI_WISH_BOUND),
            )));
        }
        let try_with = |s: &mut dyn Solver, atoms: &[&[QFormula]]| -> Option<Model> {
            s.push();
            for group in atoms {
                for a in *group {
                    s.assert(a, None);
                }
            }
            let m = match s.check(timeout) {
                SatResult::Sat => s.model(),
                _ => None,
            };
            s.pop();
            m
        };
        // Layered: hints are wishes, not requirements — drop the
        // dPhi = 0 preference first, then the ε-interior preference (an
        // overlap may exist only on a boundary), then the existence
        // hints (a model may legitimately need a small size), the
        // realizer caps last, the raw model as the floor.
        try_with(s, &[&zero_atoms, interior, &lo_atoms, &hi_atoms])
            .or_else(|| try_with(s, &[interior, &lo_atoms, &hi_atoms]))
            .or_else(|| try_with(s, &[&lo_atoms, &hi_atoms]))
            .or_else(|| try_with(s, &[&hi_atoms]))
            .or(base)
    }

    /// ε-tightened version of an under-formula: every inequality pulled
    /// `WITNESS_EPS` inside its bound, `≠` widened to a two-sided gap.
    /// Any model of the tightened formula satisfies the original, so
    /// using it as a model-selection wish is sound — and the resulting
    /// interior model survives f64 re-evaluation by the interpreter.
    ///
    /// Pure-integer atoms (collection sizes) are left exact: integers
    /// carry no rounding error, and fractional tightening would *change*
    /// their meaning (`size ≤ 1` ⇒ `size ≤ 0`), wrongly starving the
    /// interior layer.
    fn tightened(&self, f: &QFormula) -> QFormula {
        match f {
            QFormula::True => QFormula::True,
            QFormula::False => QFormula::False,
            QFormula::And(v) => QFormula::And(v.iter().map(|p| self.tightened(p)).collect()),
            QFormula::Or(v) => QFormula::Or(v.iter().map(|p| self.tightened(p)).collect()),
            QFormula::Atom(a) => {
                use adl_formula::Rel;
                let all_int = a
                    .terms()
                    .iter()
                    .all(|&(_, q)| matches!(self.hir.table.quantity(q), Quantity::Size(_)));
                if all_int {
                    return QFormula::Atom(a.clone());
                }
                let eps = rat(WITNESS_EPS);
                let rebuild = |rel: Rel, k: Rat| -> QFormula {
                    QFormula::Atom(adl_formula::LinAtom::new(
                        a.terms().iter().cloned(),
                        rel,
                        k,
                    ))
                };
                match a.rel() {
                    Rel::Lt | Rel::Le => rebuild(a.rel(), a.constant() - &eps),
                    Rel::Gt | Rel::Ge => rebuild(a.rel(), a.constant() + &eps),
                    Rel::Eq => QFormula::Atom(a.clone()),
                    Rel::Ne => QFormula::Or(vec![
                        rebuild(Rel::Le, a.constant() - &eps),
                        rebuild(Rel::Ge, a.constant() + &eps),
                    ]),
                }
            }
        }
    }

    /// `UNSAT(Ax ∧ sub⁺ ∧ ¬(sup⁻))` ⇒ sub ⊆ sup. Assertions are unnamed —
    /// no subset check reads an unsat core.
    fn subset<'o>(
        &mut self,
        sub_overs: impl IntoIterator<Item = &'o Over>,
        sup_unders: &[Under],
    ) -> bool {
        if self.solver.is_none() {
            return false;
        }
        self.push();
        let neg = Self::negated_under(sup_unders);
        if let Some(s) = self.solver.as_deref_mut() {
            for o in sub_overs {
                s.assert(o.qformula(), None);
            }
            s.assert(&neg, None);
        }
        let result = self.check(self.timeout);
        self.pop();
        matches!(result, Some(SatResult::Unsat))
    }

    /// Prove cross/intra collection refinements and assert the derived
    /// `size(A) <= size(B)` (XSUB) / `size(A) = size(B)` (XEQ) facts at the
    /// current (base) frame. Returns per-id instance counts for the axioms-used
    /// report. Sound because a fact is asserted ONLY when the subset prover
    /// reports UNSAT for the corresponding element-predicate implication over a
    /// shared base element (see XSUB catalog row); a fact already covered by an
    /// intra-source SUB axiom is skipped (no double count).
    fn reconcile(
        &mut self,
        origins: &mut BTreeMap<AssertName, CoreItem>,
    ) -> BTreeMap<&'static str, usize> {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let Some(recon) = self.recon.take() else {
            return counts;
        };
        if self.solver.is_none() || recon.is_empty() {
            return counts;
        }
        let existing = self.existing_size_le();
        let mut k = 0usize;
        for cand in &recon.candidates {
            let (a_in_b, b_in_a) = self.prove_pred_implies(&cand.phi_a, &cand.phi_b);
            // Directions to emit, as (sub_size, sup_size, catalog id).
            let facts: &[(QuantityId, QuantityId, AxiomId)] = if a_in_b && b_in_a {
                &[
                    (cand.size_a, cand.size_b, AxiomId::Xeq),
                    (cand.size_b, cand.size_a, AxiomId::Xeq),
                ]
            } else if a_in_b {
                &[(cand.size_a, cand.size_b, AxiomId::Xsub)]
            } else if b_in_a {
                &[(cand.size_b, cand.size_a, AxiomId::Xsub)]
            } else {
                &[]
            };
            for &(sub, sup, id) in facts {
                // A collection is trivially its own size; an intra-source
                // SUB fact already carries this refinement.
                if sub == sup || existing.contains(&(sub, sup)) {
                    continue;
                }
                let fact = derived_size_le(sub, sup);
                let name = AssertName::new(format!("XR{k}"));
                // Id-disambiguated labels (`size(C3#jets) <= size(C9#jets)`):
                // in a merged unit the bare first-bound name is shared by
                // both files' differently-cut `jets`, which would render the
                // flagship cross-file explanation as the self-referential
                // `size(jets) <= size(jets)` with the direction unrecoverable.
                let statement = format!(
                    "{} <= {}",
                    size_label(self.hir, sub),
                    size_label(self.hir, sup)
                );
                if let Some(s) = self.solver.as_deref_mut() {
                    s.assert(&fact, Some(name.clone()));
                }
                // Retained so a certified core containing this fact can map
                // the name back to its formula (v2 Phase 4).
                self.recon_facts.push((name.clone(), fact.clone()));
                origins.insert(
                    name,
                    CoreItem::Axiom {
                        id: id.as_str().to_owned(),
                        statement,
                    },
                );
                *counts.entry(id.as_str()).or_insert(0) += 1;
                k += 1;
            }
        }
        counts
    }

    /// Read the refinement directions `(A ⊆ B, B ⊆ A)` for two element
    /// predicates grounded on one shared base element. Precheck rejects a
    /// degenerate frame (the two under-approximations cannot co-hold, or the
    /// solver is unsure) BEFORE trusting either UNSAT — so a vacuous or flaky
    /// answer never yields a fact. Direction is read SOLELY from the two
    /// [`Self::subset`] booleans: the sub side uses `.over()` (dropping an
    /// un-encodable conjunct only weakens it — sound), the sup side uses
    /// `.under()` (an opaque conjunct becomes false, never dropped).
    fn prove_pred_implies(
        &mut self,
        phi_a: &adl_formula::Formula,
        phi_b: &adl_formula::Formula,
    ) -> (bool, bool) {
        if !self.frame_sat(phi_a, phi_b) {
            return (false, false);
        }
        let a_in_b = self.subset([&phi_a.over()], &[phi_b.under()]);
        let b_in_a = self.subset([&phi_b.over()], &[phi_a.under()]);
        (a_in_b, b_in_a)
    }

    /// Certify a disjointness UNSAT through the independent exact-rational
    /// checker (proof-system v2 Phase 4). `None` = certification off;
    /// `Some(true)` = a replay-checked Farkas certificate verifies the core
    /// (or the full frame when no core is available); `Some(false)` = the
    /// proof could not be certified — the caller reports CANDIDATE DISJOINT.
    /// Certifying the CORE ONLY is sound: UNSAT of a subset implies UNSAT of
    /// the asserted superset. A core name that maps to no known formula
    /// fails closed.
    fn certify_disjoint(
        &mut self,
        core: Option<&[AssertName]>,
        c1: &RegionCtx,
        c2: &RegionCtx,
    ) -> Option<bool> {
        if !self.certify {
            return None;
        }
        let mut fmap: BTreeMap<AssertName, QFormula> = BTreeMap::new();
        for (n, o) in c1.overs.iter().chain(c2.overs.iter()) {
            fmap.insert(n.clone(), o.qformula().clone());
        }
        for (i, inst) in self.axioms.instances.iter().enumerate() {
            fmap.insert(AssertName::new(format!("AX{i}")), inst.formula.clone());
        }
        for (n, f) in &self.recon_facts {
            fmap.insert(n.clone(), f.clone());
        }
        let formulas: Vec<QFormula> = match core {
            Some(names) if !names.is_empty() => {
                let mut v = Vec::with_capacity(names.len());
                for n in names {
                    match fmap.get(n) {
                        Some(f) => v.push(f.clone()),
                        None => return Some(false),
                    }
                }
                v
            }
            _ => fmap.into_values().collect(),
        };
        match adl_certify::certify_unsat(&formulas, &adl_certify::Budget::default()) {
            adl_certify::CertifyResult::Certified(_) => Some(true),
            adl_certify::CertifyResult::Uncertified(_) => Some(false),
        }
    }

    /// Sampling gate (proof-system v2, Phase 1): refute an UNSAT-side pair
    /// verdict against the battery through the reference interpreter. A
    /// sampled event the interpreter passes through both regions of a
    /// "proven disjoint" pair — or through `sub` but not `sup` of a proven
    /// subset — is an internal contradiction: some encoder/axiom fact is
    /// false on a real event. The verdict demotes (fail closed) and a bug
    /// diagnostic is filed; interpreter errors carry no information.
    #[allow(clippy::too_many_arguments)]
    fn gate_pair(
        &self,
        report: &mut PairReport,
        ia: usize,
        ib: usize,
        interp: &Interp<'_>,
        internal: &mut Vec<String>,
        refutations: &mut usize,
    ) {
        if self.gate_events.is_empty() {
            return;
        }
        let memb = |idx: usize, e: &adl_interp::Event| {
            interp.eval_region_membership_idx(idx, e).ok()
        };
        if report.kind == VerdictKind::ProvenDisjoint {
            for e in &self.gate_events {
                if memb(ia, e) == Some(true) && memb(ib, e) == Some(true) {
                    *refutations += 1;
                    internal.push(format!(
                        "SAMPLING GATE refuted PROVEN DISJOINT for {} vs {}: a sampled \
                         event passes both regions — an encoder/axiom fact is false on \
                         a real event; verdict demoted",
                        report.a, report.b
                    ));
                    report.kind = VerdictKind::PossiblyOverlapping;
                    report.reason = "the sampling gate refuted a disjointness proof \
                                     (internal contradiction, reported as a bug); capped \
                                     at POSSIBLY"
                        .to_owned();
                    report.core.clear();
                    break;
                }
            }
        }
        let mut gate_subset = |sub: usize, sup: usize, flag: &mut bool, label: &str| {
            if !*flag {
                return;
            }
            for e in &self.gate_events {
                if memb(sub, e) == Some(true) && memb(sup, e) == Some(false) {
                    *refutations += 1;
                    *flag = false;
                    internal.push(format!(
                        "SAMPLING GATE refuted PROVEN SUBSET ({label}) for {} vs {}: a \
                         sampled event is in the subset region but not the superset; \
                         claim withdrawn",
                        report.a, report.b
                    ));
                    break;
                }
            }
        };
        let (mut a_in_b, mut b_in_a) = (report.subset_a_in_b, report.subset_b_in_a);
        gate_subset(ia, ib, &mut a_in_b, "a within b");
        gate_subset(ib, ia, &mut b_in_a, "b within a");
        report.subset_a_in_b = a_in_b;
        report.subset_b_in_a = b_in_a;
    }

    /// Sampling gate for a proven-empty region: any sampled member refutes.
    /// Returns true when refuted (the caller demotes to NotProven).
    fn gate_empty(
        &self,
        idx: usize,
        interp: &Interp<'_>,
        internal: &mut Vec<String>,
        refutations: &mut usize,
    ) -> bool {
        for e in &self.gate_events {
            if interp.eval_region_membership_idx(idx, e).ok() == Some(true) {
                *refutations += 1;
                internal.push(format!(
                    "SAMPLING GATE refuted REGION EMPTY for {}: a sampled event is a \
                     member — an encoder/axiom fact is false on a real event; claim \
                     withdrawn",
                    self.hir.symbols.display(self.hir.regions[idx].name)
                ));
                return true;
            }
        }
        false
    }

    /// Is the shared generic-element frame satisfiable with BOTH predicates'
    /// under-approximations asserted? Guards `prove_pred_implies` against
    /// emitting a fact from disjoint/degenerate predicates or a solver
    /// `unknown` (both directions would otherwise read as a spurious IDENTICAL).
    fn frame_sat(&mut self, phi_a: &adl_formula::Formula, phi_b: &adl_formula::Formula) -> bool {
        if self.solver.is_none() {
            return false;
        }
        self.push();
        if let Some(s) = self.solver.as_deref_mut() {
            s.assert(phi_a.under().qformula(), None);
            s.assert(phi_b.under().qformula(), None);
        }
        let r = self.check(self.timeout);
        self.pop();
        matches!(r, Some(SatResult::Sat))
    }

    /// The `(sub_size, sup_size)` pairs already covered by an emitted SUB
    /// axiom, so reconciliation does not re-assert (or re-count) an intra-source
    /// refinement it already proves structurally. Matching is by FORMULA
    /// EQUALITY against `derived_size_le` (SUB builds through the same
    /// function), so a change to the size-fact encoding can never silently
    /// invert or miss the dedup.
    fn existing_size_le(&self) -> BTreeSet<(QuantityId, QuantityId)> {
        let mut out = BTreeSet::new();
        for inst in &self.axioms.instances {
            if inst.id != AxiomId::Sub {
                continue;
            }
            let QFormula::Atom(a) = &inst.formula else {
                continue;
            };
            let qs: Vec<QuantityId> = a.terms().iter().map(|&(_, q)| q).collect();
            if qs.len() != 2 {
                continue;
            }
            for (s, p) in [(qs[0], qs[1]), (qs[1], qs[0])] {
                if inst.formula == derived_size_le(s, p) {
                    out.insert((s, p));
                }
            }
        }
        out
    }

    fn bin_check(&mut self, set: &BinSetEnc, region_ctx: &RegionCtx) -> BinCheckReport {
        let region_name = self.unit.regions[set.region_idx].name.clone();
        let n = set.bins.len();
        let overs: Vec<Over> = set.bins.iter().map(adl_formula::Formula::over).collect();
        let unders: Vec<Under> = set.bins.iter().map(adl_formula::Formula::under).collect();

        let mut proven = 0usize;
        let total = n * n.saturating_sub(1) / 2;
        for i in 0..n {
            for j in i + 1..n {
                if self.bins_disjoint(region_ctx, &overs[i], &overs[j]) {
                    proven += 1;
                }
            }
        }

        let (coverage, gap_witness) = self.bin_coverage(set, region_ctx, &unders);
        BinCheckReport {
            region: region_name,
            variable: set.variable.clone(),
            n_bins: n,
            disjoint_pairs_proven: proven,
            disjoint_pairs_total: total,
            coverage,
            gap_witness,
        }
    }

    /// `UNSAT(Ax ∧ R⁺ ∧ Bᵢ⁺ ∧ Bⱼ⁺)` ⇒ bins i, j disjoint within R.
    fn bins_disjoint(&mut self, region_ctx: &RegionCtx, bi: &Over, bj: &Over) -> bool {
        if self.solver.is_some() {
            self.push();
            self.assert_overs(&region_ctx.overs, false);
            if let Some(s) = self.solver.as_deref_mut() {
                s.assert(bi.qformula(), None);
                s.assert(bj.qformula(), None);
            }
            let r = self.check(self.timeout);
            self.pop();
            return matches!(r, Some(SatResult::Unsat));
        }
        // No-solver fallback: interval spine of (R ∧ Bi) vs (R ∧ Bj).
        let mut a = region_ctx.intervals.clone();
        a.add_over(bi.qformula());
        let mut b = region_ctx.intervals.clone();
        b.add_over(bj.qformula());
        a.self_empty().is_some() || b.self_empty().is_some() || a.disjoint_with(&b).is_some()
    }

    /// `UNSAT(Ax ∧ R⁺ ∧ ⋀ᵢ ¬(Bᵢ⁻))` ⇒ the bins cover the region; a SAT
    /// answer yields the gap witness (SPEC_ANALYSIS §5).
    fn bin_coverage(
        &mut self,
        set: &BinSetEnc,
        region_ctx: &RegionCtx,
        unders: &[Under],
    ) -> (CoverageStatus, Vec<WitnessValue>) {
        if self.solver.is_none() {
            return (CoverageStatus::Unknown, Vec::new());
        }
        self.push();
        self.assert_overs(&region_ctx.overs, false);
        if let Some(s) = self.solver.as_deref_mut() {
            for u in unders {
                s.assert(&u.qformula().clone().not(), None);
            }
        }
        let result = self.check(self.timeout);
        let out = match result {
            Some(SatResult::Unsat) => (CoverageStatus::Proven, Vec::new()),
            Some(SatResult::Sat) => {
                let mut bin_qs = BTreeSet::new();
                for f in &set.bins {
                    crate::encode::formula_quantities(f, &mut bin_qs);
                }
                let witness = self
                    .solver
                    .as_deref_mut()
                    .and_then(adl_solver::Solver::model)
                    .map(|m| witness_values(self.hir, &m, &bin_qs))
                    .unwrap_or_default();
                (CoverageStatus::NotProven, witness)
            }
            _ => (CoverageStatus::Unknown, Vec::new()),
        };
        self.pop();
        out
    }
}

fn lookup_size(hir: &Hir, coll: adl_sema::CollectionId) -> Option<QuantityId> {
    // O(1) via the interner — was a linear scan of the whole quantity table,
    // a hot path under the per-witness retry loop and a scaling hazard once
    // the table spans many files.
    hir.table.quantity_id(&Quantity::Size(coll))
}

/// Witness rows for a VALIDATED overlap, read back from the validated event
/// itself (review F2): the realizer's pT-descending normalization can permute
/// which element sits at each index AFTER the model assigned values, so a
/// model row (`jets[0].pt = 21` beside a `pt > 100` filter) can describe an
/// arrangement the loader would reject — while the report labels it
/// "validated by interpreter". A quantity the interpreter cannot read back
/// from the event (rare: mentioned only by a dropped statement) keeps the
/// model value.
fn validated_witness_values(
    hir: &Hir,
    ext: &ExtDecls,
    interp: &Interp<'_>,
    json: &str,
    model: &Model,
    mentioned: &BTreeSet<QuantityId>,
) -> Vec<WitnessValue> {
    let Ok(event) = adl_interp::parse_event(json, ext) else {
        // Unreachable in practice: this exact JSON just passed the loader
        // during validation.
        return witness_values(hir, model, mentioned);
    };
    let value_of = |q: QuantityId| -> Option<f64> {
        match interp.eval_quantity(q, &event) {
            Ok(adl_interp::NumOutcome::Value(v)) => Some(v),
            _ => model.get(q),
        }
    };
    let mut rows: Vec<WitnessValue> = Vec::new();
    let mut listed: BTreeSet<QuantityId> = BTreeSet::new();
    for &q in mentioned {
        if let Some(v) = value_of(q)
            && listed.insert(q)
        {
            rows.push(WitnessValue {
                quantity: quantity_label(hir, q),
                value: v,
                derived: false,
            });
        }
    }
    for &q in mentioned {
        if let Quantity::ElemProp { coll, .. } = hir.table.quantity(q)
            && let Some(sq) = lookup_size(hir, *coll)
            && !listed.contains(&sq)
            && let Some(v) = value_of(sq)
        {
            listed.insert(sq);
            rows.push(WitnessValue {
                quantity: quantity_label(hir, sq),
                value: v,
                derived: true,
            });
        }
    }
    rows
}

/// Witness values for the report: every mentioned quantity, plus the
/// (axiom-derived) sizes of collections whose elements are mentioned.
fn witness_values(hir: &Hir, model: &Model, mentioned: &BTreeSet<QuantityId>) -> Vec<WitnessValue> {
    let mut rows: Vec<WitnessValue> = Vec::new();
    let mut listed: BTreeSet<QuantityId> = BTreeSet::new();
    for &q in mentioned {
        if let Some(v) = model.get(q)
            && listed.insert(q)
        {
            rows.push(WitnessValue {
                quantity: quantity_label(hir, q),
                value: v,
                derived: false,
            });
        }
    }
    for &q in mentioned {
        if let Quantity::ElemProp { coll, .. } = hir.table.quantity(q)
            && let Some(sq) = lookup_size(hir, *coll)
            && !listed.contains(&sq)
            && let Some(v) = model.get(sq)
        {
            listed.insert(sq);
            rows.push(WitnessValue {
                quantity: quantity_label(hir, sq),
                value: v,
                derived: true,
            });
        }
    }
    rows.sort_by(|a, b| a.quantity.cmp(&b.quantity));
    rows
}

#[cfg(test)]
mod reconcile_solver_tests {
    //! Scripted-solver coverage of `reconcile()`'s solver-outcome
    //! classification (review F22): the Unknown-rejection surface
    //! (`frame_sat` precheck, subset checks) and the SUB-covered dedup were
    //! previously untestable — no test double implemented `Solver`, so an
    //! Unknown could never be injected per-call.

    use super::*;
    use adl_axioms::AxiomInstance;
    use adl_solver::QSort;
    use std::collections::VecDeque;

    /// Returns a canned `SatResult` per `check()` call, in order; panics if
    /// the script is exhausted (a script/flow mismatch is a test bug).
    struct Scripted {
        seq: VecDeque<SatResult>,
    }

    impl Solver for Scripted {
        fn declare(&mut self, _q: QuantityId, _sort: QSort) {}
        fn push(&mut self) {}
        fn pop(&mut self) {}
        fn assert(&mut self, _f: &QFormula, _name: Option<AssertName>) {}
        fn check(&mut self, _timeout: Duration) -> SatResult {
            self.seq.pop_front().expect("script exhausted: unexpected check()")
        }
        fn model(&mut self) -> Option<Model> {
            None
        }
        fn unsat_core(&mut self) -> Option<Vec<AssertName>> {
            None
        }
        fn backend_name(&self) -> &'static str {
            "scripted"
        }
    }

    /// One reconciliation candidate (two same-base filtered jets), a scripted
    /// solver, and the axiom set to dedup against — returns the per-id counts
    /// reconcile() derived.
    fn run_reconcile(
        script: Vec<SatResult>,
        axioms_of: impl Fn(&crate::reconcile::ReconEnc) -> AxiomSet,
    ) -> BTreeMap<&'static str, usize> {
        let src = "object a\n  take Jet\n  select pt > 100\n\
                   object b\n  take Jet\n  select pt > 30\n\
                   region RA\n  select size(a) >= 1\n\
                   region RB\n  select size(b) >= 1\n";
        let ext = ExtDecls::legacy();
        let mut hir = adl_sema::analyze_str(src, "t", &ext);
        assert!(!adl_syntax::diag::has_errors(&hir.diags), "{:?}", hir.diags);
        let unit = crate::encode::encode_unit(&mut hir, src);
        let recon = crate::reconcile::build(&mut hir, &ext);
        assert_eq!(recon.candidates.len(), 1, "exactly the (a, b) candidate");
        let axioms = axioms_of(&recon);
        let mut engine = Engine {
            hir: &hir,
            ext: &ext,
            unit: &unit,
            axioms: &axioms,
            solver: Some(Box::new(Scripted { seq: script.into() })),
            solver_label: "scripted".to_owned(),
            timeout: Duration::from_secs(1),
            unit_name: "t".to_owned(),
            recon: Some(recon),
            spawn_failures: 0,
            gate_events: Vec::new(),
            certify: false,
            recon_facts: Vec::new(),
        };
        let mut origins: BTreeMap<AssertName, CoreItem> = BTreeMap::new();
        engine.reconcile(&mut origins)
    }

    fn no_axioms(_: &crate::reconcile::ReconEnc) -> AxiomSet {
        AxiomSet::default()
    }

    #[test]
    fn unknown_at_the_frame_precheck_derives_nothing() {
        // Unknown is NOT "consistent": trusting a later UNSAT over an
        // undecided frame could fabricate a vacuous refinement.
        let counts = run_reconcile(
            vec![SatResult::Unknown("timeout".to_owned())],
            no_axioms,
        );
        assert!(counts.is_empty(), "{counts:?}");
    }

    #[test]
    fn unsat_frame_derives_nothing() {
        let counts = run_reconcile(vec![SatResult::Unsat], no_axioms);
        assert!(counts.is_empty(), "{counts:?}");
    }

    #[test]
    fn unknown_on_a_subset_check_is_not_a_proof() {
        // frame SAT, then both subset checks Unknown: no direction proven.
        let counts = run_reconcile(
            vec![
                SatResult::Sat,
                SatResult::Unknown("timeout".to_owned()),
                SatResult::Unknown("timeout".to_owned()),
            ],
            no_axioms,
        );
        assert!(counts.is_empty(), "{counts:?}");
    }

    #[test]
    fn one_unsat_direction_is_xsub_two_are_xeq() {
        let counts = run_reconcile(
            vec![SatResult::Sat, SatResult::Unsat, SatResult::Sat],
            no_axioms,
        );
        assert_eq!(counts.get("XSUB"), Some(&1), "{counts:?}");
        assert_eq!(counts.get("XEQ"), None, "{counts:?}");

        let counts = run_reconcile(
            vec![SatResult::Sat, SatResult::Unsat, SatResult::Unsat],
            no_axioms,
        );
        assert_eq!(counts.get("XEQ"), Some(&2), "both equality directions");
        assert_eq!(counts.get("XSUB"), None, "{counts:?}");
    }

    #[test]
    fn sub_covered_pair_is_not_reasserted() {
        // The refinement direction proven below is already covered by an
        // emitted SUB fact over the same size ids: reconcile must skip it
        // (no double assertion, no count) — pins `existing_size_le`.
        let counts = run_reconcile(
            vec![SatResult::Sat, SatResult::Unsat, SatResult::Sat],
            |recon| {
                let c = &recon.candidates[0];
                AxiomSet {
                    instances: vec![AxiomInstance {
                        id: AxiomId::Sub,
                        formula: derived_size_le(c.size_a, c.size_b),
                        description: "size(a) <= size(b)".to_owned(),
                    }],
                }
            },
        );
        assert!(counts.is_empty(), "SUB-covered refinement must dedup: {counts:?}");
    }
}

#[cfg(test)]
mod sampling_gate_tests {
    //! The production sampling gate (proof-system v2 Phase 1): a scripted
    //! solver fabricates a false UNSAT — exactly what an encoder/axiom bug
    //! looks like — and the gate must refute it against the battery through
    //! the real interpreter, demote the verdict, and file the contradiction.

    use super::*;
    use adl_solver::QSort;
    use std::collections::VecDeque;

    struct Scripted {
        seq: VecDeque<SatResult>,
    }

    impl Solver for Scripted {
        fn declare(&mut self, _q: QuantityId, _sort: QSort) {}
        fn push(&mut self) {}
        fn pop(&mut self) {}
        fn assert(&mut self, _f: &QFormula, _name: Option<AssertName>) {}
        fn check(&mut self, _timeout: Duration) -> SatResult {
            self.seq
                .pop_front()
                .unwrap_or(SatResult::Unknown("script exhausted".to_owned()))
        }
        fn model(&mut self) -> Option<Model> {
            None
        }
        fn unsat_core(&mut self) -> Option<Vec<AssertName>> {
            None
        }
        fn backend_name(&self) -> &'static str {
            "scripted"
        }
    }

    fn run_gated(script: Vec<SatResult>, gate: usize) -> Report {
        // Two regions that GENUINELY overlap on every event with MET > 100.
        let src = "region RA\n  select MET > 100\nregion RB\n  select MET > 50\n";
        let ext = ExtDecls::legacy();
        let mut hir = adl_sema::analyze_str(src, "t", &ext);
        assert!(!adl_syntax::diag::has_errors(&hir.diags));
        let unit = crate::encode::encode_unit(&mut hir, src);
        let gate_events = if gate > 0 {
            adl_interp::sample::battery(&ext, gate)
        } else {
            Vec::new()
        };
        let axioms = AxiomSet::default();
        let engine = Engine {
            hir: &hir,
            ext: &ext,
            unit: &unit,
            axioms: &axioms,
            solver: Some(Box::new(Scripted { seq: script.into() })),
            solver_label: "scripted".to_owned(),
            timeout: Duration::from_secs(1),
            unit_name: "t".to_owned(),
            recon: None,
            spawn_failures: 0,
            gate_events,
            certify: false,
            recon_facts: Vec::new(),
        };
        engine.run()
    }

    /// run() solver-call order for this unit: region_empty ×2, then the
    /// pair's disjoint check. Unsat there fabricates PROVEN DISJOINT.
    fn poison() -> Vec<SatResult> {
        vec![SatResult::Sat, SatResult::Sat, SatResult::Unsat]
    }

    #[test]
    fn gate_demotes_a_fabricated_disjoint() {
        let r = run_gated(poison(), 64);
        assert_eq!(
            r.pairwise[0].kind,
            VerdictKind::PossiblyOverlapping,
            "{:?}",
            r.pairwise[0]
        );
        assert!(
            r.internal_diagnostics
                .iter()
                .any(|d| d.contains("SAMPLING GATE")),
            "{:?}",
            r.internal_diagnostics
        );
        let s = r.sampling.expect("gate accounting present");
        assert_eq!(s.events, 64);
        assert!(s.refutations >= 1);
    }

    #[test]
    fn disabled_gate_ships_the_fabrication() {
        // Documents WHY the gate exists: with sample_gate = 0 the same
        // scripted bug sails through as PROVEN DISJOINT.
        let r = run_gated(poison(), 0);
        assert_eq!(r.pairwise[0].kind, VerdictKind::ProvenDisjoint);
        assert!(r.sampling.is_none());
    }

    #[test]
    fn gate_leaves_true_verdicts_alone() {
        // Genuinely disjoint regions: no battery event can refute, so the
        // (scripted) proof stands and nothing is filed.
        let src = "region RA\n  select MET > 400\nregion RB\n  select MET < 200\n";
        let ext = ExtDecls::legacy();
        let mut hir = adl_sema::analyze_str(src, "t", &ext);
        let unit = crate::encode::encode_unit(&mut hir, src);
        let axioms = AxiomSet::default();
        let engine = Engine {
            hir: &hir,
            ext: &ext,
            unit: &unit,
            axioms: &axioms,
            solver: Some(Box::new(Scripted { seq: poison().into() })),
            solver_label: "scripted".to_owned(),
            timeout: Duration::from_secs(1),
            unit_name: "t".to_owned(),
            recon: None,
            spawn_failures: 0,
            gate_events: adl_interp::sample::battery(&ext, 64),
            certify: false,
            recon_facts: Vec::new(),
        };
        let r = engine.run();
        assert_eq!(r.pairwise[0].kind, VerdictKind::ProvenDisjoint);
        assert_eq!(r.sampling.unwrap().refutations, 0);
    }
}
