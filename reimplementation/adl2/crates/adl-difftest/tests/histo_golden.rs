//! Phase-9 golden: ex02_histograms over the committed seeded toy events
//! (`tests/fixtures/ex02_events.jsonl`, regenerate byte-identically with
//! `scripts/gen_ex02_events.py`). Pins the canonical `histos.json` for the
//! corpus's canonical histogram file, the honest-skip diagnostics for its
//! deferred forms (2-D, variable-bin), and byte-determinism of reruns.

use adl_interp::{Event, HistoSet, Interp, read_jsonl};
use adl_sema::{ExtDecls, Hir, analyze_str};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repo root")
        .to_path_buf()
}

fn ex02_hir(ext: &ExtDecls) -> Hir {
    let path = repo_root().join("examples/tutorials/ex02_histograms.adl");
    let src = std::fs::read_to_string(&path).expect("read ex02_histograms.adl");
    let hir = analyze_str(&src, "ex02_histograms.adl", ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "ex02 must resolve cleanly: {:#?}",
        hir.diags
    );
    hir
}

fn fixture_events(ext: &ExtDecls) -> Vec<Event> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ex02_events.jsonl");
    let text = std::fs::read_to_string(&path).expect("read committed fixture");
    read_jsonl(&text, ext).expect("fixture events must load")
}

fn accumulate<'h>(hir: &'h Hir, ext: &'h ExtDecls, events: &[Event]) -> HistoSet<'h> {
    let interp = Interp::new(hir, ext);
    let mut set = HistoSet::new(hir);
    for ev in events {
        let results = interp.run_event(ev);
        set.fill_event(&interp, ev, &results);
    }
    set
}

#[test]
fn ex02_histos_json_golden() {
    let ext = ExtDecls::legacy();
    let hir = ex02_hir(&ext);
    let events = fixture_events(&ext);
    assert_eq!(events.len(), 200, "committed fixture is 200 events");
    let set = accumulate(&hir, &ext, &events);

    // The 1-D uniform histograms instantiate; deferred forms are skipped
    // with one diagnostic each, plus the repeated-histoList note.
    let names: Vec<(&str, &str)> = set
        .histos
        .iter()
        .map(|f| (f.name.as_str(), f.region.as_str()))
        .collect();
    assert_eq!(
        names,
        [
            ("hmet", "baseline"),
            ("hnjets", "baseline"),
            ("hjet1pt", "baseline"),
            ("hjet2pt", "baseline"),
            ("hjet3pt", "baseline"),
            ("hjet1eta", "baseline"),
            ("hjet2eta", "baseline"),
            ("hjet3eta", "baseline"),
            ("hlep1pt", "singlelepton"),
            ("hlep1eta", "singlelepton"),
        ]
    );
    let diags = set.diagnostics();
    insta::assert_snapshot!("ex02_histo_diags", diags.join("\n"));
    insta::assert_snapshot!("ex02_histos_json", set.to_json(true));
}

/// Reruns are byte-identical, the structural invariants hold, and
/// `entries` ties out against an independent region-membership count.
#[test]
fn ex02_histos_deterministic_and_consistent() {
    let ext = ExtDecls::legacy();
    let hir = ex02_hir(&ext);
    let events = fixture_events(&ext);
    let a = accumulate(&hir, &ext, &events);
    let b = accumulate(&hir, &ext, &events);
    assert_eq!(
        a.to_json(true),
        b.to_json(true),
        "rerun must be byte-identical"
    );
    assert_eq!(a.to_json(false), b.to_json(false));
    assert_eq!(a.diagnostics(), b.diagnostics());

    // Independent pass count: every baseline histogram's entries equals
    // the number of accepted events (ex02 has unit weights, and the
    // baseline fill expressions always have values once the region's
    // size cuts pass).
    let interp = Interp::new(&hir, &ext);
    let baseline_passes = events
        .iter()
        .filter(|ev| {
            interp
                .eval_region_by_name("baseline", ev)
                .expect("baseline evaluates")
        })
        .count() as u64;
    assert!(baseline_passes > 0, "fixture must populate baseline");
    for f in a.histos.iter().filter(|f| f.region == "baseline") {
        assert_eq!(f.hist.entries(), baseline_passes, "histo {}", f.name);
        let Some(h) = f.h1() else {
            continue; // 2-D / variable-bin accumulators have their own goldens
        };
        // Unit weights: Σw over all bins incl. flow equals entries.
        let total: f64 = h.sumw.iter().sum::<f64>() + h.underflow_w + h.overflow_w;
        #[allow(clippy::cast_precision_loss)]
        let entries_f = f.hist.entries() as f64;
        assert!((total - entries_f).abs() < 1e-9, "histo {}", f.name);
        // tsumw counts in-range fills only.
        let in_range: f64 = h.sumw.iter().sum();
        assert!((h.tsumw - in_range).abs() < 1e-9, "histo {}", f.name);
    }
}
