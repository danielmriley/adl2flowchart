//! `adl-interp` — the reference interpreter: Event in → bool/values out
//! (SPEC_ARCHITECTURE §8).
//!
//! Evaluates resolved HIR (from `adl-sema`) over [`Event`] records read
//! from JSONL. Semantics are exactly SPEC_LANGUAGE §4 — **this crate is
//! the executable spec**, used as a user tool, as the oracle for
//! property-based verification testing, and as the differential anchor.
//!
//! Entry points: [`read_jsonl`] / [`parse_event`] (events in),
//! [`Interp`] (evaluator), [`assign_bin`] (boundary-bin rule).

pub mod eval;
pub mod event;
pub mod histo;

pub use eval::{
    BinOutcome, EvalError, Interp, NonValue, NumOutcome, RegionResult, assign_bin, wrap_dphi,
};
pub use event::{Event, EventError, EventObject, parse_event, read_jsonl};
pub use histo::{Hist1D, HistoFill, HistoSet};

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-interp";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-interp");
    }
}
