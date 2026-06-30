//! The ported legacy golden battery (TESTING.md "Golden verdicts";
//! legacy source: `legacy_parser/scripts/run_golden_tests.sh`).
//!
//! Every (file, assertion) pair below is derived from that script: the
//! dual-encoding regression suite (reject-OR band, OR with unencodable
//! branch, not-tag, define-under-OR, tag indices) and the June-2026
//! audit suite (empty-∀, define-arith, angular order, union size,
//! non-finite constants, btag discriminant) — each of these was once a
//! live false verdict in the legacy tool. These are ground-truth physics
//! verdicts: if one fails, the bug is in the analysis code, not here.

use adl_analysis::{
    AnalysisOptions, CoverageStatus, EmptyStatus, FailOn, PairReport, Report, SolverChoice,
    VerdictKind, analyze_source,
};
use adl_sema::ExtDecls;
use std::path::PathBuf;
use std::time::Duration;

fn golden_path(file: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../legacy_parser/tests/golden")
        .join(file)
}

fn run_with(file: &str, solver: SolverChoice) -> Report {
    let path = golden_path(file);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let ext = ExtDecls::legacy();
    let opts = AnalysisOptions {
        solver,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
    };
    analyze_source(&src, file, &ext, &opts)
        .unwrap_or_else(|e| panic!("{file} must parse/resolve cleanly:\n{e}"))
}

fn run(file: &str) -> Report {
    let r = run_with(file, SolverChoice::Auto);
    assert_ne!(r.solver, "none", "golden battery needs a solver backend");
    r
}

fn pair<'r>(report: &'r Report, a: &str, b: &str) -> &'r PairReport {
    report
        .pairwise
        .iter()
        .find(|p| (p.a == a && p.b == b) || (p.a == b && p.b == a))
        .unwrap_or_else(|| panic!("pair {a} vs {b} missing from report"))
}

fn region<'r>(report: &'r Report, name: &str) -> &'r adl_analysis::RegionReport {
    report
        .regions
        .iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("region {name} missing from report"))
}

// ---- encoding structure ---------------------------------------------------

#[test]
fn ite_encoded_exactly_as_guarded_or() {
    let r = run("ite_conditional_dphi.adl");
    let sr = region(&r, "SR_ite");
    assert!(sr.exact, "ITE must encode exactly");
    assert!(sr.or_clauses >= 1, "ITE expands to a guarded OR");
    let line = r
        .human()
        .lines()
        .find(|l| l.starts_with("SR_ite:"))
        .expect("coverage line")
        .to_owned();
    assert!(line.contains("(exact)") && line.contains("OR)"), "{line}");
}

#[test]
fn or_clause_encoded() {
    let r = run("or_met.adl");
    assert!(region(&r, "SR_hi_or_lo").or_clauses >= 1);
    assert!(region(&r, "SR_hi_or_lo").exact);
}

// ---- heuristic + SMT disjointness (sound direction) -------------------------

#[test]
fn disjoint_pt_intervals() {
    let r = run("disjoint_pt.adl");
    let p = pair(&r, "SR_low", "SR_high");
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint);
    // The verdict carries its reason (legacy: "UNSAT|cannot intersect").
    assert!(
        p.reason.contains("UNSAT") || p.reason.contains("cannot intersect"),
        "{}",
        p.reason
    );
    let human = r.human();
    assert!(
        human.contains("SR_low vs SR_high: PROVEN DISJOINT"),
        "pairwise line present:\n{human}"
    );
}

#[test]
fn disjoint_pt_heuristic_without_solver() {
    let r = run_with("disjoint_pt.adl", SolverChoice::NoSolver);
    assert_eq!(r.solver, "none");
    let p = pair(&r, "SR_low", "SR_high");
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint, "--no-smt fast path");
}

#[test]
fn disjoint_same_jet_index_intervals() {
    let r = run("disjoint_jet_index.adl");
    assert_eq!(
        pair(&r, "SR_high", "SR_low").kind,
        VerdictKind::ProvenDisjoint
    );
}

// ---- overlap proofs ---------------------------------------------------------

#[test]
fn met_overlap() {
    let r = run("overlap_met.adl");
    let p = pair(&r, "SR1", "SR2");
    assert!(
        matches!(
            p.kind,
            VerdictKind::ProvenOverlapping | VerdictKind::PossiblyOverlapping
        ),
        "{:?}",
        p.kind
    );
    // With a solver and a MET-only event model the witness validates.
    assert_eq!(p.kind, VerdictKind::ProvenOverlapping);
    assert_eq!(p.witness_validated, Some(true));
    assert!(!p.witness.is_empty());
}

