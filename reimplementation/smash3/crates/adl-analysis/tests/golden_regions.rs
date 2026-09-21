//! Golden-verdict regression suite over `examples/golden/*.adl`.
//!
//! Each file pins fully-known ground truth in header comments and is the
//! permanent regression net for the disjoint/overlap/empty feature:
//!
//! ```text
//! # GOLDEN <RegionA> <RegionB> DISJOINT|OVERLAPPING|POSSIBLY
//! # GOLDEN-EMPTY <Region>
//! # GOLDEN-SUBSET <Sub> <Sup> YES|NO
//! # GOLDEN-NOSOLVER <RegionA> <RegionB> DISJOINT|OVERLAPPING|POSSIBLY
//! # GOLDEN-COVERAGE <Region> PROVEN|NOT_PROVEN
//! ```
//!
//! `GOLDEN-SUBSET ... NO` is how a file pins that a claim must stay
//! UNDERIVABLE — the shape a false-PROVEN regression would take. It asserts
//! prevention, not gate reliance: the run has its gates on, so a claim the
//! gates merely withdrew would still fail the pin's intent, and the
//! accompanying `refutations` check below states that no gate fired.
//!
//! `GOLDEN-NOSOLVER` re-runs the same file with the solver DISABLED, which
//! pins a verdict to the interval fast path alone (`SolverChoice::None`
//! caps everything else at POSSIBLY).
//!
//! A file may carry several header lines (e.g. a three-region chain). For
//! each one we run the full analysis (solver required) and assert the
//! reported pairwise verdict — or region emptiness — matches the pin
//! exactly. A precision regression (PROVEN→POSSIBLY) is a real failure
//! here: these examples were hand-verified to be provable.

use adl_analysis::{
    AnalysisOptions, CoverageStatus, EmptyStatus, FailOn, SolverChoice, VerdictKind, analyze_source,
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
        refute_gate: true,
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
    Subset { sub: String, sup: String, claimed: bool },
    Coverage { region: String, proven: bool },
    NoSolver { a: String, b: String, kind: VerdictKind },
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
        } else if let Some(rest) = line.strip_prefix("# GOLDEN-SUBSET ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            assert!(
                parts.len() == 3 && (parts[2] == "YES" || parts[2] == "NO"),
                "{file}: GOLDEN-SUBSET needs `<Sub> <Sup> YES|NO`: {line:?}"
            );
            pins.push(Pin::Subset {
                sub: parts[0].to_owned(),
                sup: parts[1].to_owned(),
                claimed: parts[2] == "YES",
            });
        } else if let Some(rest) = line.strip_prefix("# GOLDEN-COVERAGE ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            assert!(
                parts.len() == 2 && (parts[1] == "PROVEN" || parts[1] == "NOT_PROVEN"),
                "{file}: GOLDEN-COVERAGE needs `<Region> PROVEN|NOT_PROVEN`: {line:?}"
            );
            pins.push(Pin::Coverage {
                region: parts[0].to_owned(),
                proven: parts[1] == "PROVEN",
            });
        } else if let Some(rest) = line.strip_prefix("# GOLDEN-NOSOLVER ") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            assert!(
                parts.len() == 3,
                "{file}: GOLDEN-NOSOLVER needs `<A> <B> <KIND>`: {line:?}"
            );
            pins.push(Pin::NoSolver {
                a: parts[0].to_owned(),
                b: parts[1].to_owned(),
                kind: expected_kind(parts[2]),
            });
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
    let mut checked_subset = 0usize;
    let mut checked_nosolver = 0usize;
    let mut checked_coverage = 0usize;
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
                // Prevention, not gate reliance: the claim must be
                // UNDERIVABLE, and the run's own refutation counters (checked
                // below) say the gates did not have to withdraw it.
                Pin::Subset { sub, sup, claimed } => {
                    let pr = report
                        .pairwise
                        .iter()
                        .find(|p| (p.a == sub && p.b == sup) || (p.a == sup && p.b == sub));
                    match pr {
                        None => failures.push(format!(
                            "{file}: no pairwise report for ({sub}, {sup})"
                        )),
                        Some(p) => {
                            let got = if p.a == sub { p.subset_a_in_b } else { p.subset_b_in_a };
                            if got == claimed {
                                checked_subset += 1;
                            } else {
                                failures.push(format!(
                                    "{file}: subset {sub} within {sup} expected {claimed} \
                                     got {got} — {}",
                                    p.reason
                                ));
                            }
                        }
                    }
                }
                // Bin coverage is the ONE proven tier with no post-hoc net
                // (no sampling gate runs on bins), so a pin here is the only
                // thing standing between a coverage claim and a user.
                Pin::Coverage { region, proven } => {
                    let bc = report.bin_checks.iter().find(|b| b.region == region);
                    match bc {
                        None => failures.push(format!("{file}: no bin check for {region}")),
                        Some(b) => {
                            let got = b.coverage == CoverageStatus::Proven;
                            if got == proven {
                                checked_coverage += 1;
                            } else {
                                failures.push(format!(
                                    "{file}: coverage of {region} expected proven={proven} \
                                     got {:?}",
                                    b.coverage
                                ));
                            }
                        }
                    }
                }
                Pin::NoSolver { a, b, kind } => {
                    let mut o = opts();
                    o.solver = SolverChoice::NoSolver;
                    let r = analyze_source(&src, &file, &ext, &o)
                        .unwrap_or_else(|e| panic!("{file} must resolve cleanly:\n{e}"));
                    let pr = r
                        .pairwise
                        .iter()
                        .find(|p| (p.a == a && p.b == b) || (p.a == b && p.b == a));
                    match pr {
                        None => failures.push(format!(
                            "{file}: no solver-less pairwise report for ({a}, {b})"
                        )),
                        Some(p) if p.kind != kind => failures.push(format!(
                            "{file}: solver-less ({a}, {b}) expected {} got {} — {}",
                            kind.human(),
                            p.kind.human(),
                            p.reason
                        )),
                        Some(_) => checked_nosolver += 1,
                    }
                }
            }
        }
        // A golden file must never need a gate: the pins assert what the
        // DERIVATION does, and a refutation would mean an encoder/axiom fact
        // is false on a real event.
        let sampled = report.sampling.map_or(0, |s| s.refutations);
        let probed = report.refute.map_or(0, |r| r.refutations);
        if sampled > 0 || probed > 0 {
            failures.push(format!(
                "{file}: gates refuted a claim ({sampled} sampling, {probed} adversarial) — \
                 a golden file must be right by derivation"
            ));
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
    eprintln!(
        "golden: {checked_pairs} pair + {checked_empty} empty + {checked_subset} subset + \
         {checked_nosolver} solver-less + {checked_coverage} coverage pins matched"
    );
    assert!(
        checked_pairs + checked_empty + checked_subset + checked_nosolver + checked_coverage > 0,
        "golden suite checked nothing"
    );
}
