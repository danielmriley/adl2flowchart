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
| Bare weight-file path token | **Not** a lexer token (P1). Recognized only in arg position by `parse_path_token()` (Rust `try_path_token`); deprecation warning. `TokKind::PathLike` remains for API compat but is unused by the lexer. |
| Operators as character tokens | Explicit `TokKind` values (`Gt`, `AndAnd`, `OrOr`, `BandIncl` for `[]`, `Arrow` for `->`, `PlusMinus` for `+-`, …) |
| `yytext` / `yylval` | `Token { kind, text, span }` — no global lexer state shared with the parser |
| Flex patterns for ids/numbers | Hand-written scans in `src/syntax/lexer.cpp` following SPEC_LANGUAGE §2 (no hyphen-eating ids; no signed-literal lexing) |
| Contextual `bins` | **Not** a hard keyword — lexed as `Ident`; `parse_region_stmt` treats bare-line `bins` as `region-ref`, otherwise as `bin-stmt` |

## Rules → `parse_X()`

| EBNF nonterminal | Parser entry |
|---|---|
| `file` | `Parser::parse_file` |
| `section` | `parse_section` |
| `info-block` / `define` / `object-block` / `region-block` / … | `parse_info_block`, `parse_define_section`, `parse_object_block`, `parse_region_block`, … |
| `cut-stmt` / `reject-stmt` / `take-stmt` / … | `parse_cut_as_region` / object cut arms, `parse_reject_stmt`, `parse_take_stmt`, … |
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
| primary / call / group | `parse_primary` / `parse_func_call` | `(…)`, `{…}ident`, literals, `ALL`/`NONE`/`true`/`false` |
| postfix | `parse_postfix` | `.ident`, `->ident`, `[i]`, `_i`, slices, trailing `_` |
| unary | `parse_unary` | `-`, then postfix |
| multiplicative | `parse_multiplicative` | `* / ^` |
| additive | `parse_additive` | `+ -` |
| comparison / band | `parse_comparison` | `> < >= <= == != ~=`, `[]` / `][` bands; chains desugar to `and` of `Cmp` |
| not | `parse_not_expr` | `not` / `!` (recursive) |
| and | `parse_and_expr` | `and` / `&&` |
| or | `parse_or_expr` | `or` / `\|\|` |
| ternary | `parse_ternary` | `?` with optional `: else` (EBNF: `[ "?" ternary [ ":" ternary ] ]`) |
| condition | `parse_condition` | = `ternary` |

Entry point for every boolean/numeric expression production in the
statement grammar is `parse_condition()` (alias of the ternary ladder).

## Dump format (P1)

`adl2::dump_ast` matches Rust `adl_syntax::dump_ast` byte-for-byte:

- 2-space indent; root `File`
- Spans as `@line:col` (1-based) on section/stmt headers
- `Binary op=and|or|+|…` (never `&&`/`||`); `Unary op=not|-`; `Cmp op=…`
- `Num` uses raw unsigned lexeme (+ grammatical sign); strings via Rust
  Debug quoting (`rust_debug_str`)

Gate: `cpp/scripts/dump_ast_corpus_gate.sh` (146 files).

## Diagnostics expectations (grammar-shaped)

Unlike a Bison `%error` / `yyerror` string dump:

1. **Never abort on first error.** Record a diagnostic with span + label
   + optional help; resync at the next statement keyword (SPEC_LANGUAGE
   §3.2). Exit nonzero if any error was recorded.
2. **Name the nonterminal.** Messages should read like the grammar
   (`expected condition after select`, `expected take-source`) so a
   reader can jump from the diagnostic to `grammar.ebnf`.
3. **No silent accept.** Constructs outside the checked fragment become
   explicit unsupported/Error nodes with a reason — never a guessed
   AST (ADR-007).
4. **P1 honesty.** Incomplete / unsupported shapes still surface as
   diagnostics or `Sort (unsupported)` dump lines (matching Rust), not
   as invented smash2-parity claims.

## Relation to `legacy_parser/`

`legacy_parser/` remains a transitional secondary oracle (ADR-009). Do
**not** copy its Bison grammar into this tree as the implementation, and
do not extend its `Expr*` / string-identity layers. When a legacy
`%token` or rule confuses you, look up the SPEC_LANGUAGE divergence
table (§3.1) and the `parse_X` that replaced it.
