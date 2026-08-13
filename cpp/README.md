# ADL2 C++ port (`cpp/`) — P4 solver / viz / analysis

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
syntax → sema → {interp ‖ formula} → axioms → solver
                                    ↘ certify ↗ analysis
viz reads HIR only; cli wires modules.
```

| Target | Status | Headers |
|---|---|---|
| **`adl2_syntax`** | **filled** (P1: RD + AST dump) | `libs/syntax/include/adl2/syntax/` |
| **`adl2_sema`** | **filled** (P2 HIR + P3 Rat/NumVal) | `libs/sema/include/adl2/sema/` |
| **`adl2_formula`** | **filled** (P3: Formula/Over/Under + encoder) | `libs/formula/include/adl2/formula/` |
| **`adl2_interp`** | **filled** (P3 run + P4 Kleene `region3`) | `libs/interp/include/adl2/interp/` |
| **`adl2_axioms`** | **filled** (P3 catalog + P4 EPRED/EPRES) | `libs/axioms/include/adl2/axioms/` |
| **`adl2_solver`** | **filled** (P4: SMT-LIB2 subprocess) | `libs/solver/include/adl2/solver/` |
| **`adl2_analysis`** | **filled** (P4 pairwise + P5 witness + P6 certify/report) | `libs/analysis/include/adl2/analysis/` |
| **`adl2_viz`** | **filled** (P4: flowchart/AST DOT) | `libs/viz/include/adl2/viz/` |
| **`adl2_certify`** | **filled** (P5 kernel; P6 wired from analysis) | `libs/certify/include/adl2/certify/` |
| `adl2_util` | stub | `libs/util/include/adl2/util/` |
| `smash2_cpp` (`libs/cli`) | check/run/dot/verify/objects | — |

Sources live under `libs/<module>/`. Prefer one reviewable PR per module
boundary when filling stubs. **No layering violations:** analysis must not
parse; certify stays a small trusted kernel over formulas (not analysis);
viz depends on sema (HIR), not AST-only meaning.

## Status (P4)

P1 filled **`adl2_syntax`**. P2 filled **`adl2_sema`**. P3 fills
**`adl2_formula`**, **`adl2_interp`**, **`adl2_axioms`**. P4 fills
**`adl2_solver`**, **`adl2_viz`**, **`adl2_analysis`** (interval + solver
pairwise), Kleene `region3`, and EPRED/EPRES emitters.

| Deliverable | Status |
|---|---|
| Modular CMake targets (crate map) | Yes |
| Hand-written RD (`grammar.ebnf` → `parse_X`) in `adl2_syntax` | Yes |
| Canonical AST dump | Yes — byte-for-byte **146 / 146** |
| Interned Quantity/Collection identity | Yes |
| CLI `--dump-hir` / `--dump-quantities` | Yes (allowlist **171 files × 2**) |
| Identity unit battery (`adl2_sema_identity`) | Yes (`PASS=128 FAIL=0`) |
| Polarity-aware Formula IR (`Over`/`Under` as types) | Yes |
| HIR → Formula encoder | Yes |
| CLI `--dump-formula` vs smash2 | Fail-closed allowlist (pinned **172**) |
| CLI `--dump-axioms` vs smash2 | Fail-closed allowlist (pinned **108**) |
| Axiom catalog (19) + emitters | Yes; EPRED/EPRES ported |
| Prohibited-axiom tests (TAG exact-name + no existence-from-mention) | Yes (`adl2_p3_unit`, `PASS=123 FAIL=0`) |
| JSONL Event loader (pT-order + NNEG/TAG domain) | Yes |
| Two-valued `run` event lines vs smash2 | Fail-closed pair list (pinned count) |
| Kleene `region3` membership | Yes (`adl2_region3`, `PASS=30 FAIL=0`) |
| CLI `dot` / `dot --ast` vs smash2 | Fail-closed allowlist (**39 files × 2**) |
| SMT-LIB2 subprocess solver (`classify` Bug-5) | Yes |
| Interval fast path + pairwise `verify` | Yes (SAT overlap is **PROVEN OVERLAPPING** only after region3) |
| Independent Farkas certify | Kernel filled (`adl2_certify_unit` PASS=64); **wired** into `analyze_hir` (CLI `verify` defaults certify ON; `--no-certify` skips). Uncertified solver-UNSAT is **CANDIDATE DISJOINT**. |
| Human verify report | Default stdout of `verify` (trust / findings / regions / matrix / pairwise). Not claimed byte-identical to smash2 (sampling/refute/recon absent). |
| `objects` | `adl2::sema::object_table` + CLI `objects` (allowlist **171**) |

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
| [`scripts/dump_axioms_corpus_gate.sh`](scripts/dump_axioms_corpus_gate.sh) | Allowlisted axiom dump-diff |
| [`scripts/dump_objects_corpus_gate.sh`](scripts/dump_objects_corpus_gate.sh) | Allowlisted objects dump-diff |
| [`scripts/interp_run_gate.sh`](scripts/interp_run_gate.sh) | Pinned `run` event-line diff |
| [`scripts/dump_dot_corpus_gate.sh`](scripts/dump_dot_corpus_gate.sh) | Allowlisted flowchart/AST DOT dump-diff |

## Build

Requires stock Ubuntu toolchain: `cmake` ≥ 3.20, `g++` or `clang++`
with C++17. **No** bison, flex, or libz3. The solver talks to a `z3`
binary on PATH (subprocess SMT-LIB2). Without z3, `verify` degrades to
the interval fast path (verdicts capped at POSSIBLY).

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
./cpp/build/smash2_cpp dot examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp dot --ast examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp verify --dump-verdicts examples/tutorials/ex01_selection.adl
./cpp/build/smash2_cpp verify examples/tutorials/ex01_selection.adl
./cpp/build/smash2_cpp objects examples/tutorials/ex01_selection.adl
```

With a dump flag, stdout is the canonical dump only, matching Rust
`smash2 check --dump-*`.

## Corpus dump-diff gates (forever oracle)

```bash
cpp/scripts/dump_ast_corpus_gate.sh
cpp/scripts/dump_hir_corpus_gate.sh
cpp/scripts/dump_formula_corpus_gate.sh
cpp/scripts/dump_axioms_corpus_gate.sh
cpp/scripts/dump_objects_corpus_gate.sh
cpp/scripts/interp_run_gate.sh
cpp/scripts/dump_dot_corpus_gate.sh
# or, if both binaries already exist:
SKIP_BUILD=1 cpp/scripts/dump_dot_corpus_gate.sh
```

AST gate: **146 / 146**. HIR gate: **171 files × 2** dumps. DOT gate: **39
files × 2** (flowchart + AST). Formula gate: **172** files (fail-closed;
`bad_syntax.adl` excluded — both sides exit 1). Axiom gate: **108**.
Objects gate: **171**. Interp gates are fail-closed
allowlists — shrinking or growing the list without bumping the pin fails CI. Files not on a list are **not claimed**.
Do not weaken `smash2` / `oracle-rust` CI.

Bare `smash2_cpp check` always resolves (Rust smash2 parity); stdout is
empty on success. `smash2_cpp run` prints event lines only (cutflow/histo
tables not yet ported). `verify` defaults to certify ON.

## Remaining vs smash2 (honest non-parity)

Still to port: cutflow/histo tables; sampling + refute gates; `--cross` /
reconcile; `--combine` bundles; ROOT `ingest` / `run --profile`; byte-
identical `verify` JSON/human reports (C++ JSON is a compact subset).
Native libz3 stays out (ADR-010).
