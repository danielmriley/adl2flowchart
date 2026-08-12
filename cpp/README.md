# ADL2 C++ port (`cpp/`) — P0 harness

From-scratch C++ reimplementation of the **smash2 architecture**
(ADR-010). This directory is the home for that port.

| Not this | That lives in |
|---|---|
| Active / forever-oracle tool | [`reimplementation/adl2`](../reimplementation/adl2) (`smash2`, Rust) |
| Legacy flex/bison tool | [`legacy_parser/`](../legacy_parser/) (transitional secondary oracle only) |

P0 is **harness + grammar packaging + ADR**, not smash2 parity.

## Goals

- Full C++ reimplementation of smash2 behavior under `cpp/` (not an
  in-place rewrite of `legacy_parser/`).
- Hand-written recursive descent (ADR-002): frozen EBNF, 1:1
  nonterminal → `parse_X`, bison map for collaborators, grammar-shaped
  diagnostics.
- Import soundness non-negotiables from ADR-003–008 + certify / HIR viz /
  exact rationals (see ADR-010 in
  [`docs/archive/specs/DECISIONS.md`](../docs/archive/specs/DECISIONS.md)).
- Keep Rust `smash2` as the **forever oracle** in CI; future parity gates
  will diff C++ outputs against it.

## Collaborator packaging (required surface)

| File | Role |
|---|---|
| [`grammar.ebnf`](grammar.ebnf) | Frozen EBNF — readable source of truth (from SPEC_LANGUAGE §3) |
| [`BISON_MAP.md`](BISON_MAP.md) | “If you know bison”: tokens, `parse_X`, precedence, diagnostics |
| `include/adl2/parser.hpp` | Declares one `parse_<nonterminal>` per major EBNF name |
| `src/syntax/` | Lexer + RD parser skeleton (many productions still stub) |

## Build

Requires stock Ubuntu toolchain: `cmake` ≥ 3.16, `g++` or `clang++`
with C++17. **No** bison, flex, z3, or other external libs for P0.

```bash
cmake -S cpp -B cpp/build -DCMAKE_BUILD_TYPE=Release
cmake --build cpp/build
ctest --test-dir cpp/build --output-on-failure
```

Binaries:

- `cpp/build/smash2_cpp` — CLI skeleton (`--help`, `check <file>`)
- `libadl2_cpp.a` — static library (syntax harness)

```bash
./cpp/build/smash2_cpp --help
./cpp/build/smash2_cpp check cpp/tests/fixtures/tiny.adl
```

`check` lexes and runs the RD harness. Full ADL is **not** implemented
yet; unsupported constructs report grammar-shaped “not implemented”
diagnostics. Exit code is nonzero when errors were recorded.

## Relation to Rust smash2 (forever oracle)

CI job `oracle-rust` builds Rust smash2 and runs

```bash
smash2 check --dump-ast examples/tutorials/ex01_selection.adl
```

so the oracle path stays green alongside this skeleton. **Future parity
gates will diff C++ AST / verify outputs against smash2** — do not weaken
or remove the existing `smash2` CI job when extending this tree.

## What’s in / out of P0

**In:** ADR-010, frozen EBNF, bison map, CMake lib+CLI, RD skeleton with
named `parse_X` entry points, layered expression precedence, smoke
tests, CI build + oracle hook.

**Out:** Sema, interpreter, verifier, certifier, axioms, viz, exact
rationals, corpus parity — those land behind explicit gates against the
Rust oracle.