#[test]
fn size_bjets_pairwise_present_and_overlap_proven() {
    let r = run("size_bjets.adl");
    let p = pair(&r, "SR_ge2", "SR_ge4");
    assert_eq!(p.kind, VerdictKind::ProvenOverlapping, "{}", p.reason);
    assert_eq!(p.witness_validated, Some(true), "{}", p.reason);
}

// ---- soundness regression suite: each was a false/missed verdict before
// ---- the legacy dual-encoding rewrite ---------------------------------------

#[test]
fn or_with_unencodable_branch_must_not_prove_disjoint() {
    let r = run("or_unencodable_branch.adl");
    let p = pair(&r, "SR_orcut", "SR_lowmet");
    assert_ne!(p.kind, VerdictKind::ProvenDisjoint);
}

#[test]
fn overlap_proved_through_the_encodable_or_branch() {
    let r = run("or_unencodable_branch.adl");
    let p = pair(&r, "SR_orcut", "SR_lowmet");
    // A joint model exists (so NOT disjoint), but the opaque external
    // function (aplanarity, SPEC_ANALYSIS §2 caveat) keeps the witness a
    // candidate the interpreter cannot validate — so this is CANDIDATE
    // OVERLAPPING, not a PROVEN claim the contract can't back.
    assert_eq!(p.kind, VerdictKind::CandidateOverlapping, "{}", p.reason);
    assert_eq!(p.witness_validated, Some(false), "{}", p.reason);
    assert!(p.reason.contains("candidate"), "{}", p.reason);
}

#[test]
fn reject_of_and_band_proves_disjoint_de_morgan() {
    let r = run("reject_and_band.adl");
    assert_eq!(
        pair(&r, "SR_band", "SR_mid").kind,
        VerdictKind::ProvenDisjoint
    );
}

#[test]
fn not_tag_proves_complementary_regions_disjoint() {
    let r = run("not_tag.adl");
    let p = pair(&r, "SR_btag", "SR_nobtag");
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint);
    assert!(
        r.human().contains("SR_btag vs SR_nobtag: PROVEN DISJOINT"),
        "{}",
        r.human()
    );
}

#[test]
fn define_under_or_stays_disjunctive() {
    let r = run("define_under_or.adl");
    assert_eq!(pair(&r, "SR_a", "SR_b").kind, VerdictKind::ProvenDisjoint);
}

#[test]
fn different_jet_indices_must_not_alias_into_one_tag_variable() {
    let r = run("tag_index.adl");
    let p = pair(&r, "SR_lead_btag", "SR_sub_nobtag");
    assert_ne!(p.kind, VerdictKind::ProvenDisjoint);
}

// ---- z3-gated section of the legacy script ----------------------------------

#[test]
fn tag_01_axiom_proves_threshold_complement_disjoint() {
    let r = run("btag_threshold.adl");
    let p = pair(&r, "SR_no", "SR_yes");
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint);
    // The explanation names the TAG axiom (explanations, SPEC_ANALYSIS §3).
    assert!(
        p.core
            .iter()
            .any(|c| matches!(c, adl_analysis::CoreItem::Axiom { id, .. } if id == "TAG")),
        "core must cite TAG: {:?}",
        p.core
    );
}

#[test]
fn ratio_cut_encoded_exactly_and_disjoint() {
    let r = run("ratio_met.adl");
    let p = pair(&r, "SR_ratio", "SR_lowmet");
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint);
    assert!(p.exact, "ratio encoding must be exact");
    assert!(region(&r, "SR_ratio").exact);
}

#[test]
fn bounded_quantifier_plus_ordering_proves_collection_cut_disjoint() {
    let r = run("collection_quant.adl");
    assert_eq!(
        pair(&r, "SR_allhard", "SR_softlead").kind,
        VerdictKind::ProvenDisjoint
    );
}

#[test]
fn unbounded_collection_cut_must_not_prove_disjoint() {
    let r = run("collection_quant.adl");
    assert_ne!(
        pair(&r, "SR_unbounded", "SR_softlead").kind,
        VerdictKind::ProvenDisjoint
    );
}

#[test]
fn complete_binning_proven_disjoint_and_covering() {
    let r = run("bins_partition.adl");
    let b = r
        .bin_checks
        .iter()
        .find(|b| b.region == "SR_binned")
        .expect("SR_binned bin check");
    assert_eq!(
        (b.n_bins, b.disjoint_pairs_proven, b.disjoint_pairs_total),
        (3, 3, 3)
    );
    assert_eq!(b.coverage, CoverageStatus::Proven);
    assert!(
        r.human()
            .contains("SR_binned [MET]: 3 bins; disjoint 3/3 pairs; coverage: proven"),
        "{}",
        r.human()
    );
}

