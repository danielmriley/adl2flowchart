//! The report's trust surface: the per-claim provenance annotations, the
//! trust summary block, the diagnostics split, matrix behaviour at scale,
//! witness rendering, and solver-failure honesty.
//!
//! These pin the REPORT layer only — no test here asserts a verdict, and
//! none may start doing so: a change that moves a verdict belongs to the
//! golden battery and the corpus gate, not here.

use adl_analysis::report::{
    Diagnostic, DiagnosticClass, EmptyStatus, FailOn, ReconFilter, RenderOptions, Report,
    SCHEMA_VERSION, VerdictKind,
};
use adl_analysis::{AnalysisOptions, SolverChoice, analyze_hir, analyze_source};
use adl_sema::{ExtDecls, analyze_str, merge_hirs};
use std::path::PathBuf;
use std::time::Duration;

fn repo(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join(rel)
}

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        ..AnalysisOptions::default()
    }
}

fn analyze(rel: &str) -> Report {
    let path = repo(rel);
    let src = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    analyze_source(&src, &name, &ExtDecls::legacy(), &opts())
        .unwrap_or_else(|e| panic!("{name} must resolve cleanly:\n{e}"))
}

fn plain(report: &Report) -> String {
    report.human_default(false)
}

// ---- per-claim trust provenance ----------------------------------------

#[test]
fn proven_disjoint_lines_carry_their_evidence() {
    let r = analyze("legacy_parser/tests/golden/disjoint_pt.adl");
    let out = plain(&r);
    let line = out
        .lines()
        .find(|l| l.contains("PROVEN DISJOINT"))
        .unwrap_or_else(|| panic!("no PROVEN DISJOINT line:\n{out}"));
    assert!(
        line.contains("[certified · gate ") && line.contains("· probes "),
        "the annotation must name certificate, sampling gate and probes: {line}"
    );
    // The interval fast path refutes on two bounds and consumes no axiom, so
    // it must NOT claim an assumption it does not rest on.
    assert!(
        !line.contains("assumes:"),
        "an interval-bounds proof uses no axiom: {line}"
    );
}

#[test]
fn a_solver_core_claim_names_only_the_assumptions_its_core_uses() {
    let r = analyze("examples/golden/disjoint_06.adl");
    let out = plain(&r);
    let with_axioms: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("PROVEN DISJOINT") && l.contains("assumes:"))
        .collect();
    assert!(
        !with_axioms.is_empty(),
        "expected at least one core-backed disjointness with an assumption:\n{out}"
    );
    // Every assumption shown must be one the run actually declares.
    let declared = r.assumption_clauses();
    for line in &with_axioms {
        let tail = line.split("assumes: ").nth(1).expect("split");
        let listed = tail.split(']').next().expect("closing bracket");
        for a in listed.split("; ") {
            assert!(
                declared.iter().any(|c| c.starts_with(a)),
                "claim assumption {a:?} is not in the run's assumption list {declared:?}"
            );
        }
    }
}

/// The complement: a core whose only axiom assumes nothing (SZ0) must show
/// no `assumes:` segment — the annotation reports what a claim rests on, not
/// what the run happened to load.
#[test]
fn an_assumption_free_core_shows_no_assumption() {
    let r = analyze("examples/Examples/CMS-SUS-16-032.adl");
    let out = plain(&r);
    let line = out
        .lines()
        .find(|l| l.contains("compressednb1 vs compressednbnc0"))
        .unwrap_or_else(|| panic!("fixture pair missing:\n{out}"));
    assert!(line.contains("(+1 axioms)"), "{line}");
    assert!(!line.contains("assumes:"), "SZ0 assumes nothing: {line}");
}

#[test]
fn overlap_claims_never_advertise_the_unsat_side_gates() {
    let r = analyze("legacy_parser/tests/golden/reject_or_band.adl");
    let out = plain(&r);
    let line = out
        .lines()
        .find(|l| l.contains("PROVEN OVERLAPPING"))
        .unwrap_or_else(|| panic!("no overlap line:\n{out}"));
    assert!(line.contains("[witness validated]"), "{line}");
    // The gates are UNSAT-side; claiming them on a SAT-side verdict would be
    // the exact overclaim the annotation exists to prevent.
    let verdict_part = line.split(" — ").next().expect("verdict half");
    assert!(!verdict_part.contains("gate "), "{line}");
    assert!(!verdict_part.contains("certified"), "{line}");
}

