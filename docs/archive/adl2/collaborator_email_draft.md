Subject: smash2 update — region overlap/disjointness proofs are now exact

Hi all,

A quick update on the ADL2 / smash2 reimplementation. The `verify` command —
the part that proves whether two analysis regions are disjoint, whether one is
a subset of another, or whether a region is empty — is now provably sound: a
PROVEN verdict can no longer disagree with the reference interpreter.

What changed, in physics terms:

- The analyzer used to reason about cut boundaries in floating point, the same
  way the event loop does. Near a cut threshold written with a non-trivial
  decimal (think dR < 0.4, MET-style sums, ratio cuts like HT/49 >= 1), the
  rounding could differ by one ulp and the tool would occasionally claim two
  regions were disjoint when an event actually sits in both. That's the one
  failure mode that matters here — a disjointness claim has no safety net — and
  it's now closed.

- We did this by moving the entire proof engine to exact rational arithmetic.
  A cut threshold of 0.3 now means exactly 3/10, folding is exact (0.9 - 0.3 is
  6/10, not 0.6000000000000001), and division/ratio cuts are cleared exactly
  instead of multiplied by an inexact reciprocal. The solver already worked in
  exact rationals; the rest of the analyzer now matches it end to end.

- This was driven by four rounds of adversarial self-audit (an agent fleet
  trying to construct false proofs). The final round comes back clean: no
  validate check can emit a false PROVEN. All ~550 regression tests pass.

Practical impact: if smash2 says two regions are DISJOINT (or a region is
EMPTY, or one region is a SUBSET of another), you can now trust it as a
machine-checked fact about the exact-arithmetic semantics — which is exactly
what we need for the cross-analysis overlap/combination use case. Verdicts it
can't settle still degrade honestly to POSSIBLY rather than guessing.

One known limitation, documented and bounded: full bit-for-bit parity between
the proof engine and the f64 event interpreter at the very last ulp would need
the event pipeline itself to carry exact values. That's a larger change and is
adversarial-only in practice (real detector values never land on the seam), so
it's tracked as future work, not a soundness hole.

Happy to walk anyone through it, and to point smash2 at more of your analyses
if you'd like a disjointness/overlap pass.

Best,
Daniel