#[test]
fn incomplete_binning_flags_possible_gap() {
    let r = run("bins_partition.adl");
    let b = r
        .bin_checks
        .iter()
        .find(|b| b.region == "SR_gap")
        .expect("SR_gap bin check");
    assert_eq!(
        (b.n_bins, b.disjoint_pairs_proven, b.disjoint_pairs_total),
        (2, 1, 1)
    );
    assert_eq!(b.coverage, CoverageStatus::NotProven);
    assert!(!b.gap_witness.is_empty(), "gap witness reported");
    assert!(
        r.human()
            .contains("SR_gap [MET]: 2 bins; disjoint 1/1 pairs; coverage: not proven"),
        "{}",
        r.human()
    );
}

// ---- audit regression suite (June 2026 adversarial audit) --------------------

#[test]
fn empty_collection_under_all_reading_no_proven_verdict() {
    let r = run("quant_empty_forall.adl");
    let p = pair(&r, "SR_nojets", "SR_hardjets");
    assert!(
        !matches!(
            p.kind,
            VerdictKind::ProvenDisjoint | VerdictKind::ProvenOverlapping
        ),
        "audit Bug 1 (empty-∀): got {:?}",
        p.kind
    );
}

#[test]
fn define_in_arithmetic_is_inlined() {
    let r = run("define_arith.adl");
    assert_eq!(
        pair(&r, "SR_a", "SR_b").kind,
        VerdictKind::ProvenDisjoint,
        "audit Bug 2: opaque define would block this proof"
    );
}

#[test]
fn reversed_angular_args_stay_convention_neutral() {
    let r = run("angular_order.adl");
    let p = pair(&r, "SR_a", "SR_b");
    assert!(
        !matches!(
            p.kind,
            VerdictKind::ProvenDisjoint | VerdictKind::ProvenOverlapping
        ),
        "audit Bug 3: got {:?}",
        p.kind
    );
    assert!(
        p.reason.contains("OPEN-2") || p.reason.contains("twin"),
        "{}",
        p.reason
    );
}

#[test]
fn union_take_must_not_get_subset_size_axiom() {
    let r = run("union_size.adl");
    for reg in &r.regions {
        assert_ne!(
            reg.empty,
            EmptyStatus::Proven,
            "audit Bug 4: union SUB axiom would prove {} empty",
            reg.name
        );
    }
}

#[test]
fn non_finite_constant_becomes_no_overlap_proof() {
    let r = run("inf_constant.adl");
    for p in &r.pairwise {
        assert_ne!(
            p.kind,
            VerdictKind::ProvenOverlapping,
            "audit Bug 5: dropped assert would fake an overlap: {} vs {}",
            p.a,
            p.b
        );
    }
}

#[test]
fn continuous_btag_discriminant_not_forced_to_01() {
    let r = run("btag_discriminant.adl");
    let p = pair(&r, "SR_a", "SR_b");
    assert_eq!(
        p.kind,
        VerdictKind::ProvenOverlapping,
        "audit Bug 6: {}",
        p.reason
    );
    assert_eq!(p.witness_validated, Some(true), "{}", p.reason);
}

#[test]
fn dphi_range_axiom_catches_vacuous_region() {
    let r = run("vacuous_dphi.adl");
    assert_eq!(region(&r, "SR_dead").empty, EmptyStatus::Proven);
    assert!(
        r.human().contains("provably selects no events"),
        "{}",
        r.human()
    );
}

#[test]
fn empty_region_disjoint_from_everything() {
    let r = run("vacuous_dphi.adl");
    assert_eq!(
        pair(&r, "SR_dead", "SR_any").kind,
        VerdictKind::ProvenDisjoint
    );
}

#[test]
fn reject_of_or_band_proves_overlap() {
    let r = run("reject_or_band.adl");
    let p = pair(&r, "SR_band", "SR_mid");
    assert_eq!(p.kind, VerdictKind::ProvenOverlapping, "{}", p.reason);
}

#[test]
fn subset_detection_mid_inside_kept_band() {
    let r = run("reject_or_band.adl");
    let p = pair(&r, "SR_band", "SR_mid");
    assert!(p.subset_b_in_a, "SR_mid within SR_band");
    assert!(!p.subset_a_in_b, "SR_band is strictly larger");
    assert!(
        r.human().contains("PROVEN SUBSET: SR_mid within SR_band"),
        "{}",
        r.human()
    );
}

