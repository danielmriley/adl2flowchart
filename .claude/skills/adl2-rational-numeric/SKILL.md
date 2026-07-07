---
name: adl2-rational-numeric
description: The exact-rational numeric core of the ADL2 (smash2) analyzer — the `adl_sema::Rat` type (newtype over num_rational::BigRational) that replaced f64 in every cut/atom that feeds a PROVEN verdict. Use this whenever touching numeric values on the analysis side: LinExpr/LinAtom coefficients or constants (adl-formula), axiom/PredLin constants (adl-axioms), interval bounds (adl-analysis/interval.rs), the solver numeral emission (adl-solver smt_real/to_parts), parsing a numeric literal into the IR (parse_rat / from_decimal_f64), or adding a new arithmetic operation over cut values. Trigger on any mention of Rat, BigRational, num-rational, exact/rational arithmetic, decimal-literal semantics, f64 constant-folding/boundary/rounding bugs, "0.3 is 3/10", or a false PROVEN traced to a shifted cut boundary. Read alongside adl2-soundness for the verdict contract.
allowed-tools: Read Edit Write Bash Grep Glob
---

# ADL2 exact-rational numeric core (`Rat`)

The analyzer's cut/atom numeric core is **exact rationals**, not `f64`. This
killed a recurring class of false-PROVEN defects (4 audit rounds): f64
constant-folding / division / reciprocal shifting a cut boundary off the
interpreter's, fabricating a disjointness proof. Paths are relative to
`reimplementation/adl2/` unless absolute.

## The type

`adl_sema::Rat` — newtype `pub struct Rat(BigRational)` in
`crates/adl-sema/src/rat.rs`. Re-exported as `adl_sema::{Rat, RatParts}`
(see `crates/adl-sema/src/lib.rs:34`). Derives `Eq, Ord, Hash` (total — no
NaN), `Clone`, `Default` (= zero).

**Decimal-literal semantics.** A literal or value `0.3` denotes **3/10
exactly** — the value the physicist wrote and exactly what the solver
consumes. This matches the legacy solver's old `rational_of`. Folding cut
arithmetic over `Rat` is therefore exact: `0.9 - 0.3` is `6/10`, **not** the
f64 `0.6000000000000001`. (Tests `the_additive_boundary_folds_exactly`,
`division_is_exact_and_guards_zero` in rat.rs.)

API surface (all `#[must_use]`):
- Constructors: `zero()`, `one()`, `from_i64(i64)`, `from_decimal_f64(f64) -> Option<Self>`.
- Arithmetic on `&Rat`: `Add Sub Mul Neg` (operator impls), `checked_div(&Rat) -> Option<Rat>` (guards /0), `powi(i32) -> Option<Rat>` (None only for `0^neg`).
- Predicates: `is_zero is_one is_negative is_positive is_integer signum()->i32 abs floor ceil`.
- Egress: `to_i64()->Option<i64>` (exact, integer-in-range only), `to_f64()->f64` (lossy — display/JSON/histogram only, **never** a soundness path), `smt_real()->String`, `to_parts()->RatParts {negative, numerator, denominator}` (lowest terms, denom>0).

## `from_decimal_f64` — the shortest-decimal bridge

`from_decimal_f64(v)` returns `None` for non-finite `v`; otherwise it reads
the **shortest round-trip decimal** of `v` (Rust's `format!("{v}")`, which is
shortest round-trip and never scientific) and parses `int[.frac]` totally:
`0.3 → 3/10`, `100.0 → 100`, `-1.5 → -3/2`, `f64::MAX → ~309-digit integer`.
It does **not** read the f64's exact dyadic value (that would give
`5404319552844595/2^54` for 0.3) — it reads what was *written*. This is the
whole point: it round-trips to the same decimal the interpreter and solver
see.

## Ingest path — `parse_rat`

Numeric literals enter the IR through
`parse_rat(s)` in `crates/adl-formula/src/encode.rs:889`:

```rust
fn parse_rat(s: &str) -> Option<Rat> {
    s.parse::<f64>().ok().and_then(Rat::from_decimal_f64)
}
```

`adl-axioms` uses the same `parse::<f64>().ok().and_then(Rat::from_decimal_f64)`
shape for range/bound literals (`crates/adl-axioms/src/lib.rs:805-806,922`);
axiom *code* constants go through `Rat::from_decimal_f64(v).expect(...)`
(lib.rs:430) since they are known-finite literals. Same `.expect("finite
constant")` pattern in `crates/adl-analysis/src/engine.rs:55`.

