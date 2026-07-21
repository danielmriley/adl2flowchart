//! `adl-certify` — an independently-checkable certifier for UNSAT claims over
//! the analyzer's linear-rational formula fragment.
//!
//! # What this crate buys
//!
//! Before this crate, a `PROVEN DISJOINT` verdict meant *the solver said
//! `unsat`*: the encoder, the axiom emission, the solver session, and the
//! solver itself were all in the trusted base. This crate replaces that trust
//! with a **replayable proof**. [`certify_unsat`] searches for a proof that a
//! conjunction of [`QFormula`]s is unsatisfiable and returns a serializable
//! [`Certificate`]; [`Certificate::replay`] re-checks that certificate with
//! nothing but exact rational arithmetic — no search, no solver, no encoder.
//! The trusted base collapses to this one crate, and in fact to the small
//! kernel reachable from `replay` (`replay` + the `saturate` decomposition +
//! `canonicalize` + `farkas_refutes`).
//!
//! # The fragment
//!
//! [`QFormula`] is negation-free by type: `True | False | Atom | And | Or`.
//! An [`adl_formula::LinAtom`] is `Σ cᵢ·qᵢ ⋈ k` with `⋈ ∈ {<, ≤, >, ≥, =, ≠}`
//! and exact-rational ([`adl_sema::Rat`]) coefficients and constant. The whole
//! input slice is treated as one conjunction (the "checked set" of an UNSAT
//! frame — region-over formulas plus axiom instances).
//!
//! # The proof system: DPLL(Farkas)
//!
//! The conjunction of the input formulas is refuted by a search over its
//! boolean structure (the `search` module, untrusted) whose result is a proof
//! tree ([`Certificate`], trusted to *check*, not to *find*):
//!
//! * **Saturation** (the `saturate` module, shared with replay) flattens
//!   `And`s, drops `True`, detects `False`, splits every `=` atom into the two
//!   bounds `≤`/`≥`, and turns every `≠` atom into a two-way `Or` (`< ∨ >`).
//!   What remains is a flat list of inequality atoms and `Or` obligations, in a
//!   deterministic left-to-right order.
//! * **Case split**: to refute `hard ∧ Or₁ ∧ …`, pick the leftmost `Or` and
//!   refute the conjunction once per disjunct; ALL disjuncts must be refuted
//!   (`A ∧ (d₁ ∨ d₂)` is unsat iff `A∧d₁` and `A∧d₂` are both unsat). The
//!   certificate records one sub-proof per disjunct, in disjunct order.
//! * **Leaf (Farkas)**: a conjunction with no remaining `Or` is a pure system
//!   of linear inequalities. It is unsatisfiable over the reals iff there are
//!   nonnegative multipliers `λ` under which the constraints sum to a false
//!   ground relation (Farkas' lemma / the Motzkin transposition theorem for
//!   mixed strict / non-strict systems). The certificate stores `λ`.
//!
//! # Why replay-checked Farkas coefficients prove real-infeasibility
//!
//! Each hard atom is put in canonical upper-bound form `aᵢ·x ⋈ᵢ bᵢ`,
//! `⋈ᵢ ∈ {<, ≤}` (`canonicalize`). Multiplying inequality `i` by a nonnegative
//! `λᵢ` preserves it, and summing preserves it, yielding
//! `(Σλᵢaᵢ)·x ⋈ (Σλᵢbᵢ)` where the combined `⋈` is `<` when some strict
//! constraint carries a positive multiplier, else `≤`. If every combined
//! coefficient `Σλᵢaᵢ` is zero, the left side is the constant `0`, so the sum
//! asserts `0 ⋈ S` with `S = Σλᵢbᵢ` — a variable-free statement that is a
//! *logical consequence of the system for every assignment*. When that
//! statement is arithmetically false (`0 ≤ S` with `S < 0`, or `0 < S` with
//! `S ≤ 0`), the system has no real solution. That is exactly what
//! `farkas_refutes` checks, and it enforces `λ ≥ 0` (a negative multiplier
//! would flip an inequality and could "prove" anything). Because the check is a
//! fixed finite computation over the *original* atoms, it needs no trust in how
//! `λ` was found.
//!
//! # Why real-infeasible implies int-infeasible (the integrality policy)
//!
//! Some quantities (collection sizes) are integer-valued in the engine. This
//! crate relaxes every quantity to the reals. The integer solution set is a
//! subset of the real solution set, so *no real solution ⇒ no integer
//! solution*: a real-infeasibility proof is a fortiori an integer-infeasibility
//! proof. The converse fails — a system can be integer-infeasible yet
//! real-feasible (e.g. `2x = 1`) — and this crate never claims those: a
//! real-feasible leaf yields [`CertifyResult::Uncertified`], and the caller
//! demotes to a CANDIDATE tier. This is conservative, never wrong. (No sort
//! metadata is passed in, so integer-only-UNSAT sets surface as
//! `Uncertified("branch satisfiable …")`; see the crate README / the returned
//! integration notes for how the engine labels them "integrality".)
//!
//! # The two guarantees
//!
//! * **Soundness of `Certified`**: [`certify_unsat`] re-runs
//!   [`Certificate::replay`] on its own output before returning `Certified`, so
//!   a certificate the trusted kernel would reject is never emitted, and (by
//!   the argument above) a replay-valid certificate proves real- (hence
//!   integer-) unsatisfiability.
//! * **No false `Certified` for a satisfiable set**: Fourier–Motzkin
//!   elimination is complete for the reals, so a satisfiable leaf is detected
//!   as feasible and aborts the whole attempt as `Uncertified`; since a single
//!   satisfiable branch witnesses satisfiability of the whole input, a
//!   satisfiable set can never be certified.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod bundle;
mod certificate;
mod constraint;
mod fm;
mod saturate;
mod search;

