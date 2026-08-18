# Fluid grammar — slice 0–2 contract

Campaign branch: `cursor/rdgen-grammar-fluid-32f3` (stacked on
`cursor/parser-generator-32f3`). Full design: grammar is the vocabulary;
`lexer.cpp` becomes scan-engine-only in a later slice. This file is the
**slice 0–2** contract so parallel worktrees do not invent a fourth design.

## Identity rule

Production membership is precedence, not meaning.

- **Alias table** (`aliases.txt`): `||`→`or`, `&&`→`and`, `!`→`not`.
  These dump as the canonical key. Sitting next to `"or"` does **not**
  make a new word an alias.
- **New word** (e.g. `"xor"` in `or-expr`): its own key. dump
  `Binary op=xor`. sema **Unsupported**, never `Or`, never
  `ArithOp::Add`.
- **Forbidden:** sibling inherit (`xor`→`KwOr`/`BinOp::Or`).
- **`sel`:** own Cut keyword (`Cut kw=sel`), via generated first-sets
  (slice 2). Sitting next to `"select"` does **not** dump `Cut kw=select`.

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

## Slice 2 (dispatch)

- Generate `parse_section`, `parse_region_stmt`, `at_section_start`,
  `at_stmt_keyword`, `is_cut_keyword`, `is_reject_keyword` into
  `parser_dispatch.inc.hpp`. Do **not** put generated decls in
  `parser.hpp` (every TU includes it; `RDGEN_GEN_DIR` is PRIVATE).
- Unmapped `keywords condition` productions are inferred generate:
  inlined as `Ident` + `Cut` with `keyword = lowercase(token.text)`.
  No `parse_foo_stmt` method is invented.
- `bins` stays contextual (`KwBin` or `Ident "bins"` + not line-end).
- `take`/`using` still prefix a region-ref (not in the EBNF Choice).
- `trigger` is in **both** section-start (object-block) and stmt-start.
- Object-block calls `is_cut_keyword` / `is_reject_keyword` and converts
  the generated `RegionStmt` — it does not inline cut/reject.

## Later slices (do not start)

3. Empty `lexer.cpp` of keyword/operator names (complete generated tables).
   Do not generate a Flex scanner. Do not start this until dispatch is in.
