This directory holds the **committed golden** for `adl2_rdgen --emit-expr`.

CMake compiles the copy generated into the build tree
(`libs/syntax/rdgen/parser_expr.inc.hpp`), not this file. `ctest`
`adl2_rdgen_expr_golden` diffs them. Update this golden in the same
commit as any emitter or `grammar.ebnf` change that alters the ladder.
Do not edit `parser_expr.inc.hpp` by hand — change `tools/rdgen/` or
the EBNF and re-emit.
