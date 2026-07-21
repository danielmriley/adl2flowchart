//! `--combine` bundle production: every certified PROVEN DISJOINT pair
//! yields a portable bundle that (a) replays through the trusted kernel,
//! (b) survives a JSON round-trip, and (c) fails closed when tampered.
//! The bundle is the offline artifact — this test is the in-tree mirror of
//! `smash2 verify --combine DIR/` + `smash2-recheck DIR/`.

use adl_analysis::report::Report;
use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_hir};
use adl_certify::CombineBundle;
use adl_sema::{ExtDecls, Hir, analyze_str, merge_hirs};
use std::time::Duration;

fn cross_combine(units: &[(&str, &str)]) -> Report {
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
        certify: true,
        combine: true,
    };
    analyze_hir(&mut merged, "", &ext, &opts)
}

// The demo pair: tight jets (pt>30) vs loose jets (pt>25), same eta window;
// SR needs >=3 tight, CR needs <=2 loose — XSUB gives PROVEN DISJOINT.
const A: &str = "\
object jets\n  take Jet\n  select pt > 30\n  select abs(eta) < 2.4\n\n\
region SR\n  select size(jets) >= 3\n";
const B: &str = "\
object jets\n  take Jet\n  select pt > 25\n  select abs(eta) < 2.4\n\n\
region CR\n  select size(jets) <= 2\n";

#[test]
fn certified_disjoint_pair_yields_replayable_bundle() {
    let report = cross_combine(&[("a", A), ("b", B)]);
    if report.solver == "none" {
        eprintln!("no solver available; skipping bundle test");
        return;
    }
    let p = &report.pairwise[0];
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint, "{}", p.reason);
    assert_eq!(p.certified, Some(true));

    assert_eq!(report.combine_bundles.len(), 1, "one bundle per certified pair");
    let bundle = &report.combine_bundles[0];
    assert_eq!((bundle.region_a.as_str(), bundle.region_b.as_str()), (p.a.as_str(), p.b.as_str()));
    assert!(bundle.replay(), "fresh bundle must replay");

    // JSON round-trip (what `--combine` writes and `smash2-recheck` reads).
    let js = serde_json::to_string_pretty(bundle).unwrap();
    let back: CombineBundle = serde_json::from_str(&js).unwrap();
    assert_eq!(&back, bundle);
    assert!(back.replay(), "bundle must replay after the file round-trip");

    // Tamper: zero the first nonzero Farkas multiplier — the linear parts
    // no longer cancel, so replay must fail. (Note a tamper that merely
    // STRENGTHENS a constraint constant is correctly still refuted by the
    // same certificate; multipliers are the right thing to corrupt.)
    let mut tampered: serde_json::Value = serde_json::from_str(&js).unwrap();
    fn zero_first_multiplier(v: &mut serde_json::Value) -> bool {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(serde_json::Value::Array(mults)) =
                    m.get_mut("Farkas").and_then(|f| f.get_mut("multipliers"))
                {
                    for entry in mults.iter_mut() {
                        if entry.as_str() != Some("0") {
                            *entry = serde_json::Value::String("0".into());
                            return true;
                        }
                    }
                }
                m.values_mut().any(zero_first_multiplier)
            }
            serde_json::Value::Array(a) => a.iter_mut().any(zero_first_multiplier),
            _ => false,
        }
    }
    assert!(
        zero_first_multiplier(&mut tampered),
        "no nonzero multiplier found to tamper"
    );
    let t: CombineBundle = serde_json::from_value(tampered).unwrap();
    assert!(!t.replay(), "tampered bundle must fail replay");
}

#[test]
fn combine_off_produces_no_bundles() {
    let ext = ExtDecls::legacy();
    let hirs: Vec<Hir> = [("a", A), ("b", B)]
        .iter()
        .map(|(n, s)| analyze_str(s, n, &ext))
        .collect();
    let refs: Vec<&Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    let opts = AnalysisOptions {
        reconcile: true,
        ..AnalysisOptions::default()
    };
    let report = analyze_hir(&mut merged, "", &ext, &opts);
    assert!(report.combine_bundles.is_empty(), "default runs must not pay for bundling");
}
