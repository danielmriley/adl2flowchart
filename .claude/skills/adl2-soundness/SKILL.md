---
name: adl2-soundness
description: The soundness contract, bug taxonomy, and verification methodology for the ADL2 overlap / disjointness / subset / vacuity analysis. Use this whenever reviewing, modifying, or debugging anything that can affect a PROVEN verdict — the verifier (adl-analysis engine/encode/witness), the formula encoder (adl-formula), the axiom catalog (adl-axioms), the witness re-validation layer, or the three-valued interpreter (adl-interp eval.rs) — even if the user only says "fix this encoder bug" or "add an axiom" without mentioning soundness. Trigger on any change touching over/under-approximation, UNSAT-direction proofs, solver result classification, Kleene/three-valued membership, opaque-quantity masking, or the PROVEN-DISJOINT regression count. Default to skepticism: a single wrong fact on the UNSAT side fabricates a false PROVEN with no safety net.
allowed-tools: Read Edit Write Bash Grep Glob
---

# ADL2 soundness contract and verification playbook

This skill governs any change that can affect a **PROVEN** verdict. The core
discipline: **soundness direction is a type.** Over-approximations feed the
UNSAT (disjoint/subset/empty) proofs; under-approximations feed the SAT
(overlap) proofs; anything outside the checked fragment is an explicit
`Unknown` that can only weaken a verdict to POSSIBLY, never flip it.

Paths below are relative to `reimplementation/adl2/` unless absolute.

## The contract (verdict table)

| Verdict | Query | Approximation | Safety net |
|---|---|---|---|
| PROVEN DISJOINT | `UNSAT(Ax ∧ A⁺ ∧ B⁺)` | OVER (`A⁺`, `B⁺`) | **none** — dangerous side |
| PROVEN SUBSET A⊆B | `UNSAT(Ax ∧ A⁺ ∧ ¬B⁻)` | OVER outer / UNDER inner | **none** — dangerous side |
| region EMPTY (vacuous) | `UNSAT(Ax ∧ R⁺)` | OVER | **none** — dangerous side |
| PROVEN OVERLAPPING | `SAT(Ax ∧ A⁻ ∧ B⁻)` + witness | UNDER (`A⁻`, `B⁻`) | interpreter re-validates the witness |
| anything else | — | — | downgrades to POSSIBLY / Unknown |

The polarity is enforced in the types (`Over` vs `Under` projections) in
`crates/adl-analysis/src/engine.rs` — see the module header (lines 6-19) and the
labelled steps: disjointness `UNSAT(Ax ∧ A⁺ ∧ B⁺)` (~line 437), subset
`self.subset(&c1.overs, &c2.unders)` (~line 454), overlap
`SAT(Ax ∧ A⁻ ∧ B⁻)` + re-validation (~line 481).

**The dangerous side is every UNSAT direction (disjoint / subset / empty).**
There is no witness there: one wrong fact yields a false PROVEN silently.
The overlap (SAT) side is re-validated by the reference interpreter — *the
interpreter is the meaning.* If the verifier and the interpreter disagree on a
satisfying event, that is a release-blocking bug, not a tuning knob.

## The exact-rational numeric core (no f64 in atoms)

The whole analyzer numeric core is **exact rationals**, not f64. The type is
`adl_sema::Rat` (`crates/adl-sema/src/rat.rs`), a newtype over
`num_rational::BigRational` with **decimal-literal semantics**: a literal or
event value `0.3` denotes `3/10` *exactly* — the shortest round-trip decimal of
the `f64` (`Rat::from_decimal_f64`, returns `None` only for non-finite, which
cannot construct an atom). This matches what the physicist wrote and exactly
what the solver consumes (`Rat::smt_real()` / `to_parts()`).

Why it is load-bearing for soundness: f64 constant-folding / division /
reciprocal silently shifts a cut boundary off the interpreter's, opening a gap
that fabricates a false PROVEN DISJOINT/EMPTY/SUBSET (four audit rounds were the
same defect class). With `Rat`, `0.9 - 0.3` folds to `6/10`, **not** the f64
`0.6000000000000001`, so the analyzer's boundary and the solver's coincide.

