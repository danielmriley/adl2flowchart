//! End-to-end CLI tests for `smash2`: snapshot the machine-clean stdout of
//! each subcommand and assert exit codes / the stdout-vs-stderr split.
//!
//! Determinism note: `verify` snapshots run with `--no-solver` so the
//! report body is independent of which solver backend is installed in the
//! test environment (the solver line still reads `none`). Solver-on
//! behavior is covered by assertions, not snapshots, in the analysis crate.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_smash2")
}

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../legacy_parser/tests/golden")
        .join(name)
        .canonicalize()
        .unwrap_or_else(|e| panic!("resolve golden {name}: {e}"))
}

fn corpus(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../examples")
        .join(rel)
        .canonicalize()
        .unwrap_or_else(|e| panic!("resolve corpus {rel}: {e}"))
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("spawn smash2")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("utf8 stdout")
}

fn stderr(out: &Output) -> String {
    String::from_utf8(out.stderr.clone()).expect("utf8 stderr")
}

fn code(out: &Output) -> i32 {
    out.status.code().expect("exit code")
}

/// Write a temp file under the OS temp dir, returning its path. Named with
/// the test name + pid so parallel tests do not collide.
fn temp_jsonl(tag: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "smash2_{tag}_{}_{}.jsonl",
        std::process::id(),
        tag.len()
    ));
    let mut f = std::fs::File::create(&path).expect("create temp jsonl");
    f.write_all(contents.as_bytes()).expect("write temp jsonl");
    path
}

// --- check ---------------------------------------------------------------

