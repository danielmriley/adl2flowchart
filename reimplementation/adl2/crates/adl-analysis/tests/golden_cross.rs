//! Golden-verdict regression suite for CROSS-FILE (merged, reconciled)
//! analysis over `examples/golden/cross/*/` (review F10).
//!
//! Each subdirectory is one merge group: all its `*.adl` files are resolved
//! (unit = file name), merged, and analyzed with `reconcile: true` — exactly
//! what `verify --cross <dir>` does. Pins live in header comments of any
//! file in the group and use the MERGED region labels:
//!
//! ```text
//! # GOLDEN-CROSS <unit.adl::RegionA> <unit.adl::RegionB> DISJOINT|OVERLAPPING|CANDIDATE|POSSIBLY
//! ```
//!
//! This corpus is the pinned baseline for reconciliation-derived verdicts:
//! the planned `abs(x)<c` precision unlock (and any encoder change) must
//! not flip a POSSIBLY pin to PROVEN (each POSSIBLY here is a deliberate
//! fail-closed case) nor lose a pinned DISJOINT.

use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_hir};
use adl_sema::{ExtDecls, Hir, analyze_str, merge_hirs};
use std::path::PathBuf;
use std::time::Duration;

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(30),
        fail_on: FailOn::default(),
        reconcile: true,
        sample_gate: 64,
        certify: true,
    }
}

fn cross_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../examples/golden/cross")
}

struct Pin {
    a: String,
    b: String,
    kind: VerdictKind,
}

fn expected_kind(tok: &str) -> VerdictKind {
    match tok {
        "DISJOINT" => VerdictKind::ProvenDisjoint,
        "OVERLAPPING" => VerdictKind::ProvenOverlapping,
        "CANDIDATE" => VerdictKind::CandidateOverlapping,
        "POSSIBLY" => VerdictKind::PossiblyOverlapping,
        other => panic!("unknown GOLDEN-CROSS kind token: {other:?}"),
    }
}

fn parse_pins(src: &str, file: &str) -> Vec<Pin> {
    let mut pins = Vec::new();
    for line in src.lines() {
        if let Some(rest) = line.trim().strip_prefix("# GOLDEN-CROSS ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            assert!(
                parts.len() == 3,
                "{file}: GOLDEN-CROSS line needs `<unit::A> <unit::B> <KIND>`: {line:?}"
            );
            pins.push(Pin {
                a: parts[0].to_owned(),
                b: parts[1].to_owned(),
                kind: expected_kind(parts[2]),
            });
        }
    }
    pins
}

#[test]
fn golden_cross_corpus_matches_pinned_verdicts() {
    let dir = cross_dir();
    let mut groups: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("golden cross dir {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.is_dir())
        .collect();
    groups.sort();
    assert!(!groups.is_empty(), "golden cross corpus must not be empty");

    let ext = ExtDecls::legacy();
    let mut checked = 0usize;
    let mut solver_seen = false;
    let mut failures: Vec<String> = Vec::new();

    for group in &groups {
        let gname = group.file_name().unwrap().to_string_lossy().into_owned();
        let mut files: Vec<PathBuf> = std::fs::read_dir(group)
            .expect("group readable")
            .map(|e| e.expect("dir entry").path())
            .filter(|p| p.extension().is_some_and(|x| x == "adl"))
            .collect();
        files.sort();
        assert!(files.len() >= 2, "{gname}: a merge group needs >= 2 files");

        let mut pins: Vec<Pin> = Vec::new();
        let mut hirs: Vec<Hir> = Vec::new();
        for path in &files {
            let unit = path.file_name().unwrap().to_string_lossy().into_owned();
            let src = std::fs::read_to_string(path).expect("readable");
            pins.extend(parse_pins(&src, &format!("{gname}/{unit}")));
            let hir = analyze_str(&src, &unit, &ext);
            assert!(
                !adl_syntax::diag::has_errors(&hir.diags),
                "{gname}/{unit} must resolve cleanly: {:?}",
                hir.diags
            );
            hirs.push(hir);
        }
        assert!(!pins.is_empty(), "{gname}: no GOLDEN-CROSS pin in any file");

        let refs: Vec<&Hir> = hirs.iter().collect();
        let mut merged = merge_hirs(&refs);
        let report = analyze_hir(&mut merged, "", &ext, &opts());
        if report.solver == "none" {
            continue;
        }
        solver_seen = true;

        for pin in pins {
            let pr = report
                .pairwise
                .iter()
                .find(|p| (p.a == pin.a && p.b == pin.b) || (p.a == pin.b && p.b == pin.a));
            match pr {
                None => failures.push(format!(
                    "{gname}: no pairwise report for ({}, {}); regions: {:?}",
                    pin.a,
                    pin.b,
                    report.regions.iter().map(|r| &r.name).collect::<Vec<_>>()
                )),
                Some(p) if p.kind != pin.kind => failures.push(format!(
                    "{gname}: ({}, {}) expected {} got {} — {}",
                    pin.a,
                    pin.b,
                    pin.kind.human(),
                    p.kind.human(),
                    p.reason
                )),
                Some(_) => checked += 1,
            }
        }
    }

    if !solver_seen {
        eprintln!("SKIP: no solver available");
        return;
    }
    assert!(
        failures.is_empty(),
        "{} golden-cross mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("golden-cross: {checked} pins matched");
    assert!(checked > 0, "golden-cross suite checked nothing");
}
