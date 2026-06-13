//! End-to-end tests for `smash2 ingest` and `run --profile`
//! (SPEC_EVENT_PIPELINE §1): the Delphes profile over the committed
//! fixtures, plus the env-gated independent uproot oracle and the
//! env-gated full-sample e2e check.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_smash2")
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../adl-ingest/fixtures")
        .join(name)
        .canonicalize()
        .unwrap_or_else(|e| panic!("resolve fixture {name}: {e}"))
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

/// Unique temp dir per test (name + pid) so parallel tests don't collide.
fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("smash2_ingest_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn path_str(p: &Path) -> &str {
    p.to_str().expect("utf8 path")
}

#[test]
fn ingest_writes_the_golden_jsonl_and_diagnoses_unmappable_content() {
    let dir = temp_dir("golden");
    let out_file = dir.join("events.jsonl");
    let out = run(&[
        "ingest",
        path_str(&fixture("delphes_mini.root")),
        "--profile",
        "delphes",
        "-o",
        path_str(&out_file),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(stdout(&out), "", "machine-clean stdout");
    let written = std::fs::read_to_string(&out_file).expect("output jsonl");
    let golden = std::fs::read_to_string(fixture("delphes_mini.expected.jsonl")).expect("golden");
    assert_eq!(written, golden, "ingest output != frozen golden");
    let err = stderr(&out);
    assert!(
        err.contains("13 LHE weights present (`Weight.Weight`), not mapped in v1"),
        "{err}"
    );
    assert!(
        err.contains("collection `Jet`: 1 unmapped leaf dropped"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ingest_is_byte_deterministic_across_runs() {
    let dir = temp_dir("determinism");
    let (a, b) = (dir.join("a.jsonl"), dir.join("b.jsonl"));
    let root = fixture("delphes_synth.root");
    for f in [&a, &b] {
        let out = run(&[
            "ingest",
            path_str(&root),
            "--profile",
            "delphes",
            "-o",
            path_str(f),
        ]);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
    }
    assert_eq!(
        std::fs::read(&a).expect("a"),
        std::fs::read(&b).expect("b"),
        "ingest output is not byte-deterministic"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn verbose_surfaces_the_profile_decide_choices() {
    let dir = temp_dir("verbose");
    let out_file = dir.join("events.jsonl");
    let out = run(&[
        "--verbose",
        "ingest",
        path_str(&fixture("delphes_mini.root")),
        "--profile",
        "delphes",
        "-o",
        path_str(&out_file),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let err = stderr(&out);
    for needle in [
        "profile delphes/1:",
        "btag_bit = 0",
        "tautag_bit = 0",
        "lepton_mass = pdg (Electron 0.000511, Muon 0.105658)",
        "fatjet_name = fatjet",
        "weight_branch = Event.Weight",
        "Jet: unmapped leaves: Jet.T",
    ] {
        assert!(err.contains(needle), "missing `{needle}` in:\n{err}");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unknown_profile_and_missing_action_are_usage_errors() {
    let out = run(&[
        "ingest",
        path_str(&fixture("delphes_mini.root")),
        "--profile",
        "atlas",
        "-o",
        "/tmp/never_written.jsonl",
    ]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("unknown profile `atlas` (known: delphes)"));

    let out = run(&[
        "ingest",
        path_str(&fixture("delphes_mini.root")),
        "--profile",
        "delphes",
    ]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("nothing to do"), "{}", stderr(&out));

    // `-o` without an input file is a usage error too.
    let out = run(&["ingest", "--profile", "delphes", "-o", "/tmp/x.jsonl"]);
    assert_eq!(code(&out), 2);
    assert!(
        stderr(&out).contains("needs a ROOT input file"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn invariant_violations_are_refusals_with_exit_one() {
    let dir = temp_dir("refusals");
    let out_file = dir.join("events.jsonl");
    let out = run(&[
        "ingest",
        path_str(&fixture("delphes_badorder.root")),
        "--profile",
        "delphes",
        "-o",
        path_str(&out_file),
    ]);
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("collection `Jet` is not pT-descending at entry 1, index 1"),
        "{}",
        stderr(&out)
    );
    assert!(!out_file.exists(), "no output on refusal");

    let out = run(&[
        "ingest",
        path_str(&fixture("delphes_nan.root")),
        "--profile",
        "delphes",
        "-o",
        path_str(&out_file),
    ]);
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("`Jet.Eta`: non-finite value at entry 0"),
        "{}",
        stderr(&out)
    );
    assert!(!out_file.exists(), "no output on refusal");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn emit_script_writes_the_oracle_renderer() {
    let dir = temp_dir("script");
    let out = run(&[
        "ingest",
        "--profile",
        "delphes",
        "--emit-script",
        path_str(&dir),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let script = std::fs::read_to_string(dir.join("to_jsonl.py")).expect("script");
    assert!(script.starts_with("#!/usr/bin/env python3"));
    assert!(script.contains("profile delphes/1"));
    assert!(script.contains("def jnum(x):"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_profile_matches_run_on_materialized_jsonl() {
    let dir = temp_dir("runparity");
    let jsonl = dir.join("events.jsonl");
    let root = fixture("delphes_mini.root");
    let adl = corpus("tutorials/ex02_histograms.adl");
    let out = run(&[
        "ingest",
        path_str(&root),
        "--profile",
        "delphes",
        "-o",
        path_str(&jsonl),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));

    let native = run(&[
        "run",
        path_str(&adl),
        path_str(&root),
        "--profile",
        "delphes",
    ]);
    let via_jsonl = run(&["run", path_str(&adl), path_str(&jsonl)]);
    assert_eq!(code(&native), 0, "{}", stderr(&native));
    assert_eq!(code(&via_jsonl), 0, "{}", stderr(&via_jsonl));
    assert_eq!(
        stdout(&native),
        stdout(&via_jsonl),
        "native ingest and materialized JSONL must evaluate identically"
    );
    assert!(
        stderr(&native).contains("LHE weights present"),
        "ingest diagnostics surface on run --profile too: {}",
        stderr(&native)
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn run_with_unknown_profile_is_a_usage_error() {
    let adl = corpus("tutorials/ex02_histograms.adl");
    let out = run(&[
        "run",
        path_str(&adl),
        path_str(&fixture("delphes_mini.root")),
        "--profile",
        "cms",
    ]);
    assert_eq!(code(&out), 2);
    assert!(stderr(&out).contains("unknown profile `cms`"));
}

#[test]
fn run_profile_refuses_unordered_input_with_exit_one() {
    let adl = corpus("tutorials/ex02_histograms.adl");
    let out = run(&[
        "run",
        path_str(&adl),
        path_str(&fixture("delphes_badorder.root")),
        "--profile",
        "delphes",
    ]);
    assert_eq!(code(&out), 1);
    assert!(
        stderr(&out).contains("not pT-descending"),
        "{}",
        stderr(&out)
    );
}

fn which(cmd: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|p| p.join(cmd))
            .find(|p| p.is_file())
    })
}

/// A python3 with uproot importable, or `None` (caller skips loudly).
fn uproot_python() -> Option<PathBuf> {
    let py = which("python3")?;
    let probe = Command::new(&py)
        .args(["-c", "import uproot"])
        .output()
        .expect("probe uproot");
    probe.status.success().then_some(py)
}

/// Env-gated independent oracle (SPEC_EVENT_PIPELINE §1.1(b)): the
/// generated uproot script must reproduce the native JSONL byte for byte
/// on both committed fixtures.
#[test]
fn native_jsonl_matches_the_uproot_script_byte_for_byte() {
    if std::env::var_os("SMASH2_RUN_UPROOT_ORACLE").is_none() {
        eprintln!("skipping ingest uproot oracle (set SMASH2_RUN_UPROOT_ORACLE=1 to enable)");
        return;
    }
    let Some(py) = uproot_python() else {
        eprintln!("skipping: python3 with uproot not available");
        return;
    };
    let dir = temp_dir("oracle");
    let out = run(&[
        "ingest",
        "--profile",
        "delphes",
        "--emit-script",
        path_str(&dir),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let script = dir.join("to_jsonl.py");

    for name in ["delphes_mini.root", "delphes_synth.root"] {
        let root = fixture(name);
        let native = dir.join(format!("{name}.native.jsonl"));
        let scripted = dir.join(format!("{name}.script.jsonl"));
        let out = run(&[
            "ingest",
            path_str(&root),
            "--profile",
            "delphes",
            "-o",
            path_str(&native),
        ]);
        assert_eq!(code(&out), 0, "{}", stderr(&out));
        let sout = Command::new(&py)
            .args([path_str(&script), path_str(&root), path_str(&scripted)])
            .output()
            .expect("run to_jsonl.py");
        assert!(
            sout.status.success(),
            "{}",
            String::from_utf8_lossy(&sout.stderr)
        );
        assert_eq!(
            std::fs::read(&native).expect("native"),
            std::fs::read(&scripted).expect("scripted"),
            "{name}: native vs uproot script bytes differ"
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// Env-gated cross-language number-text battery: the script's `jnum`
/// must agree with serde_json/ryu on adversarial values (the fixtures
/// only exercise the easy GeV-scale region).
#[test]
fn script_jnum_matches_serde_json_on_edge_values() {
    if std::env::var_os("SMASH2_RUN_UPROOT_ORACLE").is_none() {
        eprintln!("skipping jnum battery (set SMASH2_RUN_UPROOT_ORACLE=1 to enable)");
        return;
    }
    let Some(py) = uproot_python() else {
        eprintln!("skipping: python3 with uproot not available");
        return;
    };
    let dir = temp_dir("jnum");
    let out = run(&[
        "ingest",
        "--profile",
        "delphes",
        "--emit-script",
        path_str(&dir),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));

    let values: Vec<f64> = vec![
        0.0,
        -0.0,
        1.0,
        -1.5,
        0.5,
        719.5091552734375,
        -2.0999999046325684,
        123456.78125,
        0.000511,
        0.105658,
        1e-4,
        1.2345e-5,
        1e-5,
        1e-6,
        2.5e-6,
        1.5e-7,
        5e-324,
        f64::from(f32::MIN_POSITIVE),
        f64::from(1.0e-45_f32),
        1e15,
        9999999999999998.0,
        1e16,
        1.5e16,
        1e21,
        1.7976931348623157e308,
        f64::from(f32::MAX),
    ];
    let expected: Vec<String> = values
        .iter()
        .map(|v| serde_json::to_string(v).expect("finite"))
        .collect();
    let py_prog = format!(
        "import sys\n\
         sys.path.insert(0, {dir:?})\n\
         from to_jsonl import jnum\n\
         vals = [{vals}]\n\
         exp = [{exp}]\n\
         for v, e in zip(vals, exp):\n\
         \x20   got = jnum(v)\n\
         \x20   assert got == e, f'jnum({{v!r}}) = {{got}} != {{e}}'\n\
         print('JNUM-OK', len(vals))\n",
        dir = dir.to_str().expect("utf8"),
        vals = expected
            .iter()
            .map(|e| format!("float({e:?})"))
            .collect::<Vec<_>>()
            .join(", "),
        exp = expected
            .iter()
            .map(|e| format!("{e:?}"))
            .collect::<Vec<_>>()
            .join(", "),
    );
    let sout = Command::new(&py)
        .args(["-c", &py_prog])
        .output()
        .expect("run jnum battery");
    assert!(
        sout.status.success() && String::from_utf8_lossy(&sout.stdout).contains("JNUM-OK"),
        "stdout: {} stderr: {}",
        String::from_utf8_lossy(&sout.stdout),
        String::from_utf8_lossy(&sout.stderr)
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Env-gated full-sample e2e (SPEC_EVENT_PIPELINE §7 item 1, ingestion
/// fidelity): on the pinned 20k-event T2tt Delphes sample, native JSONL ==
/// script JSONL byte-identical, with the §1.1 probe values pinned for the
/// first event. Needs `SMASH2_RUN_DELPHES_E2E=1` and the sample (fetch via
/// `scripts/fetch_delphes_sample.sh`; path override:
/// `SMASH2_DELPHES_SAMPLE`).
#[test]
fn delphes_sample_ingestion_fidelity_end_to_end() {
    if std::env::var_os("SMASH2_RUN_DELPHES_E2E").is_none() {
        eprintln!("skipping Delphes e2e (set SMASH2_RUN_DELPHES_E2E=1 to enable)");
        return;
    }
    const SAMPLE_SHA256: &str = "04fae8b1d94809f799741af8351f9448b84370122b780ccf03df3b74531b89fc";
    let sample = std::env::var_os("SMASH2_DELPHES_SAMPLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join(".cache/smash2/delphes_T2tt_700_50.root")
        });
    assert!(
        sample.is_file(),
        "sample not found at {} (run scripts/fetch_delphes_sample.sh or set SMASH2_DELPHES_SAMPLE)",
        sample.display()
    );
    // Pin the input identity before trusting any numbers from it.
    let sha = Command::new("sha256sum")
        .arg(&sample)
        .output()
        .expect("sha256sum");
    let sha_text = String::from_utf8_lossy(&sha.stdout);
    assert!(
        sha_text.starts_with(SAMPLE_SHA256),
        "sample hash mismatch: {sha_text}"
    );
    let Some(py) = uproot_python() else {
        panic!("Delphes e2e requires python3 with uproot for the independent oracle");
    };

    let dir = temp_dir("e2e");
    let native = dir.join("native.jsonl");
    let out = run(&[
        "ingest",
        path_str(&sample),
        "--profile",
        "delphes",
        "-o",
        path_str(&native),
        "--emit-script",
        path_str(&dir),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("20000 LHE weights present"),
        "expected the LHE diagnostic on the full sample: {err}"
    );

    let scripted = dir.join("script.jsonl");
    let sout = Command::new(&py)
        .args([
            path_str(&dir.join("to_jsonl.py")),
            path_str(&sample),
            path_str(&scripted),
        ])
        .output()
        .expect("run to_jsonl.py");
    assert!(
        sout.status.success(),
        "{}",
        String::from_utf8_lossy(&sout.stderr)
    );

    let native_bytes = std::fs::read(&native).expect("native");
    let script_bytes = std::fs::read(&scripted).expect("scripted");
    assert_eq!(
        native_bytes, script_bytes,
        "native vs uproot script bytes differ on the full sample"
    );

    let text = String::from_utf8(native_bytes).expect("utf8");
    assert_eq!(text.lines().count(), 20000);
    // SPEC §1.1 probe values, entry 0.
    let first = text.lines().next().expect("first line");
    assert!(
        first.starts_with(r#"{"Jet":[{"pt":719.5091552734375,"#),
        "{first}"
    );
    assert!(
        first.contains(r#""MET":{"pt":653.098876953125,"#),
        "{first}"
    );
    assert!(first.ends_with(r#""weight":1.0}"#), "{first}");
    let _ = std::fs::remove_dir_all(dir);
}
