//! `adl-analysis` — pairwise region verdicts, subset and vacuous-region
//! detection, bin partition checks, witness re-validation, unsat-core
//! explanations, and the human/JSON reports (SPEC_ANALYSIS, ADR-004/008,
//! TESTING §3).
//!
//! Entry points: [`analyze_source`] (text in) and [`analyze_hir`]
//! (resolved unit in). Verdicts never fail the run by default; the
//! [`FailOn`] flags ([`Report::findings`] / [`Report::exit_code`]) are
//! the CI gate plumbing for `--fail-on=overlap|gap|empty|non-exact`.
//!
//! Solver configuration: native z3 (primary), SMT-LIB subprocess
//! (secondary), or none — in which case the sound interval fast path
//! still proves disjointness/vacuity and everything else degrades to
//! POSSIBLY (SPEC_ARCHITECTURE §7).

pub mod encode;
mod engine;
pub mod interval;
mod reconcile;
mod render;
pub mod report;
pub mod witness;

pub use report::{
    AxiomUse, BinCheckReport, CoreItem, CoverageStatus, DroppedLeaf, EmptyStatus, FailOn,
    OVERLAP_CAVEAT, PairReport, RegionReport, Report, SCHEMA_VERSION, VerdictKind, WitnessValue,
};
pub use witness::Validation;

use adl_axioms::emit_axioms;
use adl_sema::{CollectionId, ElemIndex, ExtDecls, Hir, Quantity, QuantityId, analyze_str};
use adl_solver::Solver;
use adl_syntax::diag::Diagnostic;
use std::collections::BTreeSet;
use std::time::Duration;

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-analysis";

/// Which solver backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SolverChoice {
    /// Native z3 if compiled in, else the z3 subprocess if on PATH, else
    /// no solver.
    #[default]
    Auto,
    /// Native z3 only (no solver if the feature is off).
    Native,
    /// SMT-LIB subprocess over the `z3` binary only.
    SubprocessZ3,
    /// Heuristic interval layer only; solver verdicts capped at POSSIBLY.
    NoSolver,
}

/// Analysis options.
#[derive(Debug, Clone)]
pub struct AnalysisOptions {
    pub solver: SolverChoice,
    /// Per-check solver timeout.
    pub timeout: Duration,
    pub fail_on: FailOn,
    /// Prove cross/intra-collection refinements (IDENTICAL / A⊆B) and assert
    /// the derived size facts. Set ONLY by an explicit `verify --cross` run
    /// (owner decision: reconciliation is an opt-in cross-analysis feature);
    /// off for single-file analysis, where structural interning already
    /// relates same-source collections.
    pub reconcile: bool,
}

impl Default for AnalysisOptions {
    fn default() -> Self {
        Self {
            solver: SolverChoice::Auto,
            timeout: Duration::from_secs(10),
            fail_on: FailOn::default(),
            reconcile: false,
        }
    }
}

/// Frontend (parse/sema) failure: the analysis did not run.
#[derive(Debug)]
pub struct FrontendError {
    pub diags: Vec<Diagnostic>,
    pub rendered: String,
}

impl std::fmt::Display for FrontendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.rendered)
    }
}

impl std::error::Error for FrontendError {}

fn make_solver(choice: SolverChoice) -> (Option<Box<dyn Solver>>, String) {
    match choice {
        SolverChoice::NoSolver => (None, "none".to_owned()),
        SolverChoice::Native => native_solver(),
        SolverChoice::SubprocessZ3 => subprocess_solver(),
        SolverChoice::Auto => {
            let (s, label) = native_solver();
            if s.is_some() {
                (s, label)
            } else {
                let (s, label) = subprocess_solver();
                if s.is_some() {
                    (s, label)
                } else {
                    (None, "none".to_owned())
                }
            }
        }
    }
}

