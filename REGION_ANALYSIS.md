# Region analysis and SMT

## What SMT gives you

With `z3` on PATH, `./smash -r` encodes each region’s **extracted linear constraints** (intervals on `pT`, `MET.pT`, `size(obj)`, tags, etc.) and checks pairs **R1 ∧ R2**.

| Z3 result | Meaning | Soundness |
|-----------|---------|-----------|
| **UNSAT** | **Proven disjoint** — no assignment satisfies both regions in the fragment | Sound: if UNSAT in fragment, real ADL regions cannot overlap on those facts |
| **SAT + shared dimension** | **Proven overlapping** — ∃ witness (printed) satisfying both | Sound for overlap **within the fragment**; if cuts are missing from IR, overlap in full ADL may still differ |
| **SAT, no shared dimension** | **Possibly overlapping** only — independent cuts (e.g. MET vs jet multiplicity) | SAT does not prove the SRs compete on the same physics knob |
| Heuristic only | Interval clash → disjoint; all shared canonical intervals intersect → possibly overlap | Fast; misses multi-constraint interactions Z3 catches |

SMT does **not** encode: `dφ`/`dR`, BDTs, arbitrary functions, OR branches, or per-object indexing (one variable per canonical key).

## Usage

```bash
make
./smash -r file.adl           # heuristics + Z3 if installed
./smash -r --no-smt file.adl  # heuristics only
./smash -r --json out.json file.adl
make test-disjoint
```

## Robustness (implementation)

- **Canonical keys** — related aliases (`JET.pt` vs `jets[0].pt`) share one SMT variable via union-find + object lineage.
- **Merged intervals** — multiple cuts on the same dimension are intersected before Z3.
- **Shared dimension** — SAT alone is not labeled “proven overlap” unless at least one canonical key appears in both regions.
- **Witness** — `(get-model)` summary on proven overlap.

## Limits

- Single-file; incomplete extraction → unknown or weak verdicts.
- Proven overlap is **existential** in the linear fragment, not a guarantee of non-zero efficiency in simulation.