#[test]
fn trust_block_reports_solver_nets_and_certified_share() {
    let r = analyze("examples/CMS/CMS-SUS-16-033_Delphes.adl");
    let out = plain(&r);
    assert!(out.contains("== trust =="), "{out}");
    let t = r.trust_stats();
    assert_eq!(t.certified, t.proven_disjoint, "all routes are certified");
    assert!(
        out.contains(&format!(
            "{} disjoint ({}/{} certified, 100%)",
            t.proven_disjoint, t.certified, t.proven_disjoint
        )),
        "trust block must state the certified share:\n{out}"
    );
    assert!(out.contains("certification on"), "{out}");
    assert!(out.contains("sampling gate "), "{out}");
    assert!(out.contains("refute gate "), "{out}");
    assert!(out.contains("refutations   0 sampling · 0 adversarial"), "{out}");
    // The assumption list moved here from `== axioms used ==` — one source.
    for clause in r.assumption_clauses() {
        assert!(out.contains(&clause), "missing assumption {clause}:\n{out}");
    }
}

#[test]
fn trust_block_says_so_when_a_net_is_off() {
    let path = repo("legacy_parser/tests/golden/disjoint_pt.adl");
    let src = std::fs::read_to_string(&path).unwrap();
    let r = analyze_source(
        &src,
        "t.adl",
        &ExtDecls::legacy(),
        &AnalysisOptions {
            certify: false,
            refute_gate: false,
            sample_gate: 0,
            ..opts()
        },
    )
    .unwrap();
    let out = plain(&r);
    assert!(out.contains("certification OFF"), "{out}");
    assert!(out.contains("sampling gate OFF"), "{out}");
    assert!(out.contains("refute gate OFF"), "{out}");
    // …and no claim may then advertise coverage it did not get.
    let line = out.lines().find(|l| l.contains("PROVEN DISJOINT")).unwrap();
    assert!(!line.contains("gate "), "{line}");
    assert!(!line.contains("probes "), "{line}");
    assert!(line.contains("certification off"), "{line}");
}

// ---- --explain ----------------------------------------------------------

#[test]
fn explain_prints_the_axiom_statements_help_promises() {
    let r = analyze("examples/Examples/CMS-SUS-16-032.adl");
    let out = r.human();
    for a in &r.axioms_used {
        assert!(
            out.contains(&format!("statement: {}", a.statement)),
            "axiom {} statement missing from --explain",
            a.id
        );
    }
    // …and again inside the core that consumes them.
    assert!(out.contains("  axiom "), "core axioms unexpanded:\n{out}");
    assert!(out.contains("        assumes: "), "{out}");
}

#[test]
fn explain_states_the_proof_route_and_certificate_size() {
    let r = analyze("legacy_parser/tests/golden/disjoint_pt.adl");
    let out = r.human();
    assert!(
        out.contains("proof: interval bounds; certificate: 2 formula(s) replay-checked"),
        "{out}"
    );
}

