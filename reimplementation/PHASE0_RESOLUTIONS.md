# Phase 0 resolutions (build-time defaults)

CutLang probing was not available for this build, so every OPEN item
resolves to the spec's named fallback strategy (PLAN risk table:
"convention-neutral, exactly as legacy ships today"). Each is revisitable
without architectural change once probed; [VERIFY] markers stay in
SPEC_LANGUAGE until then.

| Item | Resolution for this build |
|---|---|
| OPEN-1 unindexed collection cut | Dual bounded expansion, k=3, empty-collection case in the plus branch (legacy-equivalent, audit-Bug-1-correct) |
| OPEN-2 dPhi/dEta convention | oriented (order-sensitive) quantities; range axiom −π…π; twin axiom `x=y ∨ x=−y`; SAT-direction verdicts capped at POSSIBLY when reversed twins present |
| OPEN-3 index base / negatives | 0-based; `[-n]` is a diagnostic (`Unsupported`), `ElemIndex::FromBack` reserved |
| OPEN-4 `~=` | parsed as `!=` with a once-per-file warning that the semantics are unverified |
| OPEN-5 size aliases | `size`/`Size`/`count` case-insensitive aliases of the size quantity |
| Case sensitivity | resolution is case-insensitive; case preserved in diagnostics (legacy behavior, corpus requires it) |
| Division by zero / non-finite | enclosing comparison is false; non-finite constants cannot construct atoms |
| Solver backends | native z3 crate primary (libz3 dev confirmed present), SMT-LIB subprocess secondary; both behind the Solver trait with a shared conformance battery |
| and/or precedence | standard precedence per spec divergence 1 (corpus pre-scan: no unparenthesized mixed chains) |

Corpus gate paths: examples live at `../../examples` relative to the
workspace (`reimplementation/adl2`); legacy goldens at
`../../legacy_parser/tests/golden`.
