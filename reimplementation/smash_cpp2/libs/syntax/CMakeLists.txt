# adl2_syntax — lexer, RD parser, AST, dump_ast.
# One parse_* per grammar.ebnf nonterminal. No Flex, no Bison.
adl2_add_library(adl2_syntax
  SOURCES
    src/diag.cpp
    src/dump.cpp
    src/lexer.cpp
    src/parser.cpp
)
