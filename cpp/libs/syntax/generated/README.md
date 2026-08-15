This directory holds **committed goldens** for `adl2_rdgen`:

- `parser_expr.inc.hpp` — generated `parse_*` bodies
- `keyword_synonyms.inc.hpp` — extra lexer keyword map entries
  (empty for the frozen grammar; synonyms appear when the EBNF
  adds a word next to a known keyword)

CMake compiles the copies generated into the build tree
(`libs/syntax/rdgen/`), not these files. `ctest` diffs them.
Do not edit the `.inc.hpp` files by hand — change `tools/rdgen/`
or `grammar.ebnf` and re-emit.
