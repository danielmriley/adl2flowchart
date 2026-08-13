# P2 tests — no external test framework required.
# Requires CMake ≥ 3.20 (ctest --test-dir in CI).
# dump-ast corpus gate: cpp/scripts/dump_ast_corpus_gate.sh
# dump-hir oracle gate: cpp/scripts/dump_hir_corpus_gate.sh

set(SMASH2_CPP $<TARGET_FILE:smash2_cpp>)
set(FIXTURE_TINY "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/tiny.adl")
set(FIXTURE_BINS "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/bins_and_path.adl")
set(GOLDEN_TINY "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/tiny.adl.dump")
set(GOLDEN_BINS "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/bins_and_path.adl.dump")
set(DUMP_PARITY "${CMAKE_CURRENT_SOURCE_DIR}/dump_parity.sh")

add_executable(adl2_sema_identity identity.cpp)
target_link_libraries(adl2_sema_identity PRIVATE adl2_sema adl2_syntax)
target_compile_options(adl2_sema_identity PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME smash2_cpp_help
  COMMAND ${SMASH2_CPP} --help)

add_test(NAME smash2_cpp_check_tiny
  COMMAND ${SMASH2_CPP} check ${FIXTURE_TINY})

add_test(NAME smash2_cpp_check_bins_and_path
  COMMAND ${SMASH2_CPP} check ${FIXTURE_BINS})

add_test(NAME smash2_cpp_bare_check_parse_only_note
  COMMAND ${SMASH2_CPP} check ${FIXTURE_TINY})

add_test(NAME hir_gate_allowlist_count
  COMMAND bash ${CMAKE_SOURCE_DIR}/scripts/dump_hir_corpus_gate.sh)
set_property(TEST hir_gate_allowlist_count PROPERTY ENVIRONMENT
  "COUNT_ONLY=1"
  "SKIP_BUILD=1")

add_test(NAME smash2_cpp_dump_ast_tiny
  COMMAND bash ${DUMP_PARITY} ${SMASH2_CPP} ${FIXTURE_TINY} ${GOLDEN_TINY})

add_test(NAME smash2_cpp_dump_ast_bins_and_path
  COMMAND bash ${DUMP_PARITY} ${SMASH2_CPP} ${FIXTURE_BINS} ${GOLDEN_BINS})

add_test(NAME adl2_sema_identity
  COMMAND adl2_sema_identity)

add_executable(adl2_p3_unit p3.cpp)
target_link_libraries(adl2_p3_unit PRIVATE adl2_sema adl2_syntax adl2_formula adl2_interp adl2_axioms)
target_compile_options(adl2_p3_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_p3_unit
  COMMAND adl2_p3_unit)

add_test(NAME formula_gate_allowlist_count
  COMMAND bash ${CMAKE_SOURCE_DIR}/scripts/dump_formula_corpus_gate.sh)
set_property(TEST formula_gate_allowlist_count PROPERTY ENVIRONMENT
  "COUNT_ONLY=1"
  "SKIP_BUILD=1")

add_test(NAME interp_gate_pair_count
  COMMAND bash ${CMAKE_SOURCE_DIR}/scripts/interp_run_gate.sh)
set_property(TEST interp_gate_pair_count PROPERTY ENVIRONMENT
  "COUNT_ONLY=1"
  "SKIP_BUILD=1")

add_test(NAME layering_analysis_cannot_include_syntax
  COMMAND ${CMAKE_COMMAND}
    -DCOMPILER=${CMAKE_CXX_COMPILER}
    -DSOURCE=${CMAKE_CURRENT_SOURCE_DIR}/layering/cannot_include_syntax.cpp
    "-DINCLUDES=$<TARGET_PROPERTY:adl2_analysis,INTERFACE_INCLUDE_DIRECTORIES>"
    -P ${CMAKE_CURRENT_SOURCE_DIR}/layering/expect_compile_fail.cmake)

set_tests_properties(smash2_cpp_help PROPERTIES
  PASS_REGULAR_EXPRESSION "parse-only")

set_tests_properties(smash2_cpp_check_tiny PROPERTIES
  PASS_REGULAR_EXPRESSION "check: ok")

set_tests_properties(smash2_cpp_check_bins_and_path PROPERTIES
  PASS_REGULAR_EXPRESSION "check: ok")

set_tests_properties(smash2_cpp_bare_check_parse_only_note PROPERTIES
  PASS_REGULAR_EXPRESSION "parse-only")

set_tests_properties(adl2_sema_identity PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=128 FAIL=0")

set_tests_properties(adl2_p3_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=116 FAIL=0")
