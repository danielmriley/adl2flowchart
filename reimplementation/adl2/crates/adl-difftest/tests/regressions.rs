//! Minimized counterexamples found by the TESTING §2 heavyweight layers
//! (see `COUNTEREXAMPLES.md` at the workspace root). Every case here was
//! once a live false verdict or a verdict-stability bug — they are
//! regression-locked forever.

use adl_analysis::{AnalysisOptions, EmptyStatus, VerdictKind};
use adl_difftest::oracle::{check_sound, run_case, sample_events, summary};
use adl_interp::Event;
use adl_sema::ExtDecls;
use std::sync::OnceLock;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

fn events() -> &'static [Event] {
    static EVENTS: OnceLock<Vec<Event>> = OnceLock::new();
    EVENTS.get_or_init(|| sample_events(ext()))
}

const HEAD: &str = "object jets\n  take Jet\n\nobject eles\n  take Ele\n\n";

fn run(src: &str) -> adl_difftest::oracle::CaseRun {
    run_case(src, ext(), events(), &AnalysisOptions::default())
        .unwrap_or_else(|e| panic!("case must run: {e}"))
}

/// CE-1: false PROVEN DISJOINT. `reject` is the exact negation of its
/// condition, but a comparison over a missing element is *false*, so its
/// negation holds on the empty event — both regions contain it. The
/// unguarded encoder negated the bare atom (`pt ≥ 50` vs `pt < 50` ⇒
/// UNSAT) and proved disjointness of two overlapping regions.
/// Fixed by element-existence guards (adl-formula encoder).
#[test]
fn ce1_reject_complement_pair_is_not_disjoint() {
    let src = format!(
        "{HEAD}region RA\n  reject pT(jets[0]) < 50\n\nregion RB\n  reject pT(jets[0]) >= 50\n"
    );
    let run = run(&src);
    let pair = &run.report.pairwise[0];
    assert_ne!(
        pair.kind,
        VerdictKind::ProvenDisjoint,
        "the empty-jets event passes both regions"
    );
    // The empty event is a genuine, interpreter-validated overlap.
    assert_eq!(pair.kind, VerdictKind::ProvenOverlapping, "{}", pair.reason);
    assert_eq!(pair.witness_validated, Some(true), "{}", pair.reason);
    check_sound(&run).unwrap();
}

/// CE-2: false REGION EMPTY. Two rejects whose negations conflict on
/// `jets[0].pt` proved the region empty — but the empty-jets event
/// passes both rejects. Fixed by element-existence guards.
#[test]
fn ce2_conflicting_rejects_region_is_not_empty() {
    let src = format!(
        "{HEAD}region RA\n  reject pT(jets[0]) > 30\n  reject pT(jets[0]) < 60\n\n\
         region RB\n  select MET > 100\n"
    );
    let run = run(&src);
    assert_eq!(
        run.report.regions[0].empty,
        EmptyStatus::NotProven,
        "the empty-jets event is a member of RA"
    );
    check_sound(&run).unwrap();
}

/// CE-3: false PROVEN SUBSET (both directions claimed: regions "equal").
/// `reject pt < 50` contains the empty-jets event; `select pt >= 50`
/// does not — RA ⊄ RB. Fixed by element-existence guards.
#[test]
fn ce3_reject_is_not_subset_of_select_complement() {
    let src = format!(
        "{HEAD}region RA\n  reject pT(jets[0]) < 50\n\nregion RB\n  select pT(jets[0]) >= 50\n"
    );
    let run = run(&src);
    let pair = &run.report.pairwise[0];
    assert!(
        !pair.subset_a_in_b,
        "RA contains the empty-jets event, RB does not"
    );
    // The true subset direction must survive the fix: RB ⊆ RA.
    assert!(pair.subset_b_in_a, "every RB event passes RA's reject");
    check_sound(&run).unwrap();
}

