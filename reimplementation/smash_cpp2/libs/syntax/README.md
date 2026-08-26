# `adl2_syntax`

Lexer, hand-written recursive-descent parser, dump-shaped AST, and
canonical `dump_ast` matching smash3 `check --dump-ast`.

- Headers: `libs/syntax/include/adl2/syntax/`
- Collaborator grammar: `../../grammar.ebnf`, `../../BISON_MAP.md`
- One `parse_*` per `grammar.ebnf` nonterminal.
- Flex and Bison are not the implementation.

Diagnostics follow `LANGUAGE.md`. `~=` and `[-n]` are closed; messages
do not say an item is open.
