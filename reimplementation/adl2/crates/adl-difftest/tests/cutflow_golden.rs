//! Cutflow golden + independent recomputation (SPEC_EVENT_PIPELINE §2,
//! PLAN 10a exit): ex02_histograms over the committed 200-event toy
//! fixture pins the canonical `cutflow.json`; every step's raw count is
//! recomputed by a test-local prefix-conjunction walk (a separate code
//! path from the interpreter's traced membership walk); toy-event
//! batteries lock monotonicity, raw==sumw under unit weights, and
//! byte-determinism.

use adl_interp::{CutflowSet, Event, Interp, read_jsonl};
use adl_sema::{ExtDecls, Hir, HirRegionStmt, analyze_str};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repo root")
        .to_path_buf()
}

fn ex02_src() -> String {
    let path = repo_root().join("examples/tutorials/ex02_histograms.adl");
    std::fs::read_to_string(&path).expect("read ex02_histograms.adl")
}

fn analyze(src: &str, ext: &ExtDecls) -> Hir {
    let hir = analyze_str(src, "ex02_histograms.adl", ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "must resolve cleanly: {:#?}",
        hir.diags
    );
    hir
}

fn fixture_events(ext: &ExtDecls) -> Vec<Event> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ex02_events.jsonl");
    let text = std::fs::read_to_string(&path).expect("read committed fixture");
    read_jsonl(&text, ext).expect("fixture events must load")
}

fn accumulate(src: &str, hir: &Hir, ext: &ExtDecls, events: &[Event]) -> CutflowSet {
    let interp = Interp::new(hir, ext);
    let mut set = CutflowSet::new(hir, src);
    for ev in events {
        let (results, traces) = interp.run_event_traced(ev);
        set.record_event(ev, &results, &traces);
    }
    set
}

#[test]
fn ex02_cutflow_json_golden() {
    let ext = ExtDecls::legacy();
    let src = ex02_src();
    let hir = analyze(&src, &ext);
    let events = fixture_events(&ext);
    assert_eq!(events.len(), 200, "committed fixture is 200 events");
    let set = accumulate(&src, &hir, &ext, &events);

    assert!(set.diagnostics().is_empty(), "{:?}", set.diagnostics());
    let names: Vec<&str> = set.regions().iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        ["baseline", "singlelepton"],
        "histoList blocks contribute no cutflow"
    );
    insta::assert_snapshot!("ex02_cutflow_json", set.to_json(true));
    insta::assert_snapshot!("ex02_cutflow_table", set.text_table());
}

/// Independent oracle for the raw counts: re-evaluate each region as an
/// explicit prefix conjunction of its membership statements (via
/// `eval_bool` / `eval_region_by_name`, not the traced walk) and compare
/// survivor counts step by step.
#[test]
fn ex02_step_raw_counts_match_prefix_conjunction_recomputation() {
    let ext = ExtDecls::legacy();
    let src = ex02_src();
    let hir = analyze(&src, &ext);
    let events = fixture_events(&ext);
    let interp = Interp::new(&hir, &ext);
    let set = accumulate(&src, &hir, &ext, &events);

    for flow in set.regions() {
        let ridx = hir
            .regions
            .iter()
            .position(|r| hir.symbols.display(r.name) == flow.name)
            .expect("region exists");
        // The membership statements, in declaration order (histoList
        // references excluded — they are fill points, not steps).
        let stmts: Vec<&HirRegionStmt> = hir.regions[ridx]
            .stmts
            .iter()
            .filter(|s| match s {
                HirRegionStmt::Select(_) | HirRegionStmt::Reject(_) | HirRegionStmt::Trigger(_) => {
                    true
                }
                HirRegionStmt::Inherit { region, .. } => {
                    !hir.histolist_regions.get(*region).copied().unwrap_or(false)
                }
                _ => false,
            })
            .collect();
        assert_eq!(
            stmts.len() + 1,
            flow.steps.len(),
            "step structure of `{}`",
            flow.name
        );

        let mut expected = vec![0u64; stmts.len() + 1];
        for ev in &events {
            expected[0] += 1;
            for (k, stmt) in stmts.iter().enumerate() {
                let holds = match stmt {
                    HirRegionStmt::Select(n) | HirRegionStmt::Trigger(n) => {
                        interp.eval_bool(n, ev).expect("ex02 evaluates cleanly")
                    }
                    HirRegionStmt::Reject(n) => {
                        !interp.eval_bool(n, ev).expect("ex02 evaluates cleanly")
                    }
                    HirRegionStmt::Inherit { region, .. } => {
                        let parent = hir.symbols.display(hir.region_name_order[*region]);
                        interp
                            .eval_region_by_name(parent, ev)
                            .expect("parent evaluates cleanly")
                    }
                    _ => unreachable!("filtered above"),
                };
                if !holds {
                    break;
                }
                expected[k + 1] += 1;
            }
        }
        let actual: Vec<u64> = flow.steps.iter().map(|s| s.counts.raw).collect();
        assert_eq!(actual, expected, "region `{}`", flow.name);
        // Unit weights everywhere in ex02 + fixture: sumw == raw, errors 0.
        for s in &flow.steps {
            #[allow(clippy::cast_precision_loss)]
            let raw_f = s.counts.raw as f64;
            assert_eq!(s.counts.sumw, raw_f, "step `{}`", s.label);
            assert_eq!(s.counts.sumw2, raw_f, "step `{}`", s.label);
            assert_eq!(s.errors, 0, "step `{}`", s.label);
        }
        // Monotone: survivors never increase along the flow.
        assert!(
            actual.windows(2).all(|w| w[1] <= w[0]),
            "monotone raw counts in `{}`: {actual:?}",
            flow.name
        );
    }
}

