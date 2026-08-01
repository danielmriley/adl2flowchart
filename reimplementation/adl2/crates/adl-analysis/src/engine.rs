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
use crate::interval::{IntervalMap, RefutingPart};
use crate::report::{
    AxiomUse, BinCheckReport, CoreItem, CoverageStatus, Diagnostic, DiagnosticClass, EmptyStatus,
    OVERLAP_CAVEAT, PairReport, ProofPath, RegionReport, Report, SCHEMA_VERSION, VerdictKind,
    WitnessValue,
};
use crate::witness::{Validation, validate_witness};
use adl_axioms::{AxiomId, AxiomSet, catalog_entry, derived_size_le, quantity_label, twin_pairs};
use adl_certify::bundle::{AssertSource, BundleAssert, BundleParts, DerivedFact, Derivation};
use adl_formula::{Over, QFormula, Under};
use adl_interp::Interp;
use adl_sema::{ElemIndex, ExtDecls, Hir, Quantity, QuantityId, Rat};
use adl_solver::{AssertName, Model, QSort, SatResult, Solver};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

/// The exact formula set a certificate was checked against, in replay order,
/// paired with the certificate. This is what a `--combine` bundle is built
/// from; it is also what a derived fact carries as its own proof.
struct CertPayload {
    asserts: Vec<(AssertName, QFormula)>,
    cert: adl_certify::Certificate,
    /// Are the listed formulas the asserts' whole over-projections (`true` —
    /// solver cores and the primary interval shape), or single spine conjuncts
    /// extracted from them (`false` — the interval fallback for a cut whose
    /// disjunctive structure a bound pair cannot cross)? It rides into the
    /// bundle so a reader is never left guessing whether the quoted formula is
    /// the whole cut.
    whole: bool,
}

/// A certification outcome: `None` = certification off, `Some(true/false)` =
/// certified / not, plus the replayable payload when one was kept.
type Certified = (Option<bool>, Option<CertPayload>);

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
    /// Checks that returned `Unknown("solver reported an error: …")` — a
    /// spawnable-but-broken solver (answers `-version`, errors on every
    /// script). Distinct from [`Self::spawn_failures`] but feeds the same
    /// `solver_degraded` warning (G7). Timeouts / `unknown` answers are
    /// NOT counted — those are legitimate hard-query outcomes.
    pub solver_errors: usize,
    /// The reason string of the FIRST failed check, kept so the report can
    /// name it instead of only counting failures.
    pub first_solver_failure: Option<String>,
    /// The sampling-gate battery (proof-system v2 Phase 1): deterministic
    /// loader-valid events every UNSAT-side PROVEN verdict is refuted against
    /// through the reference interpreter before being reported. Empty = gate
    /// disabled.
    pub gate_events: Vec<adl_interp::Event>,
    /// Adversarial refute-gate probes (trustworthy-verify M1): cut-anchored
    /// and flat-spot events searched after the sampling gate. Empty when the
    /// gate is off or the unit has no usable cut constants.
    pub refute_probes: Vec<adl_interp::Event>,
    /// Whether the adversarial refute gate is enabled for this run (drives
    /// [`Report::refute`] presence even when the probe list is empty).
    pub refute_gate: bool,
    /// Certify UNSAT-direction proofs through the independent exact-rational
    /// checker (`adl-certify`): solver-UNSAT pairwise disjointness,
    /// emptiness, subset, and bin claims whose core/frame cannot be
    /// certified demote to a candidate / non-claim.
    pub certify: bool,
    /// The reconciliation facts asserted at the persistent frame, retained by
    /// name so a certified core containing an `XR{k}` member can map it back
    /// to its formula.
    pub recon_facts: Vec<(AssertName, QFormula)>,
    /// Per reconciliation fact, the derivation chain that earned it: the
    /// generic-element premises and the certificate refuting them. A bundle
    /// whose core uses `XR{k}` embeds this, so replaying the bundle re-derives
    /// the fact instead of taking it as a given. Filled only under
    /// `--certify` (without it there is no certificate to embed).
    pub recon_chains: BTreeMap<AssertName, DerivedFact>,
    /// The reconciliation ledger, filled by [`Self::reconcile`]: one row per
    /// candidate pair (related, unrelated, or skipped with a reason).
    pub recon_ledger: Vec<crate::report::ReconReport>,
    /// Advisories for structurally-identical collections whose base names
    /// differ (see [`crate::reconcile::ReconNearMiss`]).
    pub recon_near_misses: Vec<crate::report::ReconNearMissReport>,
    /// Build a portable [`adl_certify::CombineBundle`] for every certified
    /// PROVEN DISJOINT pair (CLI `--combine`). Off by default: bundling
    /// clones the certified formula set per pair.
    pub combine: bool,
    /// Bundles accumulated under `combine`; [`Self::run`] filters them
    /// against the FINAL pair verdicts (so a later demotion — e.g. by the
    /// sampling gate — never leaves a bundle for a retracted claim) and
    /// moves them into [`Report::combine_bundles`].
    pub bundles: Vec<adl_certify::CombineBundle>,
}

/// Bounded witness retry: how many distinct overlap models to try to
/// realize before downgrading to POSSIBLY.
const MAX_WITNESS_ATTEMPTS: u32 = 6;

/// Internal-diagnostic sink. Identical to the `Vec<String>` it replaces
/// except that each entry is filed with its severity at the point that
/// knows it — the alternative (re-deriving severity from message prefixes
/// in the renderer) would rot the first time a message is reworded.
#[derive(Default)]
pub(crate) struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    /// A claim was withheld or capped because its evidence did not hold up.
    /// Nothing was contradicted; the conservative outcome was taken.
    fn fail_closed(&mut self, message: String) {
        self.items.push(Diagnostic {
            class: DiagnosticClass::FailClosed,
            message,
        });
    }

    /// One part of the engine refuted a conclusion another part reached.
    fn contradiction(&mut self, message: String) {
        self.items.push(Diagnostic {
            class: DiagnosticClass::Contradiction,
            message,
        });
    }

    fn messages(&self) -> Vec<String> {
        self.items.iter().map(|d| d.message.clone()).collect()
    }
}

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

/// Label for a bundle's quantity dictionary. Like [`size_label`], except it
/// renders the reconciliation layer's *generic element* readably: that index
/// is a deliberately out-of-range sentinel (`adl_axioms::GENERIC_INDEX`) and
/// would otherwise print as `jet[4294967295].pt` — meaningless to the reader
/// the dictionary exists for. Bundle-local on purpose: the report's own
/// `quantity_label` rendering is left byte-identical.
fn bundle_label(hir: &Hir, q: QuantityId) -> String {
    if let Quantity::ElemProp { coll, index, prop } = hir.table.quantity(q)
        && matches!(index, ElemIndex::FromFront(i) if *i == adl_axioms::GENERIC_INDEX)
    {
        return format!(
            "{}[*].{} (any one element of the collection)",
            adl_sema::collection_ref(hir, *coll),
            hir.table.prop_display(*prop)
        );
    }
    size_label(hir, q)
}

/// Ledger label for a collection: the id-disambiguated `C3#jets` form, so
/// two files' same-named-but-differently-cut collections stay distinct.
fn coll_label(hir: &Hir, c: adl_sema::CollectionId) -> String {
    adl_sema::collection_ref(hir, c)
}

