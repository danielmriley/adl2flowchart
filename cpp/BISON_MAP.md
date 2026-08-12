# BISON_MAP — “if you know bison”

Short guide for collaborators who know Flex/Bison (or the
`legacy_parser/` grammar) and are landing in the hand-written
recursive-descent parser under `cpp/`.

**Bison/Flex are not the implementation.** The readable grammar is
[`grammar.ebnf`](grammar.ebnf) (from `SPEC_LANGUAGE.md` §3). Every
nonterminal there maps 1:1 to a `parse_<name>` function in
`include/adl2/parser.hpp` / `src/syntax/parser.cpp`.

## Token layer (`%token` → lexer)

| Bison-ish idea | Here |
|---|---|
| `%token SELECT REJECT DEFINE …` | `adl2::TokKind` keywords in `include/adl2/token.hpp`; matched case-insensitively by `Lexer` |
| `%token IDENT NUMBER STRING` | `TokKind::Ident`, `Int`/`Real`, `String` |
| Bare weight-file path token | `TokKind::PathLike` (lexer) + `parse_path_token()` (arg position only; deprecation warning) |
| Operators as character tokens | Explicit `TokKind` values (`Gt`, `AndAnd`, `OrOr`, `BandIncl` for `[]`, …) |
| `yytext` / `yylval` | `Token { kind, text, span }` — no global lexer state shared with the parser |
| Flex patterns for ids/numbers | Hand-written scans in `src/syntax/lexer.cpp` following SPEC_LANGUAGE §2 (no hyphen-eating ids; no signed-literal lexing) |
| Contextual `bins` | **Not** a hard keyword — lexed as `Ident`; `parse_region_stmt` treats bare-line `bins` as `region-ref`, otherwise as `bin-stmt` |

## Rules → `parse_X()`

| EBNF nonterminal | Parser entry |
|---|---|
| `file` | `Parser::parse_file` |
| `section` | `parse_section` |
| `info-block` / `define` / `object-block` / `region-block` / … | `parse_info_block`, `parse_define`, `parse_object_block`, `parse_region_block`, … |
| `cut-stmt` / `reject-stmt` / `take-stmt` / … | `parse_cut_stmt`, `parse_reject_stmt`, `parse_take_stmt`, … |
| `condition` … `primary` | layered expression parsers (below) |

If you would have written:

```bison
region_block: REGION ident region_stmts ;
```

you instead open `parse_region_block()`, consume the keyword + ident, then
loop calling `parse_region_stmt()` until the next section keyword or EOF.

## Precedence table (expressions)

SPEC_LANGUAGE deliberately uses **standard** precedence (divergence #1
from legacy: `or` binds looser than `and`). The C++ parser mirrors this
with layered functions — not `%left`/`%right` declarations:

| Tightness (high → low) | EBNF / function | Operators |
|---|---|---|
| primary / call / group | `parse_primary` / `parse_func_call` | `(…)`, `{…}ident`, literals |
| postfix | `parse_postfix` | `.ident`, `[i]`, `_i`, slices |
| unary | `parse_unary` | `-`, then postfix |
| multiplicative | `parse_multiplicative` | `* / ^` |
| additive | `parse_additive` | `+ -` |
| comparison / band | `parse_comparison` | `> < >= <= == != ~=`, `[]` / `][` bands |
| not | `parse_not_expr` | `not` / `!` (recursive) |
| and | `parse_and_expr` | `and` / `&&` |
| or | `parse_or_expr` | `or` / `\|\|` |
| ternary | `parse_ternary` | `?` with optional `: else` (EBNF: `[ "?" ternary [ ":" ternary ] ]`) |
| condition | `parse_condition` | = `ternary` |

Entry point for every boolean/numeric expression production in the
statement grammar is `parse_condition()` (alias of the ternary ladder).

## Diagnostics expectations (grammar-shaped)

Unlike a Bison `%error` / `yyerror` string dump:

1. **Never abort on first error.** Record a diagnostic with span + label
   + optional help; resync at the next statement keyword (SPEC_LANGUAGE
   §3.2). Exit nonzero if any error was recorded.
2. **Name the nonterminal.** Messages should read like the grammar
   (`expected condition after select`, `expected take-source`) so a
   reader can jump from the diagnostic to `grammar.ebnf`.
3. **No silent accept.** Constructs outside the checked fragment become
   explicit unsupported/Unknown nodes with a reason — never a guessed
   AST (ADR-007).
4. **P0 honesty.** Incomplete productions return a clear
   `not implemented: parse_<X>` diagnostic rather than pretending
   smash2 parity.

## Relation to `legacy_parser/`

`legacy_parser/` remains a transitional secondary oracle (ADR-009). Do
**not** copy its Bison grammar into this tree as the implementation, and
do not extend its `Expr*` / string-identity layers. When a legacy
`%token` or rule confuses you, look up the SPEC_LANGUAGE divergence
table (§3.1) and the `parse_X` that replaced it.
