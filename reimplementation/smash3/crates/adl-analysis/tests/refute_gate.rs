//! Trustworthy-verify M1: the adversarial refute gate.
//!
//! The gate builds cut-anchored and flat-spot probe events and runs them
//! through the reference interpreter after every UNSAT-side PROVEN; a hit
//! demotes the verdict. C4 / C5 / CE-14 are the fixtures it exists for — they
//! were false PROVEN DISJOINTs at a half-ulp flat spot.
//!
//! With the interpreter exact (M3) those pairs are genuine partitions, so the
//! probes find nothing and the PROVEN DISJOINT verdicts stand. Both halves are
//! pinned here: the search finds no shared event, AND the verify path ships
//! the verdict *with the gate on and reporting zero refutations* — i.e. the
//! net had its chance at every boundary anchor and confirmed the proof.
//!
//! The gate must also never manufacture an event outside the domain the
//! axioms assume; a probe like that can refute a TRUE claim. See
//! `empty_region_survives_a_negative_cut_anchor`.

use adl_analysis::refute::{probe_events, search_shared_membership};
use adl_analysis::{
    AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_source,
};
use adl_interp::Interp;
use adl_sema::{ExtDecls, analyze_str};
use std::time::Duration;

const C4: &str = "\
region a
  select MET + 0.5 <= 1

region b
  select MET > 0.5
";

const C5: &str = "\
region a
  select MET*0.3 <= 1

region b
  select MET >= 3.3333333333333335
";

const CE14: &str = "\
object jets
  take Jet

region a
  select pT(jets[0]) / 0.3 <= 0.1

region b
  select pT(jets[0]) > 0.03
";

fn opts(refute_gate: bool) -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
        reconcile: false,
        sample_gate: 64,
        refute_gate,
        certify: true,
        combine: false,
    }
}

/// Cut constants the fixtures declare (same set `cut_constants` extracts).
fn fixture_cuts(src: &str) -> Vec<f64> {
    if src.contains("MET + 0.5") {
        vec![0.5, 1.0]
    } else if src.contains("MET*0.3") {
        vec![0.3, 1.0, 3.3333333333333335]
    } else if src.contains("/ 0.3") {
        vec![0.3, 0.1, 0.03]
    } else {
        panic!("unknown fixture");
    }
}

/// M3b: Exact Rat membership agrees these pairs are disjoint — the probe
/// search must not invent a shared event.
fn assert_exact_no_shared(src: &str, label: &str) {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, label, &ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "{label} diags: {:?}",
        hir.diags
    );
    let probes = probe_events(&ext, &fixture_cuts(src));
    let interp = Interp::new(&hir, &ext);
    let ia = hir
        .regions
        .iter()
        .position(|r| hir.symbols.display(r.name) == "a")
        .expect("region a");
    let ib = hir
        .regions
        .iter()
        .position(|r| hir.symbols.display(r.name) == "b")
        .expect("region b");
    let hit = search_shared_membership(&interp, ia, ib, &probes);
    assert!(
        hit.is_none(),
        "{label}: Exact Rat membership must find no shared probe event"
    );
}

#[test]
fn exact_no_shared_c4() {
    assert_exact_no_shared(C4, "c4");
}

#[test]
fn exact_no_shared_c5() {
    assert_exact_no_shared(C5, "c5");
}

#[test]
fn exact_no_shared_ce14() {
    assert_exact_no_shared(CE14, "ce14");
}

fn assert_proven_disjoint_survives_the_gate(src: &str, label: &str) {
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, &format!("{label}.adl"), &ext, &opts(true)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP {label}: no solver");
        return;
    }
    let ri = r.refute.expect("refute accounting present when gate on");
    assert!(ri.probes > 0, "{label}: the gate must actually probe");
    assert_eq!(ri.refutations, 0, "{label}: probes refuted the verdict");
    assert_eq!(
        r.pairwise[0].kind,
        VerdictKind::ProvenDisjoint,
        "{label}: exact semantics make this a partition, got {:?} ({})",
        r.pairwise[0].kind,
        r.pairwise[0].reason
    );
}

#[test]
fn verify_c4_proven_disjoint_survives_the_gate() {
    assert_proven_disjoint_survives_the_gate(C4, "c4");
}

#[test]
fn verify_c5_proven_disjoint_survives_the_gate() {
    assert_proven_disjoint_survives_the_gate(C5, "c5");
}

#[test]
fn verify_ce14_proven_disjoint_survives_the_gate() {
    assert_proven_disjoint_survives_the_gate(CE14, "ce14");
}

/// A `< 0` cut on a magnitude anchors the probe generator at a negative
/// value. `pt` is non-negative by the NNEG axiom and by the loader, so the
/// region really is empty — and the gate must not fabricate a negative-pT
/// event to "refute" it. (It did: the battery injected the anchor straight
/// into the pT pool, the loader accepted it, and the sampling gate withdrew a
/// true REGION EMPTY claim as an "internal contradiction".)
#[test]
fn empty_region_survives_a_negative_cut_anchor() {
    let ext = ExtDecls::legacy();
    let src = "\
object eles
  take Ele

region R
  select 2 * pT(eles[0]) < 0
";
    let r = analyze_source(src, "negpt.adl", &ext, &opts(true)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP negpt: no solver");
        return;
    }
    assert_eq!(
        r.regions[0].empty,
        adl_analysis::EmptyStatus::Proven,
        "pT >= 0 makes R empty; gates must not refute it ({:?})",
        r.regions[0].empty
    );
    assert_eq!(r.refute.expect("gate on").refutations, 0);
    assert_eq!(r.sampling.expect("sampling on").refutations, 0);
}

/// `--no-refute-gate` / `refute_gate: false` skips the adversarial search;
/// sampling remains independent. Accounting field absent.
#[test]
fn no_refute_gate_skips_search_accounting() {
    let ext = ExtDecls::legacy();
    let r = analyze_source(C4, "c4.adl", &ext, &opts(false)).expect("resolves");
    assert!(r.refute.is_none(), "refute accounting must be absent when off");
    // Sampling gate still runs by default.
    assert!(r.sampling.is_some());
}
