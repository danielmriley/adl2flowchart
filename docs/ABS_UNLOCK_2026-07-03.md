# The abs unlock — exact `abs(x) ⋈ c` in the element-predicate encoder

**Date:** 2026-07-03 · **Scope:** `adl-axioms::encode_pred_exact` (reconciliation's
shared-element encoder) · **Status:** shipped

## What changed

`abs(eta) < 2.4` — the corpus-universal acceptance cut — was opaque to
`encode_elem_pred_generic`, so any object whose filter chain contained it
dropped out of cross-collection reconciliation: the `.under()` projection
collapsed to false and every real-corpus refinement pair fail-closed to
POSSIBLY. The encoder now expands `abs(E) ⋈ c` into its exact two-sided
linear form, mirroring `adl-formula::abs_cmp` semantics exactly:

- `c < 0` constant-folds first (`<`,`<=`,`==` → False; `>`,`>=`,`!=` → True),
  preventing the two unsound edges (`abs(x) > -1` is a tautology, not a bound).
- `c >= 0`, with `E` linearized as `terms + k`: `hi = c − k`, `lo = −c − k`;
  `Lt → E < hi ∧ E > lo`, `Le → ∧(≤,≥)`, `Gt → ∨(>,<)`, `Ge → ∨(≥,≤)`,
  `Eq → ∨(==hi,==lo)`, `Ne → ∧(≠hi,≠lo)`.
- Both orientations (`abs(E) ⋈ c` and `c ⋈ abs(E)` via `rel.flipped()`).

## Verification

- 252-cell ground-truth table (6 rels × 3 constants incl. negative × 2
  orientations × 7 values) in `adl-axioms::abs_pred_tests`.
- Interpreter agreement: `abs(eta) < 2.1` added to the `axioms_hold`
  vocabulary object (joint pad-0 battery).
- Oracle reach: `GExtra::AbsCut` in the difftest generator (both
  orientations, ETA_POOL constants incl. negatives); default + deep runs green.
- Money test `reconcile_fires_through_abs_eta_cuts` (cross_file.rs):
  `pt>30 ∧ |eta|<2.4` refines `pt>25 ∧ |eta|<2.4` → XSUB → PROVEN DISJOINT,
  certified; the reversed-window pair stays unproven (direction guard).
- Golden-cross group `examples/golden/cross/abs-refine/` pins the keystone
  DISJOINT and the wider-vs-narrower POSSIBLY (corpus 134 → 136 files).

## Measured impact (13-file CMS corpus, all 78 file-pairs, A/B)

| metric | before | after |
|---|---|---|
| file-pairs where XSUB fires | 1/78 | 28/78 |
| cross-file proven verdicts (disjoint+overlapping) | 87 | 208 |

Exact pair-keyed JSON diff over the implicated file-pairs: **98 pairs
upgraded POSSIBLY → PROVEN DISJOINT** (e.g. 032/033 +84), 1 pair upgraded to
PROVEN OVERLAPPING. Headline shape: identical `|eta|` windows with ordered pt
thresholds now XSUB-link across analyses, exactly the refinement idiom the
CMS SUS corpus is written in.

## Known honest downgrades (fail-closed, follow-up items)

Two intra-file PROVEN OVERLAPPING pairs demoted to POSSIBLY — both
witness-realizer completeness, not soundness:

1. **032 `compressed` vs `compressednc2`** — the exact encoding constrains
   electron/muon elements, the realizer materializes them, and the
   interpreter rejects on `D0` (no reference interpretation). Previously the
   collections stayed opaque, the realizer left them empty, and validation
   passed vacuously. *Follow-up: realizer should prefer size-0 for
   collections whose predicates are interpreter-opaque.*
2. **033 `SR7` vs `SR11`** — the tighter model pins jets near pt≈31 while the
   solver's free `HT` sits at 750; the interpreter derives HT from jet pTs
   and rejects. *Follow-up: realize derived scalars consistently (HT-sum
   linkage) or retry with slack-maximizing models.*

One CANDIDATE OVERLAPPING (017 preselection vs 037 baseline) also drifted to
POSSIBLY (model choice under the new facts; the candidate tier is advisory).