## Load-bearing invariant: NO f64 in an atom or a UNSAT-side constant

Everything that feeds a PROVEN proof is `Rat`:
- `LinAtom { terms: Vec<(Rat, QuantityId)>, rel, constant: Rat }` and `LinExpr` (`crates/adl-formula/src/lin.rs`). `LinAtom::new` is **infallible** — rationals never go non-finite, merge is exact. The old `LinAtomError` / fallible construction is **gone**; do not reintroduce a `Result` here.
- `PredLin` + axiom constants are `Rat` (`crates/adl-axioms/src/lib.rs`).
- Interval bounds: `Iv { lo: Option<Rat>, hi: Option<Rat>, lo_strict, hi_strict }` (`crates/adl-analysis/src/interval.rs:17`). `None` = ±inf; a present bound is the **exact** `k/c` rational. The old f64 subnormal-guard + fma-residual ulp-nudge machinery was **deleted** — exact bounds need no nudging. Do not re-add ulp slop.
- Solver numerals are built straight from `Rat`: `smt_real()` / `to_parts()` (`crates/adl-solver/src/subprocess.rs:100,108`, `native.rs:79`). The old `num.rs` / `rational_of` f64→decimal round-trip is **deleted**.

If you find an `f64` literal or fold on the analysis side feeding a cut
boundary, an atom coefficient/constant, an axiom constant, or an interval
bound, that is the bug — convert it through `parse_rat` / `from_decimal_f64`
and keep it `Rat` end to end. f64 is allowed only for genuinely irrational
quantities (`sqrt`, `dR`/angular separations, opaque functions) which the
analyzer already treats as **opaque** — no PROVEN verdict rests on them.

## Adding a new operation over cut values

1. Add it as a method on `Rat` in `rat.rs`, exact. If it can be non-finite
   (division, negative power, log…), return `Option<Rat>` and have the
   caller treat `None` as **opaque / drop the cut** — never substitute a
   default. Mirror `checked_div` (guards /0) and `powi` (None for `0^neg`).
2. A genuinely irrational op (sqrt, trig) has **no** rational value: do not
   fake one. Keep the operand symbolic/opaque exactly as the existing code
   does (encode.rs returns `LinErr::NonLinear` for a non-constant power at
   line ~880); the analyzer masks it and no PROVEN rests on it.
3. Never round, snap, or ulp-nudge a `Rat`. Exactness is the whole contract.

## Gotcha: the INTERPRETER is still f64

`adl-interp` event values are `f64` (`Event.weight: f64`, object props `f64`,
`validate_pt_descending` compares `f64` in `crates/adl-interp/src/event.rs`).
Making the interpreter evaluate the rational fragment exactly was **tried and
REVERTED**: z3 returns exact-rational witnesses that realize through f64
events, so an exact re-check rejects the rounded witness inconsistently across
equivalent renderings — the metamorphic battery flips PROVEN↔POSSIBLY. It is
SAT-side downgrade-only (never unsound) but breaks verdict consistency. True
parity needs a rational EVENT MODEL (event values as `Rat` end to end). See
`docs/EXACT_RATIONAL_PLAN.md`. Until then: **analysis is exact, witness
re-validation is f64** — a witness must round-trip through the f64 interpreter,
so do not assume the interpreter sees the exact rational the solver produced.

## Verify a numeric change

```bash
cargo test -p adl-sema   --manifest-path reimplementation/adl2/Cargo.toml   # Rat unit tests
cargo test -p adl-formula --manifest-path reimplementation/adl2/Cargo.toml  # LinAtom/encode
cargo test -p adl-axioms  --manifest-path reimplementation/adl2/Cargo.toml
```

Then the full native battery (~549 tests) for encoder/interpreter parity — see
**adl2-build-test** for the libz3 workaround (`RUSTFLAGS="-L
native=/tmp/z3lib" LD_LIBRARY_PATH=/tmp/z3lib`, or the subprocess CLI `cargo
build --release -p adl-cli --no-default-features` using `z3`).
Any verdict change must be checked against **adl2-soundness** (the PROVEN
DISJOINT count must not move unintentionally) and **adl2-corpus-sweep**.

## Cross-references

- **adl2-soundness** — the verdict contract; why a shifted boundary on the
  UNSAT side is a silent false PROVEN, and the rule that a "false PROVEN
  DISJOINT" claim is only real if the **interpreter accepts** the shared event
  (run the counterexample through `smash2 run`; non-pT-descending events are
  rejected and are NOT valid counterexamples).
- **adl2-build-test** — the no-libz3 build/test environment.
