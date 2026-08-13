# `adl2_analysis`

Verify / pairwise analysis (Rust `adl-analysis`). Must not parse.

P4 fills:

- Interval fast path (exact `Rat` bounds, And-spine only)
- Statement-granularity encode (inherit flattened)
- Subprocess-solver pairwise: `UNSAT(Ax ∧ A⁺ ∧ B⁺)` disjoint, subset flags,
  SAT unders → **CANDIDATE OVERLAPPING** (witness realization not ported)
- Compact `dump_verdicts` (`A vs B: KIND`)

Not claimed: independent Farkas certification, reconciliation, sampling/refute
gates, `--combine` bundles, PROVEN OVERLAPPING (needs region3 witness
re-validation of a realized model).

Headers: `libs/analysis/include/adl2/analysis/`
