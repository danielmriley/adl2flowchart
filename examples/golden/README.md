# Golden region/object verdict corpus

Hand-authored ADL files with **fully-known** disjoint / overlapping / empty
ground truth, used as the permanent regression net for the smash2
disjoint-overlap analyzer. Each file pins its expected verdict in header
comments; the Rust harness
`reimplementation/adl2/crates/adl-analysis/tests/golden_regions.rs` parses
those headers, runs the full analysis (z3 required), and asserts the reported
verdict matches exactly.

## Header convention

```
# GOLDEN <RegionA> <RegionB> DISJOINT|OVERLAPPING|POSSIBLY
# GOLDEN-EMPTY <Region>
# GROUND-TRUTH: <prose proof + a witness event for each side>
```

- `DISJOINT` → the analyzer must report **PROVEN DISJOINT** for the pair.
- `OVERLAPPING` → **PROVEN OVERLAPPING** (witness re-validated by the interpreter).
- `POSSIBLY` → **POSSIBLY OVERLAPPING** — the regions are not provably one or
  the other with the current axioms (a documented precision boundary, not a
  ground-truth claim about reality).
- `GOLDEN-EMPTY` → the named region must be **PROVEN EMPTY** (vacuous).

A file may carry several `# GOLDEN` lines (e.g. a three-region bin chain).
Every `# GROUND-TRUTH` line states why the verdict holds and gives a concrete
witness event for each region, so the claim is checkable by hand and by
`smash2 run`.

## Soundness contract

A passing harness is only meaningful alongside the verdict-soundness oracle
(`adl-difftest`): the oracle guarantees the analyzer never emits a false
PROVEN, so "header says DISJOINT/EMPTY **and** the tool proves it" implies the
regions really are disjoint/empty. `OVERLAPPING` pins are additionally
witness-validated through the interpreter.

## Categories

| prefix             | facet exercised                                            |
|--------------------|------------------------------------------------------------|
| `disjoint_*`       | complementary thresholds, disjoint bands, size/charge/btag partitions, ratio cuts, multi-bin partitions |
| `overlap_*`        | subsets, nested chains, shared-boundary closed bands, partial 2-D overlaps |
| `empty_*`          | contradictory cuts: thresholds, bands, anti-bands, size, pT-order, back-index order |
| `objects_*`        | object-level (filtered-collection) disjoint/overlap        |
| `features-num_*`   | numeric features: ratios, min/max, sums, reassociation, cancellation guard |
| `features-angular_*` | angular separations (`dR`, `dPhi`), unindexed/operator-scoped cuts |

When you add a file, give it a `# GOLDEN`/`# GOLDEN-EMPTY` header and a
`# GROUND-TRUTH` line, then bump the file-count tripwire in
`analysis_behaviors.rs::corpus_runs_no_solver_analysis_deterministically`.