#[test]
fn explain_keeps_the_full_witness_event_the_default_summarizes() {
    let r = analyze("examples/CMS/CMS-SUS-16-033_Delphes.adl");
    let short = plain(&r);
    let long = r.human();
    assert!(
        !short.contains(r#"{"ELECTRON""#),
        "the default report must not inline witness-event JSON"
    );
    assert!(
        short.contains("(summarized; --explain for the full event)"),
        "…and must say where the full event is:\n{short}"
    );
    assert!(
        long.contains(r#"{"ELECTRON""#),
        "--explain must keep the full event"
    );
    // No line in the default report may be a screenful.
    let widest = short.lines().map(str::len).max().unwrap_or(0);
    assert!(widest < 600, "default report has a {widest}-char line");
}

#[test]
fn grouped_lines_read_as_english() {
    let r = analyze("examples/CMS/CMS-SUS-16-033_Delphes.adl");
    let out = plain(&r);
    assert!(
        !out.contains("region the first region") && !out.contains("region the second region"),
        "grouped placeholder substitution left broken grammar:\n{out}"
    );
    assert!(
        out.contains("in the first region") || out.contains("the second region"),
        "expected at least one grouped POSSIBLY line:\n{out}"
    );
}

// ---- diagnostics triage -------------------------------------------------

#[test]
fn ordinary_downgrades_are_not_filed_as_bugs() {
    let r = analyze("examples/CMS/CMS-SUS-16-033_Delphes.adl");
    assert!(
        !r.diagnostics.is_empty(),
        "this file is the fail-closed-note fixture"
    );
    assert!(
        r.diagnostics
            .iter()
            .all(|d| d.class == DiagnosticClass::FailClosed),
        "a capped SAT-direction search contradicts no conclusion: {:?}",
        r.diagnostics
    );
    let short = plain(&r);
    assert!(short.contains("fail-closed note"), "{short}");
    assert!(!short.contains("INTERNAL CONTRADICTIONS"), "{short}");
    let long = r.human();
    assert!(long.contains("== fail-closed notes =="), "{long}");
    assert!(
        !long.contains("INTERNAL DIAGNOSTICS (bugs, please report)"),
        "fail-closed notes must not be filed under a bug-report plea"
    );
    assert!(!long.contains("== INTERNAL CONTRADICTIONS"), "{long}");
    // The flat list is untouched for existing consumers.
    assert_eq!(r.internal_diagnostics.len(), r.diagnostics.len());
    for (flat, d) in r.internal_diagnostics.iter().zip(&r.diagnostics) {
        assert_eq!(flat, &d.message);
    }
}

#[test]
fn contradictions_keep_the_loud_wording() {
    let mut r = analyze("legacy_parser/tests/golden/disjoint_pt.adl");
    r.diagnostics.push(Diagnostic {
        class: DiagnosticClass::Contradiction,
        message: "SAMPLING GATE refuted PROVEN DISJOINT for A vs B".to_owned(),
    });
    r.internal_diagnostics.push(r.diagnostics[0].message.clone());
    let short = plain(&r);
    assert!(short.contains("INTERNAL CONTRADICTIONS:"), "{short}");
    assert!(short.contains("bugs, please report"), "{short}");
    let long = r.human();
    assert!(
        long.contains("== INTERNAL CONTRADICTIONS (bugs, please report) =="),
        "{long}"
    );
}

// ---- matrix -------------------------------------------------------------

#[test]
fn matrix_is_never_dropped_silently_above_the_limit() {
    let r = analyze("examples/cl_examples/CMS-SUS-21-006.adl");
    assert!(r.regions.len() > 20, "fixture must exceed the limit");
    let out = plain(&r);
    assert!(out.contains("== verdict matrix =="), "{out}");
    assert!(
        out.contains("re-run with --matrix to print it in full"),
        "suppression must name its reason and the flag:\n{out}"
    );
    let forced = r.render_default(&RenderOptions {
        force_matrix: true,
        ..RenderOptions::default()
    });
    assert!(
        forced.contains("D disjoint   O overlapping"),
        "--matrix must print the matrix:\n{forced}"
    );
}

#[test]
fn colliding_row_labels_become_a_numbered_legend() {
    // Cross-file labels (`file.adl::SR1`) share a long prefix, so a 24-char
    // truncation makes different regions print identically.
    let ext = ExtDecls::legacy();
    let files = [
        "examples/CMS/CMS-SUS-16-033_Delphes.adl",
        "examples/Examples/CMS-SUS-16-032.adl",
    ];
    let hirs: Vec<_> = files
        .iter()
        .map(|f| {
            let p = repo(f);
            let src = std::fs::read_to_string(&p).unwrap();
            analyze_str(&src, p.file_name().unwrap().to_str().unwrap(), &ext)
        })
        .collect();
    let refs: Vec<&adl_sema::Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    let r = analyze_hir(&mut merged, "", &ext, &opts());
    let out = r.render_default(&RenderOptions {
        force_matrix: true,
        ..RenderOptions::default()
    });
    assert!(
        out.contains("labels (truncation would make two regions indistinguishable"),
        "{out}"
    );
    // Every region must appear in the legend under its FULL name.
    for region in &r.regions {
        assert!(
            out.contains(&region.name),
            "region {} is not identifiable in the matrix",
            region.name
        );
    }
}

// ---- reconciliation ledger ---------------------------------------------

#[test]
fn ledger_rows_attribute_files_and_carry_a_legend() {
    let ext = ExtDecls::legacy();
    let files = [
        "examples/CMS/CMS-SUS-16-033_Delphes.adl",
        "examples/Examples/CMS-SUS-16-032.adl",
    ];
    let hirs: Vec<_> = files
        .iter()
        .map(|f| {
            let p = repo(f);
            let src = std::fs::read_to_string(&p).unwrap();
            analyze_str(&src, p.file_name().unwrap().to_str().unwrap(), &ext)
        })
        .collect();
    let refs: Vec<&adl_sema::Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    let r = analyze_hir(
        &mut merged,
        "",
        &ext,
        &AnalysisOptions {
            reconcile: true,
            ..opts()
        },
    );
    assert!(!r.reconciliations.is_empty(), "expected a ledger");
    for row in &r.reconciliations {
        assert!(
            !row.a_units.is_empty() && !row.b_units.is_empty(),
            "unattributed ledger row {row:?}"
        );
    }
    let out = plain(&r);
    assert!(out.contains("legend: `C<id>#name [file]`"), "{out}");
    assert!(out.contains("[CMS-SUS-16-032.adl]"), "{out}");
    assert!(out.contains("[CMS-SUS-16-033_Delphes.adl]"), "{out}");

    // --explain carries the ledger too (it used to omit it entirely).
    assert!(r.human().contains("== collection reconciliation =="));

    // --recon=related drops the noise rows and says it did.
    let related = r.render_default(&RenderOptions {
        recon: ReconFilter::Related,
        ..RenderOptions::default()
    });
    let shown = related
        .lines()
        .filter(|l| l.contains("XSUB") || l.contains("XEQ"))
        .count();
    let unrelated = related
        .lines()
        .filter(|l| l.contains("neither cut set implies"))
        .count();
    assert!(shown > 0 && unrelated == 0, "{related}");
    assert!(related.contains("--recon=all for every candidate"), "{related}");
    assert_eq!(ReconFilter::parse("related"), Ok(ReconFilter::Related));
    assert!(ReconFilter::parse("nope").is_err());
}

// ---- solver failure -----------------------------------------------------

#[test]
fn a_failing_solver_is_loud_and_gateable() {
    let mut r = analyze("legacy_parser/tests/golden/disjoint_pt.adl");
    // Report-layer fixture: the engine sets both fields together (a spawn/IO
    // failure), so simulate that state rather than break a real solver here.
    r.solver_degraded = Some("2 solver check(s) failed via `z3`".to_owned());
    r.solver_failures = Some(adl_analysis::report::SolverFailures {
        spawn: 2,
        errors: 0,
        first_reason: "`z3` died mid-query (EOF on stdout)".to_owned(),
    });
    let out = plain(&r);
    assert!(
        out.lines().nth(1).is_some_and(|l| l.contains("SOLVER FAILED")),
        "the header must say so:\n{out}"
    );
    assert!(out.contains("first reason: `z3` died mid-query"), "{out}");
    assert!(out.contains("--fail-on=unknown"), "{out}");

    let gate = FailOn::parse("unknown").unwrap();
    assert!(gate.unknown);
    assert_eq!(r.exit_code(&gate), 4, "{:?}", r.findings(&gate));
    assert_eq!(r.exit_code(&FailOn::default()), 0, "default gate unchanged");
}

#[test]
fn fail_on_unknown_fires_on_an_unknown_pair() {
    let mut r = analyze("legacy_parser/tests/golden/disjoint_pt.adl");
    r.pairwise[0].kind = VerdictKind::Unknown;
    let gate = FailOn::parse("unknown").unwrap();
    assert_eq!(r.exit_code(&gate), 4);
    assert!(r.findings(&gate)[0].starts_with("unknown: "));
}

// ---- invariants ---------------------------------------------------------

#[test]
fn every_rendering_is_byte_identical_across_runs() {
    let r = analyze("examples/Examples/CMS-SUS-16-032.adl");
    for opts in [
        RenderOptions::default(),
        RenderOptions {
            force_matrix: true,
            recon: ReconFilter::Related,
            ..RenderOptions::default()
        },
    ] {
        assert_eq!(r.render_default(&opts), r.render_default(&opts));
        assert_eq!(r.render_explain(&opts), r.render_explain(&opts));
    }
    assert_eq!(r.to_json(), r.to_json());
    assert!(!plain(&r).contains('\u{1b}'), "color=false emits no ANSI");
}

#[test]
fn json_stays_v4_and_additive() {
    let r = analyze("legacy_parser/tests/golden/disjoint_pt.adl");
    let v: serde_json::Value = serde_json::from_str(&r.to_json()).unwrap();
    assert_eq!(v["schema_version"], SCHEMA_VERSION);
    assert_eq!(v["schema_version"], 4);
    // Additive keys present…
    assert_eq!(v["certification"], true);
    assert!(v["pairwise"][0]["proof_path"].is_string());
    assert!(v["pairwise"][0]["certificate_size"].is_number());
    // …and the pre-existing ones untouched in name and meaning.
    for k in [
        "schema_version",
        "unit",
        "solver",
        "regions",
        "pairwise",
        "bin_checks",
        "axioms_used",
        "internal_diagnostics",
    ] {
        assert!(v.get(k).is_some(), "lost key {k}");
    }
    // Optional additions stay absent in a healthy run.
    assert!(v.get("solver_failures").is_none());
    assert!(v.get("diagnostics").is_none());
    // Region emptiness provenance is present only where there is a claim.
    for (row, region) in v["regions"].as_array().unwrap().iter().zip(&r.regions) {
        assert_eq!(
            row.get("empty_proof").is_some(),
            matches!(region.empty, EmptyStatus::Proven | EmptyStatus::Candidate)
        );
    }
}
