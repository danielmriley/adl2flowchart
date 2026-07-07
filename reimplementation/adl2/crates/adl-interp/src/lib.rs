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

pub mod cutflow;
pub mod eval;
pub mod event;
pub mod histo;
mod json;
pub mod provenance;
pub mod sample;
pub mod sha256;
mod weights;

pub use cutflow::{BinFlow, Counts, CutStep, CutflowSet, RegionFlow};
pub use eval::{
    BinOutcome, EvalError, Interp, NonValue, NumOutcome, RegionResult, StepEval, assign_bin,
    wrap_dphi,
};
pub use event::{
    CHUNK_EVENTS, ChunkReader, Event, EventChunk, EventError, EventObject, RawChunk,
    RawChunkReader, RawLine, StreamError, StreamedEvent, parse_event, read_jsonl,
};
pub use histo::{Hist1D, Hist1DVar, Hist2D, HistAcc, HistoFill, HistoSet};
pub use provenance::{InputIdentity, Provenance};
pub use sha256::{Sha256, sha256_hex};

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
