//! The AUDIT_2026-07-28 f64-folding family (C1–C6 / CE-14), after M3+M4.
//!
//! Every one of these was a demonstrated **false PROVEN DISJOINT**: the
//! encoder folded a cut into an exact-rational atom while the interpreter
//! evaluated the same cut stepwise in f64, and at a half-ulp flat spot an
//! event satisfied both "disjoint" regions. The fix at the time was to refuse
//! the fold.
//!
//! M3 removed the cause instead of the symptom — the interpreter now
//! evaluates the rational fragment exactly — so these pairs are *genuine*
//! partitions (`MET + 0.5 <= 1` really is the complement of `MET > 0.5` when
//! both sides are exact). M4 realigned the encoder to fold them again. So the
//! contract flips: each pair must now be **ProvenDisjoint**, and the historic
//! witness must no longer be a member of both regions.
//!
//! Both halves matter. The verdict alone would be satisfied by a lucky
//! encoder; the witness check is the direct evidence that the counterexample
//! is gone rather than hidden.

use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_source};
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
        refute_gate: true,
        certify: true,
        combine: false,
    }
}

/// The pair is a real partition, and the engine proves it.
///
/// `analyze_source` runs the sampling battery and the adversarial refute
/// battery through the reference interpreter and demotes any UNSAT-side
/// PROVEN it can refute — so a surviving ProvenDisjoint is already a
/// differential statement, not just a solver result.
fn assert_proven_disjoint(src: &str, label: &str) {
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, &format!("{label}.adl"), &ext, &opts()).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP {label}: no solver");
        return;
    }
    assert_eq!(
        r.pairwise[0].kind,
        VerdictKind::ProvenDisjoint,
        "{label}: exact semantics make this a partition, got {:?} ({})",
        r.pairwise[0].kind,
        r.pairwise[0].reason
    );
}

/// The historic f64 witness must no longer sit in both regions.
fn assert_witness_no_longer_shared(src: &str, jsonl: &str, label: &str) {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, label, &ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "{label} diags: {:?}",
        hir.diags
    );
    let event = parse_event(jsonl.trim(), &ext)
        .unwrap_or_else(|e| panic!("{label}: witness failed to load: {e}\n{jsonl}"));
    let interp = Interp::new(&hir, &ext);
    let mut in_both = true;
    for name in ["a", "b"] {
        let m = interp
            .eval_region_membership(name, &event)
            .unwrap_or_else(|e| panic!("{label}: eval {name}: {e}"));
        in_both &= m;
    }
    assert!(
        !in_both,
        "{label}: the f64-seam witness is STILL a member of both regions — \
         the PROVEN DISJOINT verdict would be false"
    );
}

/// Minimal JSONL event carrying only MET (and optional HT).
fn met_ht_event(met: f64, ht: f64) -> String {
    format!(
        r#"{{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{{"pt":{met},"phi":0.0}},"HT":{ht},"triggers":{{"mu_trig":0,"el_trig":0}}}}"#
    )
}

/// CE-8 / C1 — multiplicative regrouping. `MET*0.2*0.3` is `3/50·MET`
/// exactly, which is `0.06*MET`.
///
/// Its historic witness (`MET=947.83, HT=-55.87`) is not reproduced here: a
/// negative HT is outside the domain the NNEG axiom asserts, so the loader
/// now refuses it. That is the same premise-alignment fix as the negative-pT
/// case — an event outside the axioms' domain can "refute" a true claim.
#[test]
fn c1_multiplicative_regrouping_is_a_partition() {
    let src = "\
region a
  select MET*0.2*0.3 + HT > 1

region b
  select 0.06*MET + HT <= 1
";
    assert_proven_disjoint(src, "c1");
    assert_witness_no_longer_shared(src, &met_ht_event(947.828_008_784_478_8, 55.87), "c1");
}

/// CE-9 / C2 — Neg(Num) additive literal: `0.3 + 0.1` is `4/10`.
#[test]
fn c2_neg_num_literal_is_a_partition() {
    let src = "\
region a
  select MET + -0.1 > 0.3

region b
  select MET <= 0.4
";
    assert_proven_disjoint(src, "c2");
    assert_witness_no_longer_shared(src, &met_ht_event(0.4, 0.0), "c2");
}

