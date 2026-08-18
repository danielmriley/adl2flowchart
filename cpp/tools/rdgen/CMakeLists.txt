# adl2_rdgen — host EBNF → recursive-descent emitter (RDGEN.md).
# No dependency on adl2_* libraries (would be a bootstrap cycle).

add_library(adl2_rdgen_lib STATIC
  src/ebnf.cpp
  src/check.cpp
  src/emit.cpp
  src/literals.cpp
  src/inventory.cpp
)
target_include_directories(adl2_rdgen_lib PUBLIC
  ${CMAKE_CURRENT_SOURCE_DIR}/include
)
target_compile_options(adl2_rdgen_lib PRIVATE -Wall -Wextra -Wpedantic)
target_compile_features(adl2_rdgen_lib PUBLIC cxx_std_17)

add_executable(adl2_rdgen src/main.cpp)
target_link_libraries(adl2_rdgen PRIVATE adl2_rdgen_lib)
target_compile_options(adl2_rdgen PRIVATE -Wall -Wextra -Wpedantic)

add_executable(adl2_rdgen_unit tests/rdgen_unit.cpp)
target_link_libraries(adl2_rdgen_unit PRIVATE adl2_rdgen_lib)
target_compile_options(adl2_rdgen_unit PRIVATE -Wall -Wextra -Wpedantic)
target_compile_definitions(adl2_rdgen_unit PRIVATE
  ADL2_GRAMMAR_EBNF="${CMAKE_SOURCE_DIR}/grammar.ebnf"
  ADL2_PARSER_HPP="${CMAKE_SOURCE_DIR}/libs/syntax/include/adl2/syntax/parser.hpp"
  ADL2_METHOD_MAP="${CMAKE_CURRENT_SOURCE_DIR}/method_map.txt"
)

add_test(NAME adl2_rdgen_unit COMMAND adl2_rdgen_unit)

add_executable(adl2_rdgen_inventory tests/inventory_unit.cpp)
target_link_libraries(adl2_rdgen_inventory PRIVATE adl2_rdgen_lib)
target_compile_options(adl2_rdgen_inventory PRIVATE -Wall -Wextra -Wpedantic)
target_compile_definitions(adl2_rdgen_inventory PRIVATE
  ADL2_GRAMMAR_EBNF="${CMAKE_SOURCE_DIR}/grammar.ebnf"
)
add_test(NAME adl2_rdgen_inventory COMMAND adl2_rdgen_inventory)

add_test(NAME adl2_rdgen_check
  COMMAND adl2_rdgen
    --ebnf ${CMAKE_SOURCE_DIR}/grammar.ebnf
    --parser-hpp ${CMAKE_SOURCE_DIR}/libs/syntax/include/adl2/syntax/parser.hpp
    --map ${CMAKE_CURRENT_SOURCE_DIR}/method_map.txt
    --check)
add_test(NAME adl2_rdgen_expr_golden
  COMMAND ${CMAKE_COMMAND}
    -DGEN=$<TARGET_FILE:adl2_rdgen>
    -DEBNF=${CMAKE_SOURCE_DIR}/grammar.ebnf
    -DHPP=${CMAKE_SOURCE_DIR}/libs/syntax/include/adl2/syntax/parser.hpp
    -DMAP=${CMAKE_CURRENT_SOURCE_DIR}/method_map.txt
    -DGOLDEN=${CMAKE_SOURCE_DIR}/libs/syntax/generated/parser_expr.inc.hpp
    -DOUT=${CMAKE_CURRENT_BINARY_DIR}/parser_expr.got.hpp
    -P ${CMAKE_CURRENT_SOURCE_DIR}/compare_emit.cmake)

