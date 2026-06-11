//! TESTING.md §2 — **property-based encoder vs interpreter**.
//!
//! Random small two-region cases over the fixed vocabulary (event
//! scalars, `jets`/`eles` with `pT`/`Eta`/`BTag`, sizes, one angular
//! pair) composing comparisons, bands, AND/OR/NOT, ternary, `reject`
//! and defines. Events are sampled on a boundary grid + seeded random +
//! toy-generator records (including 0-element collections). Asserted
//! contract (every violation is a REAL engine bug):
//!
//! - PROVEN DISJOINT  ⇒ no sampled event passes both regions;
//! - PROVEN OVERLAPPING ⇒ the witness passed both regions through the
//!   interpreter (engine re-validation, `witness_validated == Some(true)`);
//! - PROVEN SUBSET    ⇒ no sampled counterexample;
//! - REGION EMPTY     ⇒ no sampled member.
//!
//! Runs 2000 cases under plain `cargo test`; the `deep` feature raises
//! that to 100k (`cargo test -p adl-difftest --features deep`). The
//! `PROPTEST_CASES` env var still overrides both.

use adl_analysis::AnalysisOptions;
use adl_difftest::casegen::{RenderCtx, arb_case, render};
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

const DEFAULT_CASES: u32 = if cfg!(feature = "deep") {
    100_000
} else {
    2_000
};

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
    fn encoder_vs_interpreter(case in arb_case()) {
        let src = render(&case, &RenderCtx::default());
        let run = run_case(&src, ext(), events(), &AnalysisOptions::default())
            .map_err(|e| TestCaseError::fail(with_source(&e, &src)))?;
        check_sound(&run).map_err(|e| TestCaseError::fail(with_source(&e, &src)))?;
    }
}
