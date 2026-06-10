# Region analysis: dual encoding + SMT

`./smash -r file.adl` analyzes every pair of regions for disjointness,
overlap, and subset relations.

## How verdicts are derived

Each region's selection logic is compiled to an exact boolean formula over
per-event scalar variables (`region_formula.h`). Anything the encoder cannot
translate faithfully becomes an explicit **Unknown** leaf — it is never
silently dropped. Two projections with opposite soundness directions are
then used:

| Projection | Unknown becomes | Used for | Why it is sound |
|------------|-----------------|----------|-----------------|
| R⁺ (over-approximation) | `true` | **PROVEN DISJOINT** = UNSAT(R1⁺ ∧ R2⁺) | R ⊆ R⁺, so if even the supersets cannot intersect, the real regions cannot |
| R⁻ (under-approximation) | `false` | **PROVEN OVERLAPPING** = SAT(R1⁻ ∧ R2⁻); **PROVEN SUBSET** A⊆B = UNSAT(R1⁺ ∧ ¬R2⁻) | R⁻ ⊆ R, so a model of the subsets is a real overlap candidate |

When a region encodes with zero Unknown leaves both projections coincide and
the report says **exact encoding**.

| Verdict | Meaning |
|---------|---------|
| **PROVEN DISJOINT** | no event can satisfy both regions (interval heuristic or Z3 UNSAT on R⁺) |
| **PROVEN OVERLAPPING** | Z3 found a witness event satisfying both R⁻ formulas, on a shared constraint dimension |
| **PROVEN SUBSET: A within B** | every event passing A passes B |
| **POSSIBLY OVERLAPPING** | nothing proven: independent cuts, or only the over-approximation is SAT |
| **UNKNOWN** | solver inconclusive/timeout |

## Encoder coverage

- Comparisons (`< <= > >= == != ~=`), `[]`/`][` ranges, AND/OR/NOT,
  ternary `cond ? cut : ALL` (compiled exactly to `(g∧then) ∨ (¬g∧else)`),
  `reject` (exact negation), region inheritance (inlined), defines (boolean
  defines inlined at the reference site; value defines become named scalars),
  `size(...)` as Int, angular functions `dPhi/dR/dEta`, trigger flags,
  `abs(...)` and generic functions of indexed/scalar arguments as opaque
  scalars.
- **Quantifier guard**: a cut on a collection property without an index
  (`pt(jets) > 30` at region level) is ambiguous and becomes Unknown rather
  than being scalarized.
- **Key identity**: spelling aliases (`object_aliases.txt`) and in-file pure
  renames (`object X take Y` with no cuts) merge; *filtered* collections do
  NOT merge with their parents (`bjets[0].pt` ≠ `jets[0].pt`).
- **Background axioms** asserted with every check (true of every event):
  pT-ordering of indexed elements, `size ≥ 0`, `C[i]` referenced ⇒
  `size(C) ≥ i+1`, `size(child) ≤ size(parent)` for derived collections.

## Performance

One z3 process per file with `push`/`pop` per check (not one process per
pair). A 10-region / 45-pair file runs in ~0.15 s.

## Usage

```bash
./smash -r file.adl
./smash -r --no-smt file.adl            # heuristic only
./smash -r --legacy-region-report file.adl
./smash -r --json out.json file.adl
make test                                # goldens + corpus + z3 spike
```

## Limits

- Per-event scalar model: per-object quantification is not modeled; cuts that
  need it are reported as dropped (see per-region `dropped:` lines).
- PROVEN OVERLAPPING means a model exists in the scalar fragment; it does not
  assert simulation yield, and physically-bounded quantities (e.g. angular
  ranges) are not range-constrained yet.
- BIN statements are treated as partitioning, not constraining, the region.