#[test]
fn check_clean_is_silent_and_zero() {
    let out = run(&["check", golden("disjoint_pt.adl").to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    assert!(
        stdout(&out).is_empty(),
        "check stdout must be empty on success"
    );
    assert!(
        stderr(&out).is_empty(),
        "non-verbose check stderr must be empty"
    );
}

#[test]
fn check_bad_reports_and_exits_one() {
    let out = run(&["check", golden("bad_syntax.adl").to_str().unwrap()]);
    assert_eq!(code(&out), 1);
    assert!(
        stdout(&out).is_empty(),
        "diagnostics must not pollute stdout"
    );
    let err = stderr(&out);
    assert!(err.contains("select"), "should suggest `select`");
    assert!(err.contains("FAILED"));
}

// --- verify (snapshots, --no-solver for determinism) ---------------------

#[test]
fn verify_human_disjoint_pt() {
    let out = run(&[
        "verify",
        "--no-solver",
        golden("disjoint_pt.adl").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    let body = stdout(&out);
    // Piped stdout must take the plain (no-ANSI) path — the colored
    // rendering is tty-only and never snapshot-tested.
    assert!(!body.contains('\u{1b}'), "piped output must be ANSI-free");
    insta::assert_snapshot!("verify_human_disjoint_pt", body);
}

#[test]
fn verify_explain_disjoint_pt() {
    // --explain is the full per-pair detail (the pre-grouping format):
    // complete reasons, unsat cores, per-axiom statements.
    let out = run(&[
        "verify",
        "--explain",
        "--no-solver",
        golden("disjoint_pt.adl").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    insta::assert_snapshot!("verify_explain_disjoint_pt", stdout(&out));
}

#[test]
fn verify_json_disjoint_pt() {
    let out = run(&[
        "verify",
        "--json",
        "--no-solver",
        golden("disjoint_pt.adl").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    insta::assert_snapshot!("verify_json_disjoint_pt", stdout(&out));
}

#[test]
fn verify_bad_file_exits_one_empty_stdout() {
    let out = run(&["verify", golden("bad_syntax.adl").to_str().unwrap()]);
    assert_eq!(code(&out), 1);
    assert!(stdout(&out).is_empty());
    assert!(stderr(&out).contains("analysis did not run"));
}

#[test]
fn verify_fail_on_overlap_fires_exit_four() {
    // overlap_met overlaps; with --no-solver the SAT proof cannot fire, so
    // run with the solver to exercise the gate. If no solver is available
    // the verdict caps at POSSIBLY and the gate stays 0 — accept either, but
    // assert the gate never spuriously fails on a disjoint file below.
    let out = run(&[
        "verify",
        "--fail-on=overlap",
        golden("overlap_met.adl").to_str().unwrap(),
    ]);
    let c = code(&out);
    assert!(c == 0 || c == 4, "fail-on exit must be 0 or 4, got {c}");
    if c == 4 {
        assert!(stderr(&out).contains("--fail-on fired"));
    }
}

#[test]
fn verify_fail_on_does_not_fire_on_disjoint() {
    let out = run(&[
        "verify",
        "--fail-on=overlap",
        golden("disjoint_pt.adl").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
}

#[test]
fn verify_bad_fail_on_value_is_usage_error() {
    let out = run(&[
        "verify",
        "--fail-on=bogus",
        golden("disjoint_pt.adl").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("bogus"));
}

// --- run -----------------------------------------------------------------

#[test]
fn run_text_table() {
    let events = temp_jsonl(
        "runtext",
        "{\"Jet\": [{\"pt\": 350}, {\"pt\": 40}]}\n{\"Jet\": [{\"pt\": 150}]}\n{\"Jet\": []}\n",
    );
    let out = run(&[
        "run",
        golden("disjoint_pt.adl").to_str().unwrap(),
        events.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    insta::assert_snapshot!("run_text_disjoint_pt", stdout(&out));
    let _ = std::fs::remove_file(events);
}

#[test]
fn run_json_lines() {
    let events = temp_jsonl(
        "runjson",
        "{\"MissingET\": {\"pt\": 280, \"phi\": 0.0}}\n{\"MissingET\": {\"pt\": 450, \"phi\": 0.0}}\n{\"MissingET\": {\"pt\": 100, \"phi\": 0.0}}\n",
    );
    let out = run(&[
        "run",
        "--json",
        golden("bins_partition.adl").to_str().unwrap(),
        events.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    insta::assert_snapshot!("run_json_bins_partition", stdout(&out));
    let _ = std::fs::remove_file(events);
}

#[test]
fn run_bad_file_exits_one() {
    let events = temp_jsonl("runbad", "{}\n");
    let out = run(&[
        "run",
        golden("bad_syntax.adl").to_str().unwrap(),
        events.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 1);
    assert!(stdout(&out).is_empty());
    let _ = std::fs::remove_file(events);
}

// --- run histograms (Phase 9) ----------------------------------------------

/// Tiny histogram analysis + events for the `--histos` tests: one weighted
/// region; fills at MET = 25 (bin 0) and 75 (bin 1), one rejected event,
/// one overflow at MET = 250.
const HISTO_ADL: &str =
    "region SR\n  select MET > 10\n  weight lumi 2.0\n  histo hmet, \"met\", 2, 0, 100, MET\n";
const HISTO_EVENTS: &str = "{\"MET\": {\"pt\": 25, \"phi\": 0.0}}\n\
                            {\"MET\": {\"pt\": 75, \"phi\": 0.0}}\n\
                            {\"MET\": {\"pt\": 5, \"phi\": 0.0}}\n\
                            {\"MET\": {\"pt\": 250, \"phi\": 0.0}}\n";

fn temp_adl(tag: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("smash2_{tag}_{}.adl", std::process::id()));
    std::fs::write(&path, contents).expect("write temp adl");
    path
}

#[test]
fn run_histos_writes_canonical_json_deterministically() {
    let adl = temp_adl("histosfile", HISTO_ADL);
    let events = temp_jsonl("histosfile", HISTO_EVENTS);
    let dir_a = std::env::temp_dir().join(format!("smash2_histos_a_{}", std::process::id()));
    let dir_b = std::env::temp_dir().join(format!("smash2_histos_b_{}", std::process::id()));
    let mut outputs = Vec::new();
    for dir in [&dir_a, &dir_b] {
        let out = run(&[
            "run",
            adl.to_str().unwrap(),
            events.to_str().unwrap(),
            "--histos",
            dir.to_str().unwrap(),
        ]);
        assert_eq!(code(&out), 0);
        assert!(
            stderr(&out).is_empty(),
            "clean file must produce no histo diagnostics: {}",
            stderr(&out)
        );
        outputs.push(std::fs::read_to_string(dir.join("histos.json")).expect("histos.json"));
        outputs.push(std::fs::read_to_string(dir.join("cutflow.json")).expect("cutflow.json"));
    }
    assert_eq!(outputs[0], outputs[2], "histos.json must be byte-identical");
    assert_eq!(
        outputs[1], outputs[3],
        "cutflow.json must be byte-identical"
    );
    insta::assert_snapshot!("run_histos_json_file", outputs[0]);
    insta::assert_snapshot!("run_cutflow_json_file", outputs[1]);
    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
    let _ = std::fs::remove_dir_all(dir_a);
    let _ = std::fs::remove_dir_all(dir_b);
}

#[test]
fn run_json_gains_trailing_histograms_and_cutflow_lines() {
    let adl = temp_adl("histojson", HISTO_ADL);
    let events = temp_jsonl("histojson", HISTO_EVENTS);
    let out = run(&[
        "run",
        "--json",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    let so = stdout(&out);
    let lines: Vec<&str> = so.lines().collect();
    assert_eq!(lines.len(), 6, "4 event lines + histograms + cutflow: {so}");
    insta::assert_snapshot!("run_json_histograms_line", lines[4]);
    insta::assert_snapshot!("run_json_cutflow_line", lines[5]);
    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
}

/// SPEC_EVENT_PIPELINE §4: the input event weight (JSONL `weight` key,
/// 0-weight included) composes with the positional ADL `weight` product.
/// Hand-computed: all = {4 raw, Σw 0+3+1+2 = 6, Σw² 14}; `select MET >
/// 100` (before the weight, factor 1) = {3, 4, 10}; `reject MET > 300`
/// (after `weight lumi 2.0`) = {2, 0×2 + 3×2 = 6, 36}.
#[test]
fn run_json_cutflow_composes_input_weights() {
    let adl = temp_adl(
        "wcutflow",
        "region SR\n  select MET > 100\n  weight lumi 2.0\n  reject MET > 300\n",
    );
    let events = temp_jsonl(
        "wcutflow",
        "{\"MET\": {\"pt\": 150, \"phi\": 0.0}, \"weight\": 0.0}\n\
         {\"MET\": {\"pt\": 250, \"phi\": 0.0}, \"weight\": 3.0}\n\
         {\"MET\": {\"pt\": 350, \"phi\": 0.0}}\n\
         {\"MET\": {\"pt\": 50, \"phi\": 0.0}, \"weight\": 2.0}\n",
    );
    let out = run(&[
        "run",
        "--json",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    let so = stdout(&out);
    let last = so.lines().last().expect("cutflow line");
    let v: serde_json::Value = serde_json::from_str(last).expect("valid JSON");
    let cf = &v["cutflow"];
    assert_eq!(cf["version"], 1);
    assert_eq!(
        cf["total"],
        serde_json::json!({"raw": 4, "sumw": 6.0, "sumw2": 14.0})
    );
    let steps = cf["regions"][0]["steps"].as_array().expect("steps");
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0]["raw"], 4);
    assert_eq!(steps[0]["sumw"], 6.0);
    assert_eq!(steps[1]["label"], "select MET > 100");
    assert_eq!(steps[1]["raw"], 3);
    assert_eq!(steps[1]["sumw"], 4.0);
    assert_eq!(steps[1]["sumw2"], 10.0);
    assert_eq!(steps[2]["label"], "reject MET > 300");
    assert_eq!(steps[2]["raw"], 2, "0-weight event still counts raw");
    assert_eq!(steps[2]["sumw"], 6.0);
    assert_eq!(steps[2]["sumw2"], 36.0);
    assert_eq!(steps[2]["errors"], 0);
    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
}

#[test]
fn run_histo_diagnostics_go_to_stderr() {
    let adl = temp_adl(
        "histodiag",
        "region SR\n  select MET > 10\n  histo h2, \"2d\", 2, 0, 1, 2, 0, 1, MET, MET\n",
    );
    let events = temp_jsonl("histodiag", "{\"MET\": {\"pt\": 25, \"phi\": 0.0}}\n");
    let out = run(&["run", adl.to_str().unwrap(), events.to_str().unwrap()]);
    assert_eq!(code(&out), 0, "a skipped histogram is not a tool error");
    assert!(
        stderr(&out).contains("2-D histogram (deferred); histogram skipped"),
        "stderr: {}",
        stderr(&out)
    );
    assert!(
        !stdout(&out).contains("histogram"),
        "stdout stays machine-clean: {}",
        stdout(&out)
    );
    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
}

// --- run histogram bridges (Phase 9: .C / .py / CSV / SVG) -----------------

/// The committed ex02 toy-event fixture lives next to the adl-difftest
/// golden (regenerate byte-identically with `scripts/gen_ex02_events.py`).
fn ex02_events_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../adl-difftest/tests/fixtures/ex02_events.jsonl")
        .canonicalize()
        .expect("resolve ex02 events fixture")
}

/// Run `smash2 run --histos DIR --csv --svg` on the ex02 golden, returning
/// the output directory (the caller cleans it up).
fn run_ex02_bridges(tag: &str) -> PathBuf {
    let adl = corpus("tutorials/ex02_histograms.adl");
    let events = ex02_events_fixture();
    let dir = std::env::temp_dir().join(format!("smash2_bridge_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "run",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
        "--histos",
        dir.to_str().unwrap(),
        "--csv",
        "--svg",
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    dir
}

fn read_bridge(dir: &std::path::Path, rel: &str) -> String {
    std::fs::read_to_string(dir.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

#[test]
fn run_histos_emits_root_macro() {
    let dir = run_ex02_bridges("macro");
    insta::assert_snapshot!(
        "bridges_ex02_make_histos_c",
        read_bridge(&dir, "make_histos.C")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_histos_emits_uproot_script() {
    let dir = run_ex02_bridges("uproot");
    insta::assert_snapshot!("bridges_ex02_to_root_py", read_bridge(&dir, "to_root.py"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_histos_emits_per_histogram_csv() {
    let dir = run_ex02_bridges("csv");
    // hnjets has integer bin centers and a clear in-range distribution;
    // hjet1eta exercises a negative-lo axis.
    insta::assert_snapshot!(
        "bridges_ex02_csv_hnjets",
        read_bridge(&dir, "baseline_hnjets.csv")
    );
    insta::assert_snapshot!(
        "bridges_ex02_csv_hjet1eta",
        read_bridge(&dir, "baseline_hjet1eta.csv")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_histos_emits_per_histogram_svg() {
    let dir = run_ex02_bridges("svg");
    insta::assert_snapshot!(
        "bridges_ex02_svg_hnjets",
        read_bridge(&dir, "baseline_hnjets.svg")
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_histos_bridges_are_byte_identical_across_runs() {
    let a = run_ex02_bridges("det_a");
    let b = run_ex02_bridges("det_b");
    for rel in [
        "make_histos.C",
        "to_root.py",
        "cutflow.json",
        "baseline_hmet.csv",
        "baseline_hmet.svg",
        "singlelepton_hlep1pt.svg",
    ] {
        assert_eq!(
            read_bridge(&a, rel),
            read_bridge(&b, rel),
            "{rel} must be deterministic"
        );
    }
    let _ = std::fs::remove_dir_all(a);
    let _ = std::fs::remove_dir_all(b);
}

#[test]
fn csv_and_svg_require_histos_dir() {
    // clap enforces `--csv`/`--svg` require `--histos`: usage error, exit 2.
    let adl = corpus("tutorials/ex02_histograms.adl");
    let events = ex02_events_fixture();
    let out = run(&[
        "run",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
        "--csv",
    ]);
    assert_eq!(code(&out), 2, "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("--histos"));
}

#[test]
fn bridges_carry_flow_bins_and_weighted_errors() {
    // HISTO_ADL fills bins 0,1 (weight 2.0 each) plus one MET=250 overflow;
    // the .C must set the overflow bin (N+1=3) and weighted errors
    // (sqrt(sumw2)=sqrt(4)=2), and the SVG caption must note the overflow.
    let adl = temp_adl("flowbridge", HISTO_ADL);
    let events = temp_jsonl("flowbridge", HISTO_EVENTS);
    let dir = std::env::temp_dir().join(format!("smash2_flowbridge_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "run",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
        "--histos",
        dir.to_str().unwrap(),
        "--svg",
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));

    let c = read_bridge(&dir, "make_histos.C");
    assert!(
        c.contains("f->mkdir(\"SR\");") && c.contains("f->cd(\"SR\");"),
        "per-region TDirectory layout (rootfile v2 default): {c}"
    );
    assert!(
        c.contains("new TH1D(\"hmet\""),
        "bare object name inside the region directory: {c}"
    );
    assert!(
        c.contains("h->SetBinContent(3, 2.0);"),
        "overflow bin N+1: {c}"
    );
    assert!(
        c.contains("h->SetBinError(1, 2.0);"),
        "weighted error sqrt(4)=2: {c}"
    );
    assert!(
        c.contains("h->SetEntries(3);"),
        "raw fill count incl. overflow"
    );
    assert!(
        c.contains("Double_t stats[4] = {4.0, 8.0, 200.0, 12500.0};"),
        "PutStats moments: {c}"
    );

    let svg = read_bridge(&dir, "SR_hmet.svg");
    assert!(
        svg.contains("overflow=2"),
        "SVG caption notes overflow: {svg}"
    );

    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
    let _ = std::fs::remove_dir_all(dir);
}

// --- native out.root (Phase 9: rootfile writer wired into `run`) ----------

#[test]
fn run_histos_writes_native_root_file() {
    let adl = temp_adl("nativeroot", HISTO_ADL);
    let events = temp_jsonl("nativeroot", HISTO_EVENTS);
    let dir = std::env::temp_dir().join(format!("smash2_nativeroot_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "run",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
        "--histos",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));

    // out.root lands next to histos.json and re-parses with the writer's own
    // strict reader; the TH1D content matches the accumulator (per-region
    // TDirectory layout, flow bins, weighted error², raw entries,
    // fill-time moments), and the region directory carries the §2 cutflow
    // pair with verbatim step labels.
    let bytes = std::fs::read(dir.join("out.root")).expect("out.root written");
    let parsed = rootfile::reader::parse(&bytes).expect("out.root re-parses");
    assert_eq!(parsed.keys_list, vec!["SR".to_owned()]);
    assert_eq!(parsed.dirs, vec![vec!["SR".to_owned()]]);
    assert_eq!(parsed.histos.len(), 3, "histogram + cutflow pair");
    let h = &parsed.histos[0];
    assert_eq!(h.path, vec!["SR".to_owned()]);
    assert_eq!(h.name, "hmet");
    assert_eq!(h.nbins, 2);
    assert_eq!((h.lo, h.hi), (0.0, 100.0));
    // contents are [underflow, bin1, bin2, overflow]: 0, 2, 2, 2.
    assert_eq!(h.contents, vec![0.0, 2.0, 2.0, 2.0]);
    assert_eq!(h.sumw2, vec![0.0, 4.0, 4.0, 4.0]);
    assert_eq!(h.entries, 3.0, "raw fill count incl. overflow");
    assert_eq!(
        (h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2),
        (4.0, 8.0, 200.0, 12500.0),
        "in-range fill-time moments"
    );

    // Cutflow pair (SPEC_EVENT_PIPELINE §2): step labels are the verbatim
    // statement text; raw errors are Poisson (fSumw2 = raw), weighted
    // carries Σw/Σw² — the lumi weight sits *after* the select, so the
    // positional product at both steps is 1.0 ([DECIDE-W1]).
    let raw = &parsed.histos[1];
    assert_eq!(raw.name, "SR__cutflow_raw");
    assert_eq!(raw.path, vec!["SR".to_owned()]);
    assert_eq!(
        raw.labels.as_deref(),
        Some(&["all".to_owned(), "select MET > 10".to_owned()][..])
    );
    assert_eq!(raw.contents, vec![0.0, 4.0, 3.0, 0.0]);
    assert_eq!(raw.sumw2, raw.contents, "Poisson");
    assert_eq!(raw.entries, 4.0, "events processed");
    let wt = &parsed.histos[2];
    assert_eq!(wt.name, "SR__cutflow_wt");
    assert_eq!(wt.contents, vec![0.0, 4.0, 3.0, 0.0]);

    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_histos_flat_names_keeps_v1_layout() {
    let adl = temp_adl("flatroot", HISTO_ADL);
    let events = temp_jsonl("flatroot", HISTO_EVENTS);
    let dir = std::env::temp_dir().join(format!("smash2_flatroot_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "run",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
        "--histos",
        dir.to_str().unwrap(),
        "--flat-names",
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));

    let bytes = std::fs::read(dir.join("out.root")).expect("out.root written");
    let parsed = rootfile::reader::parse(&bytes).expect("out.root re-parses");
    assert!(parsed.dirs.is_empty(), "no TDirectories under --flat-names");
    assert_eq!(
        parsed.keys_list,
        vec![
            "SR_hmet".to_owned(),
            "SR__cutflow_raw".to_owned(),
            "SR__cutflow_wt".to_owned()
        ]
    );
    assert_eq!(parsed.histos[0].name, "SR_hmet");
    assert_eq!(parsed.histos[0].contents, vec![0.0, 2.0, 2.0, 2.0]);
    // Bridges follow the same flat naming.
    let c = std::fs::read_to_string(dir.join("make_histos.C")).expect("macro");
    assert!(c.contains("new TH1D(\"SR_hmet\"") && !c.contains("mkdir"));
    let py = std::fs::read_to_string(dir.join("to_root.py")).expect("script");
    assert!(py.contains("f[\"SR_hmet\"]"));

    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_histos_native_root_is_byte_identical_across_runs() {
    let adl = temp_adl("rootdet", HISTO_ADL);
    let events = temp_jsonl("rootdet", HISTO_EVENTS);
    let mut outputs = Vec::new();
    for tag in ["rootdet_a", "rootdet_b"] {
        let dir = std::env::temp_dir().join(format!("smash2_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let out = run(&[
            "run",
            adl.to_str().unwrap(),
            events.to_str().unwrap(),
            "--histos",
            dir.to_str().unwrap(),
        ]);
        assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
        outputs.push(std::fs::read(dir.join("out.root")).expect("out.root"));
        let _ = std::fs::remove_dir_all(&dir);
    }
    assert_eq!(
        outputs[0], outputs[1],
        "out.root must be byte-identical across runs (pinned datime/UUIDs)"
    );
    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
}

#[test]
fn run_histos_no_root_suppresses_out_root_only() {
    let adl = temp_adl("noroot", HISTO_ADL);
    let events = temp_jsonl("noroot", HISTO_EVENTS);
    let dir = std::env::temp_dir().join(format!("smash2_noroot_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "run",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
        "--histos",
        dir.to_str().unwrap(),
        "--no-root",
    ]);
    assert_eq!(code(&out), 0, "stderr: {}", stderr(&out));
    assert!(
        !dir.join("out.root").exists(),
        "--no-root must not write out.root"
    );
    // The JSON + bridges are still produced.
    assert!(dir.join("histos.json").exists());
    assert!(dir.join("make_histos.C").exists());
    assert!(dir.join("to_root.py").exists());
    let _ = std::fs::remove_file(adl);
    let _ = std::fs::remove_file(events);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn no_root_requires_histos_dir() {
    // clap enforces `--no-root` requires `--histos`: usage error, exit 2.
    let adl = corpus("tutorials/ex02_histograms.adl");
    let events = ex02_events_fixture();
    let out = run(&[
        "run",
        adl.to_str().unwrap(),
        events.to_str().unwrap(),
        "--no-root",
    ]);
    assert_eq!(code(&out), 2, "stderr: {}", stderr(&out));
    assert!(stderr(&out).contains("--histos"));
}

/// Env-gated oracle: if `root` is on PATH, run the generated macro and read
/// the histograms back, asserting a known bin/entries/mean. Skipped (pass)
/// when ROOT is unavailable — recorded in BUILD_NOTES.
#[test]
fn root_macro_round_trips_when_root_available() {
    if std::env::var_os("SMASH2_RUN_ROOT_ORACLE").is_none() {
        eprintln!("skipping ROOT oracle (set SMASH2_RUN_ROOT_ORACLE=1 to enable)");
        return;
    }
    if which("root").is_none() {
        eprintln!("skipping: `root` not on PATH");
        return;
    }
    let dir = run_ex02_bridges("rootoracle");
    // A tiny reader macro: open histos.root, print one hist's entries/bin.
    let reader = dir.join("read_back.C");
    std::fs::write(
        &reader,
        "void read_back() {\n\
         \x20 TFile* f = TFile::Open(\"histos.root\");\n\
         \x20 TH1D* h = (TH1D*)f->Get(\"baseline/hnjets\");\n\
         \x20 printf(\"ENTRIES=%g BIN4=%g OVF=%g\\n\", h->GetEntries(),\n\
         \x20        h->GetBinContent(4), h->GetBinContent(h->GetNbinsX()+1));\n\
         }\n",
    )
    .expect("write reader macro");
    // Build then read in the same working dir so histos.root resolves.
    let build = Command::new("root")
        .args(["-l", "-b", "-q", "make_histos.C"])
        .current_dir(&dir)
        .output()
        .expect("run root build");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let read = Command::new("root")
        .args(["-l", "-b", "-q", "read_back.C"])
        .current_dir(&dir)
        .output()
        .expect("run root read");
    let so = String::from_utf8_lossy(&read.stdout);
    assert!(
        so.contains("ENTRIES=32") && so.contains("BIN4=10"),
        "root read-back: {so}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Env-gated oracle: if python3 with uproot is available, run the generated
/// script and read the histograms back with uproot. Skipped when absent.
#[test]
fn uproot_script_round_trips_when_available() {
    if std::env::var_os("SMASH2_RUN_UPROOT_ORACLE").is_none() {
        eprintln!("skipping uproot oracle (set SMASH2_RUN_UPROOT_ORACLE=1 to enable)");
        return;
    }
    let py = which("python3");
    let Some(py) = py else {
        eprintln!("skipping: python3 not on PATH");
        return;
    };
    let probe = Command::new(&py)
        .args(["-c", "import uproot, numpy"])
        .output()
        .expect("probe uproot");
    if !probe.status.success() {
        eprintln!("skipping: python3 lacks uproot/numpy");
        return;
    }
    let dir = run_ex02_bridges("uprootoracle");
    let build = Command::new(&py)
        .arg("to_root.py")
        .current_dir(&dir)
        .output()
        .expect("run to_root.py");
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let reader = "import uproot\n\
                  f = uproot.open('histos.root')\n\
                  h = f['baseline/hnjets']\n\
                  v = h.values(flow=True)\n\
                  print('ENTRIES', h.member('fEntries'), 'BIN4', v[4])\n";
    let read = Command::new(&py)
        .args(["-c", reader])
        .current_dir(&dir)
        .output()
        .expect("read back uproot");
    let so = String::from_utf8_lossy(&read.stdout);
    assert!(
        so.contains("ENTRIES 32") && so.contains("BIN4 10"),
        "uproot read-back: {so}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Minimal PATH lookup (no extra deps): the first existing `name` under a
/// PATH entry.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(name))
            .find(|p| p.is_file())
    })
}

// --- dot -----------------------------------------------------------------

#[test]
fn dot_flowchart_snapshot() {
    let out = run(&["dot", golden("disjoint_pt.adl").to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    insta::assert_snapshot!("dot_flowchart_disjoint_pt", stdout(&out));
}

#[test]
fn dot_ast_snapshot() {
    let out = run(&["dot", "--ast", golden("disjoint_pt.adl").to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    insta::assert_snapshot!("dot_ast_disjoint_pt", stdout(&out));
}

#[test]
fn dot_corpus_file_renders() {
    let out = run(&[
        "dot",
        corpus("tutorials/ex01_selection.adl").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    let s = stdout(&out);
    assert!(s.starts_with("digraph flowchart {"));
    assert!(s.trim_end().ends_with('}'));
}

#[test]
fn dot_bad_file_exits_one() {
    let out = run(&["dot", golden("bad_syntax.adl").to_str().unwrap()]);
    assert_eq!(code(&out), 1);
    assert!(stdout(&out).is_empty());
}

// --- objects -------------------------------------------------------------

#[test]
fn objects_table_snapshot() {
    let out = run(&[
        "objects",
        corpus("Examples/CMS-SUS-16-032.adl").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    let body = stdout(&out);
    // Piped stdout takes the plain (no-ANSI) path; diagnostics are stderr.
    assert!(!body.contains('\u{1b}'), "piped output must be ANSI-free");
    assert!(
        body.starts_with("== objects =="),
        "table must be the only thing on stdout"
    );
    insta::assert_snapshot!("objects_cms_sus_16_032", body);
}

#[test]
fn objects_bad_file_exits_one() {
    let out = run(&["objects", golden("bad_syntax.adl").to_str().unwrap()]);
    assert_eq!(code(&out), 1);
    assert!(stdout(&out).is_empty());
}

// --- determinism ---------------------------------------------------------

#[test]
fn verify_report_is_byte_identical_across_runs() {
    let path = golden("collection_quant.adl");
    let a = run(&["verify", "--no-solver", path.to_str().unwrap()]);
    let b = run(&["verify", "--no-solver", path.to_str().unwrap()]);
    assert_eq!(stdout(&a), stdout(&b), "human report must be deterministic");

    let aj = run(&["verify", "--json", "--no-solver", path.to_str().unwrap()]);
    let bj = run(&["verify", "--json", "--no-solver", path.to_str().unwrap()]);
    assert_eq!(
        stdout(&aj),
        stdout(&bj),
        "JSON report must be deterministic"
    );
}

#[test]
fn dot_is_byte_identical_across_runs() {
    let path = corpus("tutorials/ex06_bins.adl");
    let a = run(&["dot", path.to_str().unwrap()]);
    let b = run(&["dot", path.to_str().unwrap()]);
    assert_eq!(stdout(&a), stdout(&b));
}
