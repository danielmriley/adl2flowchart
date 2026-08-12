# ADL2 C++ port (`cpp/`) — P1 syntax + AST dumps

From-scratch C++ reimplementation of the **smash2 architecture**
(ADR-010). This directory is the home for that port.

| Not this | That lives in |
|---|---|
| Active / forever-oracle tool | [`reimplementation/adl2`](../reimplementation/adl2) (`smash2`, Rust) |
| Legacy flex/bison tool | [`legacy_parser/`](../legacy_parser/) (transitional secondary oracle only) |

## Status (P1)

P1 expands the P0 harness into a **dump-compatible** recursive-descent
parser and wires a **corpus dump-diff gate** against Rust smash2.

| Deliverable | Status |
|---|---|
| Hand-written RD (`grammar.ebnf` → `parse_X`) | Yes — checked fragment |
| Canonical AST dump (`adl_syntax::dump_ast` format) | Yes — byte-for-byte |
| CLI `smash2_cpp check --dump-ast` | Yes |
| Corpus dump-diff vs `smash2 check --dump-ast` | **146 / 146** green |
| Sema / interpreter / verifier / certifier | Deferred (post-P1) |

Unsupported constructs still emit honest diagnostics (no silent accept).
Sema and later pipelines are out of scope for P1.

## Collaborator packaging (required surface)

| File | Role |
|---|---|
| [`grammar.ebnf`](grammar.ebnf) | Frozen EBNF — readable source of truth (from SPEC_LANGUAGE §3) |
| [`BISON_MAP.md`](BISON_MAP.md) | “If you know bison”: tokens, `parse_X`, precedence, diagnostics |
| `include/adl2/parser.hpp` | Declares one `parse_<nonterminal>` per major EBNF name |
| `include/adl2/ast.hpp` / `dump.hpp` | Dump-shaped AST + `dump_ast` (Rust format) |
| `src/syntax/` | Lexer + RD parser + dump |
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

Binaries:

- `cpp/build/smash2_cpp` — CLI (`--help`, `check [--dump-ast] <file>`)
- `libadl2_cpp.a` — static library (syntax + dump)

```bash
./cpp/build/smash2_cpp --help
./cpp/build/smash2_cpp check cpp/tests/fixtures/tiny.adl
./cpp/build/smash2_cpp check --dump-ast examples/tutorials/ex00_helloworld.adl
```

`check` lexes and parses. With `--dump-ast`, stdout is the canonical AST
dump only (diagnostics on stderr), matching Rust
`smash2 check --dump-ast`. Exit code is nonzero when errors were recorded.

## Corpus dump-diff gate (forever oracle)

Rust `smash2` is the forever oracle. The P1 gate diffs C++ dumps against
it over the same 146-file `examples/` corpus as
`adl-syntax/tests/corpus_gate.rs`:

```bash
# builds smash2_cpp + smash2 (cargo -p adl-cli --no-default-features), then diffs
cpp/scripts/dump_ast_corpus_gate.sh

# or, if both binaries already exist:
SKIP_BUILD=1 cpp/scripts/dump_ast_corpus_gate.sh
```

Byte-for-byte match is required (no normalization). Do not weaken or
remove the existing `smash2` / `oracle-rust` CI jobs when extending this
tree.

## Relation to Rust smash2

| CI job | Role |
|---|---|
| `smash2` | Full Rust workspace build + test (primary gate) |
| `oracle-rust` | Forever-oracle smoke: `smash2 check --dump-ast` on a tutorial |
| `adl2-cpp` | C++ build + ctest + **dump-ast corpus gate** vs smash2 |

## What’s in / out of P1

**In:** Rich AST aligned with `adl_syntax::ast`, complete-enough RD for
the checked fragment, `dump_ast` parity, CLI `--dump-ast`, 146-file
corpus dump-diff script + CI, README / BISON_MAP updates.

**Out:** Sema, interpreter, verifier, certifier, axioms, viz, exact
rationals — those land behind explicit later gates against the Rust
oracle.

### Notable P1 implementation choices

- Path-tokens are recognized **only in argument position** (Rust
  `try_path_token`), not in the lexer — greedy path lexing incorrectly
  swallowed expressions like `photons.pt/j.pt`.
- Comparison chains (`a < x < b`) desugar to nested `Binary op=and` of
  `Cmp` nodes, matching Rust.
- Binary/`or`/`and`/`not` dump names are canonical (`or`/`and`/`not`),
  never `||`/`&&`/`!`.
- String Debug quoting for dump fields matches Rust `{:?}`.
