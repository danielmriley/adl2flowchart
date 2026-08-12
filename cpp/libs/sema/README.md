# `adl2_sema` (P2 — filled)

Name resolution, interned Quantity/Collection identity, define resolution,
fragment tagging, and HIR — the C++ port of Rust `adl-sema`
(SPEC_ARCHITECTURE §4). Downstream of syntax only; public headers never
include parser types.

**P1** left this as a stub so the crate map existed in CMake. **P2** fills
it. Do not blob resolve/HIR into syntax or analysis.

## Public API (`libs/sema/include/adl2/sema/`)

| Header | Role |
|---|---|
| `resolve.hpp` | `analyze_str` — parse + resolve (Rust `analyze_str`) |
| `hir.hpp` | HIR nodes, regions, objects, fragment tags |
| `quantity.hpp` | interned `Collection` / `Quantity` / `QuantityTable` (no string keys) |
| `intern.hpp` | case-insensitive `Symbol` / `SymbolTable` |
| `ext.hpp` | `ExtDecls::legacy()` (embedded `legacy_parser/adl/*.txt`) |
| `dump.hpp` | `hir_dump` / `quantity_table_dump` (Rust Debug format) |
| `ops.hpp` / `diag.hpp` | ops + diagnostics copied so HIR does not include syntax |
| `sema.hpp` | umbrella |

`analyze(FileAst)` stays private (`src/resolver.hpp`) so formula / interp /
viz / analysis never see parser headers.

## Identity

Collections and quantities are interned by **structural** keys
(`std::map` over typed ids / enums / interned symbols). Names are labels.
Unsupported element predicates never share intern ids; context-tainted
externals do not intern; unresolved objects use `<unit>::<name>#unresolved`.

Out-of-fragment constructs are tagged `Fragment::unsupported(reason)` —
honest Unknown, never a silent accept.

## Dumps / CLI

```bash
./cpp/build/smash2_cpp check --dump-hir examples/tutorials/ex00_helloworld.adl
./cpp/build/smash2_cpp check --dump-quantities examples/tutorials/ex00_helloworld.adl
```

Byte-for-byte vs Rust `smash2 check --dump-hir` / `--dump-quantities`.
Logic lives here; cli only wires the flags.

## Tests

- ctest `adl2_sema_identity` — port of `adl-sema/tests/identity.rs`
- `cpp/scripts/dump_hir_corpus_gate.sh` — live dump-diff vs smash2 over
  the fail-closed allowlist in `cpp/tests/hir_gate_files.txt`
  (tutorials + key goldens; **38 files pinned**). Remaining `examples/`
  HIR dumps are P2b. Bare `smash2_cpp check` is parse-only (not a sema
  check); use `--dump-hir` / `--dump-quantities`.

## CMake

`adl2_sema` **PRIVATE**-depends on `adl2_syntax` (includes) and
`INTERFACE $<LINK_ONLY:adl2_syntax>` (static-lib link without leaking
parser headers).
