# BISON_MAP

For collaborators who know Flex/Bison. The grammar is `grammar.ebnf`.
Each nonterminal maps to one function in `libs/syntax/src/parser.cpp`.

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

| EBNF | Function |
|---|---|
| `file` | `parse_file` |
| `section` | dispatch in `parse_file` |
| `info-block` | `parse_info_block` |
| `define` | `parse_define_section` / `parse_object_define` |
| `object-block` | `parse_object_block` |
| `take-stmt` | `parse_take_stmt` |
| `region-block` | `parse_region_block` |
| `region-stmt` | `parse_region_stmt` |
| `bin-stmt` | `parse_bin_stmt` |
| `condition` | `parse_condition` |
| `ternary` | `parse_ternary` |
| `or-expr` / `and-expr` / `not-expr` | `parse_or_expr` / `parse_and_expr` / `parse_not_expr` |
| `comparison` | `parse_comparison` |
| `additive` / `multiplicative` / `unary` | `parse_additive` / `parse_multiplicative` / `parse_unary` |
| `postfix` / `primary` | `parse_postfix` / `parse_primary` |

A bison rule `region_block: REGION ident region_stmts` is
`parse_region_block`. It consumes the keyword and name, then loops
`parse_region_stmt` until the next section keyword or EOF.

## Hostile productions (stay hand-written)

- Column-1 `define` vs indented object-define (`at_column_one`)
- Contextual `bins`
- Path tokens in arg position
- Particle lists (`pT(jets[0] jets[1])`)
- Bin-body fork (number list vs boolean condition)
- Newline-greedy info lines, counts, sort

Do not hide these in `%expect`.
