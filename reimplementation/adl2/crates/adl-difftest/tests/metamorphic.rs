//! TESTING.md §2 — **metamorphic battery**: semantics-preserving source
//! transforms must yield consistent verdicts (DISJOINT, EMPTY, and subset
//! flags identical — explanations/witness values may differ in wording; the
//! sole tolerated difference is PROVEN-vs-POSSIBLY OVERLAPPING, a heuristic
//! witness-realization artifact — see `Summary::consistent`) AND identical
//! interpreter membership on every sampled event.
//!
//! Transforms:
//! 1. `swap(A, B)` declaration-order symmetry;
//! 2. `reject c` ≡ `select not c` (both polarities);
//! 3. double negation (`not not c` ≡ `c`);
//! 4. inline vs named define;
//! 5. inherited region vs textually-pasted region;
//! 6. pure-rename object invariance (`object jets2 take jets`).

use adl_analysis::AnalysisOptions;
use adl_difftest::casegen::{GCase, RbMode, RenderCtx, arb_case, arb_case_with_define, render};
use adl_difftest::oracle::{run_case, sample_events, summary, with_source};
use adl_interp::Event;
use adl_sema::ExtDecls;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use std::sync::OnceLock;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

fn events() -> &'static [Event] {
    static EVENTS: OnceLock<Vec<Event>> = OnceLock::new();
    EVENTS.get_or_init(|| sample_events(ext()))
}

const DEFAULT_CASES: u32 = if cfg!(feature = "deep") { 10_000 } else { 250 };

fn config() -> ProptestConfig {
    let mut c = ProptestConfig::default(); // honors PROPTEST_CASES
    if std::env::var_os("PROPTEST_CASES").is_none() {
        c.cases = DEFAULT_CASES;
    }
    c.failure_persistence = None;
    c
}

/// Run both renderings of `case` and require identical verdicts and
/// identical interpreter membership.
fn must_agree(
    case: &GCase,
    base: &RenderCtx,
    variant: &RenderCtx,
    what: &str,
) -> Result<(), TestCaseError> {
    let opts = AnalysisOptions::default();
    let src1 = render(case, base);
    let src2 = render(case, variant);
    let r1 = run_case(&src1, ext(), events(), &opts)
        .map_err(|e| TestCaseError::fail(with_source(&e, &src1)))?;
    let r2 = run_case(&src2, ext(), events(), &opts)
        .map_err(|e| TestCaseError::fail(with_source(&e, &src2)))?;

    if r1.passes != r2.passes {
        let i = r1
            .passes
            .iter()
            .zip(&r2.passes)
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        return Err(TestCaseError::fail(format!(
            "{what}: interpreter membership differs at sampled event {i}: \
             base {:?} vs variant {:?}\n--- base ---\n{src1}\n--- variant ---\n{src2}",
            r1.passes[i], r2.passes[i]
        )));
    }

    let s1 = summary(&r1.report).map_err(TestCaseError::fail)?;
    let s2 = summary(&r2.report).map_err(TestCaseError::fail)?;
    // Soundness facts (disjoint/empty/subset) must match exactly; only the
    // PROVEN-vs-POSSIBLY overlapping proof strength may differ (heuristic
    // witness realization). The interpreter-membership check above is strict.
    if !s1.consistent(&s2) {
        return Err(TestCaseError::fail(format!(
            "{what}: verdicts differ:\n  base    {s1:?}\n  variant {s2:?}\n\
             base reason: {}\n  variant reason: {}\n\
             base internal: {:?}\n  variant internal: {:?}\n\
             --- base ---\n{src1}\n--- variant ---\n{src2}",
            r1.report.pairwise[0].reason,
            r2.report.pairwise[0].reason,
            r1.report.internal_diagnostics,
            r2.report.internal_diagnostics,
        )));
    }
    Ok(())
}

proptest! {
    #![proptest_config(config())]

    /// 1. Declaration-order swap: verdicts are pair-symmetric.
    #[test]
    fn swap_symmetry(case in arb_case()) {
        let variant = RenderCtx { swap_regions: true, ..RenderCtx::default() };
        must_agree(&case, &RenderCtx::default(), &variant, "swap(A,B)")?;
    }

    /// 2. `reject c` ≡ `select not c` (and `select c` ≡ `reject not c`).
    #[test]
    fn reject_is_select_not(case in arb_case()) {
        let variant = RenderCtx { flip_polarity: true, ..RenderCtx::default() };
        must_agree(&case, &RenderCtx::default(), &variant, "reject ≡ select not")?;
    }

    /// 3. Double negation is the identity.
    #[test]
    fn double_negation(case in arb_case()) {
        let variant = RenderCtx { double_neg: true, ..RenderCtx::default() };
        must_agree(&case, &RenderCtx::default(), &variant, "not not c ≡ c")?;
    }

    /// 4. A named boolean define is its inlined body.
    #[test]
    fn inline_vs_named_define(case in arb_case_with_define()) {
        let variant = RenderCtx { inline_defines: true, ..RenderCtx::default() };
        must_agree(&case, &RenderCtx::default(), &variant, "inline vs named define")?;
    }

    /// 5. Inheriting a region ≡ pasting its statements.
    #[test]
    fn inherit_vs_paste(case in arb_case()) {
        let inherit = RenderCtx { rb_mode: RbMode::InheritRa, ..RenderCtx::default() };
        let paste = RenderCtx { rb_mode: RbMode::PasteRa, ..RenderCtx::default() };
        must_agree(&case, &inherit, &paste, "inherit vs paste")?;
    }

    /// 6. A pure rename (`object jets2 take jets`, no cuts) is identity.
    #[test]
    fn pure_rename_invariance(case in arb_case()) {
        let variant = RenderCtx {
            colls: ["jets2", "eles2"],
            alias_objects: true,
            ..RenderCtx::default()
        };
        must_agree(&case, &RenderCtx::default(), &variant, "pure rename")?;
    }
}
