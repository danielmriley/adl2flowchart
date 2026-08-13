# `adl2_analysis`

Verify / pairwise analysis (Rust `adl-analysis`). Must not parse.

P4 fills:

- Interval fast path (exact `Rat` bounds, And-spine only)
- Statement-granularity encode (inherit flattened)
- Subprocess-solver pairwise: `UNSAT(Ax ∧ A⁺ ∧ B⁺)` disjoint, subset flags,
  SAT unders + Kleene `region3` witness → **PROVEN OVERLAPPING** only if both
  regions accept the realized event; otherwise Candidate / Possibly
- Compact `dump_verdicts` (`A vs B: KIND`)

Not claimed: independent Farkas certification (kernel exists, not wired),
reconciliation, sampling/refute gates, `--combine` bundles.

Headers: `libs/analysis/include/adl2/analysis/`