/// The analysis units whose regions mention a collection — the ledger's file
/// attribution for a `C<id>#name` label. A merged unit namespaces its regions
/// `<unit>::<region>`, so the unit is the part before the LAST `::` (a unit
/// label can itself be a path containing `::`; a region identifier cannot).
///
/// Descriptive only: it reads the encoded quantity sets the analysis already
/// built, and answers "where was this declared", never "what is it".
fn coll_units(hir: &Hir, unit: &UnitEnc, c: adl_sema::CollectionId) -> Vec<String> {
    let mentions = |q: QuantityId| match hir.table.quantity(q) {
        Quantity::Size(x) => *x == c,
        Quantity::ElemProp { coll, .. } => *coll == c,
        _ => false,
    };
    let mut out: Vec<String> = Vec::new();
    for r in &unit.regions {
        let Some((file, _)) = r.name.rsplit_once("::") else {
            continue;
        };
        if out.iter().any(|u| u == file) {
            continue;
        }
        if r.quantities.iter().copied().any(mentions) {
            out.push(file.to_owned());
        }
    }
    out.sort();
    out
}

/// The detector base a collection flattens to, for the ledger's `base`
/// column. `None` for shapes with no single filter chain.
fn base_label(hir: &Hir, c: adl_sema::CollectionId) -> Option<String> {
    hir.table
        .filter_chain(c)
        .map(|(base, _)| hir.symbols.display(base).to_owned())
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
        .filter_map(|(q, v)| {
            let f = v.to_f64();
            let s = if f.is_finite() && f.abs() < 1e9 {
                (f * GRID).round() / GRID
            } else {
                f
            };
            Rat::from_decimal_f64(s).map(|r| (q, r))
        })
        .collect();
    Model::from_values(snapped)
}

