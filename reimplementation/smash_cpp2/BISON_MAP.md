# BISON_MAP

For collaborators who know Flex/Bison. The grammar is `grammar.ebnf`.
Each nonterminal maps to one function in `libs/syntax/src/parser.cpp`
(`method_map.txt` is the complete list). Keyword dispatch is
`stmt_dispatch.hpp` — one table for sections, one for region-stmts.
`scripts/grammar_check.py` checks the EBNF, the map, the tables, and
FIRST overlaps without parsing ADL.

Flex and Bison are not the implementation.

## Tokens

| Bison idea | smash_cpp2 |
|---|---|
| Keyword tokens | `TokKind` in `libs/syntax/include/adl2/syntax/token.hpp`. Case-insensitive. |
| `IDENT` / numbers / strings | `Ident`, `Int` / `Real`, `String` |
| Path tokens | Not lexed. `parse_path_token` in arg position only. |
| `[]` / `][` | `BandIncl` / `BandExcl` tokens. Distinct from `[` indexing. |
| `bins` | Ident. `parse_region_stmt` decides region-ref vs bin-stmt. |

## Rules to functions

See `method_map.txt`. A bison rule `region_block: REGION ident region_stmts`
is `parse_region_block`. It consumes the keyword and name, then loops
`parse_region_stmt` until the next section keyword or EOF. Adding a
statement is a `parse_*` plus one `kRegionStmtTable` row, not a third
if-chain.

## Hostile productions (stay hand-written)

Listed in `grammar_hooks.txt` when they are FIRST overlaps. Also:

- Column-1 `define` vs indented object-define (`at_column_one`)
- Contextual `bins`
- Path tokens in arg position
- Particle lists (`pT(jets[0] jets[1])`)
- Bin-body fork (number list vs boolean condition)
- Newline-greedy info lines, counts, sort

Do not hide these in `%expect`.