What is exact vs what is not:
- **Exact (the rational fragment):** `LinExpr`/`LinAtom` coefficients and the
  RHS constant are `Rat` (`crates/adl-formula/src/lin.rs`); folding `+ - * /`
  is exact. `adl-axioms` `PredLin` + axiom constants are `Rat`. Interval bounds
  `Iv.lo`/`Iv.hi` are `Option<Rat>` (`None` = ±∞), each bound the **exact `k/c`
  rational** (`crates/adl-analysis/src/interval.rs`).
- **Not rational → stays opaque:** `sqrt`, angular separations (`dR`/`dPhi`),
  and opaque functions have no rational value; callers keep them in f64 and the
  analyzer already treats them as opaque, so no PROVEN verdict rests on them.

Consequences for anyone touching this core:
- **`Rat` atom construction is INFALLIBLE.** Rationals are always finite, so the
  old `LinAtomError` / non-finite rejection is GONE — do not reintroduce a
  fallible-construction path or an f64 intermediate in atom/interval building.
- The encoder entry point is `parse_rat(s) = s.parse::<f64>().ok()
  .and_then(Rat::from_decimal_f64)` (`adl-formula/src/encode.rs`). New numeric
  literals must route through it, never through bare f64 arithmetic.
- The old f64 machinery was **deleted**, not disabled: `interval.rs`'s
  subnormal-guard + fma-residual ulp-nudge, and the solver's `num.rs` /
  `rational_of` f64→decimal round-trip. Do not resurrect them — the `Rat` path
  is exact by construction and a nudge would now *widen* an interval unsoundly.
- Subnormal literals are rejected at the lexer (`adl-syntax/src/lexer.rs`,
  alongside scientific notation): they are the one place f64 and its shortest
  decimal diverge. Keep that rejection.

## Where the soundness-critical code lives

- `crates/adl-analysis/src/engine.rs` — orchestrates the queries; enforces
  Over/Under polarity in the types; bounded witness retry (`MAX_WITNESS_ATTEMPTS = 6`)
  before downgrade to POSSIBLY; unsat-core → source-span mapping.
- `crates/adl-analysis/src/encode.rs` — region → formula, Over/Under projection.
- `crates/adl-analysis/src/witness.rs` — `validate_witness` (line 47);
  calls `interp.eval_region_membership_idx` (line 84). This is the SAT-side net.
- `crates/adl-analysis/src/interval.rs` — interval fast path (verdicts capped
  at POSSIBLY without a solver).
- `crates/adl-formula/src/{formula,encode,lin}.rs` — quantity formula IR, the
  comparison/relational encoding, linear-atom normalization.
- `crates/adl-axioms/src/lib.rs` — the background-fact CATALOG. Every fact here
  is asserted into UNSAT proofs, so every fact must be *true of every physical
  event*. `pt_ordered` (line 450), ORD emit (~line 485), `idom` / IDOM
  (~line 692, guard `if !self.pt_ordered(parent)` ~line 700).
- `crates/adl-interp/src/eval.rs` — the reference interpreter and the
  three-valued layer: `enum Tri` (line 443), `region3` (574), `truth3` (617),
  `num3` (744), entry `eval_region_membership` (229).
- `crates/adl-solver/src/subprocess.rs` — `classify()` (line 212): maps solver
  output to `SatResult`. `crates/adl-solver/src/native.rs` — primary libz3 path.
- `crates/adl-difftest/` — the differential property oracle (encoder vs
  interpreter), the real guard against divergence.

## Bug taxonomy — every one of these produces a *false PROVEN*

1. **Wrong-polarity cut handling.** A dropped or `Unknown` cut handled on the
   wrong side so a region's encoding is wrongly *strengthened* (an `A⁺` that
   should have stayed weak, or an `A⁻` that got extra constraints). Strengthening
   the over-approx fabricates disjointness; strengthening anything inside an
   UNSAT proof fabricates the proof. Audit in `encode.rs` / `engine.rs`.

