//! Regression pins for the AUDIT_2026-07-28 f64-folding false-PROVEN class
//! (C1–C6). Each case was a demonstrated false PROVEN DISJOINT: the encoder
//! folded a comparison operand into an exact-rational atom whose boundary
//! diverges from the interpreter's stepwise-f64 evaluation at a half-ulp
//! flat spot. The strict f64-exactness rule refuses those folds; these tests
//! lock the post-fix contract (NOT ProvenDisjoint + interpreter accepts
//! the witness in both regions). See docs/archive/adl2/COUNTEREXAMPLES.md
//! CE-8…CE-13 and docs/archive/reports/AUDIT_2026-07-28_VALIDATION_ENGINE.md.

use adl_analysis::{
    AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_source,
};
use adl_interp::{Interp, parse_event};
use adl_sema::{ExtDecls, analyze_str};
use std::time::Duration;

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
        reconcile: false,
        sample_gate: 64,
        certify: true,
        combine: false,
    }
}

fn assert_not_proven_disjoint(src: &str, label: &str) {
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, &format!("{label}.adl"), &ext, &opts()).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP {label}: no solver");
        return;
    }
    assert_ne!(
        r.pairwise[0].kind,
        VerdictKind::ProvenDisjoint,
        "{label}: must NOT be ProvenDisjoint, got {:?} ({})",
        r.pairwise[0].kind,
        r.pairwise[0].reason
    );
}

fn assert_witness_in_both(src: &str, jsonl: &str, label: &str) {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, label, &ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "{label} diags: {:?}",
        hir.diags
    );
    let event = parse_event(jsonl.trim(), &ext).unwrap_or_else(|e| {
        panic!("{label}: witness failed to load: {e}\n{jsonl}");
    });
    let interp = Interp::new(&hir, &ext);
    for name in ["a", "b"] {
        let m = interp
            .eval_region_membership(name, &event)
            .unwrap_or_else(|e| panic!("{label}: eval {name}: {e}"));
        assert!(m, "{label}: witness must be a member of region {name}");
    }
}

/// Minimal JSONL event carrying only MET (and optional HT).
fn met_ht_event(met: f64, ht: Option<f64>) -> String {
    match ht {
        Some(ht) => format!(
            r#"{{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{{"pt":{met},"phi":0.0}},"HT":{ht},"triggers":{{"mu_trig":0,"el_trig":0}}}}"#
        ),
        None => format!(
            r#"{{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{{"pt":{met},"phi":0.0}},"HT":0.0,"triggers":{{"mu_trig":0,"el_trig":0}}}}"#
        ),
    }
}

/// CE-8 / C1 — multiplicative regrouping.
#[test]
fn c1_multiplicative_regrouping_not_proven_disjoint() {
    let src = "\
region a
  select MET*0.2*0.3 + HT > 1

region b
  select 0.06*MET + HT <= 1
";
    assert_not_proven_disjoint(src, "c1");
    assert_witness_in_both(
        src,
        &met_ht_event(947.8280087844788, Some(-55.869680527068724)),
        "c1",
    );
}

/// CE-9 / C2 — Neg(Num) additive literal.
#[test]
fn c2_neg_num_literal_not_proven_disjoint() {
    let src = "\
region a
  select MET + -0.1 > 0.3

region b
  select MET <= 0.4
";
    assert_not_proven_disjoint(src, "c2");
    assert_witness_in_both(src, &met_ht_event(0.4, None), "c2");
}

/// CE-10 / C3 — abs_cmp re-flatten of multi-op inner.
#[test]
fn c3_abs_cancellation_not_proven_disjoint() {
    let src = "\
region a
  select abs(MET + HT - HT) < 50

region b
  select abs(MET) >= 50
";
    assert_not_proven_disjoint(src, "c3");
    assert_witness_in_both(
        src,
        &met_ht_event(50.0, Some(1_152_921_504_606_846_976.0)),
        "c3",
    );
}

/// CE-11 / C4 — single dyadic add (the guard's previously-allowed case).
#[test]
fn c4_dyadic_add_not_proven_disjoint() {
    let src = "\
region a
  select MET + 0.5 <= 1

region b
  select MET > 0.5
";
    assert_not_proven_disjoint(src, "c4");
    assert_witness_in_both(src, &met_ht_event(0.5000000000000001, None), "c4");
}

/// CE-12 / C5 — single multiply by non-dyadic constant.
#[test]
fn c5_single_mul_not_proven_disjoint() {
    let src = "\
region a
  select MET*0.3 <= 1

region b
  select MET >= 3.3333333333333335
";
    assert_not_proven_disjoint(src, "c5");
    assert_witness_in_both(src, &met_ht_event(3.3333333333333335, None), "c5");
}

/// CE-13 / C6 — constant-only multiplicative fold; both regions genuinely
/// share the f64-emulated boundary, so the verdict may be ProvenOverlapping
/// (or weaker-but-not-disjoint).
#[test]
fn c6_const_mul_not_proven_disjoint() {
    let src = "\
region a
  select MET <= 0.1 * 3

region b
  select MET >= 0.30000000000000004
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "c6.adl", &ext, &opts()).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP c6: no solver");
        return;
    }
    let kind = r.pairwise[0].kind;
    assert_ne!(
        kind,
        VerdictKind::ProvenDisjoint,
        "c6: must NOT be ProvenDisjoint, got {:?} ({})",
        kind,
        r.pairwise[0].reason
    );
    // Correct positive: both regions share the f64 boundary — ProvenOverlapping
    // is ideal; Possibly/Candidate/Unknown are acceptable weakenings.
    assert!(
        matches!(
            kind,
            VerdictKind::ProvenOverlapping
                | VerdictKind::CandidateOverlapping
                | VerdictKind::PossiblyOverlapping
                | VerdictKind::Unknown
        ),
        "c6: expected overlapping-or-weaker, got {kind:?}"
    );
    assert_witness_in_both(src, &met_ht_event(0.30000000000000004, None), "c6");
}

/// CE-14 — constant-denominator ratio clearing (`L/d ⋈ c` → exact `L ⋈ c·d`)
/// with non-power-of-two `d`. Demonstrated 2026-07-29 double-check: interval
/// path emitted PROVEN DISJOINT while the interpreter accepted a shared jet.
#[test]
fn c14_ratio_const_den_not_proven_disjoint() {
    let src = "\
object jets
  take Jet

region a
  select pT(jets[0]) / 0.3 <= 0.1

region b
  select pT(jets[0]) > 0.03
";
    let wit = r#"{"Jet":[{"pt":0.030000000000000002,"eta":0.0,"phi":0.0,"m":0.0}],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{"pt":0.0,"phi":0.0},"HT":0.0,"triggers":{"mu_trig":0,"el_trig":0}}"#;
    assert_not_proven_disjoint(src, "c14");
    assert_witness_in_both(src, wit, "c14");
}
