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

use rootfile::{CutflowStep, FlowBin, H1Spec, H1VarSpec, H2Spec, RootFile, pack_datime};

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

/// Run an inline uproot script against a file we wrote; assert it prints OK.
fn run_inline(py: &Path, script: &str, file: &Path, what: &str) {
    let out = Command::new(py)
        .args(["-c", script, file.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{what} failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("OK"),
        "{what}: no OK marker"
    );
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

// --- SPEC_EVENT_PIPELINE §3 additions (rootfile v2) ------------------------

/// Regenerate the v2 uproot reference and assert every vendored v2 fixture
/// (three object payloads, the TH1D+TH2D StreamerInfo record, and the three
/// single-class rawstreamer chunks) is still byte-identical. Combined with
/// the offline unit tests in src/th1d.rs, src/th2d.rs and src/file.rs this
/// closes the byte-diff chain ours == uproot for all v2 forms.
#[test]
fn vendored_v2_fixtures_match_freshly_generated_uproot_reference() {
    let Some(py) = python_or_skip("vendored_v2_fixtures_match_freshly_generated_uproot_reference")
    else {
        return;
    };
    let dir = tmpdir("ref_v2");
    let reference = dir.join("reference_v2.root");
    run(&py, "make_reference_v2.py", &[reference.to_str().unwrap()]);
    run(
        &py,
        "extract_reference_v2.py",
        &[reference.to_str().unwrap(), dir.to_str().unwrap()],
    );

    for fixture in [
        "reference_th1d_var_payload.bin",
        "reference_th1d_labeled_payload.bin",
        "reference_th2d_payload.bin",
        "streamerinfo_v2.bin",
        "rawstreamer_th2_v5.bin",
        "rawstreamer_th2d_v4.bin",
        "rawstreamer_tobjstring_v1.bin",
    ] {
        let fresh = std::fs::read(dir.join(fixture)).unwrap();
        let vendored = std::fs::read(fixtures().join(fixture)).unwrap();
        assert_eq!(
            fresh, vendored,
            "vendored {fixture} no longer matches uproot output \
             (uproot upgrade? regenerate fixtures per fixtures/PROVENANCE.md)"
        );
    }

    std::fs::remove_dir_all(&dir).unwrap();
}

/// Variable-bin TH1D (TAxis fXbins): uproot must read back the exact edges,
/// flow-inclusive values, variances and stats.
#[test]
fn uproot_reads_back_varbin_th1d() {
    let Some(py) = python_or_skip("uproot_reads_back_varbin_th1d") else {
        return;
    };
    let dir = tmpdir("varbin");
    let ours = dir.join("varbin.root");
    RootFile::create()
        .add_th1d_var_at(
            &[],
            "h_var",
            &H1VarSpec {
                title: "varbin",
                edges: &[0.0, 30.0, 70.0, 150.0, 400.0],
                sumw: &[2.0, 0.0, 3.25, 4.0],
                sumw2: &[4.0, 0.0, 5.0625, 8.0],
                under: FlowBin { w: 1.5, w2: 2.25 },
                over: FlowBin { w: 0.5, w2: 0.25 },
                entries: 11.0,
                tsumw: 9.25,
                tsumw2: 17.0625,
                tsumwx: 300.5,
                tsumwx2: 20000.25,
            },
        )
        .unwrap()
        .finish(&ours)
        .unwrap();
    let script = r#"
import sys
import numpy as np
import uproot
with uproot.open(sys.argv[1]) as f:
    h = f["h_var"]
    assert h.classname == "TH1D"
    assert np.array_equal(h.axis().edges(), [0.0, 30.0, 70.0, 150.0, 400.0]), h.axis().edges()
    assert np.array_equal(h.values(flow=True), [1.5, 2.0, 0.0, 3.25, 4.0, 0.5])
    assert np.array_equal(h.variances(flow=True), [2.25, 4.0, 0.0, 5.0625, 8.0, 0.25])
    m = h.all_members
    assert m["fXaxis"].member("fXmin") == 0.0 and m["fXaxis"].member("fXmax") == 400.0
    assert np.array_equal(np.asarray(m["fXaxis"].member("fXbins")), [0.0, 30.0, 70.0, 150.0, 400.0])
    assert (m["fEntries"], m["fTsumw"], m["fTsumw2"], m["fTsumwx"], m["fTsumwx2"]) == \
        (11.0, 9.25, 17.0625, 300.5, 20000.25)
    hh = h.to_hist()  # full axis/metadata interpretation round-trip
    assert np.array_equal(hh.values(flow=True), [1.5, 2.0, 0.0, 3.25, 4.0, 0.5])
print("OK")
"#;
    run_inline(&py, script, &ours, "uproot varbin readback");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// TH2D: uproot must read back the 2-D flow-inclusive values/variances and
/// all seven stats moments, and the file must carry TH2D streamers.
#[test]
fn uproot_reads_back_th2d() {
    let Some(py) = python_or_skip("uproot_reads_back_th2d") else {
        return;
    };
    let dir = tmpdir("th2d");
    let ours = dir.join("th2d.root");
    let contents: Vec<f64> = (0..20).map(|i| f64::from(i) * 0.5).collect();
    let sumw2: Vec<f64> = (0..20).map(|i| f64::from(i) * 0.25).collect();
    RootFile::create()
        .add_th2d_at(
            &[],
            "h2_met_njets",
            &H2Spec {
                title: "MET vs njets",
                nx: 3,
                xlo: 0.0,
                xhi: 300.0,
                ny: 2,
                ylo: 0.0,
                yhi: 4.0,
                sumw: &contents,
                sumw2: &sumw2,
                entries: 95.0,
                tsumw: 47.5,
                tsumw2: 23.75,
                tsumwx: 5125.0,
                tsumwx2: 880625.0,
                tsumwy: 95.5,
                tsumwy2: 250.25,
                tsumwxy: 10250.5,
            },
        )
        .unwrap()
        .finish(&ours)
        .unwrap();
    let script = r#"
import sys
import numpy as np
import uproot
with uproot.open(sys.argv[1]) as f:
    assert "TH2D" in f.file.streamers and "TH2" in f.file.streamers
    h = f["h2_met_njets"]
    assert h.classname == "TH2D"
    # Global-bin order is x fastest; uproot returns values indexed [x][y].
    want = np.arange(20, dtype=np.float64).reshape(4, 5) * 0.5  # [y][x]
    got = h.values(flow=True)
    assert got.shape == (5, 4), got.shape
    assert np.array_equal(got, want.T), got
    assert np.array_equal(h.variances(flow=True), (np.arange(20).reshape(4, 5) * 0.25).T)
    m = h.all_members
    assert m["fXaxis"].member("fNbins") == 3 and m["fYaxis"].member("fNbins") == 2
    assert m["fYaxis"].member("fXmax") == 4.0
    stats = (m["fEntries"], m["fTsumw"], m["fTsumw2"], m["fTsumwx"], m["fTsumwx2"],
             m["fTsumwy"], m["fTsumwy2"], m["fTsumwxy"])
    assert stats == (95.0, 47.5, 23.75, 5125.0, 880625.0, 95.5, 250.25, 10250.5), stats
    assert m["fScalefactor"] == 1.0
    hh = h.to_hist()
    assert np.array_equal(hh.values(flow=True), want.T)
print("OK")
"#;
    run_inline(&py, script, &ours, "uproot th2d readback");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// The SPEC_EVENT_PIPELINE §2 cutflow pair: labeled axes (THashList of
/// TObjStrings), raw/weighted contents and Poisson vs Sumw2 errors.
#[test]
fn uproot_reads_back_cutflow_pair_with_labels() {
    let Some(py) = python_or_skip("uproot_reads_back_cutflow_pair_with_labels") else {
        return;
    };
    let dir = tmpdir("cutflow");
    let ours = dir.join("cutflow.root");
    let steps = [
        CutflowStep {
            label: "all",
            raw: 20,
            sumw: 19.5,
            sumw2: 20.25,
        },
        CutflowStep {
            label: "select MET > 200",
            raw: 12,
            sumw: 11.25,
            sumw2: 11.0,
        },
        CutflowStep {
            label: "reject nbjets == 0",
            raw: 5,
            sumw: 4.75,
            sumw2: 4.5,
        },
    ];
    RootFile::create()
        .add_cutflow_at(&["SR1"], "SR1", &steps, 20)
        .unwrap()
        .finish(&ours)
        .unwrap();
    let script = r#"
import sys
import numpy as np
import uproot
with uproot.open(sys.argv[1]) as f:
    assert "TObjString" in f.file.streamers
    raw = f["SR1/SR1__cutflow_raw"]
    wt = f["SR1/SR1__cutflow_wt"]
    labels = ["all", "select MET > 200", "reject nbjets == 0"]
    assert [str(s) for s in raw.axis().labels()] == labels, raw.axis().labels()
    assert [str(s) for s in wt.axis().labels()] == labels
    assert np.array_equal(raw.values(), [20.0, 12.0, 5.0])
    assert np.array_equal(raw.variances(), [20.0, 12.0, 5.0])  # Poisson
    assert np.array_equal(wt.values(), [19.5, 11.25, 4.75])
    assert np.array_equal(wt.variances(), [20.25, 11.0, 4.5])  # Sumw2
    assert raw.member("fEntries") == 20.0 and wt.member("fEntries") == 20.0
    assert raw.member("fTsumw") == 37.0  # binned stats, never zeros
print("OK")
"#;
    run_inline(&py, script, &ours, "uproot cutflow readback");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// TNamed provenance carrier (§6): name + title round-trip at the file top
/// level, next to per-region directories.
#[test]
fn uproot_reads_back_tnamed_and_directories() {
    let Some(py) = python_or_skip("uproot_reads_back_tnamed_and_directories") else {
        return;
    };
    let dir = tmpdir("dirs");
    let ours = dir.join("dirs.root");
    RootFile::create()
        .add_tnamed_at(&[], "smash2_provenance", "{\"tool\":\"smash2 0.1.0\"}")
        .unwrap()
        .add_th1d_at(&["SR1"], "h_met", &reference_spec())
        .unwrap()
        .add_th1d_at(&["SR1", "sub"], "deep", &reference_spec())
        .unwrap()
        .add_th1d_at(&["SR2"], "h_met", &reference_spec())
        .unwrap()
        .finish(&ours)
        .unwrap();
    let script = r#"
import sys
import numpy as np
import uproot
with uproot.open(sys.argv[1]) as f:
    keys = f.keys(recursive=True)
    assert keys == ["smash2_provenance;1", "SR1;1", "SR1/h_met;1",
                    "SR1/sub;1", "SR1/sub/deep;1", "SR2;1", "SR2/h_met;1"], keys
    p = f["smash2_provenance"]
    assert p.classname == "TNamed"
    assert p.member("fName") == "smash2_provenance"
    assert p.member("fTitle") == '{"tool":"smash2 0.1.0"}'
    for path in ["SR1/h_met", "SR1/sub/deep", "SR2/h_met"]:
        h = f[path]
        assert np.array_equal(h.values(flow=True), [1.5, 2.0, 0.0, 3.25, 4.0, 0.5]), path
    # Directory objects resolve as directories.
    assert f.classnames(recursive=False)["SR1;1"] == "TDirectory"
    assert f["SR1"].keys(recursive=False) == ["h_met;1", "sub;1"]
print("OK")
"#;
    run_inline(&py, script, &ours, "uproot tnamed/directory readback");
    std::fs::remove_dir_all(&dir).unwrap();
}

/// hadd smoke test for the per-region TDirectory layout (env-gated on a
/// ROOT installation): merging two files must sum contents per directory
/// path and preserve the layout. Loud SKIP without `hadd` on PATH.
#[test]
fn hadd_merges_per_region_directory_layout() {
    if Command::new("hadd").arg("-h").output().is_err() {
        eprintln!(
            "SKIPPED hadd_merges_per_region_directory_layout: no `hadd` on PATH \
             (install ROOT to exercise the merge gate)"
        );
        return;
    }
    let Some(py) = python_or_skip("hadd_merges_per_region_directory_layout") else {
        return;
    };
    let dir = tmpdir("hadd");
    let (a, b, merged) = (dir.join("a.root"), dir.join("b.root"), dir.join("m.root"));
    for path in [&a, &b] {
        RootFile::create()
            .with_datime(pack_datime(2026, 6, 12, 0, 0, 0))
            .with_uuids([0; 16], [0; 16])
            .add_th1d_at(&["SR1"], "h_met", &reference_spec())
            .unwrap()
            .finish(path)
            .unwrap();
    }
    let out = Command::new("hadd")
        .args([
            "-f",
            merged.to_str().unwrap(),
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "hadd failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let script = r#"
import sys
import numpy as np
import uproot
with uproot.open(sys.argv[1]) as f:
    h = f["SR1/h_met"]
    assert np.array_equal(h.values(flow=True), [3.0, 4.0, 0.0, 6.5, 8.0, 1.0])
    assert h.member("fEntries") == 22.0
print("OK")
"#;
    run_inline(&py, script, &merged, "uproot merged readback");
    std::fs::remove_dir_all(&dir).unwrap();
}
