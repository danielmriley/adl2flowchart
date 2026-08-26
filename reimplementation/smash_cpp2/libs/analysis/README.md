# `adl2_analysis`

Verify / pairwise analysis (smash3 `adl-analysis`). Must not parse.
smash2_cpp `cpp/libs/analysis` is the algorithm reference. smash3
`verify` is the pairwise-summary oracle.

P4 fills:

- Interval fast path (exact `Rat` bounds, And-spine only)
- Statement-granularity encode (inherit flattened)
- Subprocess-solver pairwise: `UNSAT(Ax ∧ A⁺ ∧ B⁺)` disjoint (uncertified
  solver-UNSAT is **CANDIDATE DISJOINT**), subset flags via
  `UNSAT(Ax ∧ A⁺ ∧ ¬(B⁻))` with `¬(B⁻)` one Or of NNF negations (fail-closed
  under certify), SAT unders + Kleene `region3` witness → **PROVEN OVERLAPPING**
  only if both regions accept the realized event; otherwise Candidate / Possibly.
  Oriented twin pairs (OPEN-2) cap SAT-direction overlap at POSSIBLY.
  UNKNOWN only when the solver is inconclusive in **both** directions; a SAT-side
  unknown or an encoding-gap Unsat is POSSIBLY, never a certified claim.
- Compact `dump_verdicts` (`A vs B: KIND`)

Sampling/refute gates, `--cross` reconciliation (XSUB/XEQ), and
smash2-schema `verify --json` stay later units. `--combine` writes
`smash2-combine/2` bundles. Recheck is the `smash_cpp2-recheck` binary.

Headers: `libs/analysis/include/adl2/analysis/`
