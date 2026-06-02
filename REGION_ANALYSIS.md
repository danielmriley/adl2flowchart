# Region analysis pipeline (phases 0–4)

## Usage

```bash
make
./smash -r file.adl              # legacy disjointness + IR report
./smash -r --smt file.adl        # add Z3 on linear constraints (needs `z3` on PATH)
./smash -r --json out.json file.adl
make test-disjoint               # golden fixtures under tests/golden/
make test-corpus                 # all examples/*.adl parse + -r
```

## Architecture

1. **Gather IR** — `gatherRegionConstraints()` in `semantic_checks.cpp` builds merged per-region atoms (inheritance, reject complement, defines, size, tags, dφ/dR where supported).
2. **Legacy report** — `analyzeRegionDisjointness()` / `analyzeObjectDisjointness()` (human-readable proofs).
3. **IR layer** — `region_analysis.cpp`: pairwise **disjoint** (interval clash), **overlap possible** (all related intervals compatible), optional **Z3** (`QF_LRA`) on scalar/size keys; skips dφ/dR/BDT.

## Phases

| Phase | Status | Deliverable |
|-------|--------|-------------|
| 0 | Done | Merge `disjoint_dev` → `main`, golden tests, corpus scripts |
| 1 | Done | `RegionConstraintSet`, JSON export, overlap heuristic |
| 2 | Done | Z3 subprocess spike (`scripts/phase2_z3_spike.sh`, Delphes 033) |
| 3 | Done | `-r --smt` linear fragment |
| 4 | Done | CI workflow (optional z3), `make test-*` |

## Limits

- Single-file only; no cross-file alias alignment beyond `object_aliases.txt`.
- OR / complex defines / arbitrary functions → unknown unless extracted to intervals.
- SMT uses one real variable per constraint key (no per-object indexing).