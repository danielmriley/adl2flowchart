//! uproot oracle tests (SPEC_ROOT_WRITER.md §5.1–§5.2).
//!
//! Requires a Python with uproot 5.x + numpy. Probe order:
//! 1. `$ROOTFILE_PYTHON`
//! 2. `<workspace>/.venv-uproot/bin/python` (created per BUILD_NOTES.md)
//! 3. `python3` on PATH
//!
//! Without a usable interpreter the tests SKIP (loudly); set
//! `ROOTFILE_REQUIRE_UPROOT=1` to turn the skip into a failure (CI).
//!
//! Intentional byte-level divergences from an uproot-written file (all
//! documented in BUILD_NOTES.md): no dead initial StreamerInfo allocation
//! (so nfree=1, not 2), exactly-sized keys list (no 256-byte padding), and
//! record order name/histos/StreamerInfo/keys/free. The TH1D record payload
//! and the StreamerInfo data blob are byte-identical, which is what these
//! tests pin.

use std::path::{Path, PathBuf};
use std::process::Command;

use rootfile::{FlowBin, H1Spec, RootFile};

fn tools() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tools")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

/// Find a Python that can `import uproot`, or None.
fn uproot_python() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(p) = std::env::var("ROOTFILE_PYTHON") {
        candidates.push(PathBuf::from(p));
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.venv-uproot/bin/python"));
    candidates.push(PathBuf::from("python3"));
    candidates.into_iter().find(|py| {
        Command::new(py)
            .args(["-c", "import uproot, numpy"])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// None => skip (or panic under ROOTFILE_REQUIRE_UPROOT=1).
fn python_or_skip(test: &str) -> Option<PathBuf> {
    match uproot_python() {
        Some(py) => Some(py),
        None => {
            if std::env::var("ROOTFILE_REQUIRE_UPROOT").as_deref() == Ok("1") {
                panic!("{test}: ROOTFILE_REQUIRE_UPROOT=1 but no Python with uproot found");
            }
            eprintln!(
                "SKIPPED {test}: no Python with uproot (set ROOTFILE_PYTHON or create \
                 .venv-uproot per BUILD_NOTES.md; ROOTFILE_REQUIRE_UPROOT=1 makes this fatal)"
            );
            None
        }
    }
}

fn run(py: &Path, script: &str, args: &[&str]) -> std::process::Output {
    let out = Command::new(py)
        .arg(tools().join(script))
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("running {script}: {e}"));
    assert!(
        out.status.success(),
        "{script} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

fn tmpdir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rootfile_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Pinned spec; keep in sync with tools/make_reference.py.
fn reference_spec<'a>() -> H1Spec<'a> {
    H1Spec {
        title: "MET [GeV]",
        nbins: 4,
        lo: 0.0,
        hi: 100.0,
        sumw: &[2.0, 0.0, 3.25, 4.0],
        sumw2: &[4.0, 0.0, 5.0625, 8.0],
        under: FlowBin { w: 1.5, w2: 2.25 },
        over: FlowBin { w: 0.5, w2: 0.25 },
        entries: 11.0,
        tsumw: 9.25,
        tsumw2: 17.0625,
        tsumwx: 300.5,
        tsumwx2: 20000.25,
    }
}

/// (a) + (c): regenerate the uproot reference file, re-extract the
/// StreamerInfo blob and TH1D payload, and assert both vendored fixtures
/// are still byte-identical. Combined with the offline unit test
/// `payload_matches_uproot_reference_bytes` (our serializer == payload
/// fixture), this is the full byte-diff chain ours == uproot.
#[test]
fn vendored_fixtures_match_freshly_generated_uproot_reference() {
    let Some(py) = python_or_skip("vendored_fixtures_match_freshly_generated_uproot_reference")
    else {
        return;
    };
    let dir = tmpdir("ref");
    let reference = dir.join("reference.root");
    run(&py, "make_reference.py", &[reference.to_str().unwrap()]);
    run(
        &py,
        "extract_streamerinfo.py",
        &[reference.to_str().unwrap(), dir.to_str().unwrap()],
    );

    let fresh_blob = std::fs::read(dir.join("streamerinfo_th1d.bin")).unwrap();
    let vendored_blob = std::fs::read(fixtures().join("streamerinfo_th1d.bin")).unwrap();
    assert_eq!(
        fresh_blob, vendored_blob,
        "vendored StreamerInfo blob no longer matches uproot output \
         (uproot upgrade? regenerate fixtures per fixtures/PROVENANCE.md)"
    );

    let fresh_payload = std::fs::read(dir.join("reference_th1d_payload.bin")).unwrap();
    let vendored_payload = std::fs::read(fixtures().join("reference_th1d_payload.bin")).unwrap();
    assert_eq!(
        fresh_payload, vendored_payload,
        "TH1D payload fixture drifted"
    );

    std::fs::remove_dir_all(&dir).unwrap();
}

/// (b): write a file with this crate and read it back with uproot,
/// asserting contents, errors, edges, entries and the full stats array.
#[test]
fn uproot_reads_back_our_file_exactly() {
    let Some(py) = python_or_skip("uproot_reads_back_our_file_exactly") else {
        return;
    };
    let dir = tmpdir("ours");
    let ours = dir.join("ours.root");
    RootFile::create()
        .add_th1d("h_met", &reference_spec())
        .unwrap()
        .finish(&ours)
        .unwrap();
    let out = run(&py, "check_with_uproot.py", &[ours.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("OK"));
    std::fs::remove_dir_all(&dir).unwrap();
}

/// Multi-histogram file: uproot must list every key (cycle 1) and read each.
#[test]
fn uproot_lists_and_reads_multi_histo_file() {
    let Some(py) = python_or_skip("uproot_lists_and_reads_multi_histo_file") else {
        return;
    };
    let dir = tmpdir("multi");
    let ours = dir.join("multi.root");
    let mut rf = RootFile::create();
    for name in ["SR1_h_met", "SR2_h_met", "baseline_h_njets"] {
        rf = rf.add_th1d(name, &reference_spec()).unwrap();
    }
    rf.finish(&ours).unwrap();
    let script = r#"
import sys, uproot
with uproot.open(sys.argv[1]) as f:
    names = f.keys()
    assert names == ["SR1_h_met;1", "SR2_h_met;1", "baseline_h_njets;1"], names
    for n in names:
        h = f[n]
        assert h.member("fEntries") == 11.0, n
        assert list(h.values(flow=True)) == [1.5, 2.0, 0.0, 3.25, 4.0, 0.5], n
print("OK")
"#;
    let out = Command::new(&py)
        .args(["-c", script, ours.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "uproot multi-histo readback failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