/// CE-10 / C3 — `MET + HT - HT` cancels exactly.
#[test]
fn c3_abs_cancellation_is_a_partition() {
    let src = "\
region a
  select abs(MET + HT - HT) < 50

region b
  select abs(MET) >= 50
";
    assert_proven_disjoint(src, "c3");
    assert_witness_no_longer_shared(
        src,
        &met_ht_event(50.0, 1_152_921_504_606_846_976.0),
        "c3",
    );
}

/// CE-11 / C4 — single dyadic add. The f64 sum `0.5000000000000001 + 0.5`
/// rounded to exactly 1.0; the rational sum does not.
#[test]
fn c4_dyadic_add_is_a_partition() {
    let src = "\
region a
  select MET + 0.5 <= 1

region b
  select MET > 0.5
";
    assert_proven_disjoint(src, "c4");
    assert_witness_no_longer_shared(src, &met_ht_event(0.500_000_000_000_000_1, 0.0), "c4");
}

/// CE-12 / C5 — multiply by a non-dyadic constant.
#[test]
fn c5_single_mul_is_a_partition() {
    let src = "\
region a
  select MET*0.3 <= 1

region b
  select MET >= 3.3333333333333335
";
    assert_proven_disjoint(src, "c5");
    assert_witness_no_longer_shared(src, &met_ht_event(3.333_333_333_333_333_5, 0.0), "c5");
}

/// CE-13 / C6 — constant-only multiplicative fold. `0.1 * 3` is `3/10`, so
/// the gap between it and the literal `0.30000000000000004` is real.
#[test]
fn c6_const_mul_is_a_partition() {
    let src = "\
region a
  select MET <= 0.1 * 3

region b
  select MET >= 0.30000000000000004
";
    assert_proven_disjoint(src, "c6");
    assert_witness_no_longer_shared(src, &met_ht_event(0.300_000_000_000_000_04, 0.0), "c6");
}

/// CE-14 — constant-denominator ratio clearing with non-power-of-two `d`.
/// `pT/0.3 <= 0.1` clears to `pT <= 3/100` exactly, which is what the
/// interpreter computes now.
#[test]
fn c14_ratio_const_den_is_a_partition() {
    let src = "\
object jets
  take Jet

region a
  select pT(jets[0]) / 0.3 <= 0.1

region b
  select pT(jets[0]) > 0.03
";
    let wit = r#"{"Jet":[{"pt":0.030000000000000002,"eta":0.0,"phi":0.0,"m":0.0}],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{"pt":0.0,"phi":0.0},"HT":0.0,"triggers":{"mu_trig":0,"el_trig":0}}"#;
    assert_proven_disjoint(src, "c14");
    assert_witness_no_longer_shared(src, wit, "c14");
}

/// The seam M4 itself opened and closed: the encoder folded `0.1 + 0.2` in
/// f64 (`0.30000000000000004`) while the interpreter had moved to exact
/// rationals (`3/10`). `verify` reported proven_disjoint for this pair while
/// `run` accepted `MET = 0.30000000000000004` in BOTH regions — the event is
/// above `3/10` (so region A holds) and is exactly region B's point band.
#[test]
fn m4_const_fold_seam_is_overlapping_not_disjoint() {
    let ext = ExtDecls::legacy();
    let src = "\
region a
  select MET > 0.1 + 0.2

region b
  select MET [] 0.30000000000000004 0.30000000000000004
";
    let r = analyze_source(src, "m4_seam.adl", &ext, &opts()).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP m4 seam: no solver");
        return;
    }
    assert_ne!(
        r.pairwise[0].kind,
        VerdictKind::ProvenDisjoint,
        "the fl-point event is in both regions: {}",
        r.pairwise[0].reason
    );

    // And directly: the interpreter accepts it in both.
    let hir = analyze_str(src, "m4_seam.adl", &ext);
    let event = parse_event(&met_ht_event(0.300_000_000_000_000_04, 0.0), &ext).unwrap();
    let interp = Interp::new(&hir, &ext);
    for name in ["a", "b"] {
        assert!(
            interp.eval_region_membership(name, &event).unwrap(),
            "region {name} must contain the fl-point event"
        );
    }
}
