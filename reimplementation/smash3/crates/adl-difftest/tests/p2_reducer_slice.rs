//! P2 part-A focused encoder-vs-interpreter difftests: reducers
//! (`any`/`all`), the `min`/`max ⋈ c` monotone desugar, and static
//! slices. Each case is a hand-written two-region file over the shared
//! `sample_events` vocabulary; [`check_sound`] asserts the *exact* same
//! contract the random battery does (TESTING.md §2):
//!
//! - PROVEN DISJOINT ⇒ no sampled event passes both regions (over ⊇ interp);
//! - PROVEN SUBSET ⇒ no sampled counterexample;
//! - PROVEN OVERLAPPING ⇒ the witness re-validated through the interpreter;
//! - REGION EMPTY ⇒ no sampled member.
//!
//! A violation here is a real over/under-approximation bug in the new
//! reducer/slice encoding (the over-approx wrongly excluded a real member,
//! or the under-approx wrongly included a non-member).

use adl_analysis::AnalysisOptions;
use adl_difftest::oracle::{check_sound, run_case, sample_events};
use adl_interp::{Event, Interp};
use adl_sema::{ExtDecls, analyze_str};
use std::sync::OnceLock;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

fn events() -> &'static [Event] {
    static EVENTS: OnceLock<Vec<Event>> = OnceLock::new();
    EVENTS.get_or_init(|| sample_events(ext()))
}

/// Assert the soundness contract holds on `src` over the full sample.
#[track_caller]
fn assert_sound(src: &str) {
    let run = run_case(src, ext(), events(), &AnalysisOptions::default())
        .unwrap_or_else(|e| panic!("frontend/interp error:\n{e}\n--- src ---\n{src}"));
    if let Err(e) = check_sound(&run) {
        panic!("soundness violated: {e}\n--- src ---\n{src}");
    }
}

/// Interpreter membership of region `name` in `src` over every sample
/// event (a vector of pass/fail flags).
#[track_caller]
fn membership(src: &str, name: &str) -> Vec<bool> {
    let hir = analyze_str(src, "p2.adl", ext());
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "must resolve: {:#?}\n{src}",
        hir.diags
    );
    let interp = Interp::new(&hir, ext());
    events()
        .iter()
        .enumerate()
        .map(|(i, e)| {
            interp
                .eval_region_by_name(name, e)
                .unwrap_or_else(|err| panic!("event {i} interp error: {}", err.reason))
        })
        .collect()
}

/// The min/max desugar must not drift from the fold it stands for: the
/// interpreter membership of `max(e) ⋈ c` (desugared) must equal the
/// independent `nonempty ∧ ∃ e ⋈ c` (for `>`) / `empty ∨ ∀ e ⋈ c`
/// readings spelled with explicit `any`/`all`/`size` constructs.
#[track_caller]
fn assert_same_membership(reducer_src: &str, equivalent_src: &str) {
    let a = membership(reducer_src, "RA");
    let b = membership(equivalent_src, "RA");
    assert_eq!(
        a, b,
        "desugar drifted from its fold\n--- reducer ---\n{reducer_src}\n--- equivalent ---\n{equivalent_src}"
    );
}

const PRELUDE: &str = "object jets\n  take Jet\n\nobject eles\n  take Ele\n\n";

fn case(ra: &str, rb: &str) -> String {
    format!("{PRELUDE}region RA\n{ra}\nregion RB\n{rb}\n")
}

// ---- any / all -----------------------------------------------------------

#[test]
fn all_pt_threshold_vs_negation_is_sound() {
    // RA: every jet has pT > 100; RB: some jet has pT <= 100 (the negation).
    // These are NOT disjoint in general (empty jets ⇒ all=true and any=false),
    // but the contract must hold either way under sampling.
    assert_sound(&case(
        "  select all (pT(jets) > 100)\n",
        "  select any (pT(jets) <= 100)\n",
    ));
}

#[test]
fn any_pt_band_sound() {
    assert_sound(&case(
        "  select any (pT(jets) > 200)\n",
        "  reject any (pT(jets) > 200)\n",
    ));
}

#[test]
fn all_vs_any_size_gated_sound() {
    // all(pT>50) with size>=1 vs any(pT<50) with size>=1 — overlap structure
    // exercises the Any-minus / All-plus boundary.
    assert_sound(&case(
        "  select size(jets) >= 1\n  select all (pT(jets) > 50)\n",
        "  select size(jets) >= 1\n  select any (pT(jets) < 50)\n",
    ));
}

// ---- min / max desugar ---------------------------------------------------

#[test]
fn max_gt_desugars_to_any_sound() {
    // max(pT) > 200 ⇔ any(pT > 200). The reject side is the all(<=) reading.
    assert_sound(&case(
        "  select max (pT(jets)) > 200\n",
        "  reject max (pT(jets)) > 200\n",
    ));
}