/// `¬(⋀ q = v)` over the mentioned quantities of `model`: excludes this
/// assignment so the solver proposes a different overlap model.
fn blocking_clause(model: &Model, mentioned: &BTreeSet<QuantityId>) -> Option<QFormula> {
    let mut parts = Vec::new();
    for &q in mentioned {
        // Model values are already exact Rat — assert ≠ without an f64 bridge.
        if let Some(v) = model.get(q) {
            let atom = adl_formula::LinAtom::single(q, adl_formula::Rel::Ne, v);
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

/// The refinement directions between two element predicates, each with the
/// replayable proof that established it (present only under `--certify`).
#[derive(Default)]
struct PredImplies {
    a_in_b: bool,
    b_in_a: bool,
    a_chain: Option<CertPayload>,
    b_chain: Option<CertPayload>,
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
        let mut internal = Diagnostics::default();

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
        let recon_counts = self.reconcile(&mut origins, &mut internal);

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
                for (n, o) in &overs {
                    intervals.add_over(n, o);
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
        let mut refute_refutations = 0usize;
        let mut region_reports = Vec::new();
        for (r, ctx) in self.unit.regions.iter().zip(&ctxs) {
            let (mut empty, mut empty_core, mut empty_proof) =
                self.region_empty(ctx, &r.name, &origins, &mut internal);
            if matches!(empty, EmptyStatus::Proven | EmptyStatus::Candidate)
                && self.gate_empty(r.idx, &interp, &mut internal, &mut gate_refutations)
            {
                empty = EmptyStatus::NotProven;
                empty_core = Vec::new();
                empty_proof = None;
            }
            if matches!(empty, EmptyStatus::Proven | EmptyStatus::Candidate)
                && self.refute_empty(r.idx, &interp, &mut internal, &mut refute_refutations)
            {
                empty = EmptyStatus::NotProven;
                empty_core = Vec::new();
                empty_proof = None;
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
                empty_proof,
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
                    &mut refute_refutations,
                );
                pairwise.push(pair);
            }
        }

        // -- bins --------------------------------------------------------------
        let mut bin_checks = Vec::new();
        for set in &self.unit.bin_sets {
            let report = self.bin_check(set, &ctxs[set.region_idx], &mut internal);
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

        // Keep only bundles whose pair SURVIVED as PROVEN DISJOINT: the
        // sampling gate (or any later demotion) retracting a verdict must
        // also retract its portable artifact.
        let mut combine_bundles = std::mem::take(&mut self.bundles);
        combine_bundles.retain(|b| {
            pairwise.iter().any(|p: &PairReport| {
                p.kind == VerdictKind::ProvenDisjoint && p.a == b.region_a && p.b == b.region_b
            })
        });

        Report {
            schema_version: SCHEMA_VERSION,
            unit: self.unit_name.clone(),
            solver: self.solver_label.clone(),
            sampling: (!self.gate_events.is_empty()).then_some(crate::report::SamplingInfo {
                events: self.gate_events.len(),
                refutations: gate_refutations,
            }),
            refute: self.refute_gate.then_some(crate::report::RefuteInfo {
                probes: self.refute_probes.len(),
                refutations: refute_refutations,
            }),
            solver_degraded: (self.spawn_failures + self.solver_errors > 0).then(|| {
                format!(
                    "{} solver check(s) failed via `{}` ({} spawn/IO, {} solver error); \
                     affected verdicts degraded to UNKNOWN/POSSIBLY",
                    self.spawn_failures + self.solver_errors,
                    self.solver_label,
                    self.spawn_failures,
                    self.solver_errors
                )
            }),
            solver_failures: (self.spawn_failures + self.solver_errors > 0).then(|| {
                crate::report::SolverFailures {
                    spawn: self.spawn_failures,
                    errors: self.solver_errors,
                    first_reason: self
                        .first_solver_failure
                        .clone()
                        .unwrap_or_else(|| "reason not recorded".to_owned()),
                }
            }),
            certification: self.certify,
            regions: region_reports,
            pairwise,
            bin_checks,
            reconciliations: std::mem::take(&mut self.recon_ledger),
            recon_near_misses: std::mem::take(&mut self.recon_near_misses),
            axioms_used,
            internal_diagnostics: internal.messages(),
            diagnostics: internal.items,
            combine_bundles,
        }
    }

    fn check(&mut self, timeout: Duration) -> Option<SatResult> {
        let r = self.solver.as_deref_mut().map(|s| s.check(timeout));
        if let Some(ref result) = r {
            self.note_check_result(result);
        }
        r
    }

    /// Account for a solver `check` outcome toward the `solver_degraded`
    /// warning. Call sites that bypass [`Self::check`] (witness retry,
    /// `refined_model`) must route through this so spawn/error failures
    /// cannot stay silent (G7).
    fn note_check_result(&mut self, result: &SatResult) {
        // The two classes live with the backend that writes the reasons
        // (`adl_solver`), so a new failure mode cannot drift out of the
        // accounting: process failures first, broken-solver errors second,
        // and hard-query `unknown`/`timeout` counted by neither.
        let failed = result.is_process_failure() || result.is_solver_error();
        if result.is_process_failure() {
            self.spawn_failures += 1;
        } else if result.is_solver_error() {
            self.solver_errors += 1;
        }
        // Keep the FIRST reason verbatim: a report that only says "12 checks
        // failed" leaves the reader no way to act without re-running.
        if failed
            && self.first_solver_failure.is_none()
            && let SatResult::Unknown(why) = result
        {
            self.first_solver_failure = Some(why.clone());
        }
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
        region: &str,
        origins: &BTreeMap<AssertName, CoreItem>,
        internal: &mut Diagnostics,
    ) -> (EmptyStatus, Vec<CoreItem>, Option<ProofPath>) {
        // Interval-only emptiness runs no solver, but it is still a bound
        // refutation the kernel can check — so check it. A rejection is a
        // kernel/interval disagreement (a bug), reported rather than used to
        // demote a verdict the interval layer stands behind.
        if let Some(empty) = ctx.intervals.self_empty() {
            let lookup = |n: &AssertName| over_of(&ctx.overs, n);
            if let Some(Err(why)) = self.certify_interval(&empty.parts(), lookup) {
                internal.contradiction(format!(
                    "INTERVAL CERTIFICATE unavailable for the emptiness of region {region}: \
                     {why}. The interval layer refuted the region's own spine but the replay \
                     kernel would not confirm it — one of the two is wrong; the verdict is \
                     left as it was."
                ));
            }
            return (
                EmptyStatus::Proven,
                Vec::new(),
                Some(ProofPath::Interval),
            );
        }
        if self.solver.is_none() {
            return (EmptyStatus::Unknown, Vec::new(), None);
        }
        self.push();
        self.assert_overs(&ctx.overs, true);
        let result = self.check(self.timeout);
        let out = match result {
            Some(SatResult::Unsat) => {
                let core_names = self
                    .solver
                    .as_deref_mut()
                    .and_then(adl_solver::Solver::unsat_core);
                let items: Vec<CoreItem> = core_names
                    .iter()
                    .flatten()
                    .filter_map(|n| origins.get(n).cloned())
                    .collect();
                let (certified, _) = self.certify_named_unsat(core_names.as_deref(), &[ctx]);
                let status = if certified == Some(false) {
                    EmptyStatus::Candidate
                } else {
                    EmptyStatus::Proven
                };
                (status, items, Some(ProofPath::SolverCore))
            }
            Some(SatResult::Sat) => (EmptyStatus::NotProven, Vec::new(), None),
            _ => (EmptyStatus::Unknown, Vec::new(), None),
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
        internal: &mut Diagnostics,
    ) -> PairReport {
        let shared: Vec<QuantityId> = ra
            .quantities
            .intersection(&rb.quantities)
            .copied()
            .collect();
        // A presence indicator is not a DIMENSION the two regions cut on —
        // it is the bookkeeping that says the dimension has a value. Listing
        // `defined(jets[0].pt)` beside `MET.pt` would tell the reader the
        // regions share a physics variable they do not.
        let shared_dimensions: Vec<String> = shared
            .iter()
            .filter(|&&q| !matches!(self.hir.table.quantity(q), Quantity::Present(_)))
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
            proof_path: None,
            certificate_size: None,
        };

        // 1. Interval fast path (also the no-solver fallback). No solver runs
        //    here, but the refutation is still a two-atom Farkas proof, so it
        //    is certified and bundled like any other (M2: no proven tier
        //    without a receipt).
        if let Some(d) = ca.intervals.disjoint_with(&cb.intervals) {
            report.kind = VerdictKind::ProvenDisjoint;
            report.reason = format!(
                "intervals cannot intersect on {}: {} requires {}, {} requires {}",
                quantity_label(self.hir, d.q),
                ra.name,
                d.a.human(),
                rb.name,
                d.b.human()
            );
            report.proof_path = Some(ProofPath::Interval);
            self.certify_interval_pair(&mut report, &d.parts, ca, cb, origins, internal);
            return report;
        }
        for (ctx, enc) in [(ca, ra), (cb, rb)] {
            if let Some(empty) = ctx.intervals.self_empty() {
                report.kind = VerdictKind::ProvenDisjoint;
                report.reason = format!(
                    "region {} provably selects no events ({}), so the pair cannot intersect",
                    enc.name,
                    empty.human()
                );
                report.proof_path = Some(ProofPath::Interval);
                self.certify_interval_pair(&mut report, &empty.parts(), ca, cb, origins, internal);
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
            let (certified, cert_payload) = self.certify_disjoint(core_names.as_deref(), c1, c2);
            self.pop();
            report.certified = certified;
            report.proof_path = Some(ProofPath::SolverCore);
            // The certified set IS the unsat core (certifying a subset of an
            // UNSAT set is sound), so its size is the core's.
            if certified == Some(true) {
                report.certificate_size = core_names.as_ref().map(Vec::len);
            }
            if let Some(payload) = cert_payload {
                self.push_bundle(&report.a, &report.b, &payload, origins, internal);
            }
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
                            let retry = s.check(timeout);
                            // G7: bypass of Engine::check — still account.
                            self.note_check_result(&retry);
                            if !matches!(retry, SatResult::Sat) {
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
                                    // Fail-closed, not a contradiction: the
                                    // engine reached no overlap conclusion to
                                    // refute — the SAT-direction search ran out
                                    // of realizable models and capped.
                                    internal.fail_closed(format!(
                                        "WITNESS NOT REALIZED for {} vs {}: witness validation \
                                         failed for every model tried within the retry budget, \
                                         so the overlap is capped at POSSIBLY and no claim is \
                                         made — {why}",
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
        // Collections an OPAQUE REDUCER iterates (`sum(jets.pT)` interns as
        // `reduce.sum(jets, …)`). The realizer cannot make an event whose
        // reduction equals the model's value for such a quantity — it has no
        // reference interpretation on this side — so validation of a
        // `sum ⋈ k` cut rests on the realized element set alone. Within the
        // model's OWN bounds a bigger all-pass collection satisfies strictly
        // more `size >= n` cuts and makes every non-negative sum larger, so
        // "as many elements as the realizer will build" is the model most
        // likely to survive re-validation. Wished, never required: on UNSAT
        // the layer is dropped and the plain model is used, so this changes
        // WHICH legal model is realized and nothing about which models are
        // legal.
        let mut bulk_hints: BTreeSet<QuantityId> = BTreeSet::new();
        for &q in mentioned {
            if let Quantity::ExternalFn { name, args } = self.hir.table.quantity(q)
                && self.hir.symbols.key(*name).starts_with("reduce.")
                && let Some(adl_sema::QuantityArg::Collection(c)) = args.first()
                && let Some(sq) = lookup_size(self.hir, *c)
            {
                bulk_hints.insert(sq);
            }
        }
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
        let bulk_atoms: Vec<QFormula> = bulk_hints
            .iter()
            .map(|&sq| {
                QFormula::Atom(adl_formula::LinAtom::single(
                    sq,
                    adl_formula::Rel::Ge,
                    rat(crate::witness::MAX_REALIZED_F),
                ))
            })
            .collect();
        // G7: route each layered check through note_check_result so a
        // spawn/error Unknown here still trips solver_degraded.
        let try_with = |engine: &mut Self, atoms: &[&[QFormula]]| -> Option<Model> {
            let timeout = engine.timeout;
            let s = engine.solver.as_deref_mut()?;
            s.push();
            for group in atoms {
                for a in *group {
                    s.assert(a, None);
                }
            }
            let result = s.check(timeout);
            let m = match &result {
                SatResult::Sat => s.model(),
                _ => None,
            };
            s.pop();
            engine.note_check_result(&result);
            m
        };
        let base = self.solver.as_deref_mut()?.model();
        // Layered: hints are wishes, not requirements — drop the
        // dPhi = 0 preference first, then the ε-interior preference (an
        // overlap may exist only on a boundary), then the existence
        // hints (a model may legitimately need a small size), the
        // realizer caps last, the raw model as the floor.
        try_with(self, &[&zero_atoms, interior, &lo_atoms, &hi_atoms, &bulk_atoms])
            .or_else(|| try_with(self, &[interior, &lo_atoms, &hi_atoms, &bulk_atoms]))
            .or_else(|| try_with(self, &[&zero_atoms, interior, &lo_atoms, &hi_atoms]))
            .or_else(|| try_with(self, &[interior, &lo_atoms, &hi_atoms]))
            .or_else(|| try_with(self, &[&lo_atoms, &hi_atoms]))
            .or_else(|| try_with(self, &[&hi_atoms]))
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
    ///
    /// **Presence indicators are exempt for the same reason, and it is not
    /// cosmetic**: an indicator is 0 or 1, PRES asserts `p ≤ 1`, so
    /// tightening `p ≥ 1` to `p ≥ 1 + ε` makes the whole interior wish
    /// UNSAT — the layered `refined_model` then falls back past its best
    /// layer and returns a model the realizer cannot turn into a validating
    /// event. (Measured: three CMS-SUS-16-033 overlaps lost exactly this way
    /// when PRES landed.)
    fn tightened(&self, f: &QFormula) -> QFormula {
        match f {
            QFormula::True => QFormula::True,
            QFormula::False => QFormula::False,
            QFormula::And(v) => QFormula::And(v.iter().map(|p| self.tightened(p)).collect()),
            QFormula::Or(v) => QFormula::Or(v.iter().map(|p| self.tightened(p)).collect()),
            QFormula::Atom(a) => {
                use adl_formula::Rel;
                let exact_grid = a.terms().iter().all(|&(_, q)| {
                    matches!(
                        self.hir.table.quantity(q),
                        Quantity::Size(_) | Quantity::Present(_)
                    )
                });
                if exact_grid {
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
    /// no subset check reads an unsat core. Under `--certify`, an uncertified
    /// solver UNSAT is not a subset claim (fail closed).
    fn subset<'o>(
        &mut self,
        sub_overs: impl IntoIterator<Item = &'o Over>,
        sup_unders: &[Under],
    ) -> bool {
        self.subset_proof(sub_overs, sup_unders).0
    }

    /// [`Self::subset`] plus the replayable proof behind it, so a caller that
    /// turns a subset into a *derived fact* can carry the derivation with the
    /// fact instead of asserting it as a given. The payload is present exactly
    /// when the claim was certified (so under `--certify` a fact without a
    /// chain is a fact without a proof, and is refused).
    fn subset_proof<'o>(
        &mut self,
        sub_overs: impl IntoIterator<Item = &'o Over>,
        sup_unders: &[Under],
    ) -> (bool, Option<CertPayload>) {
        if self.solver.is_none() {
            return (false, None);
        }
        self.push();
        let neg = Self::negated_under(sup_unders);
        // Name every query atom so an unsat core can pin certification to the
        // small relevant set — dumping Ax∪query into Farkas search is both a
        // wall-time bomb (cms-scale bins/subsets) and unnecessary.
        let mut named: Vec<(AssertName, QFormula)> = Vec::new();
        if let Some(s) = self.solver.as_deref_mut() {
            for (k, o) in sub_overs.into_iter().enumerate() {
                let name = AssertName::new(format!("QSUB{k}"));
                let f = o.qformula().clone();
                s.assert(&f, Some(name.clone()));
                named.push((name, f));
            }
            let neg_name = AssertName::new("QSUBNEG");
            s.assert(&neg, Some(neg_name.clone()));
            named.push((neg_name, neg));
        }
        let result = self.check(self.timeout);
        let core = matches!(result, Some(SatResult::Unsat))
            .then(|| {
                self.solver
                    .as_deref_mut()
                    .and_then(adl_solver::Solver::unsat_core)
            })
            .flatten();
        self.pop();
        if !matches!(result, Some(SatResult::Unsat)) {
            return (false, None);
        }
        let (flag, payload) = self.certify_named_formulas_chain(core.as_deref(), &named);
        (flag != Some(false), payload)
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
        internal: &mut Diagnostics,
    ) -> BTreeMap<&'static str, usize> {
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let Some(recon) = self.recon.take() else {
            return counts;
        };
        // The ledger records what reconciliation concluded — including pairs
        // dropped before any proof ran, which would otherwise surface only as
        // an unexplained POSSIBLY downstream.
        for s in &recon.skipped {
            self.recon_ledger.push(crate::report::ReconReport {
                a: coll_label(self.hir, s.coll_a),
                b: coll_label(self.hir, s.coll_b),
                outcome: crate::report::ReconOutcome::Skipped,
                base: None,
                note: s.reason.clone(),
                a_units: coll_units(self.hir, self.unit, s.coll_a),
                b_units: coll_units(self.hir, self.unit, s.coll_b),
            });
        }
        for n in &recon.near_misses {
            self.recon_near_misses
                .push(crate::report::ReconNearMissReport {
                    a: coll_label(self.hir, n.coll_a),
                    b: coll_label(self.hir, n.coll_b),
                    base_a: n.base_a.clone(),
                    base_b: n.base_b.clone(),
                });
        }
        if self.solver.is_none() || recon.is_empty() {
            return counts;
        }
        let existing = self.existing_size_le();
        let mut k = 0usize;
        for cand in &recon.candidates {
            let PredImplies {
                a_in_b,
                b_in_a,
                a_chain,
                b_chain,
            } = self.prove_pred_implies(&cand.phi_a, &cand.phi_b);
            let (label_a, label_b) = (
                coll_label(self.hir, cand.coll_a),
                coll_label(self.hir, cand.coll_b),
            );
            self.recon_ledger.push(crate::report::ReconReport {
                a: coll_label(self.hir, cand.coll_a),
                b: coll_label(self.hir, cand.coll_b),
                outcome: match (a_in_b, b_in_a) {
                    (true, true) => crate::report::ReconOutcome::Equivalent,
                    (true, false) => crate::report::ReconOutcome::ARefinesB,
                    (false, true) => crate::report::ReconOutcome::BRefinesA,
                    (false, false) => crate::report::ReconOutcome::Unrelated,
                },
                base: base_label(self.hir, cand.coll_a),
                note: if a_in_b || b_in_a {
                    String::new()
                } else {
                    "neither cut set implies the other".to_owned()
                },
                a_units: coll_units(self.hir, self.unit, cand.coll_a),
                b_units: coll_units(self.hir, self.unit, cand.coll_b),
            });
            // Directions to emit, as (sub_size, sup_size, catalog id, the
            // subset refutation the direction rests on).
            let facts: Vec<(QuantityId, QuantityId, AxiomId, Option<&CertPayload>)> =
                if a_in_b && b_in_a {
                    vec![
                        (cand.size_a, cand.size_b, AxiomId::Xeq, a_chain.as_ref()),
                        (cand.size_b, cand.size_a, AxiomId::Xeq, b_chain.as_ref()),
                    ]
                } else if a_in_b {
                    vec![(cand.size_a, cand.size_b, AxiomId::Xsub, a_chain.as_ref())]
                } else if b_in_a {
                    vec![(cand.size_b, cand.size_a, AxiomId::Xsub, b_chain.as_ref())]
                } else {
                    Vec::new()
                };
            for (sub, sup, id, chain) in facts {
                // A collection is trivially its own size; an intra-source
                // SUB fact already carries this refinement.
                if sub == sup || existing.contains(&(sub, sup)) {
                    continue;
                }
                // Under --certify a fact with no replayable derivation is a
                // fact with no proof. It is not asserted: everything that
                // would have leaned on it falls back to POSSIBLY, which is the
                // honest answer, and the reason is filed rather than silent.
                // (This is the fail-closed twin of `subset` refusing an
                // uncertified UNSAT — in practice the two agree, so this
                // branch fires only if that invariant ever breaks.)
                if self.certify && chain.is_none() {
                    internal.fail_closed(format!(
                        "RECONCILIATION FACT WITHHELD for {label_a} / {label_b}: the subset \
                         refutation behind it produced no replayable certificate, so the \
                         derived size fact is not asserted."
                    ));
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
                // …and, with the certificate, so a bundle can carry the
                // fact's own derivation instead of asserting it as a given.
                if let Some(payload) = chain {
                    let sub_first = sub == cand.size_a;
                    let (from, to) = if sub_first {
                        (&label_a, &label_b)
                    } else {
                        (&label_b, &label_a)
                    };
                    self.recon_chains.insert(
                        name.clone(),
                        DerivedFact::new(
                            name.0.clone(),
                            id.as_str().to_owned(),
                            statement.clone(),
                            &fact,
                            vec![Derivation::new(
                                format!(
                                    "every element passing the cuts of {from} also passes those \
                                     of {to}, so {from} is a subset of {to} element-wise: \
                                     UNSAT(over({from}) AND NOT under({to})) over one shared \
                                     generic element"
                                ),
                                payload
                                    .asserts
                                    .iter()
                                    .map(|(n, f)| {
                                        BundleAssert::new(
                                            n.0.clone(),
                                            f,
                                            AssertSource::Query {
                                                role: query_role(&n.0, from, to),
                                            },
                                        )
                                    })
                                    .collect(),
                                payload.cert.clone(),
                            )],
                        ),
                    );
                }
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
    ) -> PredImplies {
        if !self.frame_sat(phi_a, phi_b) {
            return PredImplies::default();
        }
        let (a_in_b, a_chain) = self.subset_proof([&phi_a.over()], &[phi_b.under()]);
        let (b_in_a, b_chain) = self.subset_proof([&phi_b.over()], &[phi_a.under()]);
        PredImplies {
            a_in_b,
            b_in_a,
            a_chain,
            b_chain,
        }
    }

    /// Shared fail-closed certification of a claimed-UNSAT formula set.
    /// `None` = certification off; `Some(true)` + optional certificate =
    /// replay-checked Farkas proof; `Some(false)` = uncertified.
    fn certify_unsat_core(
        &self,
        formulas: &[QFormula],
    ) -> (Option<bool>, Option<adl_certify::Certificate>) {
        if !self.certify {
            return (None, None);
        }
        match adl_certify::certify_unsat(formulas, &adl_certify::Budget::default()) {
            adl_certify::CertifyResult::Certified(cert) => (Some(true), Some(cert)),
            adl_certify::CertifyResult::Uncertified(_) => (Some(false), None),
        }
    }

    /// Resolve a solver unsat core against a named formula map and certify
    /// **only those formulas**. Fail-closed when the core is missing/empty
    /// or names an unknown assert — never dump the full axiom frame into
    /// Farkas search (that path was a multi-minute hang on cms-scale bins).
    /// Certifying a core subset is sound: UNSAT of a subset ⇒ UNSAT of the
    /// asserted superset.
    fn certify_named_formulas(
        &self,
        core: Option<&[AssertName]>,
        extra: &[(AssertName, QFormula)],
    ) -> Option<bool> {
        self.certify_named_formulas_payload(core, extra).0
    }

    fn certify_named_formulas_payload(
        &self,
        core: Option<&[AssertName]>,
        extra: &[(AssertName, QFormula)],
    ) -> Certified {
        self.certify_named_formulas_inner(core, extra, self.combine)
    }

    /// As [`Self::certify_named_formulas_payload`], but always keeping the
    /// payload. Used where the certificate is needed for its own sake (a
    /// derived fact's chain) rather than for a `--combine` bundle.
    fn certify_named_formulas_chain(
        &self,
        core: Option<&[AssertName]>,
        extra: &[(AssertName, QFormula)],
    ) -> Certified {
        self.certify_named_formulas_inner(core, extra, true)
    }

    fn certify_named_formulas_inner(
        &self,
        core: Option<&[AssertName]>,
        extra: &[(AssertName, QFormula)],
        keep_payload: bool,
    ) -> Certified {
        if !self.certify {
            return (None, None);
        }
        let Some(names) = core.filter(|n| !n.is_empty()) else {
            return (Some(false), None);
        };
        let mut fmap: BTreeMap<AssertName, QFormula> = BTreeMap::new();
        for (n, f) in extra {
            fmap.insert(n.clone(), f.clone());
        }
        for (i, inst) in self.axioms.instances.iter().enumerate() {
            fmap.insert(AssertName::new(format!("AX{i}")), inst.formula.clone());
        }
        for (n, f) in &self.recon_facts {
            fmap.insert(n.clone(), f.clone());
        }
        let mut named = Vec::with_capacity(names.len());
        for n in names {
            match fmap.get(n) {
                Some(f) => named.push((n.clone(), f.clone())),
                None => return (Some(false), None),
            }
        }
        let formulas: Vec<QFormula> = named.iter().map(|(_, f)| f.clone()).collect();
        let (flag, cert) = self.certify_unsat_core(&formulas);
        match (flag, cert) {
            (Some(true), Some(cert)) => {
                let payload = keep_payload.then_some(CertPayload {
                    asserts: named,
                    cert,
                    whole: true,
                });
                (Some(true), payload)
            }
            (flag, _) => (flag, None),
        }
    }

    /// Certify a named UNSAT core drawn from `regions`' overs plus
    /// axioms/recon facts. Missing/empty cores fail closed (Candidate),
    /// never fall back to certifying the entire frame.
    fn certify_named_unsat(
        &self,
        core: Option<&[AssertName]>,
        regions: &[&RegionCtx],
    ) -> Certified {
        let mut extra = Vec::new();
        for ctx in regions {
            for (n, o) in &ctx.overs {
                extra.push((n.clone(), o.qformula().clone()));
            }
        }
        self.certify_named_formulas_payload(core, &extra)
    }

    /// Pairwise disjointness wrapper: certify the two-region named core and
    /// optionally emit a combine payload.
    fn certify_disjoint(
        &self,
        core: Option<&[AssertName]>,
        c1: &RegionCtx,
        c2: &RegionCtx,
    ) -> Certified {
        self.certify_named_unsat(core, &[c1, c2])
    }

    /// Certify an interval-path refutation. An interval proof is already a
    /// Farkas certificate of the smallest kind — a lower and an upper bound on
    /// one quantity, multipliers `1/|c|` — so it is *constructed* directly
    /// (`adl_certify::certify_bounds`, no solver, no search) and then accepted
    /// or rejected by the same replay kernel every other tier goes through.
    ///
    /// Two shapes are tried, strongest first: the WHOLE over-projections of
    /// the participating asserts — so the artifact quotes exactly what the
    /// engine asserts, with nothing extracted — and, when a cut carries
    /// disjunctive structure a bound pair cannot cross, the spine conjuncts
    /// that set the bounds (each a top-level conjunct of its cut).
    ///
    /// `None` = certification disabled. `Some(Err(_))` = the kernel would not
    /// confirm what the interval layer proved: a disagreement between two
    /// pieces of this tool, hence an internal diagnostic. It is deliberately
    /// NOT a demotion — the interval layer's verdict is unchanged from before
    /// certification existed, and silently downgrading would bury the bug.
    fn certify_interval(
        &self,
        parts: &[RefutingPart],
        lookup: impl Fn(&AssertName) -> Option<QFormula>,
    ) -> Option<Result<CertPayload, String>> {
        if !self.certify {
            return None;
        }
        if parts.is_empty() {
            return Some(Err("the interval layer reported no refuting atoms".to_owned()));
        }
        let mut whole: Vec<(AssertName, QFormula)> = Vec::new();
        for p in parts {
            let name = p.src();
            if whole.iter().any(|(n, _)| n == name) {
                continue; // both bounds from one cut: one formula covers both
            }
            let Some(f) = lookup(name) else {
                return Some(Err(format!(
                    "no over-projection recorded for assert {}",
                    name.0
                )));
            };
            whole.push((name.clone(), f));
        }
        let forms: Vec<QFormula> = whole.iter().map(|(_, f)| f.clone()).collect();
        if let Some(cert) = adl_certify::certify_bounds(&forms) {
            return Some(Ok(CertPayload {
                asserts: whole,
                cert,
                whole: true,
            }));
        }
        let mut lean: Vec<(AssertName, QFormula)> = Vec::new();
        for p in parts {
            match p {
                RefutingPart::Conjunct(n, a) => {
                    lean.push((n.clone(), QFormula::Atom(a.clone())));
                }
                // A constant-false cut has no atom to extract; it is refuted
                // whole or not at all, and `certify_bounds` above already saw
                // it — so reaching here means the kernel rejected it.
                RefutingPart::Whole(n) => {
                    return Some(Err(format!(
                        "the kernel did not accept the constant-false cut {}",
                        n.0
                    )));
                }
            }
        }
        let forms: Vec<QFormula> = lean.iter().map(|(_, f)| f.clone()).collect();
        match adl_certify::certify_bounds(&forms) {
            Some(cert) => Some(Ok(CertPayload {
                asserts: lean,
                cert,
                whole: false,
            })),
            None => Some(Err(
                "the replay kernel did not accept the bound pair the interval layer refuted on"
                    .to_owned(),
            )),
        }
    }

    /// Record the certification outcome of an interval-path PROVEN DISJOINT
    /// pair on the report, and emit its bundle under `--combine`.
    fn certify_interval_pair(
        &mut self,
        report: &mut PairReport,
        parts: &[RefutingPart],
        ca: &RegionCtx,
        cb: &RegionCtx,
        origins: &BTreeMap<AssertName, CoreItem>,
        internal: &mut Diagnostics,
    ) {
        let outcome = {
            let lookup = |n: &AssertName| over_of(&ca.overs, n).or_else(|| over_of(&cb.overs, n));
            self.certify_interval(parts, lookup)
        };
        match outcome {
            None => {} // certification disabled
            Some(Ok(payload)) => {
                report.certified = Some(true);
                report.certificate_size = Some(payload.asserts.len());
                self.push_bundle(&report.a, &report.b, &payload, origins, internal);
            }
            Some(Err(why)) => internal.contradiction(format!(
                "INTERVAL CERTIFICATE unavailable for PROVEN DISJOINT {} vs {}: {why}. The \
                 interval layer and the replay kernel disagree — one of them is wrong; the \
                 verdict is left as it was and no certification is claimed.",
                report.a, report.b
            )),
        }
    }

    /// Assemble and keep the portable bundle for a certified claim. Emits
    /// nothing (and files a diagnostic) if the assembled bundle does not
    /// replay — the same defensive gate `certify_unsat` applies to its own
    /// output, and the one thing standing between an unbacked `XR{k}` given
    /// and a published artifact.
    fn push_bundle(
        &mut self,
        region_a: &str,
        region_b: &str,
        payload: &CertPayload,
        origins: &BTreeMap<AssertName, CoreItem>,
        internal: &mut Diagnostics,
    ) {
        if !self.combine {
            return;
        }
        let mut asserts = Vec::with_capacity(payload.asserts.len());
        let mut derived_facts: Vec<DerivedFact> = Vec::new();
        for (name, f) in &payload.asserts {
            let source = match self.recon_chains.get(name) {
                Some(fact) => {
                    if !derived_facts.iter().any(|d| d.name == fact.name) {
                        derived_facts.push(fact.clone());
                    }
                    AssertSource::Derived {
                        fact: fact.name.clone(),
                    }
                }
                None => assert_source(origins, name, payload.whole),
            };
            asserts.push(BundleAssert::new(name.0.clone(), f, source));
        }
        let hir = self.hir;
        let bundle = adl_certify::CombineBundle::new(
            BundleParts {
                region_a: region_a.to_owned(),
                region_b: region_b.to_owned(),
                asserts,
                derived_facts,
                certificate: payload.cert.clone(),
            },
            |q| bundle_label(hir, QuantityId(q)),
        );
        if bundle.replay() {
            self.bundles.push(bundle);
        } else {
            internal.fail_closed(format!(
                "BUNDLE WITHHELD for {region_a} vs {region_b}: the assembled certificate \
                 bundle does not replay (most likely a reconciliation fact used as a given \
                 without an embedded derivation). The verdict stands on the analysis; the \
                 portable artifact does not, so none was written."
            ));
        }
    }

    /// Sampling gate + adversarial refute gate for an UNSAT-side pair.
    /// Sampling runs first (fixed battery); then the cut-anchored refute
    /// search. Either hit demotes fail-closed and files a diagnostic.
    #[allow(clippy::too_many_arguments)]
    fn gate_pair(
        &self,
        report: &mut PairReport,
        ia: usize,
        ib: usize,
        interp: &Interp<'_>,
        internal: &mut Diagnostics,
        sample_refutations: &mut usize,
        refute_refutations: &mut usize,
    ) {
        let memb = |idx: usize, e: &adl_interp::Event| {
            interp.eval_region_membership_idx(idx, e).ok()
        };
        if report.kind == VerdictKind::ProvenDisjoint && !self.gate_events.is_empty() {
            for e in &self.gate_events {
                if memb(ia, e) == Some(true) && memb(ib, e) == Some(true) {
                    *sample_refutations += 1;
                    internal.contradiction(format!(
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
                    // G2: a retracted PROVEN must not keep advertising a
                    // replay-checked certificate — `certified: true` with
                    // `kind: possibly_overlapping` would lie to JSON consumers.
                    // The proof provenance goes with it, for the same reason.
                    report.certified = None;
                    report.proof_path = None;
                    report.certificate_size = None;
                    break;
                }
            }
        }
        if report.kind == VerdictKind::ProvenDisjoint
            && crate::refute::search_shared_membership(interp, ia, ib, &self.refute_probes)
                .is_some()
        {
            *refute_refutations += 1;
            internal.contradiction(format!(
                "REFUTE GATE refuted PROVEN DISJOINT for {} vs {}: an adversarial \
                 probe event passes both regions — an encoder/axiom fact is false on \
                 a real event; verdict demoted",
                report.a, report.b
            ));
            report.kind = VerdictKind::PossiblyOverlapping;
            report.reason = "the refute gate refuted a disjointness proof \
                             (internal contradiction, reported as a bug); capped \
                             at POSSIBLY"
                .to_owned();
            report.core.clear();
            report.certified = None;
            report.proof_path = None;
            report.certificate_size = None;
        }
        // Subset claims clear their boolean flags only (no pairwise
        // `certified` field); certification already ran inside `subset`.
        // Scoped separately so the two closures do not both borrow `internal`.
        let (mut a_in_b, mut b_in_a) = (report.subset_a_in_b, report.subset_b_in_a);
        {
            let mut sample_subset = |sub: usize, sup: usize, flag: &mut bool, label: &str| {
                if !*flag || self.gate_events.is_empty() {
                    return;
                }
                for e in &self.gate_events {
                    if memb(sub, e) == Some(true) && memb(sup, e) == Some(false) {
                        *sample_refutations += 1;
                        *flag = false;
                        internal.contradiction(format!(
                            "SAMPLING GATE refuted PROVEN SUBSET ({label}) for {} vs {}: a \
                             sampled event is in the subset region but not the superset; \
                             claim withdrawn",
                            report.a, report.b
                        ));
                        break;
                    }
                }
            };
            sample_subset(ia, ib, &mut a_in_b, "a within b");
            sample_subset(ib, ia, &mut b_in_a, "b within a");
        }
        {
            let mut refute_subset = |sub: usize, sup: usize, flag: &mut bool, label: &str| {
                if !*flag {
                    return;
                }
                if crate::refute::search_subset_counterexample(
                    interp,
                    sub,
                    sup,
                    &self.refute_probes,
                )
                .is_some()
                {
                    *refute_refutations += 1;
                    *flag = false;
                    internal.contradiction(format!(
                        "REFUTE GATE refuted PROVEN SUBSET ({label}) for {} vs {}: an \
                         adversarial probe is in the subset region but not the superset; \
                         claim withdrawn",
                        report.a, report.b
                    ));
                }
            };
            refute_subset(ia, ib, &mut a_in_b, "a within b");
            refute_subset(ib, ia, &mut b_in_a, "b within a");
        }
        report.subset_a_in_b = a_in_b;
        report.subset_b_in_a = b_in_a;
    }

    /// Sampling gate for a proven-empty region: any sampled member refutes.
    /// Returns true when refuted (the caller demotes to NotProven).
    fn gate_empty(
        &self,
        idx: usize,
        interp: &Interp<'_>,
        internal: &mut Diagnostics,
        refutations: &mut usize,
    ) -> bool {
        for e in &self.gate_events {
            if interp.eval_region_membership_idx(idx, e).ok() == Some(true) {
                *refutations += 1;
                internal.contradiction(format!(
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

    /// Adversarial refute gate for a proven/candidate-empty region.
    fn refute_empty(
        &self,
        idx: usize,
        interp: &Interp<'_>,
        internal: &mut Diagnostics,
        refutations: &mut usize,
    ) -> bool {
        if crate::refute::search_membership(interp, idx, &self.refute_probes).is_some() {
            *refutations += 1;
            internal.contradiction(format!(
                "REFUTE GATE refuted REGION EMPTY for {}: an adversarial probe is a \
                 member — an encoder/axiom fact is false on a real event; claim \
                 withdrawn",
                self.hir.symbols.display(self.hir.regions[idx].name)
            ));
            return true;
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

    fn bin_check(
        &mut self,
        set: &BinSetEnc,
        region_ctx: &RegionCtx,
        internal: &mut Diagnostics,
    ) -> BinCheckReport {
        let region_name = self.unit.regions[set.region_idx].name.clone();
        let n = set.bins.len();
        let overs: Vec<Over> = set.bins.iter().map(adl_formula::Formula::over).collect();
        let unders: Vec<Under> = set.bins.iter().map(adl_formula::Formula::under).collect();

        let mut proven = 0usize;
        let total = n * n.saturating_sub(1) / 2;
        for i in 0..n {
            for j in i + 1..n {
                if self.bins_disjoint(region_ctx, &overs[i], &overs[j], &region_name, internal) {
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
    /// Under `--certify`, an uncertified solver UNSAT is not counted as proven.
    fn bins_disjoint(
        &mut self,
        region_ctx: &RegionCtx,
        bi: &Over,
        bj: &Over,
        region: &str,
        internal: &mut Diagnostics,
    ) -> bool {
        let bi_name = AssertName::new("QBINI");
        let bj_name = AssertName::new("QBINJ");
        if self.solver.is_some() {
            self.push();
            // Named region overs + bin atoms → core-scoped certification.
            self.assert_overs(&region_ctx.overs, true);
            let bi_f = bi.qformula().clone();
            let bj_f = bj.qformula().clone();
            if let Some(s) = self.solver.as_deref_mut() {
                s.assert(&bi_f, Some(bi_name.clone()));
                s.assert(&bj_f, Some(bj_name.clone()));
            }
            let r = self.check(self.timeout);
            let core = matches!(r, Some(SatResult::Unsat))
                .then(|| {
                    self.solver
                        .as_deref_mut()
                        .and_then(adl_solver::Solver::unsat_core)
                })
                .flatten();
            self.pop();
            if !matches!(r, Some(SatResult::Unsat)) {
                return false;
            }
            let mut extra: Vec<(AssertName, QFormula)> = region_ctx
                .overs
                .iter()
                .map(|(n, o)| (n.clone(), o.qformula().clone()))
                .collect();
            extra.push((bi_name, bi_f));
            extra.push((bj_name, bj_f));
            return self.certify_named_formulas(core.as_deref(), &extra) != Some(false);
        }
        // No-solver / interval fallback. No unsat core here either, but the
        // bound refutation is certifiable on its own terms — same treatment as
        // the pairwise interval path.
        let mut a = region_ctx.intervals.clone();
        a.add_over(&bi_name, bi);
        let mut b = region_ctx.intervals.clone();
        b.add_over(&bj_name, bj);
        let parts = a
            .self_empty()
            .map(|e| e.parts())
            .or_else(|| b.self_empty().map(|e| e.parts()))
            .or_else(|| a.disjoint_with(&b).map(|d| d.parts));
        let Some(parts) = parts else {
            return false;
        };
        let lookup = |n: &AssertName| {
            if n == &bi_name {
                Some(bi.qformula().clone())
            } else if n == &bj_name {
                Some(bj.qformula().clone())
            } else {
                over_of(&region_ctx.overs, n)
            }
        };
        if let Some(Err(why)) = self.certify_interval(&parts, lookup) {
            internal.contradiction(format!(
                "INTERVAL CERTIFICATE unavailable for a disjoint bin pair of region {region}: \
                 {why}. The interval layer and the replay kernel disagree — one of them is \
                 wrong; the count is left as it was."
            ));
        }
        true
    }

    /// `UNSAT(Ax ∧ R⁺ ∧ ⋀ᵢ ¬(Bᵢ⁻))` ⇒ the bins cover the region; a SAT
    /// answer yields the gap witness (SPEC_ANALYSIS §5). Under `--certify`,
    /// an uncertified solver UNSAT is reported as [`CoverageStatus::NotProven`].
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
        self.assert_overs(&region_ctx.overs, true);
        let mut extra: Vec<(AssertName, QFormula)> = region_ctx
            .overs
            .iter()
            .map(|(n, o)| (n.clone(), o.qformula().clone()))
            .collect();
        if let Some(s) = self.solver.as_deref_mut() {
            for (k, u) in unders.iter().enumerate() {
                let name = AssertName::new(format!("QBINNEG{k}"));
                let neg = u.qformula().clone().not();
                s.assert(&neg, Some(name.clone()));
                extra.push((name, neg));
            }
        }
        let result = self.check(self.timeout);
        let out = match result {
            Some(SatResult::Unsat) => {
                let core = self
                    .solver
                    .as_deref_mut()
                    .and_then(adl_solver::Solver::unsat_core);
                if self.certify_named_formulas(core.as_deref(), &extra) == Some(false) {
                    (CoverageStatus::NotProven, Vec::new())
                } else {
                    (CoverageStatus::Proven, Vec::new())
                }
            }
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

/// What a `subset` query formula plays in the refutation behind a derived
/// fact — the premise names are the engine's (`QSUB<k>` / `QSUBNEG`), which
/// say nothing to a reader of the bundle.
fn query_role(name: &str, sub: &str, sup: &str) -> String {
    if name == "QSUBNEG" {
        format!("negation of the under-projection of the {sup} element predicate")
    } else {
        format!("over-projection of the {sub} element predicate (conjunct {name})")
    }
}

/// The over-projection the engine asserted under `name`, if it is one of
/// `overs`. Linear scan: a region's statement list is short and this runs once
/// per proven interval refutation.
fn over_of(overs: &[(AssertName, Over)], name: &AssertName) -> Option<QFormula> {
    overs
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, o)| o.qformula().clone())
}

/// Bundle provenance for an assert, from the same origin map that feeds the
/// human `--explain` output. `whole` says whether the bundled formula is the
/// cut's entire over-projection or one spine conjunct of it.
fn assert_source(
    origins: &BTreeMap<AssertName, CoreItem>,
    name: &AssertName,
    whole: bool,
) -> AssertSource {
    match origins.get(name) {
        Some(CoreItem::Cut { region, line, text }) => AssertSource::Cut {
            region: region.clone(),
            line: *line,
            text: text.clone(),
            whole,
        },
        Some(CoreItem::Axiom { id, statement }) => AssertSource::Axiom {
            id: id.clone(),
            statement: statement.clone(),
            assumption: AxiomId::ALL
                .into_iter()
                .find(|a| a.as_str() == id)
                .map_or_else(String::new, |a| catalog_entry(a).assumption.to_owned()),
        },
        None => AssertSource::Unattributed,
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
            _ => model.get_f64(q),
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
        if let Some(v) = model.get_f64(q)
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
            && let Some(v) = model.get_f64(sq)
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
            solver_errors: 0,
            first_solver_failure: None,
            gate_events: Vec::new(),
            refute_probes: Vec::new(),
            refute_gate: false,
            certify: false,
            combine: false,
            recon_ledger: Vec::new(),
            recon_near_misses: Vec::new(),
            bundles: Vec::new(),
            recon_facts: Vec::new(),
            recon_chains: BTreeMap::new(),
        };
        let mut origins: BTreeMap<AssertName, CoreItem> = BTreeMap::new();
        let mut internal = Diagnostics::default();
        engine.reconcile(&mut origins, &mut internal)
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
            solver_errors: 0,
            first_solver_failure: None,
            gate_events,
            refute_probes: Vec::new(),
            refute_gate: false,
            certify: false,
            combine: false,
            recon_ledger: Vec::new(),
            recon_near_misses: Vec::new(),
            bundles: Vec::new(),
            recon_facts: Vec::new(),
            recon_chains: BTreeMap::new(),
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
        // Battery is at least the requested size; cut-constant injection
        // may append dedicated MET/HT boundary events on top.
        assert!(s.events >= 64, "events={}", s.events);
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
            solver_errors: 0,
            first_solver_failure: None,
            gate_events: adl_interp::sample::battery(&ext, 64),
            refute_probes: Vec::new(),
            refute_gate: false,
            certify: false,
            combine: false,
            recon_ledger: Vec::new(),
            recon_near_misses: Vec::new(),
            bundles: Vec::new(),
            recon_facts: Vec::new(),
            recon_chains: BTreeMap::new(),
        };
        let r = engine.run();
        assert_eq!(r.pairwise[0].kind, VerdictKind::ProvenDisjoint);
        assert_eq!(r.sampling.unwrap().refutations, 0);
    }

    /// G2: a sampling-gate demotion must clear `certified`. Otherwise the
    /// final JSON can ship `"kind": "possibly_overlapping", "certified": true`
    /// — a retracted proof that still looks independently verified.
    #[test]
    fn gate_demotion_clears_certified_flag() {
        let src = "region RA\n  select MET > 100\nregion RB\n  select MET > 50\n";
        let ext = ExtDecls::legacy();
        let mut hir = adl_sema::analyze_str(src, "t", &ext);
        let unit = crate::encode::encode_unit(&mut hir, src);
        let axioms = AxiomSet::default();
        let gate_events = adl_interp::sample::battery(&ext, 64);
        let engine = Engine {
            hir: &hir,
            ext: &ext,
            unit: &unit,
            axioms: &axioms,
            solver: None,
            solver_label: "none".to_owned(),
            timeout: Duration::from_secs(1),
            unit_name: "t".to_owned(),
            recon: None,
            spawn_failures: 0,
            solver_errors: 0,
            first_solver_failure: None,
            gate_events,
            refute_probes: Vec::new(),
            refute_gate: false,
            certify: false,
            combine: false,
            recon_ledger: Vec::new(),
            recon_near_misses: Vec::new(),
            bundles: Vec::new(),
            recon_facts: Vec::new(),
            recon_chains: BTreeMap::new(),
        };
        let mut report = PairReport {
            a: "RA".to_owned(),
            b: "RB".to_owned(),
            kind: VerdictKind::ProvenDisjoint,
            reason: "fabricated".to_owned(),
            exact: true,
            shared_dimensions: Vec::new(),
            subset_a_in_b: false,
            subset_b_in_a: false,
            witness: Vec::new(),
            witness_validated: None,
            certified: Some(true),
            core: vec![CoreItem::Cut {
                region: "RA".to_owned(),
                line: 1,
                text: "MET > 100".to_owned(),
            }],
            proof_path: Some(ProofPath::SolverCore),
            certificate_size: Some(1),
        };
        let interp = Interp::new(&hir, &ext);
        let mut internal = Diagnostics::default();
        let mut sample_refutations = 0;
        let mut refute_refutations = 0;
        engine.gate_pair(
            &mut report,
            unit.regions[0].idx,
            unit.regions[1].idx,
            &interp,
            &mut internal,
            &mut sample_refutations,
            &mut refute_refutations,
        );
        assert_eq!(report.kind, VerdictKind::PossiblyOverlapping);
        assert!(sample_refutations >= 1);
        assert_eq!(
            report.certified, None,
            "demoted verdict must not keep certified: true"
        );
        assert!(report.core.is_empty());
    }
}
