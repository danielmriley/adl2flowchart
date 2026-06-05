# SMT robustness plan (v2)

## Goals

1. **Correctness** — fewer false merge/false overlap from key aliasing.
2. **Coverage** — encode more extracted cuts (angular, integer size).
3. **Usefulness** — coverage metrics, subset hints, one clear report.
4. **Ops** — reliable Z3 invocation, CI stays green.

## Work packages

### WP1: Index-aware SMT keys (P0)
- **Problem:** Union-find uses `constraintKeysRelated` → `objectFromConstraintKey` strips `[i]`, merging `jets[0].pt` with `jets[1].pt`.
- **Fix:** Add `constraintKeyStem()` retaining bracket indices; relate keys only if stems match OR same object+property without index on both sides.
- **Test:** `tests/golden/disjoint_jet_index.adl` — SR_a: pT(jets[0])>300, SR_b: pT(jets[1])<200 → PROVEN DISJOINT (not merged).

### WP2: Mixed arithmetic (P0)
- `size(...)` → Z3 **Int**; scalar pT/MET → **Real**; logic `QF_LIA` when any Int else `QF_LRA`.
- Discrete tags stay as Real equality (0/1).

### WP3: Angular constraints in SMT (P1)
- Remove `dphi`/`dR`/`dEta` from SMT blocklist; treat as Real interval vars (already extracted).

### WP4: Fragment coverage (P1)
- Per region: `encodable / total` constraints; per pair: list keys only in one region.
- JSON field `fragment_coverage`.

### WP5: Subset detection (P2)
- If every shared canonical interval in R2 ⊆ R1 → `PossiblySubset` (heuristic).

### WP6: Unified `-r` output (P1)
- Default: object analysis + `region_analysis` only.
- `--legacy-region-report` for old verbose disjointness printer.

### WP7: Z3 plumbing (P2)
- Write script to stdin; structured witness parse; 15s timeout.

## Out of scope (v3)
- Full OR/ITE disjunction encoding.
- Per-event simulation overlap yields.
- libz3 link.

## Review synthesis (subagents)

- Fix **both** `canonicalConstraintKey` and `constraintKeysRelated` for indices.
- Use **QF_LIRA** for mixed Int `size` + Real scalars (not QF_LIA alone).
- Defer WP5 subset and WP7 stdin timeout; require z3 in CI.

## Implemented (v2)

- [x] WP1 index-aware canonical keys + bracket guard in `constraintKeysRelated`
- [x] WP2 QF_LIRA / Int size vars
- [x] WP3 angular keys encodable in SMT
- [x] WP4 fragment_coverage in JSON and report
- [x] WP6 unified `-r` (legacy behind `--legacy-region-report`)
- [x] Golden: `disjoint_jet_index.adl`, `independent_jet_index.adl`
## v3 (implemented)

- [x] OR disjunctions in SMT
- [x] ITE implications in SMT
- [x] Coverage warnings (<50% encodable / select encoding)
- [x] Golden: `ite_conditional_dphi.adl`, `or_met.adl`
- [ ] WP5 subset (deferred)
- [ ] WP7 Z3 stdin (deferred; timeout raised to 15s)

## Success criteria
- All golden + corpus pass.
- Delphes 033 spike passes.
- `disjoint_jet_index.adl` proves disjoint on `jets[0]`; different indices do not false-merge.