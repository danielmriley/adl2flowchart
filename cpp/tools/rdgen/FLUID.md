# Fluid grammar — slice 0+1 contract

Campaign branch: `cursor/rdgen-grammar-fluid-32f3` (stacked on
`cursor/parser-generator-32f3`). Full design: grammar is the vocabulary;
`lexer.cpp` becomes scan-engine-only in a later slice. This file is the
**slice 0+1** contract so parallel worktrees do not invent a fourth design.

## Identity rule

Production membership is precedence, not meaning.

- **Alias table** (`aliases.txt`): `||`→`or`, `&&`→`and`, `!`→`not`.
  These dump as the canonical key. Sitting next to `"or"` does **not**
  make a new word an alias.
- **New word** (e.g. `"xor"` in `or-expr`): its own key. dump
  `Binary op=xor`. sema **Unsupported**, never `Or`, never
  `ArithOp::Add`.
- **Forbidden:** sibling inherit (`xor`→`KwOr`/`BinOp::Or`).
- **`sel`:** out of this slice. Do not keep or extend the select-synonym
  mutate. Statement keywords need generated dispatch (slice 2).

## Slice ownership (do not cross)

| Worktree / branch | Owns | Must not touch |
|---|---|---|
| `rdgen-probes` / `cursor/rdgen-probes-32f3` | new files under `cpp/tests/fixtures/`; dump-ast ctests in `cpp/tests/CMakeLists.txt` | anything under `cpp/tools/rdgen/`, `ast.hpp`, `lexer.cpp`, `parser.cpp`, `sema/` |
| `rdgen-identity` / `cursor/rdgen-op-identity-32f3` | emit, literals (kill inherit), `ast.hpp` key, dump, mutate graph, `rdgen_unit` / `mutate_parse`, `resolve_expr.cpp` Add-default, `RDGEN.md` | `cpp/tests/CMakeLists.txt`; `inventory.cpp` / `inventory.hpp`; new `examples/*.adl` |
| `rdgen-inventory` / `cursor/rdgen-inventory-32f3` | `inventory.cpp` / `inventory.hpp`, unit coverage, `--dump-inventory` if needed | `emit.cpp`, `literals.cpp`, `ast.hpp`, `dump.cpp`, mutate*, `sema/`, `cpp/tests/` |

`aliases.txt` and this file are owned by the integration branch. Identity
**reads** the alias table; inventory **classifies** literals against it
and the extras pin. Do not rewrite `aliases.txt` unless the three stock
rows are wrong.

## Must not break

- Frozen `grammar.ebnf`: dump-ast 146 / `CROSS_ORACLE=1` byte-identical.
- `smash2_cpp_dump_ast_tiny` / `bins_and_path`.
- `bins` stays `Ident`. `path-token` not greedy. `->` still `Arrow`.
- No Flex/Bison/Python. No new `examples/*.adl`.
- `CXX=g++`. Host tool `adl2_rdgen` stays unlinkable against `adl2_*`.

## Later slices (do not start)

2. Generate `parser.hpp` decls + Choice dispatch + first-set predicates.
3. Empty `lexer.cpp` of keyword/operator names (complete generated tables).
