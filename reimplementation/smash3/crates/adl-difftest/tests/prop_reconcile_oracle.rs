//! Property-based oracle over the RECONCILIATION path (review F8): the
//! encoder-vs-interpreter net previously had zero reach here — every difftest
//! entry point ran `reconcile: false`, so a derived XSUB/XEQ size fact (an
//! UNSAT-side emission feeding PROVEN verdicts) was guarded only by the
//! hand-picked `cross_file.rs` scenarios.
//!
//! Reconciliation candidates arise intra-unit too (two filtered objects over
//! one base), so the single-unit oracle applies directly: generate random
//! tight/loose pt chains over `Jet` with random size-cut regions, analyze
//! with `reconcile: true`, and require every verdict to hold on the sampled
//! events — a PROVEN DISJOINT fabricated by a wrong size fact is refuted by
//! any sampled event the interpreter passes through both regions.
//!
//! Runs 256 cases under plain `cargo test`; the `deep` feature raises that
//! to 4096. `PROPTEST_CASES` overrides both.

use adl_analysis::AnalysisOptions;
use adl_difftest::oracle::{check_sound, run_case, sample_events, with_source};
use adl_interp::Event;
use adl_sema::ExtDecls;
use proptest::prelude::*;
use std::sync::OnceLock;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

fn events() -> &'static [Event] {
    static EVENTS: OnceLock<Vec<Event>> = OnceLock::new();
    EVENTS.get_or_init(|| sample_events(ext()))
}

const DEFAULT_CASES: u32 = if cfg!(feature = "deep") { 4_096 } else { 256 };

fn config() -> ProptestConfig {
    let mut c = ProptestConfig::default(); // honors PROPTEST_CASES
    if std::env::var_os("PROPTEST_CASES").is_none() {
        c.cases = DEFAULT_CASES;
    }
    c.failure_persistence = None; // counterexamples become explicit tests
    c
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn reconcile_derived_verdicts_hold_on_sampled_events(
        lo in 20u32..=60,
        gap in 1u32..=80,
        tight_ub in proptest::option::of(150u32..=300),
        loose_ub in proptest::option::of(150u32..=300),
        na in 0u32..=4,
        nb in 0u32..=4,
        ge_a in any::<bool>(),
        ge_b in any::<bool>(),
    ) {
        // tight = Jet pt > lo+gap [< tight_ub]; loose = Jet pt > lo [< loose_ub].
        // Depending on the bounds, tight may or may not refine loose — both
        // the fact-fires and the fact-must-not-fire shapes are generated.
        let hi = lo + gap;
        let mut tight_cuts = format!("  select pt > {hi}\n");
        if let Some(u) = tight_ub {
            tight_cuts.push_str(&format!("  select pt < {u}\n"));
        }
        let mut loose_cuts = format!("  select pt > {lo}\n");
        if let Some(u) = loose_ub {
            loose_cuts.push_str(&format!("  select pt < {u}\n"));
        }
        let src = format!(
            "object tight\n  take Jet\n{tight_cuts}\
             object loose\n  take Jet\n{loose_cuts}\
             region RA\n  select size(tight) {} {na}\n\
             region RB\n  select size(loose) {} {nb}\n",
            if ge_a { ">=" } else { "<=" },
            if ge_b { ">=" } else { "<=" },
        );
        let opts = AnalysisOptions {
            reconcile: true,
            ..AnalysisOptions::default()
        };
        let run = run_case(&src, ext(), events(), &opts)
            .map_err(|e| TestCaseError::fail(with_source(&e, &src)))?;
        check_sound(&run).map_err(|e| TestCaseError::fail(with_source(&e, &src)))?;
    }
}
