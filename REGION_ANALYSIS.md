# Region analysis and SMT

## What SMT gives you

With `z3` on PATH, `./smash -r` encodes extracted constraints and checks **R1 ∧ R2** per region pair.

| Verdict | Meaning |
|---------|---------|
| **PROVEN DISJOINT** | Z3 **UNSAT** — no assignment satisfies both regions in the encoded fragment |
| **PROVEN OVERLAPPING** | Z3 **SAT** + shared constraint dimension + witness model |
| **POSSIBLY OVERLAPPING** | Heuristic interval intersection, or SAT without shared dimension |
| **UNKNOWN** | Non-encodable cuts, timeout, or inconclusive |

## Encoding (v2)

- **QF_LIRA** when `size(...)` present (Int); else **QF_LRA** (Real).
- **Index-aware keys** — `jets[0].pt` and `jets[1].pt` stay distinct; lineage merge only when bracket indices match.
- **Angular cuts** — `dphi`/`dR`/`dEta` interval keys are SMT-encoded when extracted.
- **Fragment coverage** — each region reports `SMT-encodable N/M` and JSON `fragment_coverage`.

## Encoding (v3)

- **OR** — `select A || B` → `(assert (or (and ...) (and ...)))`.
- **ITE** — `cond ? cut : ALL` → `(assert (=> guard then))`; else branch when not `ALL`.
- **Coverage warnings** — printed when encodable atoms or encoded selects fall below **50%**.

## Usage

```bash
./smash -r file.adl
./smash -r --no-smt file.adl
./smash -r --legacy-region-report file.adl   # verbose legacy disjointness block
./smash -r --json out.json file.adl
make test-disjoint
```

## Limits

- Single-file; incomplete extraction → weak verdicts (see coverage warnings).
- Deeply nested OR/ITE may not fully flatten; BDTs and arbitrary functions still skipped.
- Proven overlap = ∃ model in fragment, not simulation yield.