#[cfg(feature = "native")]
fn native_solver() -> (Option<Box<dyn Solver>>, String) {
    (
        Some(Box::new(adl_solver::NativeSolver::new())),
        "z3-native".to_owned(),
    )
}

#[cfg(not(feature = "native"))]
fn native_solver() -> (Option<Box<dyn Solver>>, String) {
    (None, "none".to_owned())
}

fn subprocess_solver() -> (Option<Box<dyn Solver>>, String) {
    if adl_solver::subprocess_available("z3") {
        (
            Some(Box::new(adl_solver::SubprocessSolver::z3())),
            "smtlib-subprocess(z3)".to_owned(),
        )
    } else {
        (None, "none".to_owned())
    }
}

/// Analyze ADL source text end to end.
///
/// # Errors
/// Returns [`FrontendError`] when parsing or resolution reports errors —
/// the analysis itself never fails the run (SPEC_ANALYSIS §6).
pub fn analyze_source(
    src: &str,
    unit_name: &str,
    ext: &ExtDecls,
    opts: &AnalysisOptions,
) -> Result<Report, FrontendError> {
    let mut hir = analyze_str(src, unit_name, ext);
    if adl_syntax::diag::has_errors(&hir.diags) {
        let rendered = adl_syntax::diag::render(src, unit_name, &hir.diags);
        return Err(FrontendError {
            diags: hir.diags,
            rendered,
        });
    }
    Ok(analyze_hir(&mut hir, src, ext, opts))
}

/// Analyze a resolved unit. Mutates the HIR in place: `retag_opaque_externals`
/// re-tags region-statement node fragments, and encoding plus axiom emission
/// intern helper quantities into the quantity table.
pub fn analyze_hir(hir: &mut Hir, src: &str, ext: &ExtDecls, opts: &AnalysisOptions) -> Report {
    encode::retag_opaque_externals(hir);
    let unit = encode::encode_unit(hir, src);

    let mut qs: BTreeSet<QuantityId> = BTreeSet::new();
    for r in &unit.regions {
        qs.extend(r.quantities.iter().copied());
    }
    for s in &unit.bin_sets {
        for f in &s.bins {
            encode::formula_quantities(f, &mut qs);
        }
    }
    let axioms = emit_axioms(hir, ext, &qs);

    // Eagerly intern size quantities for every collection with mentioned
    // elements, so the witness-refinement hints (engine) and the witness
    // builder have stable ids without further table mutation.
    let mut elem_colls: BTreeSet<CollectionId> = BTreeSet::new();
    let mut all_q = qs.clone();
    all_q.extend(axioms.quantities());
    for &q in &all_q {
        if let Quantity::ElemProp {
            coll,
            index: ElemIndex::FromFront(_),
            ..
        } = hir.table.quantity(q)
        {
            elem_colls.insert(*coll);
        }
    }
    for c in elem_colls {
        let _ = hir.table.intern_quantity(Quantity::Size(c));
    }

    // Build the reconciliation encoding while the table is still mutable
    // (interns the shared generic-element quantities). Only in an explicit
    // cross run; otherwise same-source collections already share ids.
    let recon = opts.reconcile.then(|| reconcile::build(hir, ext));

    let unit_name = hir.unit.clone();
    let (solver, solver_label) = make_solver(opts.solver);
    let engine = engine::Engine {
        hir,
        ext,
        unit: &unit,
        axioms: &axioms,
        solver,
        solver_label,
        timeout: opts.timeout,
        unit_name,
        recon,
        spawn_failures: 0,
    };
    engine.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-analysis");
    }

    #[test]
    fn fail_on_parses() {
        let f = FailOn::parse("overlap,empty").unwrap();
        assert!(f.overlap && f.empty && !f.gap && !f.non_exact);
        let f = FailOn::parse("non-exact").unwrap();
        assert!(f.non_exact);
        assert!(FailOn::parse("bogus").is_err());
        assert_eq!(FailOn::parse("").unwrap(), FailOn::default());
    }
}
