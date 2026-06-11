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
    insta::assert_snapshot!("verify_human_disjoint_pt", stdout(&out));
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
