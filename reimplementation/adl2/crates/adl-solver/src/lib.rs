//! `adl-solver` — the [`Solver`] trait and its two conformance-tested
//! backends (SPEC_ARCHITECTURE §7, ADR-006).
//!
//! - **Primary**: [`NativeSolver`] over the `z3` crate (native libz3
//!   bindings). Typed terms, incremental push/pop, models and unsat cores
//!   without string protocols — malformed input is unrepresentable
//!   (legacy audit Bug 5 layer 2).
//! - **Secondary**: [`SubprocessSolver`] speaking SMT-LIB2 to a solver
//!   binary on `PATH` (z3 by default). Any `(error …)` output makes the
//!   check come back [`SatResult::Unknown`] — **never** something
//!   silently weaker (legacy audit Bug 5: a dropped assert produced a
//!   false PROVEN OVERLAPPING).
//!
//! Both backends must pass the same conformance battery
//! (`tests/conformance.rs`): sat/unsat, models, unsat cores, push/pop,
//! integer sorts, timeout behavior and (subprocess) error injection.
//!
//! No solver at all is also a supported configuration: the analysis layer
//! degrades to its heuristic interval fast path with verdicts capped at
//! POSSIBLY (SPEC_ARCHITECTURE §7).

#[cfg(feature = "native")]
pub mod native;
pub mod subprocess;

#[cfg(feature = "native")]
pub use native::NativeSolver;
pub use subprocess::SubprocessSolver;

use adl_formula::QFormula;
use adl_sema::QuantityId;
use std::collections::BTreeMap;
use std::time::Duration;

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-solver";

/// Name attached to an assertion so unsat cores can be mapped back to
/// source spans / axiom catalog entries (the explanations feature,
/// SPEC_ANALYSIS §3).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AssertName(pub String);

impl AssertName {
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for AssertName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Outcome of one `check` call. `Unknown` carries a human-readable
/// reason ("timeout", "(error …) output", "spawn failed: …", …); it can
/// weaken a verdict to POSSIBLY, never flip it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SatResult {
    Sat,
    Unsat,
    Unknown(String),
}

impl SatResult {
    #[must_use]
    pub fn is_sat(&self) -> bool {
        matches!(self, SatResult::Sat)
    }

    #[must_use]
    pub fn is_unsat(&self) -> bool {
        matches!(self, SatResult::Unsat)
    }
}

/// Sort of a solver variable. Collection sizes are integers (the QF_LIRA
/// fragment); everything else is real-valued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QSort {
    Real,
    Int,
}

/// A satisfying assignment, keyed by quantity. Values for `Int`-sorted
/// quantities are integral `f64`s.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Model {
    values: BTreeMap<QuantityId, f64>,
}

impl Model {
    #[must_use]
    pub fn from_values(values: BTreeMap<QuantityId, f64>) -> Self {
        Self { values }
    }

    #[must_use]
    pub fn get(&self, q: QuantityId) -> Option<f64> {
        self.values.get(&q).copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (QuantityId, f64)> + '_ {
        self.values.iter().map(|(&q, &v)| (q, v))
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// The solver interface (SPEC_ARCHITECTURE §7).
///
/// Beyond the spec signature, `declare` fixes a quantity's sort before
/// first use; quantities mentioned in an assertion without a prior
/// `declare` default to `Real`.
pub trait Solver {
    /// Fix `q`'s sort. Must precede any assertion mentioning `q`;
    /// redundant identical declarations are fine.
    fn declare(&mut self, q: QuantityId, sort: QSort);
    fn push(&mut self);
    fn pop(&mut self);
    fn assert(&mut self, f: &QFormula, name: Option<AssertName>);
    /// `Sat`/`Unsat`/`Unknown` — no text-protocol leakage: any backend
    /// error surfaces as `Unknown` with its reason.
    fn check(&mut self, timeout: Duration) -> SatResult;
    /// The model of the most recent `Sat` check (with completion: every
    /// declared quantity gets a value). `None` otherwise.
    fn model(&mut self) -> Option<Model>;
    /// The unsat core of the most recent `Unsat` check, as the named
    /// assertions involved. `None` if the last check was not `Unsat`.
    fn unsat_core(&mut self) -> Option<Vec<AssertName>>;
    /// Human-readable backend label for reports.
    fn backend_name(&self) -> &'static str;
}

/// Is the native backend compiled in?
#[must_use]
pub fn native_available() -> bool {
    cfg!(feature = "native")
}

/// Is `cmd` (an SMT-LIB2 solver binary, e.g. `z3`) runnable?
#[must_use]
pub fn subprocess_available(cmd: &str) -> bool {
    std::process::Command::new(cmd)
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-solver");
    }
}
