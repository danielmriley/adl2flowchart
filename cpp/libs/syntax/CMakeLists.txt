# adl2_syntax — lexer, RD parser, AST, dump_ast (Rust adl-syntax).
# P1 fills this module. No dependency on sema/analysis/etc.
adl2_add_library(adl2_syntax
  SOURCES
    src/diag.cpp
    src/dump.cpp
    src/lexer.cpp
    src/parser.cpp
)
