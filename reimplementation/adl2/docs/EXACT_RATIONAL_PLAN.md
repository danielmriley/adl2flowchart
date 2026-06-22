# Exact-rational cut arithmetic (airtight PROVEN)

## Problem
`verify` proves region relations over exact-real arithmetic (z3 already
consumes coefficients as exact decimal rationals via `adl-solver/src/num.rs`),
but two places still use stepwise `f64`:
1. the **encoder** folds constants in `f64` (`0.9 - 0.3 → 0.6000000000000001`
   instead of `6/10`), so the atom boundary is off by ulps;
2. the **interpreter** evaluates cuts in `f64`.

At a non-dyadic boundary the two disagree, fabricating a false PROVEN
DISJOINT/EMPTY/SUBSET (the UNSAT side has no witness re-validation net).
Round-3 repro: `region A: MET + 0.3 > 0.9` vs `region B: MET in [0.6,
0.6000000000000001]` → false PROVEN DISJOINT (interpreter accepts MET =
0.6000000000000001 in both).

## Semantics (the contract)
A numeric literal/value denotes its **shortest round-trip decimal** as an
exact rational — `0.3 = 3/10`, exactly the value the physicist wrote and
exactly what the solver already uses (`rational_of`). Cut arithmetic over the
**rational fragment** (`+ - * /` of event scalars, element properties, sizes,
and rational literals) is exact. Irrational ops (`sqrt`, `dPhi`/`dEta`/`dR`,
opaque external functions) are NOT rational; the analyzer already treats them
as opaque (no PROVEN on the UNSAT side), so the interpreter evaluates them in
`f64` and a comparison touching one falls back to `f64` (no PROVEN to
contradict).

## Design
- New `adl_sema::Rat` (newtype over `num_rational::BigRational`): exact
  rational with `from_decimal_f64` (shortest-decimal → rational, matching
  `rational_of`), `to_f64_nearest`, and directional `to_f64_round_{up,down}`
  for the interval fast path. Lives in `adl-sema` (the common base of
  formula/axioms/solver/analysis/interp).
- **adl-formula**: `LinExpr`/`LinAtom` coefficients + constant become `Rat`;
  all folding (`combine`/`scale`/`ratio`/`abs`/`band`) is exact. `Rel::eval`
  over `Rat`.
- **adl-axioms**: `PredLin` and axiom constants become `Rat`.
- **adl-solver**: build z3 terms straight from `Rat` (numer/denom); drop the
  `f64 → decimal` round-trip. Model extraction returns `Rat` witnesses.
- **adl-analysis/interval**: bounds as `Rat` (exact), or `f64` rounded
  OUTWARD from the `Rat` bound (sound superset) for the fast comparison.
- **adl-interp/eval**: `num` returns `Num { Exact(Rat), Approx(f64) }`;
  rational leaves/ops stay `Exact`, irrational ops degrade to `Approx`. A
  comparison of two `Exact` is exact (matches the analyzer); any `Approx`
  side compares in `f64`.

## Phases (each: build + test + clippy before next)
1. `Rat` type + decimal conversions + unit tests.
2. adl-formula → `Rat`.
3. adl-axioms → `Rat`.
4. adl-solver → consume/emit `Rat`.
5. adl-analysis interval → `Rat`/outward-rounded.
6. adl-interp `num` → exact/approx.
7. Verify defect #1 repro fixed; full battery green; clippy; re-run audit.

## Outcome
Phases 1–5 landed: the **analyzer** (encoder, axioms, interval, solver) now
reasons in exact rationals end to end. Every demonstrated false-PROVEN defect
(rounds 1–3, including the additive-boundary #1) is fixed, because the UNSAT
side (PROVEN DISJOINT/EMPTY/SUBSET) is decided purely by the exact analyzer and
never consults the interpreter. 549 tests green, clippy clean.

## Phase 6 deferred (exact interpreter)
Making the interpreter evaluate the rational fragment exactly was implemented
and then **reverted**: it broke witness re-validation. The solver returns
exact-rational witnesses, but they are realized into `f64` events; an exact
re-check then rejects the rounded witness, and this fires inconsistently across
logically-equivalent renderings (the `metamorphic` battery: a witness that
re-validates in one form is a candidate-overlap in another → PROVEN vs
POSSIBLY). The re-validation is a SAT-side, downgrade-only guard, so this never
fabricated a false PROVEN — but it broke verdict consistency.

True bit-exact interpreter parity requires a **rational event model**: event
values carried as `Rat` end to end (JSON ingest → `Event` → eval → witness
realization), so a solver witness round-trips without rounding. That is a much
larger change (the whole event pipeline) and is the right home for Phase 6 if
the residual exact-analyzer-vs-f64-interpreter gap on the DISJOINT side ever
needs closing. That gap is adversarial-only and was not constructible here.
