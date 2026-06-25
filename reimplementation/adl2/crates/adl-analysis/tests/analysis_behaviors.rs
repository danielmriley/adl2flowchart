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

/// A back-indexed element (`coll[-k]`) is a sound free leaf for the
/// disjoint/subset (UNSAT) direction, but the witness builder cannot
/// realize it, so an overlap (SAT) that depends on it caps at POSSIBLY
/// rather than chase a model-dependent witness. These two regions overlap
/// only on MET (both also gate on `jets[-1]`), so the verdict downgrades.
#[test]
fn back_index_overlap_caps_at_possibly() {
    let src = "\
object jets
  take Jet

region RA
  select jets[-1].pT > 10
  select MET > 50

region RB
  select jets[-1].pT > 10
  select MET < 100
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "backidx_overlap.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    assert_eq!(
        p.kind,
        VerdictKind::PossiblyOverlapping,
        "back-index overlap must cap at POSSIBLY, got {:?} ({})",
        p.kind,
        p.reason
    );
    assert!(
        p.reason.contains("back-indexed element"),
        "reason should name the back-index cap: {}",
        p.reason
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

/// P3 Combination witness realizer: an overlap over a composite tuple count
/// (`size(K->cand) >= 1`) must be REALIZED through the interpreter — the
/// realizer builds the binder source collection, the interpreter materializes
/// the disjoint combination and forms a value-distinct pair, then validates
/// the overlap. Before P3 this hard-failed ("composite projection in witness")
/// and downgraded to POSSIBLY.
#[test]
fn composite_overlap_witness_validates_through_the_interpreter() {
    let src = "\
object jets
  take Jet
  select pT > 30

object bjets
  take jets
  select btag == 1

composite dijet
  take disjoint(jets j1, jets j2)
  candidate jj = j1 + j2

region SR_x
  select size(dijet->jj) >= 1

region SR_y
  select size(dijet->jj) >= 1
  select size(bjets) >= 0
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "composite_overlap.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    assert_eq!(
        p.kind,
        VerdictKind::ProvenOverlapping,
        "composite overlap must validate, got {:?} ({})",
        p.kind,
        p.reason
    );
    assert_eq!(
        p.witness_validated,
        Some(true),
        "the interpreter must validate the realized composite witness"
    );
    assert!(
        r.internal_diagnostics.is_empty(),
        "no internal diagnostic for a realizable composite: {:?}",
        r.internal_diagnostics
    );
}

/// The realizer NEVER fabricates a false Validated: when a composite region's
/// membership depends on the candidate's opaque invariant mass (`mass(jj)`),
/// the interpreter cannot evaluate it and the witness stays a CANDIDATE
/// (verdict keeps its caveat / downgrades), never a false PROVEN OVERLAPPING.
#[test]
fn composite_opaque_mass_falls_to_candidate_not_false_validated() {
    let src = "\
object jets
  take Jet
  select pT > 30

composite dijet
  take disjoint(jets j1, jets j2)
  candidate jj = j1 + j2

region SR_x
  select size(dijet->jj) >= 1
  select mass(dijet->jj[0]) > 50

region SR_y
  select size(dijet->jj) >= 1
  select mass(dijet->jj[0]) < 200
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "composite_mass.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    // The opaque mass blocks a fully-validated overlap: the verdict must NOT
    // be a clean PROVEN OVERLAPPING with witness_validated == Some(true).
    assert_ne!(
        p.witness_validated,
        Some(true),
        "an opaque candidate mass must NOT produce a validated witness: {}",
        p.reason
    );
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "the regions are not disjoint; mass(jj) is a free var: {}",
        p.reason
    );
}

/// SOUNDNESS-CRITICAL: two value-position numeric reducers over DIFFERENT
/// collections (`sum(jets.pT)` vs `sum(eles.pT)`) are distinct free
/// quantities — they share no band, so `sum(jets.pT) > 400` and
/// `sum(eles.pT) <= 400` must NOT be PROVEN DISJOINT (an event can satisfy
/// both). A false PROVEN here would mean the two reducers collided onto one
/// quantity id.
#[test]
fn distinct_reducer_collections_are_not_proven_disjoint() {
    let src = "\
object jets
  take Jet
object eles
  take Ele

region SR_x
  select sum(jets.pT) > 400

region SR_y
  select sum(eles.pT) <= 400
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "two_sums.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "sum(jets.pT) and sum(eles.pT) are independent free vars; \
         proving the pair disjoint fabricates a false PROVEN: {}",
        p.reason
    );
}

/// CAPABILITY: two regions whose bands sit on the SAME structurally-interned
/// reducer (`define HT = sum(jets.pT)`; `HT > 400` vs `HT in [60,400]`) ARE
/// PROVEN DISJOINT — the cancellation the fix restores (the interval engine
/// sees one shared free var and proves `(400,inf]` ∩ `[60,400] = ∅`).
#[test]
fn shared_reducer_band_is_proven_disjoint() {
    let src = "\
object jets
  take Jet
define HT = sum(jets.pT)

region SR_hi
  select HT > 400

region SR_lo
  select HT [] 60 400
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "shared_ht.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves cleanly");
    if r.solver == "none" {
        eprintln!("SKIP: no solver available");
        return;
    }
    let p = &r.pairwise[0];
    assert_eq!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "a shared interned reducer must let the interval engine prove \
         (400,inf] vs [60,400] disjoint: {}",
        p.reason
    );
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
