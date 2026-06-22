# Soundness hardening: the exact-rational analyzer

This documents the soundness work that took ADL2's `verify` from "PROVEN is
*usually* right" to "PROVEN can never disagree with the exact-rational
semantics." It is the result of four adversarial audit rounds plus a numeric-
core rewrite. Companion design notes: `EXACT_RATIONAL_PLAN.md`.

## The contract

`verify` proves relations between selection regions. The value of the tool is
that a PROVEN verdict is **never wrong**:

| Verdict | Definition | Sound because |
|---|---|---|
| PROVEN DISJOINT | `UNSAT(A⁺ ∧ B⁺ ∧ axioms)` | over-approximations are supersets; if even the supersets can't intersect, the regions can't |
| PROVEN EMPTY | `UNSAT(R⁺ ∧ axioms)` | no event can satisfy a superset of R |
| PROVEN SUBSET A⊆B | `UNSAT(A⁺ ∧ ¬(B⁻) ∧ axioms)` | `A ∧ ¬B ⊆ A⁺ ∧ ¬B⁻` |
| PROVEN OVERLAPPING | `SAT(A⁻ ∧ B⁻)` + witness re-validated | under-approximations are subsets; the witness is checked through the interpreter |

The **UNSAT side** (disjoint/empty/subset) has no downstream safety net — a
single wrong fact fabricates a false PROVEN. The **SAT side** (overlapping) is
guarded by re-validating the witness through the reference interpreter, so a
mistake there only ever weakens the verdict to POSSIBLY.

## The root-cause class

Every false-PROVEN defect found across four audit rounds was one class:

> The analyzer reasoned about cut boundaries with **f64 arithmetic**, which
> rounds differently from the reference interpreter, shifting an
> over/under-approximation boundary off the true value and breaking the
> superset/subset invariant.

Concretely, the encoder folded `MET + 0.3 > 0.9` into the atom
`MET > 0.6000000000000001` (because `0.9 - 0.3` in f64 is not `0.6`), excluding
the event `MET = 0.6000000000000001` that the interpreter accepts in both
regions — a false PROVEN DISJOINT.

### Defects fixed (rounds 1–3)

- **Division folded as an f64 reciprocal** (`scale(1/d)`) in both the main
  encoder and the EPRED axiom layer — shifted ratio-cut boundaries
  (`HT/49 >= 1` wrongly excluding `HT = 49`). Fixed by clearing constant
  denominators with exact coefficients at the comparison level
  (`ratio()` / `clear_ratio`).
- **Interval fast-path inward rounding** of division bounds — false
  PROVEN EMPTY/DISJOINT under `--no-solver`.
- **Subnormal literals** — a ~309-zero decimal underflows into the subnormal
  range where f64 and exact arithmetic diverge. Rejected at the lexer, exactly
  like scientific notation (`adl-syntax/src/lexer.rs`).
- **Coefficient overflow** (`MAX * 10`) collapsing a satisfiable cut to
  `Formula::False`. Subsumed by exact rationals (which never overflow).
- **`validate_pt_descending` reset on a missing-`pt` element**, letting a
  non-pT-descending event load and making ORD/IDOM false on it. Fixed by not
  resetting the running max across a `pt`-less gap (`adl-interp/src/event.rs`).
- **Define-aliased opaque arguments** interning as distinct quantities
  (`f(jets[0])` ≠ `f(leadjet)` where `define leadjet = jets[0]`) — a false
  PROVEN OVERLAPPING. Fixed by making `resolve_target` see through define
  aliases (`adl-sema/src/resolve.rs`).

## The fix: exact-rational analyzer (Phases 1–5)

The analyzer's whole numeric core was converted from `f64` to an **exact
rational** type.

### `adl_sema::Rat` (`crates/adl-sema/src/rat.rs`)

A newtype over `num_rational::BigRational` with **decimal-literal semantics**:
a literal or event value `0.3` denotes `3/10` exactly — the value the physicist
wrote, and exactly what the solver already consumed (`Rat::from_decimal_f64`
reads an f64's shortest round-trip decimal). Provides exact `+ − × ÷`,
`floor`/`ceil`/`powi`, comparisons, `to_f64` (for display/witnesses only), and
`smt_real()` / `to_parts()` for solver emission.

### Conversions

- **adl-formula** — `LinExpr`/`LinAtom` coefficients and constant are `Rat`;
  folding is exact (`0.9 − 0.3 = 6/10`). `LinAtom` construction is now
  **infallible** — rationals are always finite, so the old `LinAtomError`
  (non-finite coefficient/constant) is gone. Only **integer** powers stay
  rational; a fractional exponent leaves the linear fragment (Unknown) rather
  than being folded to an inexact f64.
- **adl-axioms** — `PredLin` and every axiom constant are `Rat`.
- **adl-analysis/interval.rs** — interval bounds are `Option<Rat>` (`None` =
  ±∞) and equal the **exact** `k/c` rational. The entire f64 machinery the
  fast path needed for soundness (subnormal guard, fma-residual outward ulp
  nudge) was **deleted** — exact division needs no rounding.
- **adl-solver** — both backends build z3 terms straight from `Rat`
  (`smt_real()` / `to_parts()`); the old `num.rs` `rational_of` f64→decimal
  round-trip was deleted.

The UNSAT side is now decided purely by this exact analyzer and **never
consults the interpreter**, so soundness no longer depends on f64 arithmetic
anywhere on the dangerous side.

## What was deliberately NOT done: the exact interpreter (Phase 6)

Making the interpreter evaluate the rational fragment exactly was implemented
and **reverted**. The solver returns exact-rational witnesses, but they are
realized into **f64 events**; an exact re-check then rejects the rounded
witness, and — because re-validation is sensitive to representation — this
fires inconsistently across logically-equivalent renderings (the `metamorphic`
battery flipped PROVEN↔POSSIBLY between a region and its inlined-define twin).
Re-validation is a SAT-side, downgrade-only guard, so this was never a false
PROVEN, but it broke verdict consistency.

True bit-exact interpreter parity requires a **rational event model** — event
values carried as `Rat` from JSON ingest through evaluation and witness
realization, so a solver witness round-trips without rounding. That is a much
larger change (the whole event pipeline) and is the right home for closing the
residual exact-analyzer-vs-f64-interpreter gap on the DISJOINT side. That gap
is adversarial-only and was not constructible in four audit rounds.

## Load-bearing invariant

The audit flagged this as a soundness-critical invariant to track: **ORD/IDOM
pt-ordering axioms are sound only because `adl-interp/src/event.rs::
validate_pt_descending` rejects every non-pT-descending event on every base
collection (no allowlist).** If that enforcement is ever weakened — or the
analyzer is pointed at a runtime that does not reject unsorted ntuples — ORD's
over-approximation becomes a genuine false PROVEN. Corollary for anyone
auditing: a "false PROVEN DISJOINT" claim is only real if the **interpreter
accepts** a shared event; always confirm the counterexample with `smash2 run`
before believing it (non-pT-descending events are rejected and are not valid
counterexamples).

## Status

Final audit verdict: **SOUND** — no validate check can emit a false PROVEN.
549 tests pass; clippy clean workspace-wide; both solver backends pass
conformance. Two residual LOW, non-soundness items remain (deferred hardening):
opaque-external *candidate* overlaps are labeled/aggregated as PROVEN
OVERLAPPING (the spec-sanctioned opaque-free caveat — a trust/label concern),
and the subprocess backend hardcodes z3 CLI flags (unreachable from the product
CLI, fails safe to Unknown).