#[test]
fn min_gt_desugars_to_all_sound() {
    // min(pT) > 100 ⇔ all(pT > 100); the negation is any(pT <= 100).
    assert_sound(&case(
        "  select min (pT(jets)) > 100\n",
        "  select max (pT(jets)) <= 100\n",
    ));
}

#[test]
fn max_window_disjoint_is_sound() {
    // RA needs a high-pT jet (max > 400); RB caps every jet (max < 50).
    // For a NON-empty jets these are disjoint; the empty case makes both
    // cut-false (max over empty ⇒ comparison-false), so still disjoint.
    assert_sound(&case(
        "  select size(jets) >= 1\n  select max (pT(jets)) > 400\n",
        "  select size(jets) >= 1\n  select max (pT(jets)) < 50\n",
    ));
}

#[test]
fn min_lt_and_min_gt_partition_sound() {
    assert_sound(&case(
        "  select min (pT(jets)) > 100\n",
        "  select min (pT(jets)) < 100\n",
    ));
}

// The desugar must match the REAL min/max fold on every event, INCLUDING
// the empty collection (USER ANSWER 2: min/max over empty ⇒ cut-false).
// `any` matches that for free; `all` is vacuously true on empty, so the
// All-target desugar (min>, max<) is guarded with `size > 0` — the
// manually-written equivalent therefore carries the same guard.

#[test]
fn max_gt_desugar_matches_fold() {
    // max(pT) > 200 ⇔ ∃ jet pT > 200 ⇔ any(pT > 200) (empty ⇒ both false).
    let prelude = PRELUDE;
    assert_same_membership(
        &format!("{prelude}region RA\n  select max (pT(jets)) > 200\n"),
        &format!("{prelude}region RA\n  select any (pT(jets) > 200)\n"),
    );
}

#[test]
fn min_gt_desugar_matches_fold() {
    // min(pT) > 100 ⇔ nonempty ∧ ∀ jet pT > 100 (empty ⇒ cut-false).
    let prelude = PRELUDE;
    assert_same_membership(
        &format!("{prelude}region RA\n  select min (pT(jets)) > 100\n"),
        &format!("{prelude}region RA\n  select size(jets) > 0\n  select all (pT(jets) > 100)\n"),
    );
}

#[test]
fn max_lt_desugar_matches_fold() {
    // max(pT) < 50 ⇔ nonempty ∧ ∀ jet pT < 50 (empty ⇒ cut-false).
    let prelude = PRELUDE;
    assert_same_membership(
        &format!("{prelude}region RA\n  select max (pT(jets)) < 50\n"),
        &format!("{prelude}region RA\n  select size(jets) > 0\n  select all (pT(jets) < 50)\n"),
    );
}

#[test]
fn min_lt_desugar_matches_fold() {
    // min(pT) < 100 ⇔ ∃ jet pT < 100 ⇔ any(pT < 100) (empty ⇒ both false).
    let prelude = PRELUDE;
    assert_same_membership(
        &format!("{prelude}region RA\n  select min (pT(jets)) < 100\n"),
        &format!("{prelude}region RA\n  select any (pT(jets) < 100)\n"),
    );
}

#[test]
fn minmax_empty_collection_is_cut_false() {
    // Pin the empty boundary directly: with NO jets, every monotone
    // min/max comparison must be FALSE (the NonValue ⇒ cut-false rule),
    // never vacuously true. The sample includes 0-jet events.
    let prelude = PRELUDE;
    let zero_jets = membership(&format!("{prelude}region RA\n  select size(jets) == 0\n"), "RA");
    assert!(
        zero_jets.iter().any(|&z| z),
        "sample must contain at least one 0-jet event"
    );
    for cut in [
        "min (pT(jets)) > 100",
        "max (pT(jets)) < 50",
        "min (pT(jets)) < 100",
        "max (pT(jets)) > 200",
    ] {
        let mem = membership(&format!("{prelude}region RA\n  select {cut}\n"), "RA");
        for (i, &z) in zero_jets.iter().enumerate() {
            assert!(
                !(z && mem[i]),
                "event {i} has 0 jets but `{cut}` accepted it (min/max empty must be cut-false)"
            );
        }
    }
}

// ---- static slice --------------------------------------------------------

#[test]
fn slice_size_bound_sound() {
    // size of a [:2] slice is <= 2 and <= size(jets); cross with a size cut.
    assert_sound(&case(
        "  select size(jets[0:2]) >= 2\n",
        "  select size(jets) <= 1\n",
    ));
}

#[test]
fn slice_reducer_all_sound() {
    // all over a static slice (`jets[:2]`): exact k=2 conjunction.
    assert_sound(&case(
        "  select all (pT(jets[0:2]) > 100)\n",
        "  reject all (pT(jets[0:2]) > 100)\n",
    ));
}

#[test]
fn slice_open_ended_reducer_sound() {
    // open-ended slice (`jets[1:]`) falls back to the bounded Dual.
    assert_sound(&case(
        "  select any (pT(jets[1:]) > 200)\n",
        "  reject any (pT(jets[1:]) > 200)\n",
    ));
}
