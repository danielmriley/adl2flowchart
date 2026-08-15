# adl2_rdgen — host EBNF → recursive-descent emitter (RDGEN.md).
# No dependency on adl2_* libraries (would be a bootstrap cycle).

add_library(adl2_rdgen_lib STATIC
  src/ebnf.cpp
  src/check.cpp
  src/emit.cpp
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
