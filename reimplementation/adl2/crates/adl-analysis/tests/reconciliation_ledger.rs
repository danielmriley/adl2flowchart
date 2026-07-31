//! The reconciliation ledger and its near-miss advisories.
//!
//! The ledger makes cross-file collection identity VISIBLE: which
//! collections were related (and by which axiom), which were not, and
//! which were skipped with why. The advisories name the one assumption
//! that would unlock a blocked pair — without ever deriving it.
//!
//! The load-bearing negative test is `jet_vs_electron_is_never_advised`:
//! two DIFFERENT detector objects routinely carry identical cuts, and
//! suggesting they be identified would be advice toward an unsound claim.

use adl_analysis::report::{ReconOutcome, Report};
use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, analyze_hir};
use adl_sema::{ExtDecls, Hir, analyze_str, merge_hirs};
use std::time::Duration;

fn cross(units: &[(&str, &str)]) -> Report {
    let ext = ExtDecls::legacy();
    let hirs: Vec<Hir> = units.iter().map(|(n, s)| analyze_str(s, n, &ext)).collect();
    for h in &hirs {
        assert!(!adl_syntax::diag::has_errors(&h.diags), "{}: {:#?}", h.unit, h.diags);
    }
    let refs: Vec<&Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    let opts = AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
        reconcile: true,
        sample_gate: 64,
        refute_gate: true,
        certify: true,
        combine: false,
    };
    analyze_hir(&mut merged, "", &ext, &opts)
}

/// Differently-NAMED collections over the same detector base, with
/// equivalent cuts, appear in the ledger as an XEQ equivalence.
#[test]
fn equivalent_collections_are_reported_as_equivalent() {
    let a = "object ajets\n  take Jet\n  select pt > 30\nregion RA\n  select size(ajets) >= 3\n";
    let b = "object bjets\n  take Jet\n  select pt > 20\n  select pt > 30\nregion RB\n  select size(bjets) <= 2\n";
    let r = cross(&[("a", a), ("b", b)]);
    if r.solver == "none" {
        eprintln!("no solver; skipping");
        return;
    }
    let row = r
        .reconciliations
        .iter()
        .find(|x| x.a.contains("ajets") && x.b.contains("bjets"))
        .expect("a ledger row for the two collections");
    assert_eq!(row.outcome, ReconOutcome::Equivalent);
    assert_eq!(row.outcome.axiom(), Some("XEQ"));
    assert_eq!(row.base.as_deref(), Some("jet"));
    // And it must be visible in the human report.
    let text = r.human_default(false);
    assert!(text.contains("== collection reconciliation =="), "{text}");
    assert!(text.contains("XEQ"), "{text}");
}

/// A strictly tighter collection is reported as refining the looser one.
#[test]
fn refinement_is_reported_with_direction() {
    let a = "object tight\n  take Jet\n  select pt > 30\nregion RA\n  select size(tight) >= 3\n";
    let b = "object loose\n  take Jet\n  select pt > 25\nregion RB\n  select size(loose) <= 2\n";
    let r = cross(&[("a", a), ("b", b)]);
    if r.solver == "none" {
        return;
    }
    let row = r
        .reconciliations
        .iter()
        .find(|x| x.a.contains("tight") || x.b.contains("tight"))
        .expect("a ledger row");
    // Whichever order the pair was enumerated in, the tighter one refines.
    let tight_is_a = row.a.contains("tight");
    let expected = if tight_is_a {
        ReconOutcome::ARefinesB
    } else {
        ReconOutcome::BRefinesA
    };
    assert_eq!(row.outcome, expected, "row: {row:?}");
    assert_eq!(row.outcome.axiom(), Some("XSUB"));
}

/// Identical cut structure over two PRIVATE base names cannot be related
/// from the text — the ledger says so as an advisory, deriving nothing.
#[test]
fn private_bases_with_identical_cuts_are_advised_not_derived() {
    let a = "object trkA\n  take SignalTrack\n  select pt > 30\nregion RA\n  select size(trkA) >= 3\n";
    let b = "object trkB\n  take dispTrack\n  select pt > 30\nregion RB\n  select size(trkB) <= 1\n";
    let r = cross(&[("a", a), ("b", b)]);
    if r.solver == "none" {
        return;
    }
    assert_eq!(
        r.recon_near_misses.len(),
        1,
        "expected one advisory, got {:?}",
        r.recon_near_misses
    );
    let n = &r.recon_near_misses[0];
    assert!(n.base_a != n.base_b, "advisory must name two different bases");

    // Advisory ONLY: no XSUB/XEQ was derived, so the pair stays unproven.
    assert!(
        !r.axioms_used.iter().any(|a| a.id == "XSUB" || a.id == "XEQ"),
        "an advisory must never derive a size fact"
    );
    let text = r.human_default(false);
    assert!(text.contains("identical cut structure but different bases"), "{text}");
}

/// THE load-bearing negative: two DIFFERENT known detector objects with
/// identical cuts must NEVER be advised as identifiable. Jets are not
/// electrons, however alike their kinematic cuts look.
#[test]
fn jet_vs_electron_is_never_advised() {
    let a = "object jx\n  take Jet\n  select pt > 40\nregion RA\n  select size(jx) >= 3\n";
    let b = "object ex\n  take Electron\n  select pt > 40\nregion RB\n  select size(ex) <= 1\n";
    let r = cross(&[("a", a), ("b", b)]);
    if r.solver == "none" {
        return;
    }
    assert!(
        r.recon_near_misses.is_empty(),
        "two distinct detector objects must never be advised as the same: {:?}",
        r.recon_near_misses
    );
}

/// Single-file analysis does not run reconciliation, so the report keeps
/// its original shape (no ledger section).
#[test]
fn single_file_reports_have_no_ledger() {
    let ext = ExtDecls::legacy();
    let src = "object jets\n  take Jet\n  select pt > 30\nregion R\n  select size(jets) >= 1\n";
    let r = adl_analysis::analyze_source(src, "u", &ext, &AnalysisOptions::default()).unwrap();
    assert!(r.reconciliations.is_empty());
    assert!(r.recon_near_misses.is_empty());
    assert!(!r.human_default(false).contains("collection reconciliation"));
}
