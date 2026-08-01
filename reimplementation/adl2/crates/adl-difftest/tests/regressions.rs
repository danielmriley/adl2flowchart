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
    // M3a: values load as shortest-decimal Rat (float_roundtrip → from_decimal_f64).
    let want_eta = adl_sema::Rat::from_decimal_f64(50.999_999_046_325_684).unwrap();
    let want_ht = adl_sema::Rat::from_decimal_f64(25.999_999_046_325_684).unwrap();
    assert_eq!(eta, &want_eta, "loader must not perturb values");
    assert_eq!(e.scalars["ht"], want_ht);
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
        proof_path: None,
        certificate_size: None,
    };
    let report = Report {
        schema_version: SCHEMA_VERSION,
        unit: "synthetic".to_owned(),
        solver: "synthetic".to_owned(),
        solver_degraded: None,
        solver_failures: None,
        certification: false,
        sampling: None,
        refute: None,
        regions: Vec::new(),
        pairwise: vec![pair],
        bin_checks: Vec::new(),
        reconciliations: Vec::new(),
        recon_near_misses: Vec::new(),
        axioms_used: Vec::new(),
        internal_diagnostics: Vec::new(),
        diagnostics: Vec::new(),
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
/// certify instantly. `Summary::consistent` was widened to treat {PROVEN,
/// CANDIDATE} DISJOINT as one class — which held only until the same core
/// sensitivity surfaced on the subset flag, where there is no candidate tier
/// (CE-17). Inheritance is now canonicalized in the encoder, so this case
/// agrees EXACTLY; the assertion below says so.
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
    // R3 (SPEC_PRESENCE_MODEL §9 step 6), restored 2026-08-01. RA is TRULY
    // empty — `¬d0 ∧ d0`, complements of ONE predicate, which holds even
    // over absence events (absence soft-falses d0's inner comparisons, so
    // `not d0` holds and `d0` fails). From 2026-07-25 this was pinned
    // fail-closed at NotProven: `guarded_not` widened the superset of every
    // negation over a possibly-absent quantity, and d0 negates property
    // cuts. With presence in the leaves the two sides are exact complements
    // again and the emptiness is derivable — as is the disjoint tier the
    // suspended comment named.
    assert_eq!(s1.empty_ra, EmptyStatus::Proven, "{s1:?}");
    assert_eq!(s1.empty_rb, EmptyStatus::Proven, "RB inherits RA: {s1:?}");
    assert_eq!(s1.kind, VerdictKind::ProvenDisjoint, "{s1:?}");
    // Since CE-17 canonicalized the encoding, inheritance no longer changes
    // the core the certifier sees, so the two renderings agree EXACTLY here —
    // stronger than the `consistent` class equality the battery still needs
    // for the rewrites that do legitimately move the core (define inlining).
    assert_eq!(
        s1, s2,
        "inherit vs paste must produce the same verdict:\n  {s1:?}\n  {s2:?}"
    );
    check_sound(&r1).unwrap();
    check_sound(&r2).unwrap();
}

/// CE-17: CE-7's core-shape sensitivity, this time on the SUBSET flag —
/// where there is no "candidate" tier to absorb it. `RB` inherits `RA` and
/// adds a cut, so `RB ⊆ RA` holds by construction; the paste rendering
/// claimed it and the inherit rendering did not.
///
/// Why: an `Inherit` used to encode as ONE named assert holding the whole
/// inherited region, so the solver's minimized unsat core could not drop a
/// single inherited cut, and `adl-certify` had to refute all of them —
/// 2²⁰ case splits here, past its 100 000-branch budget. The paste core is
/// `{MET > 400, ¬RA⁻}` and certifies instantly. Since M1 an uncertifiable
/// subset UNSAT is no claim at all, so the tiers diverged into a hard
/// `rb_in_ra` mismatch (CI, 2026-07-31, metamorphic `inherit_vs_paste`).
///
/// Fixed by canonicalizing the *encoding*, not the assertion:
/// `encode::flatten_inherits` expands inheritance to the same per-statement
/// granularity pasting produces, so both renderings emit byte-identical
/// formulas and every downstream query — solver, core, certificate — is the
/// same one (`inherit_and_paste_encode_identically` in adl-analysis pins the
/// encoder-level identity).
#[test]
fn ce17_inherit_vs_paste_subset_claim_survives_certification() {
    // Twenty disjunctions each implied by `MET > 400`: redundant for the
    // subset proof, so a minimized core drops them — but only if it can name
    // them one by one.
    let redundant: String = (1..=20)
        .map(|i| format!("  select (MET > {i} or HT > {i})\n"))
        .collect();
    let ra_body = format!("  select MET > 400\n{redundant}");
    let rb_extra = "  select HT < 800\n";
    let inherit = format!("{HEAD}region RA\n{ra_body}\nregion RB\n  RA\n{rb_extra}");
    let paste = format!("{HEAD}region RA\n{ra_body}\nregion RB\n{ra_body}{rb_extra}");

    let r1 = run(&inherit);
    let r2 = run(&paste);
    assert_eq!(r1.passes, r2.passes, "interpreter membership must not move");
    let s1 = summary(&r1.report).unwrap();
    let s2 = summary(&r2.report).unwrap();
    // The whole summary, not just `consistent`: subset flags are hard facts.
    assert_eq!(s1, s2, "inherit and paste must produce the same verdict");
    assert!(
        s1.rb_in_ra,
        "RB adds a cut to RA, so RB ⊆ RA — and it is provable: {s1:?}"
    );
    // The claim itself is checked against the sampled events, so a "fix" that
    // made both sides claim a FALSE subset would fail here, not pass.
    check_sound(&r1).unwrap();
    check_sound(&r2).unwrap();
}
