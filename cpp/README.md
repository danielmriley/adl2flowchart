# ADL2 C++ port (`cpp/`) — P1 syntax + AST dumps

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

| Target | P1 | Headers |
|---|---|---|
| **`adl2_syntax`** | **filled** (RD + AST dump) | `include/adl2/syntax/` |
| `adl2_sema` … `adl2_viz`, `adl2_util` | stubs | `include/adl2/<module>/` |
| `smash2_cpp` (`libs/cli`) | dump wiring only | — |

Sources live under `libs/<module>/`. Prefer one reviewable PR per module
boundary when filling stubs. **No layering violations:** analysis must not
parse; certify stays a small trusted kernel; viz depends on sema (HIR), not
AST-only meaning.

## Status (P1)

P1 fills **`adl2_syntax`** and wires CLI dump + a corpus dump-diff gate
against Rust smash2. Other module targets exist as stubs so the map is
real in CMake.

| Deliverable | Status |
|---|---|
| Modular CMake targets (crate map) | Yes |
| Hand-written RD (`grammar.ebnf` → `parse_X`) in `adl2_syntax` | Yes |
| Canonical AST dump (`adl_syntax::dump_ast` format) | Yes — byte-for-byte |
| CLI `smash2_cpp check --dump-ast` | Yes (cli → syntax only) |
| Corpus dump-diff vs `smash2 check --dump-ast` | **146 / 146** green |
| Sema / interp / formula / … | Stub targets only |

Unsupported constructs still emit honest diagnostics (no silent accept).

## Collaborator packaging (syntax surface)

| File | Role |
|---|---|
| [`MODULES.md`](MODULES.md) | Crate/CMake map + dependency spine |
| [`grammar.ebnf`](grammar.ebnf) | Frozen EBNF for `adl2_syntax` |
| [`BISON_MAP.md`](BISON_MAP.md) | “If you know bison” → `parse_X` |
| `include/adl2/syntax/parser.hpp` | One `parse_<nonterminal>` per major EBNF name |
| `include/adl2/syntax/{ast,dump}.hpp` | Dump-shaped AST + `dump_ast` |
| `libs/syntax/` | Lexer + RD parser + dump implementation |
| [`scripts/dump_ast_corpus_gate.sh`](scripts/dump_ast_corpus_gate.sh) | Build both tools; fail on dump mismatch |

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

- `cpp/build/smash2_cpp` — CLI (`check [--dump-ast] <file>`); links `adl2_syntax`
- `cpp/build/libs/syntax/libadl2_syntax.a` — P1 implementation
- `cpp/build/libs/*/libadl2_*.a` — stub anchors (built in the default graph)

```bash
./cpp/build/smash2_cpp check --dump-ast examples/tutorials/ex00_helloworld.adl
```

With `--dump-ast`, stdout is the canonical AST dump only (diagnostics on
stderr), matching Rust `smash2 check --dump-ast`.

## Corpus dump-diff gate (forever oracle)

```bash
cpp/scripts/dump_ast_corpus_gate.sh
# or, if both binaries already exist:
SKIP_BUILD=1 cpp/scripts/dump_ast_corpus_gate.sh
```

Same 146-file `examples/` corpus as `adl-syntax/tests/corpus_gate.rs`.
Byte-for-byte match required. Do not weaken `smash2` / `oracle-rust` CI.

## What’s in / out of P1

**In:** Module CMake map; `adl2_syntax` dump parity; CLI wiring; 146-file
corpus gate; stub libs for the rest of the spine.

**Out:** Filling sema/formula/interp/axioms/solver/analysis/certify/viz —
each behind its own phase/PR against the Rust oracle.

### Notable syntax choices

- Path-tokens recognized **only in argument position** (Rust
  `try_path_token`).
- Comparison chains desugar to nested `Binary op=and` of `Cmp`.
- Dump uses canonical `and`/`or`/`not` and Rust Debug string quoting.