2. **Unsound axioms.** Over-strong background facts asserted into UNSAT proofs
   (`adl-axioms/src/lib.rs`). Real example fixed this session: ORD/IDOM
   asserted pT-descending order on **filtered-UNION** collections, but the
   interpreter *concatenates* union parts without a pT-merge — so the union is
   not globally pT-descending. Fix: `pt_ordered` must walk the filter chain to a
   `Collection::Base` *transitive root*; a `Filtered` over a UNION is not
   pT-ordered. Any new fact needs a one-line justification of why it holds for
   every physical event under every reading.

3. **Solver `unknown`/`timeout`/`unsupported`/`(error …)` mapped to UNSAT.**
   That fabricates PROVEN. `subprocess.rs::classify()` already maps all four to
   `SatResult::Unknown` (Audit-Bug-5; tests `error_output_is_unknown`,
   `unsupported_command_is_unknown_not_the_answer`). Never weaken this; preserve
   the same discipline in `native.rs`.

4. **Opaque-quantity masking in witness validation** — see next section.

5. **Encoder/interpreter divergence** — guarded by `adl-difftest`'s
   property oracle. If the encoder accepts an event the interpreter rejects (or
   vice versa) the SAT-side net is breached.

6. **f64 boundary drift (NOW STRUCTURALLY PREVENTED).** Before the exact-rational
   core, f64 constant-folding/division/reciprocal shifted a cut boundary off the
   interpreter's, fabricating a false PROVEN DISJOINT/EMPTY/SUBSET — the dominant
   defect class across four audit rounds. It is now prevented by construction
   (atoms/intervals are `Rat`). **The regression is reintroducing any f64 into
   the rational fragment** (atom coefficients, interval bounds, axiom constants,
   the encoder's literal path). Treat a new f64 there as a soundness bug, not a
   style nit.

## The opaque-masking invariants (subtle — cost 5 verification rounds)

Witness validation must use **non-short-circuiting three-valued (Kleene)**
membership so a *decidable False is never hidden behind an Unknown*. "Unknown"
arises from opaque externals with no reference interpretation (e.g. `sum`,
`bdt`) or missing data. It lives in `adl-interp/src/eval.rs` as
`region3` / `truth3` / `num3` (+ the `Tri` enum), reached via
`Interp::eval_region_membership`, and consumed by `witness.rs::validate_witness`.

Invariants that layer must keep — verify after any edit to `eval.rs`:

- Prefer a decidable **False over Unknown**: across statements, inside
  `and` / `or` / `not` / ternary, across `Inherit` edges, and across `<region>`
  (`RegionPred`) references in **both boolean and numeric position** (`num3`).
- §4.4 **soft non-value is ABSORBING** — it wins over a blocking opaque operand
  in `Cmp` and `Binary` (a div-by-zero / missing comparison evaluates to a
  decidable False, not Unknown).
- An undecidable ternary guard is still **decidable when both branches agree**.
- **Documented residual:** comparison-over-ternary distribution is *not* done
  (e.g. `(opaque ? missing : 5) > 1000`). This is strictly conservative — it can
  only weaken to POSSIBLY / a caveated candidate, never fabricate a pass. Do not
  "fix" it by adding distribution unless you re-run the full adversarial battery;
  the risk is reintroducing a masking path.

## pt-ordering: a counterexample is only real if the interpreter accepts it

The ORD/IDOM pt-ordering axioms (`adl-axioms/src/lib.rs`) are sound **only
because** `adl-interp/src/event.rs::validate_pt_descending` (line 538) REJECTS
every non-pT-descending event on **every base collection** — there is no
allowlist. So a "false PROVEN DISJOINT" claim is real **only if the interpreter
ACCEPTS a shared event**. Any such claim MUST be checked by running the
counterexample event through `smash2 run`: a non-pT-descending event is rejected
(`EventError::NotPtDescending`) and is **not a valid counterexample**. Three
separate audit rounds wrongly flagged a false-PROVEN that evaporated once the
event was run through the interpreter. Always do this check before escalating.

(Note: `validate_pt_descending` skips elements with no `pt` *without* resetting
`prev` — a missing-pt element does not reset the descending chain. Do not
"fix" this into a reset; it would weaken the ordering the axioms rely on.)

## Phase 6: the exact interpreter was tried and REVERTED

Making `adl-interp` evaluate the rational fragment exactly (to close the residual
exact-analyzer-vs-f64-interpreter gap) was implemented and then **reverted**. It
broke witness re-validation: z3 returns exact-rational witnesses, but they realize
through **f64 events**, so an exact re-check rejects the rounded witness — and it
fires inconsistently across logically-equivalent renderings (the `metamorphic`
battery flips PROVEN ↔ POSSIBLY). It is SAT-side, downgrade-only, so it never
made a verdict *unsound*, but it broke verdict *consistency*. True bit-exact
parity needs a **rational event model** (event values carried as `Rat` end to
end: JSON ingest → `Event` → eval → witness realization). That is a whole-pipeline
change and the right home for Phase 6 if the DISJOINT-side residual ever needs
closing (it is adversarial-only). See `docs/EXACT_RATIONAL_PLAN.md` §"Phase 6
deferred". Do NOT re-attempt exact interpretation without the rational event model.

## Key regression invariant

Across the **68-file corpus** (`examples/`,
68 `*.adl`), the **PROVEN DISJOINT count must not change unless you intend it.**
Removing or weakening an axiom can only **reduce** disjoint proofs. An
**increase** is a red flag for a newly-introduced unsound fact — investigate
before committing. (See the **adl2-corpus-sweep** skill for running the sweep and
diffing verdict counts.)

## How to verify a soundness fix

This is the methodology that actually caught the subtle instances. Do not stop
at the first green test — each round found a subtler member of the same class.

**(a) Unit-test the owning crate directly** (no z3 needed for adl-formula /
adl-axioms / adl-interp):

```bash
cargo test -p adl-axioms --manifest-path reimplementation/adl2/Cargo.toml
cargo test -p adl-formula --manifest-path reimplementation/adl2/Cargo.toml
cargo test -p adl-interp --manifest-path reimplementation/adl2/Cargo.toml
```

**(b) End-to-end on crafted adversarial ADL** with the CLI (binary `smash2`):

```bash
# build once (see adl2-build-test for the libz3 workaround if native fails)
cargo build -p adl-cli --manifest-path reimplementation/adl2/Cargo.toml
# --explain shows the proof chain (unsat cores, per-axiom statements, witnesses)
reimplementation/adl2/target/debug/smash2 verify --explain /path/to/adversarial.adl
```

Craft the input to *attempt* a false PROVEN: a filtered-union region that "looks"
disjoint only via the axiom you touched, an opaque-quantity region for the
masking class, a region whose disjointness hinges on a solver timeout.

**(c) Run the FULL native battery** — the difftest oracle is the real guard
against encoder/interpreter divergence:

```bash
cargo test --workspace --manifest-path reimplementation/adl2/Cargo.toml
# deep encoder-vs-interpreter property battery (100k cases vs the default 2000):
cargo test -p adl-difftest --features deep --manifest-path reimplementation/adl2/Cargo.toml
```

If the native (libz3) link fails, see the **adl2-build-test** skill for the
workaround; the subprocess backend (`--no-default-features` on adl-solver) is the
fallback the CI second job uses.

**(d) Adversarial verification.** Spawn independent skeptics whose only job is to
either construct an input that *still* triggers the bug, or one that the fix now
*breaks* (a real overlap the fix downgraded to POSSIBLY is also a failure —
soundness must not cost all precision). Default to skepticism. The fix is not
done until the skeptics cannot break it. Round after round found subtler
instances of the same class — budget for at least two adversarial passes.

## Cross-references

- **adl2-build-test** — building the workspace, the libz3 linking workaround,
  running the test suites and the two solver-backend jobs.
- **adl2-corpus-sweep** — running `scripts/corpus_gate.sh` and the full
  `smash2 verify` sweep over `examples/`, and diffing verdict counts to enforce
  the PROVEN DISJOINT regression invariant.
