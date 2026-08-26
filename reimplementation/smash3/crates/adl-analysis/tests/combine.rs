//! `--combine` bundle production: every PROVEN DISJOINT pair yields a
//! portable bundle that (a) replays through the trusted kernel, (b) survives a
//! JSON round-trip, (c) fails closed when tampered, (d) carries the derivation
//! chain of every reconciliation fact it leans on, and (e) is byte-identical
//! across runs. The bundle is the offline artifact — this test is the in-tree
//! mirror of `smash2 verify --combine DIR/` + `smash2-recheck DIR/`.

use adl_analysis::report::Report;
use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_hir};
use adl_certify::CombineBundle;
use adl_certify::bundle::AssertSource;
use adl_sema::{ExtDecls, Hir, analyze_str, merge_hirs};
use std::time::Duration;

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
        reconcile: true,
        sample_gate: 64,
        refute_gate: true,
        certify: true,
        combine: true,
    }
}

fn cross_combine(units: &[(&str, &str)]) -> Report {
    let ext = ExtDecls::legacy();
    let hirs: Vec<Hir> = units.iter().map(|(n, s)| analyze_str(s, n, &ext)).collect();
    for h in &hirs {
        assert!(!adl_syntax::diag::has_errors(&h.diags), "{}: {:#?}", h.unit, h.diags);
    }
    let refs: Vec<&Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    analyze_hir(&mut merged, "", &ext, &opts())
}

fn cross_combine_files(dir: &str) -> Report {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join(dir);
    let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", root.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "adl"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .adl files in {}", root.display());
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|p| {
            (
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(p).unwrap(),
            )
        })
        .collect();
    let pairs: Vec<(&str, &str)> = sources
        .iter()
        .map(|(n, s)| (n.as_str(), s.as_str()))
        .collect();
    cross_combine(&pairs)
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

/// The flagship cross-file claim rests on an `XR<k>` size fact. That fact must
/// arrive with its own proof, not as a given: the bundle carries the
/// element-predicate refutation behind it, and replay re-derives it.
#[test]
fn reconciliation_facts_travel_with_their_derivation() {
    let report = cross_combine(&[("a", A), ("b", B)]);
    if report.solver == "none" {
        eprintln!("no solver available; skipping chain test");
        return;
    }
    let bundle = &report.combine_bundles[0];
    let used: Vec<&str> = bundle
        .asserts
        .iter()
        .filter_map(|a| match &a.source {
            AssertSource::Derived { fact } => Some(fact.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        !used.is_empty(),
        "this pair is only disjoint via a reconciliation fact; none is in the core"
    );
    for name in &used {
        let fact = bundle
            .derived_facts
            .iter()
            .find(|f| f.name == *name)
            .unwrap_or_else(|| panic!("no embedded derivation for {name}"));
        assert_eq!(fact.axiom, "XSUB");
        assert!(!fact.derivations.is_empty(), "a fact with no proof");
        for d in &fact.derivations {
            assert!(d.replay(), "embedded derivation does not replay: {}", d.claim);
            assert!(!d.premises.is_empty());
        }
    }

    // Everything the bundle mentions is named.
    assert!(!bundle.quantities.is_empty());

    // Removing the chain must break the bundle — the fact would be a given.
    let mut stripped = bundle.clone();
    stripped.derived_facts.clear();
    assert!(!stripped.replay(), "an unbacked XR fact was accepted");
}

/// The other interval route: a region whose own spine is empty makes every
/// pair containing it disjoint. That verdict runs no solver either, and it too
/// must arrive with a replayable receipt — the corpus happens not to contain
/// one, which is exactly why it is pinned here.
#[test]
fn a_self_empty_region_certifies_its_pairs() {
    const EMPTY: &str = "\
object jets\n  take Jet\n  select pt > 30\n\n\
region VAC\n  select size(jets) >= 3\n  select size(jets) <= 2\n\n\
region OK\n  select MET > 100\n";
    let report = cross_combine(&[("u", EMPTY)]);
    let p = report
        .pairwise
        .iter()
        .find(|p| p.reason.starts_with("region "))
        .unwrap_or_else(|| panic!("no self-empty pair: {:#?}", report.pairwise));
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint);
    assert_eq!(p.certified, Some(true), "{}", p.reason);
    let bundle = report
        .combine_bundles
        .iter()
        .find(|b| b.region_a == p.a && b.region_b == p.b)
        .expect("no bundle for the self-empty pair");
    assert!(bundle.replay());
    // Two opposing bounds from ONE region, each traced to its own cut.
    assert_eq!(bundle.asserts.len(), 2);
    assert!(report.internal_diagnostics.is_empty(), "{:#?}", report.internal_diagnostics);
}

/// Byte-determinism: the artifact is a function of the inputs alone. No wall
/// clock, no ordering that depends on hashing or on which run this is.
#[test]
fn two_runs_produce_byte_identical_bundles() {
    let one = cross_combine(&[("a", A), ("b", B)]);
    let two = cross_combine(&[("a", A), ("b", B)]);
    if one.solver == "none" {
        eprintln!("no solver available; skipping determinism test");
        return;
    }
    let js = |r: &Report| serde_json::to_string_pretty(&r.combine_bundles).unwrap();
    assert_eq!(js(&one), js(&two), "bundles differ between identical runs");
    assert!(
        !js(&one).contains("timestamp") && !js(&one).contains("generated_at"),
        "a bundle must carry no wall-clock field"
    );
}

/// Coverage: on the golden cross corpus, EVERY surviving PROVEN DISJOINT pair
/// must leave a bundle. A proven tier without a receipt is exactly what this
/// work exists to eliminate, so the assertion is 100%, not "most".
#[test]
fn every_proven_disjoint_pair_has_a_bundle() {
    for dir in [
        "examples/golden/cross/refine-disjoint",
        "examples/golden/cross/xeq-equivalent",
    ] {
        let report = cross_combine_files(dir);
        if report.solver == "none" {
            eprintln!("no solver available; skipping coverage test");
            return;
        }
        let proven: Vec<&adl_analysis::report::PairReport> = report
            .pairwise
            .iter()
            .filter(|p| p.kind == VerdictKind::ProvenDisjoint)
            .collect();
        assert!(!proven.is_empty(), "{dir}: no PROVEN DISJOINT pair to cover");
        for p in &proven {
            assert_eq!(p.certified, Some(true), "{dir}: {} vs {} uncertified", p.a, p.b);
            let bundle = report
                .combine_bundles
                .iter()
                .find(|b| b.region_a == p.a && b.region_b == p.b)
                .unwrap_or_else(|| panic!("{dir}: no bundle for {} vs {}", p.a, p.b));
            assert!(bundle.replay(), "{dir}: bundle for {} vs {} does not replay", p.a, p.b);
        }
        assert_eq!(
            report.combine_bundles.len(),
            proven.len(),
            "{dir}: bundle count must match the proven-disjoint count"
        );
        assert!(
            report.internal_diagnostics.is_empty(),
            "{dir}: internal diagnostics filed: {:#?}",
            report.internal_diagnostics
        );
    }
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
    let report = analyze_hir(
        &mut merged,
        "",
        &ext,
        &AnalysisOptions {
            reconcile: true,
            ..AnalysisOptions::default()
        },
    );
    assert!(report.combine_bundles.is_empty(), "default runs must not pay for bundling");
}
