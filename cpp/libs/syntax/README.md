# `adl2_syntax` (P1 — filled)

Lexer, hand-written recursive-descent parser, dump-shaped AST, and
canonical `dump_ast` matching Rust `adl-syntax` / `smash2 check --dump-ast`.

- Headers: `libs/syntax/include/adl2/syntax/`
- Collaborator grammar: `../../grammar.ebnf`, `../../BISON_MAP.md`
- No dependency on sema / analysis / solver.

CLI dump wiring lives in `adl2_cli` (`libs/cli`); this library owns parse/dump
logic. P2 added lexer notes for `_<digit>` splits (Rust `adl-syntax` parity)
so HIR diagnostic sections match; dump-ast output is unchanged (no diags).