pub use bundle::CombineBundle;
pub use certificate::{CertNode, Certificate, QRat};

use adl_formula::QFormula;

/// Hard cap on case-split nesting depth, independent of [`Budget::max_branches`]
/// — it keeps both the search recursion and the replay recursion off a deep
/// stack. Exceeding it fails closed (`Uncertified` / `replay == false`), never
/// panics. Both the searcher and [`Certificate::replay`] honour it, so a
/// certificate is never produced deeper than it can be re-checked. Kept modest
/// so the search and replay recursions fit comfortably on a small (2 MiB) test
/// thread stack; real unsat cores are shallow (a handful of nested splits), so
/// this is never the binding limit in practice.
pub(crate) const MAX_DEPTH: usize = 1024;

/// Resource limits for a certification attempt. Over-budget is always
/// [`CertifyResult::Uncertified`] — never a wrong answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// Maximum number of case-split branches (disjunct choices) the search may
    /// enter before giving up. Guards against boolean blow-up.
    pub max_branches: usize,
    /// Maximum number of linear atoms in a single conjunctive leaf (after
    /// `=`-splitting). Guards the per-leaf Fourier–Motzkin cost.
    pub max_atoms: usize,
}

impl Default for Budget {
    /// Generous defaults sized for solver unsat cores (typically 2–10 members):
    /// 100 000 branches, 128 atoms per leaf.
    fn default() -> Self {
        Self {
            max_branches: 100_000,
            max_atoms: 128,
        }
    }
}

/// The outcome of [`certify_unsat`].
#[derive(Debug, Clone, PartialEq)]
pub enum CertifyResult {
    /// A replayable proof that the input conjunction is (real-, hence integer-)
    /// unsatisfiable. Guaranteed: `certificate.replay(formulas) == true`.
    Certified(Certificate),
    /// No proof was produced. The reason is a stable-prefixed string:
    /// `"budget: …"` (branch / atom / depth cap), `"shape: …"` (Fourier–Motzkin
    /// fill-in blow-up), or `"branch satisfiable: …"` (a leaf is real-feasible,
    /// so the set is not UNSAT — this subsumes integer-only-UNSAT sets under the
    /// real relaxation).
    Uncertified(String),
}

impl CertifyResult {
    /// Was a certificate produced?
    #[must_use]
    pub fn is_certified(&self) -> bool {
        matches!(self, CertifyResult::Certified(_))
    }

    /// The certificate, if certified.
    #[must_use]
    pub fn certificate(&self) -> Option<&Certificate> {
        match self {
            CertifyResult::Certified(c) => Some(c),
            CertifyResult::Uncertified(_) => None,
        }
    }

    /// The uncertified reason string, if not certified.
    #[must_use]
    pub fn reason(&self) -> Option<&str> {
        match self {
            CertifyResult::Uncertified(r) => Some(r),
            CertifyResult::Certified(_) => None,
        }
    }
}

/// Try to certify that the conjunction of `formulas` is unsatisfiable.
///
/// On success returns [`CertifyResult::Certified`] wrapping a [`Certificate`]
/// that has already passed [`Certificate::replay`] (the search never hands back
/// a proof the trusted kernel rejects). On any other outcome — a real-feasible
/// leaf, an exceeded budget, or a Fourier–Motzkin blow-up — returns
/// [`CertifyResult::Uncertified`] with a reason. Never panics.
#[must_use]
pub fn certify_unsat(formulas: &[QFormula], budget: &Budget) -> CertifyResult {
    let mut searcher = search::Searcher::new(budget);
    match searcher.refute(formulas, 0) {
        Ok(root) => {
            let cert = Certificate::new(root);
            // Defensive gate: never emit a certificate the trusted kernel would
            // reject. This makes `Certified ⇒ replay == true` hold by
            // construction, regardless of any bug in the (untrusted) search.
            if cert.replay(formulas) {
                CertifyResult::Certified(cert)
            } else {
                CertifyResult::Uncertified(
                    "shape: constructed certificate failed self-replay".to_string(),
                )
            }
        }
        Err(reason) => CertifyResult::Uncertified(reason),
    }
}

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-certify";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-certify");
    }

    #[test]
    fn empty_set_is_satisfiable_not_certified() {
        // The empty conjunction is trivially true, hence satisfiable.
        let r = certify_unsat(&[], &Budget::default());
        assert!(!r.is_certified());
    }
}
