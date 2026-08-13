# ADL2 C++ port (`cpp/`) — P3 interp / formula / axioms

From-scratch C++ reimplementation of the **smash2 architecture**
(ADR-010). This directory is the home for that port.

| Not this | That lives in |
|---|---|
| Active / forever-oracle tool | [`reimplementation/adl2`](../reimplementation/adl2) (`smash2`, Rust) |
| Legacy flex/bison tool | [`legacy_parser/`](../legacy_parser/) (transitional secondary oracle only) |

## Module layout (locked)

**Not a single smash-shaped blob.** CMake targets mirror the Rust crate map.
See [`MODULES.md`](MODULES.md) for the full spine and layering rules.

```
syntax → sema → {interp ‖ formula} → axioms → solver → analysis → certify
viz reads HIR only; cli wires modules.
```

| Target | Status | Headers |
|---|---|---|
| **`adl2_syntax`** | **filled** (P1: RD + AST dump) | `libs/syntax/include/adl2/syntax/` |
| **`adl2_sema`** | **filled** (P2 HIR + P3 Rat/NumVal) | `libs/sema/include/adl2/sema/` |
| **`adl2_formula`** | **filled** (P3: Formula/Over/Under + encoder) | `libs/formula/include/adl2/formula/` |
| **`adl2_interp`** | **filled** (P3: JSONL + two-valued run) | `libs/interp/include/adl2/interp/` |
| **`adl2_axioms`** | **filled** (P3: 19-entry catalog + emitters) | `libs/axioms/include/adl2/axioms/` |
| `adl2_solver` … `adl2_viz`, `adl2_util` | stubs | `libs/<module>/include/adl2/<module>/` |
| `smash2_cpp` (`libs/cli`) | dump/run wiring only | — |

Sources live under `libs/<module>/`. Prefer one reviewable PR per module
boundary when filling stubs. **No layering violations:** analysis must not
parse; certify stays a small trusted kernel; viz depends on sema (HIR), not
AST-only meaning.

## Status (P3)

P1 filled **`adl2_syntax`**. P2 filled **`adl2_sema`**. P3 fills
**`adl2_formula`**, **`adl2_interp`**, **`adl2_axioms`** and wires dumps /
`run` plus fail-closed oracle gates against Rust smash2.

| Deliverable | Status |
|---|---|
| Modular CMake targets (crate map) | Yes |
| Hand-written RD (`grammar.ebnf` → `parse_X`) in `adl2_syntax` | Yes |
| Canonical AST dump | Yes — byte-for-byte **146 / 146** |
| Interned Quantity/Collection identity | Yes |
| CLI `--dump-hir` / `--dump-quantities` | Yes (P2 allowlist **38 files × 2**) |
| Identity unit battery (`adl2_sema_identity`) | Yes (`PASS=128 FAIL=0`) |
| Polarity-aware Formula IR (`Over`/`Under` as types) | Yes |
| HIR → Formula encoder | Yes |
| CLI `--dump-formula` vs smash2 | Fail-closed allowlist (pinned count) |
| Axiom catalog (19) + emitters | Yes; EPRED/EPRES emitters stubbed |
| Prohibited-axiom tests (TAG exact-name) | Yes (`adl2_p3_unit`) |
| JSONL Event loader (pT-order + NNEG/TAG domain) | Yes |
| Two-valued `run` event lines vs smash2 | Fail-closed pair list (pinned count) |
| Solver / analysis / certify / viz | Stub targets only |

Unsupported constructs still emit honest diagnostics (no silent accept).

## Collaborator packaging

| File | Role |
|---|---|
| [`MODULES.md`](MODULES.md) | Crate/CMake map + dependency spine |
| [`grammar.ebnf`](grammar.ebnf) | Frozen EBNF for `adl2_syntax` |
| [`BISON_MAP.md`](BISON_MAP.md) | “If you know bison” → `parse_X` |
| [`scripts/dump_ast_corpus_gate.sh`](scripts/dump_ast_corpus_gate.sh) | 146-file AST dump-diff vs smash2 |
| [`scripts/dump_hir_corpus_gate.sh`](scripts/dump_hir_corpus_gate.sh) | Allowlisted HIR/quantity dump-diff |
| [`scripts/dump_formula_corpus_gate.sh`](scripts/dump_formula_corpus_gate.sh) | Allowlisted formula dump-diff |
| [`scripts/interp_run_gate.sh`](scripts/interp_run_gate.sh) | Pinned `run` event-line diff |

## Build

Requires stock Ubuntu toolchain: `cmake` ≥ 3.20, `g++` or `clang++`
with C++17. **No** bison, flex, z3, or other external libs for the C++
binary.

```bash
cmake -S cpp -B cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build cpp/build
ctest --test-dir cpp/build --output-on-failure
```

Some images default `CXX` to clang without libstdc++; prefer `CXX=g++`.

```bash
./cpp/build/smash2_cpp check --dump-ast examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp check --dump-hir examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp check --dump-formula examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp run examples/tutorials/ex00_helloworld.adl cpp/tests/fixtures/ex00_events.jsonl
```

With a dump flag, stdout is the canonical dump only, matching Rust
`smash2 check --dump-*`.

## Corpus dump-diff gates (forever oracle)

```bash
cpp/scripts/dump_ast_corpus_gate.sh
cpp/scripts/dump_hir_corpus_gate.sh
cpp/scripts/dump_formula_corpus_gate.sh
cpp/scripts/interp_run_gate.sh
# or, if both binaries already exist:
SKIP_BUILD=1 cpp/scripts/dump_formula_corpus_gate.sh
```

AST gate: **146 / 146**. HIR gate: **38 files × 2** dumps. Formula and
interp gates are fail-closed allowlists — shrinking or growing the list
without bumping the pin fails CI. Files not on a list are **not claimed**.
Do not weaken `smash2` / `oracle-rust` CI.

Bare `smash2_cpp check` (no dump flag) is **parse-only**. It is not Rust
`smash2 check`. `smash2_cpp run` prints event lines only (cutflow/histo
tables deferred).

## What’s in / out of P3

**In:** `adl2_formula` (IR + encoder + dump); `adl2_axioms` (catalog +
emitters); `adl2_interp` (JSONL + two-valued run); CLI dumps/`run`; P3
unit tests; allowlisted formula and interp oracle gates; P1 dump-ast and
P2 dump-hir/identity kept green.

**Out:** Filling solver/analysis/certify/viz; Kleene `region3` membership;
cutflow/histo tables; full 146-file formula dump; complete EPRED/EPRES
emitters. Each later module behind its own phase/PR against the Rust oracle.