add_test(NAME adl2_rdgen_keywords_golden
  COMMAND ${CMAKE_COMMAND}
    -DGEN=$<TARGET_FILE:adl2_rdgen>
    -DEBNF=${CMAKE_SOURCE_DIR}/grammar.ebnf
    -DHPP=${CMAKE_SOURCE_DIR}/libs/syntax/include/adl2/syntax/parser.hpp
    -DMAP=${CMAKE_CURRENT_SOURCE_DIR}/method_map.txt
    -DGOLDEN=${CMAKE_SOURCE_DIR}/libs/syntax/generated/keyword_synonyms.inc.hpp
    -DOUT=${CMAKE_CURRENT_BINARY_DIR}/keyword_synonyms.got.hpp
    -P ${CMAKE_CURRENT_SOURCE_DIR}/compare_keywords.cmake)

# Compile-time proof: the same grammar.ebnf plus two sibling synonyms
# (xor of or, sel of select) rebuilds lexer keywords; AST is wired
# without editing parser.cpp / lexer.cpp.
set(MUTATE_DIR ${CMAKE_CURRENT_BINARY_DIR}/mutate)
set(MUTATE_EBNF ${MUTATE_DIR}/grammar.ebnf)
set(MUTATE_KWS ${MUTATE_DIR}/keyword_synonyms.inc.hpp)
add_custom_command(
  OUTPUT ${MUTATE_EBNF} ${MUTATE_KWS}
  COMMAND ${CMAKE_COMMAND}
          -DGEN=$<TARGET_FILE:adl2_rdgen>
          -DEBNF=${CMAKE_SOURCE_DIR}/grammar.ebnf
          -DHPP=${CMAKE_SOURCE_DIR}/libs/syntax/include/adl2/syntax/parser.hpp
          -DMAP=${CMAKE_CURRENT_SOURCE_DIR}/method_map.txt
          -DOUT_EBNF=${MUTATE_EBNF}
          -DOUT_KWS=${MUTATE_KWS}
          -P ${CMAKE_CURRENT_SOURCE_DIR}/mutate.cmake
  DEPENDS
    adl2_rdgen
    ${CMAKE_SOURCE_DIR}/grammar.ebnf
    ${CMAKE_CURRENT_SOURCE_DIR}/mutate.cmake
    ${CMAKE_CURRENT_SOURCE_DIR}/method_map.txt
  COMMENT "adl2_rdgen: mutate grammar.ebnf (xor/sel) and emit keyword synonyms"
  VERBATIM
)
add_custom_target(adl2_rdgen_mutate_kws DEPENDS ${MUTATE_EBNF} ${MUTATE_KWS})

add_executable(adl2_rdgen_mutate_parse
  tests/mutate_parse.cpp
  ${CMAKE_SOURCE_DIR}/libs/syntax/src/diag.cpp
  ${CMAKE_SOURCE_DIR}/libs/syntax/src/dump.cpp
  ${CMAKE_SOURCE_DIR}/libs/syntax/src/lexer.cpp
  ${CMAKE_SOURCE_DIR}/libs/syntax/src/parser.cpp
)
target_include_directories(adl2_rdgen_mutate_parse PRIVATE
  ${MUTATE_DIR}
  ${CMAKE_BINARY_DIR}/libs/syntax/rdgen
  ${CMAKE_SOURCE_DIR}/libs/syntax/include
)
target_compile_options(adl2_rdgen_mutate_parse PRIVATE -Wall -Wextra -Wpedantic)
add_dependencies(adl2_rdgen_mutate_parse adl2_rdgen_mutate_kws adl2_syntax_rdgen)
set_property(SOURCE ${CMAKE_SOURCE_DIR}/libs/syntax/src/lexer.cpp
  APPEND PROPERTY OBJECT_DEPENDS ${MUTATE_KWS})
set_property(SOURCE ${CMAKE_SOURCE_DIR}/libs/syntax/src/parser.cpp
  APPEND PROPERTY OBJECT_DEPENDS
  ${CMAKE_BINARY_DIR}/libs/syntax/rdgen/parser_expr.inc.hpp)
add_test(NAME adl2_rdgen_mutate_parse COMMAND adl2_rdgen_mutate_parse)
