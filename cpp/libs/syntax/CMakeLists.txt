# adl2_syntax — lexer, RD parser, AST, dump_ast (Rust adl-syntax).
# Expression-ladder parse_* bodies are generated at compile time from
# grammar.ebnf by adl2_rdgen (see ../../RDGEN.md). Hooks stay in parser.cpp.

set(RDGEN_GEN_DIR ${CMAKE_CURRENT_BINARY_DIR}/rdgen)
set(RDGEN_EXPR ${RDGEN_GEN_DIR}/parser_expr.inc.hpp)
set(RDGEN_EBNF ${CMAKE_SOURCE_DIR}/grammar.ebnf)
set(RDGEN_MAP ${CMAKE_SOURCE_DIR}/tools/rdgen/method_map.txt)
set(RDGEN_HPP ${CMAKE_CURRENT_SOURCE_DIR}/include/adl2/syntax/parser.hpp)

add_custom_command(
  OUTPUT ${RDGEN_EXPR}
  COMMAND ${CMAKE_COMMAND} -E make_directory ${RDGEN_GEN_DIR}
  COMMAND adl2_rdgen
          --ebnf ${RDGEN_EBNF}
          --parser-hpp ${RDGEN_HPP}
          --map ${RDGEN_MAP}
          --check
          --emit-expr ${RDGEN_EXPR}
  DEPENDS
    adl2_rdgen
    ${RDGEN_EBNF}
    ${RDGEN_HPP}
    ${RDGEN_MAP}
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/ebnf.cpp
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/check.cpp
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/emit.cpp
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/main.cpp
  COMMENT "adl2_rdgen: check grammar.ebnf ↔ parse_* and emit expression ladder"
  VERBATIM
)

add_custom_target(adl2_syntax_rdgen DEPENDS ${RDGEN_EXPR})

adl2_add_library(adl2_syntax
  SOURCES
    src/diag.cpp
    src/dump.cpp
    src/lexer.cpp
    src/parser.cpp
)

add_dependencies(adl2_syntax adl2_syntax_rdgen)
target_include_directories(adl2_syntax PRIVATE ${RDGEN_GEN_DIR})
set_source_files_properties(${RDGEN_EXPR} PROPERTIES
  GENERATED TRUE
  HEADER_FILE_ONLY TRUE
)
target_sources(adl2_syntax PRIVATE ${RDGEN_EXPR})
set_source_files_properties(src/parser.cpp PROPERTIES
  OBJECT_DEPENDS ${RDGEN_EXPR}
)