/// CE-4 (loader): serde_json's default float parsing is lossy — event
/// values were perturbed by several ulps on load, breaking bit-exact
/// witness validation and loader fidelity. Fixed by the
/// `float_roundtrip` feature (adl-interp Cargo.toml).
#[test]
fn ce4_event_loader_floats_roundtrip() {
    let e = adl_interp::parse_event(
        r#"{"Electron":[{"eta":50.999999046325684,"pt":1.0}],"HT":25.999999046325684}"#,
        ext(),
    )
    .unwrap();
    let eta_key = ext().prop_canon("eta").0;
    let eta = e.collections["electron"][0].get(&eta_key).unwrap();
    assert_eq!(
        eta, 50.999_999_046_325_684_f64,
        "loader must not perturb values"
    );
    assert_eq!(e.scalars["ht"], 25.999_999_046_325_684_f64);
}

/// CE-5 (verdict stability): swap(A,B) flipped PROVEN OVERLAPPING to
/// POSSIBLY because witness realization depended on the solver's
/// arbitrary model (sizes beyond the realizer cap, dPhi at the wrap
/// discontinuity, boundary vertices breaking f64 re-evaluation, π
/// contagion through equality sums). Fixed by canonical pairwise query
/// order + layered model refinement (interior/ε, dyadic dPhi bounds,
/// size caps) + bounded witness retry with dyadic snapping.
/// This case is the original swap divergence, locked in both orders.
#[test]
fn ce5_swap_symmetry_of_dphi_size_overlap() {
    let ra = "region RA\n  reject ((BTag(eles[0]) <= 0 or BTag(eles[0]) > 0) and (not (BTag(jets[0]) + HT > 100) or 2 * pT(eles[1]) [] 200 800))\n  reject (((not (BTag(eles[0]) >= 1) or size(jets) == 1)) ? ((size(jets) > 1 and (MET < 200 and size(eles) >= 2))) : ((BTag(eles[1]) + dPhi(jets[0], eles[0]) > 200 or size(eles) > 2)))\n";
    let rb = "region RB\n  select (((dPhi(jets[0], eles[0]) <= 0) ? (dPhi(jets[0], eles[0]) <= -3)) and (MET <= 50 and (BTag(eles[0]) [] 0 1 or HT - BTag(eles[1]) > 50)))\n  select ((HT > 50 and (dPhi(jets[0], eles[0]) > 1.5 and pT(jets[0]) != 25)) or (dPhi(jets[0], eles[0]) >= -1.5 and BTag(jets[1]) ][ 0 0))\n";
    let r1 = run(&format!("{HEAD}{ra}\n{rb}"));
    let r2 = run(&format!("{HEAD}{rb}\n{ra}"));
    let s1 = summary(&r1.report).unwrap();
    let s2 = summary(&r2.report).unwrap();
    assert_eq!(s1, s2, "swap(A,B) must not change verdicts");
    check_sound(&r1).unwrap();
    check_sound(&r2).unwrap();
}

/// CE-6 (witness completeness): a region can reference event data only
/// through statements whose atoms folded away (`dPhi − dPhi < 25`
/// becomes `True`), so the model never pins them; synthetic witness
/// objects must still carry the standard property set, and missing
/// event-level scalars must default as free values instead of hard-
/// failing validation.
#[test]
fn ce6_folded_atom_properties_still_realize() {
    let src = format!(
        "{HEAD}region RA\n  select size(jets) >= 1\n  select size(eles) >= 1\n  \
         select dPhi(jets[0], eles[0]) - dPhi(jets[0], eles[0]) < 25\n\n\
         region RB\n  select size(jets) >= 1\n"
    );
    let run = run(&src);
    let pair = &run.report.pairwise[0];
    assert_eq!(pair.kind, VerdictKind::ProvenOverlapping, "{}", pair.reason);
    assert_eq!(pair.witness_validated, Some(true), "{}", pair.reason);
    check_sound(&run).unwrap();
}

