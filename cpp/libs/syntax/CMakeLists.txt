# adl2_syntax — lexer, RD parser, AST, dump_ast (Rust adl-syntax).
# parse_* bodies + keyword synonyms are generated at compile time from
# grammar.ebnf by adl2_rdgen (see ../../RDGEN.md). Hooks stay in parser.cpp.

set(RDGEN_GEN_DIR ${CMAKE_CURRENT_BINARY_DIR}/rdgen)
set(RDGEN_EXPR ${RDGEN_GEN_DIR}/parser_expr.inc.hpp)
set(RDGEN_KWS ${RDGEN_GEN_DIR}/keyword_synonyms.inc.hpp)
set(RDGEN_EBNF ${CMAKE_SOURCE_DIR}/grammar.ebnf)
set(RDGEN_MAP ${CMAKE_SOURCE_DIR}/tools/rdgen/method_map.txt)
set(RDGEN_HPP ${CMAKE_CURRENT_SOURCE_DIR}/include/adl2/syntax/parser.hpp)

add_custom_command(
  OUTPUT ${RDGEN_EXPR} ${RDGEN_KWS}
  COMMAND ${CMAKE_COMMAND} -E make_directory ${RDGEN_GEN_DIR}
  COMMAND adl2_rdgen
          --ebnf ${RDGEN_EBNF}
          --parser-hpp ${RDGEN_HPP}
          --map ${RDGEN_MAP}
          --check
          --emit-expr ${RDGEN_EXPR}
          --emit-keywords ${RDGEN_KWS}
  DEPENDS
    adl2_rdgen
    ${RDGEN_EBNF}
    ${RDGEN_HPP}
    ${RDGEN_MAP}
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/ebnf.cpp
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/check.cpp
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/emit.cpp
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/literals.cpp
    ${CMAKE_SOURCE_DIR}/tools/rdgen/src/main.cpp
  COMMENT "adl2_rdgen: check grammar.ebnf, emit parse_* + keyword synonyms"
  VERBATIM
)

add_custom_target(adl2_syntax_rdgen DEPENDS ${RDGEN_EXPR} ${RDGEN_KWS})

adl2_add_library(adl2_syntax
  SOURCES
    src/diag.cpp
    src/dump.cpp
    src/lexer.cpp
    src/parser.cpp
)

add_dependencies(adl2_syntax adl2_syntax_rdgen)
target_include_directories(adl2_syntax PRIVATE ${RDGEN_GEN_DIR})
set_source_files_properties(${RDGEN_EXPR} ${RDGEN_KWS} PROPERTIES
  GENERATED TRUE
  HEADER_FILE_ONLY TRUE
)
target_sources(adl2_syntax PRIVATE ${RDGEN_EXPR} ${RDGEN_KWS})
set_property(SOURCE src/parser.cpp APPEND PROPERTY OBJECT_DEPENDS ${RDGEN_EXPR})
set_property(SOURCE src/lexer.cpp APPEND PROPERTY OBJECT_DEPENDS ${RDGEN_KWS})
