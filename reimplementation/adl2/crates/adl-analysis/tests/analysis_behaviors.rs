//! Production-behavior tests beyond the golden battery:
//! - witness re-validation DOWNGRADES an unrealizable model to POSSIBLY
//!   and files an internal diagnostic (TESTING.md §3 — production
//!   behavior, not test-only);
//! - the whole 68-file corpus runs the no-solver analysis without
//!   panics, deterministically (SPEC_ARCHITECTURE §9).

use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_source};
use adl_sema::ExtDecls;
use std::path::PathBuf;
use std::time::Duration;

fn opts(solver: SolverChoice) -> AnalysisOptions {
    AnalysisOptions {
        solver,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
    }
}

/// The realizer builds all-pass events (every base element passes every
/// filter), so a model that NEEDS a partially-failing base collection
/// cannot validate: the verdict must downgrade to POSSIBLY with an
/// internal diagnostic — never display a witness the interpreter
/// rejects.
#[test]
fn unrealizable_witness_downgrades_with_internal_diagnostic() {
    let src = "\
object jets
  take Jet
  select pT > 30

region SR_x
  select size(Jet) == 2
  select size(jets) == 1

region SR_y
  select size(jets) == 1
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "partial_filter.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    assert_eq!(
        p.kind,
        VerdictKind::PossiblyOverlapping,
        "must downgrade, got {:?} ({})",
        p.kind,
        p.reason
    );
    assert!(
        p.reason.contains("re-validation failed") || p.reason.contains("downgraded"),
        "{}",
        p.reason
    );
    assert!(p.witness.is_empty(), "no rejected witness may be displayed");
    assert!(
        r.internal_diagnostics
            .iter()
            .any(|d| d.contains("witness validation failed")),
        "internal-error diagnostic filed: {:?}",
        r.internal_diagnostics
    );
}

/// EPRED soundness: an object filter `pt / d ⋈ c` must clear the constant
/// denominator with EXACT coefficients, not fold the f64 reciprocal `1/d`
/// (which asserts a predicate stronger than the truth). A jet with pt == 49
/// is a genuine member of `{Jet : pt/49 >= 1}`, so the two regions below
/// share the event pt == 49 — the analyzer must NOT prove either region
/// empty nor the pair disjoint.
#[test]
fn epred_ratio_filter_does_not_fabricate_empty_or_disjoint() {
    let src = "\
object jets
  take Jet
  select pt / 49 >= 1

region A
  select jets[0].pt <= 49

region B
  select jets[0].pt >= 49
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "epred_ratio.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    for reg in &r.regions {
        assert_ne!(
            reg.empty,
            adl_analysis::EmptyStatus::Proven,
            "region {} falsely proven empty (pt=49 is a member)",
            reg.name
        );
    }
    let p = &r.pairwise[0];
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "regions share pt=49, must not be PROVEN DISJOINT: {}",
        p.reason
    );
    assert_eq!(p.kind, VerdictKind::ProvenOverlapping, "{}", p.reason);
}

/// Encoder soundness: a coefficient that OVERFLOWS to non-finite (here
/// `MAX + MAX`) puts the cut outside the linear fragment — the interpreter
/// still evaluates it per-event and gets a finite result for small inputs.
/// It must be Unknown, NOT constant-false; treating it as false would
/// fabricate a PROVEN EMPTY (the cut `MAX*MET - (0 - MAX*MET) > 0` accepts
/// MET = 0.1 in the interpreter).
#[test]
fn coefficient_overflow_is_unknown_not_empty() {
    let big = format!("{:.1}", f64::MAX);
    let src = format!(
        "region A\n  select {big} * MET - (0 - {big} * MET) > 0\nregion B\n  select MET > 0\n"
    );
    let ext = ExtDecls::legacy();
    let r = analyze_source(&src, "overflow.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    let a = r
        .regions
        .iter()
        .find(|reg| reg.name == "A")
        .expect("region A");
    assert_ne!(
        a.empty,
        adl_analysis::EmptyStatus::Proven,
        "overflow cut must not be proven empty"
    );
}

/// Identity soundness: a `define` that aliases a particle must make
/// `f(alias)` and `f(literal)` the SAME opaque quantity. Otherwise the two
/// intern as distinct free quantities and the solver finds a spurious model
/// where one physical scalar takes two values — a false PROVEN OVERLAPPING
/// between cuts that are decidably disjoint (`tagger(jets[0]) > 100` and
/// `tagger(jets[0]) < 50`).
#[test]
fn define_aliased_opaque_arg_matches_the_literal() {
    let src = "\
object jets
  take Jet
  select pt > 30
define leadjet = jets[0]
region RA
  select size(jets) >= 1
  select tagger(jets[0]) > 100
region RB
  select size(jets) >= 1
  select tagger(leadjet) < 50
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "alias.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    assert_eq!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "aliased opaque arg must intern identically to the literal (got {:?}: {})",
        p.kind,
        p.reason
    );
}