#[test]
fn check_sound_flags_mislabelled_validated_candidate() {
    // Review F16: the oracle's CANDIDATE-consistency branch was unreachable
    // by the opaque-free generator, so nothing pinned the labelling contract
    // "a validated overlap must be PROVEN OVERLAPPING, never CANDIDATE".
    // Feed it a synthetic mislabelled pair and assert it fires.
    use adl_analysis::report::{PairReport, Report, SCHEMA_VERSION, VerdictKind};
    use adl_difftest::oracle::{CaseRun, check_sound};
    let pair = PairReport {
        a: "RA".to_owned(),
        b: "RB".to_owned(),
        kind: VerdictKind::CandidateOverlapping,
        reason: String::new(),
        exact: true,
        shared_dimensions: Vec::new(),
        subset_a_in_b: false,
        subset_b_in_a: false,
        witness: Vec::new(),
        witness_validated: Some(true),
        certified: None,
        core: Vec::new(),
    };
    let report = Report {
        schema_version: SCHEMA_VERSION,
        unit: "synthetic".to_owned(),
        solver: "synthetic".to_owned(),
        solver_degraded: None,
        sampling: None,
        regions: Vec::new(),
        pairwise: vec![pair],
        bin_checks: Vec::new(),
        axioms_used: Vec::new(),
        internal_diagnostics: Vec::new(),
        combine_bundles: Vec::new(),
    };
    let run = CaseRun {
        report,
        passes: Vec::new(),
    };
    let err = check_sound(&run).expect_err("mislabelled candidate must be an oracle error");
    assert!(
        err.contains("CANDIDATE OVERLAPPING but witness_validated"),
        "{err}"
    );
}

/// CE-7: verdict-stability. Inherit (`RB` = bare `RA` reference) vs paste
/// (RA's statements inlined) flipped PROVEN DISJOINT to CANDIDATE DISJOINT:
/// the UNSAT is deterministic, but z3's minimized core is not invariant
/// under inlining — the inherit core was {one select, the monolithic RA
/// reference conjunction} and the certificate search exceeded its case-split
/// budget on it, while the paste core was two small facts (`d0 ∧ ¬d0`) that
/// certify instantly. Certification strength is core-shape-dependent by
/// design, so `Summary::consistent` treats {PROVEN, CANDIDATE} DISJOINT as
/// one class — this pins both that equivalence and the still-strict facts
/// (disjointness itself, empties, subsets).
#[test]
fn ce7_inherit_vs_paste_certification_tier_wobble() {
    let define =
        "define d0 = not ((Eta(jets[1]) >= 2 or (pT(eles[1]) <= 50 and pT(eles[-1]) >= 100)))\n\n";
    let ra = "region RA\n  select not (d0)\n  \
         select ((((pT(jets[0]) <= 100 and pT(jets[-1]) <= 0) or dPhi(jets[0], eles[0]) == -1.5) \
         or (dPhi(jets[0], eles[0]) * size(jets) > 25 and MET < 50)) and (d0 or d0))\n\n";
    let rb_extra = "  select ((MET + BTag(eles[1]) [] 0 25 and (size(eles) < 2 and \
         min(dPhi(jets[0], eles[0]), Eta(eles[0])) < 3)) and (BTag(eles[-2]) != 1 and \
         pT(eles[0]) + 1.1 != 100))\n";
    let inherit = format!("{HEAD}{define}{ra}region RB\n  RA\n{rb_extra}");
    let paste = {
        let ra_body = ra
            .trim_start_matches("region RA\n")
            .trim_end_matches("\n\n");
        format!("{HEAD}{define}{ra}region RB\n{ra_body}\n{rb_extra}")
    };
    let r1 = run(&inherit);
    let r2 = run(&paste);
    assert_eq!(r1.passes, r2.passes, "interpreter membership must not move");
    let s1 = summary(&r1.report).unwrap();
    let s2 = summary(&r2.report).unwrap();
    // RA is empty (¬d0 ∧ d0), so the pair is UNSAT-disjoint in both
    // renderings; only the certification tier may differ.
    assert!(
        matches!(
            s1.kind,
            VerdictKind::ProvenDisjoint | VerdictKind::CandidateDisjoint
        ),
        "{s1:?}"
    );
    assert_eq!(s1.empty_ra, EmptyStatus::Proven, "{s1:?}");
    assert!(
        s1.consistent(&s2),
        "inherit vs paste must stay consistent:\n  {s1:?}\n  {s2:?}"
    );
    check_sound(&r1).unwrap();
    check_sound(&r2).unwrap();
}
