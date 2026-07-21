//! Golden-verdict regression suite over `examples/golden/*.adl`.
//!
//! Each file pins fully-known ground truth in header comments and is the
//! permanent regression net for the disjoint/overlap/empty feature:
//!
//! ```text
//! # GOLDEN <RegionA> <RegionB> DISJOINT|OVERLAPPING|POSSIBLY
//! # GOLDEN-EMPTY <Region>
//! ```
//!
//! A file may carry several header lines (e.g. a three-region chain). For
//! each one we run the full analysis (solver required) and assert the
//! reported pairwise verdict — or region emptiness — matches the pin
//! exactly. A precision regression (PROVEN→POSSIBLY) is a real failure
//! here: these examples were hand-verified to be provable.

use adl_analysis::{
    AnalysisOptions, EmptyStatus, FailOn, SolverChoice, VerdictKind, analyze_source,
};
use adl_sema::ExtDecls;
use std::path::PathBuf;
use std::time::Duration;

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(30),
        fail_on: FailOn::default(),
        reconcile: false,
        sample_gate: 64,
        certify: true,
        combine: false,
    }
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../examples/golden")
}

#[derive(Debug, Clone)]
enum Pin {
    Pair { a: String, b: String, kind: VerdictKind },
    Empty { region: String },
}

fn expected_kind(tok: &str) -> VerdictKind {
    match tok {
        "DISJOINT" => VerdictKind::ProvenDisjoint,
        "OVERLAPPING" => VerdictKind::ProvenOverlapping,
        "CANDIDATE" => VerdictKind::CandidateOverlapping,
        "POSSIBLY" => VerdictKind::PossiblyOverlapping,
        other => panic!("unknown GOLDEN kind token: {other:?}"),
    }
}

/// Parse every `# GOLDEN ...` / `# GOLDEN-EMPTY ...` header line in a file.
fn parse_pins(src: &str, file: &str) -> Vec<Pin> {
    let mut pins = Vec::new();
    for line in src.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("# GOLDEN-EMPTY ") {
            let region = rest.split_whitespace().next().unwrap_or_else(|| {
                panic!("{file}: malformed GOLDEN-EMPTY line: {line:?}")
            });
            pins.push(Pin::Empty { region: region.to_owned() });
        } else if let Some(rest) = line.strip_prefix("# GOLDEN ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            assert!(
                parts.len() == 3,
                "{file}: GOLDEN line needs `<A> <B> <KIND>`: {line:?}"
            );
            pins.push(Pin::Pair {
                a: parts[0].to_owned(),
                b: parts[1].to_owned(),
                kind: expected_kind(parts[2]),
            });
        }
    }
    pins
}

#[test]
fn golden_corpus_matches_pinned_verdicts() {
    let dir = golden_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("golden dir {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "adl"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "golden corpus must not be empty");

    let ext = ExtDecls::legacy();
    let mut checked_pairs = 0usize;
    let mut checked_empty = 0usize;
    let mut solver_seen = false;
    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let src = std::fs::read_to_string(path).expect("readable");
        let pins = parse_pins(&src, &file);
        assert!(!pins.is_empty(), "{file}: no GOLDEN header — every golden file must pin a verdict");

        let report = analyze_source(&src, &file, &ext, &opts())
            .unwrap_or_else(|e| panic!("{file} must resolve cleanly:\n{e}"));
        if report.solver == "none" {
            continue;
        }
        solver_seen = true;

        for pin in pins {
            match pin {
                Pin::Pair { a, b, kind } => {
                    let pr = report.pairwise.iter().find(|p| {
                        (p.a == a && p.b == b) || (p.a == b && p.b == a)
                    });
                    match pr {
                        None => failures.push(format!(
                            "{file}: no pairwise report for ({a}, {b}); \
                             regions present: {:?}",
                            report.regions.iter().map(|r| &r.name).collect::<Vec<_>>()
                        )),
                        Some(p) if p.kind != kind => failures.push(format!(
                            "{file}: ({a}, {b}) expected {} got {} — {}",
                            kind.human(),
                            p.kind.human(),
                            p.reason
                        )),
                        Some(_) => checked_pairs += 1,
                    }
                }
                Pin::Empty { region } => {
                    let rr = report.regions.iter().find(|r| r.name == region);
                    match rr {
                        None => failures.push(format!(
                            "{file}: no region report for {region}"
                        )),
                        Some(r) if r.empty != EmptyStatus::Proven => failures.push(format!(
                            "{file}: region {region} expected PROVEN EMPTY got {:?}",
                            r.empty
                        )),
                        Some(_) => checked_empty += 1,
                    }
                }
            }
        }
    }

    if !solver_seen {
        eprintln!("SKIP: no solver available for any golden file");
        return;
    }
    assert!(
        failures.is_empty(),
        "{} golden verdict mismatch(es):\n{}",
        failures.len(),
        failures.join("\n")
    );
    eprintln!("golden: {checked_pairs} pair pins + {checked_empty} empty pins matched");
    assert!(checked_pairs + checked_empty > 0, "golden suite checked nothing");
}