/// A validated witness, for contrast: same shapes, realizable model.
#[test]
fn realizable_witness_validates_and_proves_overlap() {
    let src = "\
object jets
  take Jet
  select pT > 30

region SR_x
  select size(jets) >= 1

region SR_y
  select size(jets) >= 2
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "realizable.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    assert_eq!(p.kind, VerdictKind::ProvenOverlapping, "{}", p.reason);
    assert_eq!(p.witness_validated, Some(true));
    // SR_y within SR_x.
    assert!(p.subset_b_in_a);
    assert!(!p.subset_a_in_b);
}

/// CMS-SUS-16-032 transcription-bug class (CORPUS gap 1): an opaque
/// pt-named external call inside an impossible ratio must prove the
/// region EMPTY — `(pT(...) + MET)/MET < 0.5` with `MET > 250` forces
/// `pT(...) < -125`, contradicting the NNEG axiom on pt-named opaques.
#[test]
fn opaque_pt_in_impossible_ratio_proves_region_empty() {
    use adl_analysis::EmptyStatus;
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/opaque_pt_ratio_empty.adl");
    let src = std::fs::read_to_string(&path).expect("fixture readable");
    let ext = ExtDecls::legacy();
    let r = analyze_source(
        &src,
        "opaque_pt_ratio_empty.adl",
        &ext,
        &opts(SolverChoice::Auto),
    )
    .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let impossible = r
        .regions
        .iter()
        .find(|reg| reg.name == "SR_impossible_ratio")
        .expect("region present");
    assert_eq!(
        impossible.empty,
        EmptyStatus::Proven,
        "impossible ratio over a pt-named opaque must prove EMPTY"
    );
    assert!(
        impossible.empty_core.iter().any(|c| {
            let h = c.human();
            h.contains("NNEG") && h.contains("pT(...)")
        }),
        "the emptiness core must rest on the NNEG opaque-pt instance: {:?}",
        impossible.empty_core
    );
    let sane = r
        .regions
        .iter()
        .find(|reg| reg.name == "SR_sane")
        .expect("region present");
    assert_ne!(sane.empty, EmptyStatus::Proven, "control region stays live");
}

#[test]
fn corpus_runs_no_solver_analysis_deterministically() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../examples");
    let mut files: Vec<PathBuf> = walk(&dir);
    files.sort();
    assert_eq!(files.len(), 68, "shared corpus has 68 ADL files");
    let ext = ExtDecls::legacy();
    let mut analyzed = 0usize;
    for path in &files {
        let src = std::fs::read_to_string(path).expect("readable");
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let run = |s: &str| {
            analyze_source(s, &name, &ext, &opts(SolverChoice::NoSolver))
                .unwrap_or_else(|e| panic!("{name} must resolve cleanly:\n{e}"))
        };
        let a = run(&src);
        let b = run(&src);
        assert_eq!(a.to_json(), b.to_json(), "{name}: deterministic JSON");
        assert_eq!(a.human(), b.human(), "{name}: deterministic report");
        assert_eq!(
            a.human_default(false),
            b.human_default(false),
            "{name}: deterministic default report"
        );
        assert!(
            !a.human_default(false).contains('\u{1b}'),
            "{name}: plain default report must carry no ANSI escapes"
        );
        // No-solver degradation: nothing stronger than interval proofs.
        for p in &a.pairwise {
            assert_ne!(
                p.kind,
                VerdictKind::ProvenOverlapping,
                "{name}: SAT-direction proofs need a solver + witness"
            );
        }
        analyzed += 1;
    }
    assert_eq!(analyzed, 68);
}

fn walk(dir: &PathBuf) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("examples dir exists") {
        let entry = entry.expect("dir entry");
        let p = entry.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else if p.extension().is_some_and(|e| e == "adl") {
            out.push(p);
        }
    }
    out
}