#[test]
fn smt_proven_overlap_size_bjets() {
    let r = run("size_bjets.adl");
    assert_eq!(
        pair(&r, "SR_ge2", "SR_ge4").kind,
        VerdictKind::ProvenOverlapping
    );
}

#[test]
fn independent_jet_indices_may_overlap() {
    let r = run("independent_jet_index.adl");
    let p = pair(&r, "SR_lead_high", "SR_sub_low");
    assert!(
        matches!(
            p.kind,
            VerdictKind::ProvenOverlapping | VerdictKind::PossiblyOverlapping
        ),
        "{:?}: {}",
        p.kind,
        p.reason
    );
}

// ---- error reporting ----------------------------------------------------------

#[test]
fn parse_errors_report_the_offending_line_and_fail() {
    let path = golden_path("bad_syntax.adl");
    let src = std::fs::read_to_string(&path).unwrap();
    let ext = ExtDecls::legacy();
    let err = analyze_source(&src, "bad_syntax.adl", &ext, &AnalysisOptions::default())
        .expect_err("bad input must fail");
    assert!(
        err.rendered.contains("bad_syntax.adl:5:"),
        "error must point at line 5:\n{}",
        err.rendered
    );
}

// ---- JSON export ----------------------------------------------------------------

#[test]
fn json_export_carries_the_verdicts() {
    let r = run("disjoint_pt.adl");
    let json = r.to_json();
    assert!(json.contains("\"proven_disjoint\""), "{json}");
    assert!(json.contains("\"schema_version\": 1"), "{json}");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    assert!(parsed["pairwise"].is_array());
}

// ---- determinism + degradation + fail-on (SPEC_ANALYSIS §6) ----------------------

#[test]
fn reports_are_byte_identical_across_runs() {
    for file in ["disjoint_pt.adl", "bins_partition.adl", "size_bjets.adl"] {
        let a = run(file);
        let b = run(file);
        assert_eq!(a.human(), b.human(), "{file} human report determinism");
        assert_eq!(a.to_json(), b.to_json(), "{file} JSON determinism");
    }
}

#[test]
fn no_solver_degradation_caps_at_possibly() {
    let r = run_with("overlap_met.adl", SolverChoice::NoSolver);
    let p = pair(&r, "SR1", "SR2");
    assert_eq!(p.kind, VerdictKind::PossiblyOverlapping);
    assert!(p.reason.contains("no solver"), "{}", p.reason);
}

#[test]
fn fail_on_plumbing_gates_findings_explicitly() {
    // Verdicts never fail the run by default.
    let r = run("vacuous_dphi.adl");
    assert_eq!(r.exit_code(&FailOn::default()), 0);
    let f = FailOn::parse("empty").unwrap();
    assert_eq!(r.exit_code(&f), 4);
    assert!(r.findings(&f).iter().any(|m| m.contains("SR_dead")));

    let r = run("bins_partition.adl");
    assert_eq!(r.exit_code(&FailOn::default()), 0);
    let f = FailOn::parse("gap").unwrap();
    assert_eq!(r.exit_code(&f), 4, "SR_gap coverage not proven");

    let r = run("overlap_met.adl");
    let f = FailOn::parse("overlap").unwrap();
    assert_eq!(r.exit_code(&f), 4);

    let r = run("quant_empty_forall.adl");
    let f = FailOn::parse("non-exact").unwrap();
    assert_eq!(
        r.exit_code(&f),
        4,
        "OPEN-1 Dual makes SR_hardjets non-exact"
    );
}

// ---- backend parity spot-check (subprocess runs the same physics) ------------------

#[test]
fn subprocess_backend_spot_check() {
    if !adl_solver::subprocess_available("z3") {
        eprintln!("SKIP: no z3 binary on PATH");
        return;
    }
    let r = run_with("disjoint_pt.adl", SolverChoice::SubprocessZ3);
    assert_eq!(r.solver, "smtlib-subprocess(z3)");
    assert_eq!(
        pair(&r, "SR_low", "SR_high").kind,
        VerdictKind::ProvenDisjoint
    );
    let r = run_with("btag_threshold.adl", SolverChoice::SubprocessZ3);
    assert_eq!(
        pair(&r, "SR_no", "SR_yes").kind,
        VerdictKind::ProvenDisjoint
    );
    let r = run_with("vacuous_dphi.adl", SolverChoice::SubprocessZ3);
    assert_eq!(
        pair(&r, "SR_dead", "SR_any").kind,
        VerdictKind::ProvenDisjoint
    );
}
