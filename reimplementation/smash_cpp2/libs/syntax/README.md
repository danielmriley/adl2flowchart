# `adl2_syntax`

Lexer, hand-written recursive-descent parser, dump-shaped AST, and
canonical `dump_ast` matching smash3 `check --dump-ast`.

- Headers: `libs/syntax/include/adl2/syntax/`
- Collaborator grammar: `../../grammar.ebnf`, `../../method_map.txt`,
  `../../BISON_MAP.md`, `stmt_dispatch.hpp`
- One `parse_*` per `grammar.ebnf` nonterminal. New keywords go in
  the statement/section table, not a third if-chain.
- `../../scripts/grammar_check.py` is the early warning (no ADL parse).
- Flex and Bison are not the implementation.

Diagnostics follow `LANGUAGE.md`. `~=` and `[-n]` are closed; messages
do not say an item is open.