#[test]
fn ex02_cutflow_is_byte_deterministic() {
    let ext = ExtDecls::legacy();
    let src = ex02_src();
    let hir = analyze(&src, &ext);
    let events = fixture_events(&ext);
    let a = accumulate(&src, &hir, &ext, &events);
    let b = accumulate(&src, &hir, &ext, &events);
    assert_eq!(a.to_json(true), b.to_json(true));
    assert_eq!(a.to_json(false), b.to_json(false));
    assert_eq!(a.text_table(), b.text_table());
}

/// [DECIDE-W1] corpus lint (SPEC_EVENT_PIPELINE §4): positional weight
/// composition equals the former whole-region product whenever all
/// `weight` statements precede all fill points. Verify that holds for
/// every corpus file (the `bad/` parse-error fixtures excluded), making
/// the switch a non-breaking refinement — any future file where it
/// differs raises the runtime diagnostic this test greps for.
#[test]
fn corpus_has_no_weight_after_fill_point() {
    let corpus = repo_root().join("examples");
    let ext = ExtDecls::legacy();
    let mut checked = 0usize;
    let mut stack = vec![corpus];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read corpus dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                if path.file_name().is_some_and(|n| n == "bad") {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "adl") {
                let src = std::fs::read_to_string(&path).expect("read corpus file");
                let name = path.file_name().unwrap().to_string_lossy().into_owned();
                let hir = analyze_str(&src, &name, &ext);
                let diags = adl_interp::HistoSet::new(&hir).diagnostics().join("\n");
                assert!(
                    !diags.contains("[DECIDE-W1]"),
                    "{name}: positional differs from whole-product:\n{diags}"
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked >= 30,
        "corpus sweep looks too small: {checked} files"
    );
}

/// Toy-event battery over a region exercising trigger + reject + select:
/// the final step's raw count must equal the membership count from the
/// (untraced) `run_event` path, for several seeds.
#[test]
fn toy_events_final_step_matches_membership() {
    let src = "region SR\n  trigger mu_trig\n  select size(Jet) >= 2\n  reject MET > 400\n";
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, "toy.adl", &ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "{:#?}",
        hir.diags
    );
    let interp = Interp::new(&hir, &ext);

    for seed in [1u64, 7, 42] {
        let events = adl_difftest::toy_events(seed, 300, &ext).expect("toy events load");
        let set = accumulate(src, &hir, &ext, &events);
        let flow = &set.regions()[0];
        let passes = events
            .iter()
            .filter(|ev| {
                interp
                    .run_event(ev)
                    .first()
                    .is_some_and(|r| r.pass == Ok(true))
            })
            .count() as u64;
        let last = flow.steps.last().expect("steps");
        assert_eq!(last.counts.raw, passes, "seed {seed}");
        assert_eq!(flow.steps[0].counts.raw, 300, "seed {seed}");
        let raws: Vec<u64> = flow.steps.iter().map(|s| s.counts.raw).collect();
        assert!(
            raws.windows(2).all(|w| w[1] <= w[0]),
            "seed {seed}: {raws:?}"
        );
    }
}
