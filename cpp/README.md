# ADL2 C++ port (`cpp/`) — P2 sema / HIR

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
| **`adl2_sema`** | **filled** (P2: HIR + identity + resolve) | `libs/sema/include/adl2/sema/` |
| `adl2_formula` … `adl2_viz`, `adl2_util` | stubs | `libs/<module>/include/adl2/<module>/` |
| `smash2_cpp` (`libs/cli`) | dump wiring only | — |

Sources live under `libs/<module>/`. Prefer one reviewable PR per module
boundary when filling stubs. **No layering violations:** analysis must not
parse; certify stays a small trusted kernel; viz depends on sema (HIR), not
AST-only meaning.

## Status (P2)

P1 filled **`adl2_syntax`**. P2 fills **`adl2_sema`** and wires HIR /
quantity dumps + a fail-closed dump-diff gate against Rust smash2.

| Deliverable | Status |
|---|---|
| Modular CMake targets (crate map) | Yes |
| Hand-written RD (`grammar.ebnf` → `parse_X`) in `adl2_syntax` | Yes |
| Canonical AST dump (`adl_syntax::dump_ast` format) | Yes — byte-for-byte |
| CLI `smash2_cpp check` (no dump flag) | **Parse-only** (documented; not Rust-equivalent) |
| CLI `smash2_cpp check --dump-ast` | Yes (parse-only; P1 gate unchanged) |
| Corpus dump-ast vs `smash2 check --dump-ast` | **146 / 146** green |
| Interned Quantity/Collection identity (no string keys) | Yes |
| `analyze_str` → HIR + fragment tags | Yes |
| CLI `--dump-hir` / `--dump-quantities` | Yes (cli → syntax + sema) |
| Identity unit battery (`adl2_sema_identity`) | Yes (port of `adl-sema/tests/identity.rs`) |
| HIR/quantity dump-diff vs smash2 | Fail-closed allowlist (tutorials + key goldens) |
| Sema links syntax **PRIVATE** (no parser leak) | Yes + layering ctest |
| Formula / interp / axioms / solver / analysis / certify / viz | Stub targets only |

Unsupported constructs still emit honest diagnostics (no silent accept).

## Collaborator packaging

| File | Role |
|---|---|
| [`MODULES.md`](MODULES.md) | Crate/CMake map + dependency spine |
| [`grammar.ebnf`](grammar.ebnf) | Frozen EBNF for `adl2_syntax` |
| [`BISON_MAP.md`](BISON_MAP.md) | “If you know bison” → `parse_X` |
| `libs/syntax/include/adl2/syntax/parser.hpp` | One `parse_<nonterminal>` per major EBNF name |
| `libs/sema/include/adl2/sema/` | HIR, identity, `analyze_str`, dumps |
| [`scripts/dump_ast_corpus_gate.sh`](scripts/dump_ast_corpus_gate.sh) | 146-file AST dump-diff vs smash2 |
| [`scripts/dump_hir_corpus_gate.sh`](scripts/dump_hir_corpus_gate.sh) | Allowlisted HIR/quantity dump-diff vs smash2 |

## Build

Requires stock Ubuntu toolchain: `cmake` ≥ 3.20, `g++` or `clang++`
with C++17. **No** bison, flex, z3, or other external libs for the C++
binary.

```bash
cmake -S cpp -B cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build cpp/build
ctest --test-dir cpp/build --output-on-failure
```

Binaries / libs:

- `cpp/build/smash2_cpp` — CLI; links `adl2_syntax` + `adl2_sema`
- `cpp/build/libs/syntax/libadl2_syntax.a` — P1 implementation
- `cpp/build/libs/sema/libadl2_sema.a` — P2 implementation
- `cpp/build/libs/*/libadl2_*.a` — remaining stub anchors

```bash
./cpp/build/smash2_cpp check --dump-ast examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp check --dump-hir examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp check --dump-quantities examples/tutorials/ex00_helloworld.adl
```

With a dump flag, stdout is the canonical dump only (diagnostics on
stderr for `--dump-ast`), matching Rust `smash2 check --dump-*`.

## Corpus dump-diff gates (forever oracle)

```bash
cpp/scripts/dump_ast_corpus_gate.sh          # 146 files, AST
cpp/scripts/dump_hir_corpus_gate.sh          # allowlist × {hir, quantities}
# or, if both binaries already exist:
SKIP_BUILD=1 cpp/scripts/dump_ast_corpus_gate.sh
SKIP_BUILD=1 cpp/scripts/dump_hir_corpus_gate.sh
```

AST gate: same 146-file `examples/` corpus as `adl-syntax/tests/corpus_gate.rs`.
Both dump commands must **exit 0**; dumps must start with `File`; then
byte-for-byte match.

HIR gate: fail-closed allowlist in [`tests/hir_gate_files.txt`](tests/hir_gate_files.txt)
(14 tutorials + 24 goldens = **38 files**, pinned as `EXPECTED_FILES=38`
in the gate script and ctest `hir_gate_allowlist_count`). Shrinking or
growing the list without bumping the pin fails CI. Both `--dump-hir` and
`--dump-quantities` must exit 0; dumps must start with `unit:`; then
byte-for-byte match. Files not on the list are **not claimed** (P2b:
remaining examples corpus; `bad_syntax.adl` exits 1). Do not weaken
`smash2` / `oracle-rust` CI.

Bare `smash2_cpp check` (no dump flag) is **parse-only** until a later
phase. It is not Rust `smash2 check` (which always resolves). Help text
and a stderr note state this; do not treat a green bare `check` as sema
parity.

## What’s in / out of P2

**In:** `adl2_sema` (HIR, interned identity, resolve, dumps); CLI dump
flags; identity tests; allowlisted HIR oracle gate; P1 dump-ast gate kept
green; layering test kept.

**Out:** Filling formula/interp/axioms/solver/analysis/certify/viz;
full 146-file HIR corpus; `object_table` dump; Rust `merge` / `rat` /
`num` (not required for HIR dumps). Each later module behind its own
phase/PR against the Rust oracle.

### Notable syntax/sema choices

- Path-tokens recognized **only in argument position** (Rust
  `try_path_token`).
- Comparison chains desugar to nested `Binary op=and` of `Cmp`.
- Dump uses canonical `and`/`or`/`not` and Rust Debug string quoting.
- `_<digit>` is the underscore-indexing operator; the lexer emits the
  same notes as Rust so HIR diagnostic sections match.
- Sema public headers duplicate ops/span/diag so they never `#include`
  parser headers.
