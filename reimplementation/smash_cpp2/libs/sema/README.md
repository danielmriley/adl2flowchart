# `adl2_sema` (U03)

Name resolution, interned Quantity/Collection identity, define resolution,
fragment tagging, and HIR. Ported from smash2_cpp `cpp/libs/sema`. smash3
`check --dump-hir` / `--dump-quantities` is the dump oracle.

Public headers do not include parser types. Downstream of syntax only.

## Public API (`libs/sema/include/adl2/sema/`)

| Header | Role |
|---|---|
| `resolve.hpp` | `analyze_str` — parse + resolve |
| `hir.hpp` | HIR nodes, regions, objects, fragment tags |
| `quantity.hpp` | interned `Collection` / `Quantity` / `QuantityTable` (no string keys) |
| `intern.hpp` | case-insensitive `Symbol` / `SymbolTable` |
| `ext.hpp` | `ExtDecls::legacy()` (embedded `legacy_parser/adl/*.txt`) |
| `dump.hpp` | `hir_dump` / `quantity_table_dump` / `object_table` |
| `ops.hpp` / `diag.hpp` | ops + diagnostics copied so HIR does not include syntax |
| `rat.hpp` / `num.hpp` | exact `Rat` / `NumVal` (`0.3` is `3/10`) |
| `sema.hpp` | umbrella |

`analyze(FileAst)` stays private (`src/resolver.hpp`).

## Identity

Collections and quantities are interned by structural keys. Names are
labels. Unsupported element predicates never share intern ids.
`jets[-1].pt` is `FromBack(1)`. `size` / `Size` / `count` are aliases.

Out-of-fragment constructs are tagged `Fragment::unsupported(reason)`.

## Dumps

```bash
./reimplementation/smash_cpp2/build/smash_cpp2 check --dump-hir examples/tutorials/ex00_helloworld.adl
./reimplementation/smash_cpp2/build/smash_cpp2 check --dump-quantities examples/tutorials/ex00_helloworld.adl
```

Stdout must match smash3. Logic lives here; cli only wires the flags.
