# P1 smoke tests — no external test framework required.
# Requires CMake ≥ 3.20 (ctest --test-dir in CI).
# Full corpus dump-diff vs Rust smash2 lives in
# cpp/scripts/dump_ast_corpus_gate.sh (CI job adl2-cpp).

set(SMASH2_CPP $<TARGET_FILE:smash2_cpp>)
set(FIXTURE_TINY "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/tiny.adl")
set(FIXTURE_BINS "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/bins_and_path.adl")

add_test(NAME smash2_cpp_help
  COMMAND ${SMASH2_CPP} --help)

add_test(NAME smash2_cpp_check_tiny
  COMMAND ${SMASH2_CPP} check ${FIXTURE_TINY})

add_test(NAME smash2_cpp_check_bins_and_path
  COMMAND ${SMASH2_CPP} check ${FIXTURE_BINS})

add_test(NAME smash2_cpp_dump_ast_tiny
  COMMAND ${SMASH2_CPP} check --dump-ast ${FIXTURE_TINY})

set_tests_properties(smash2_cpp_help PROPERTIES
  PASS_REGULAR_EXPRESSION "check")

set_tests_properties(smash2_cpp_check_tiny PROPERTIES
  PASS_REGULAR_EXPRESSION "check: ok")

# warnings (path-token deprecation) are fine; errors are not.
set_tests_properties(smash2_cpp_check_bins_and_path PROPERTIES
  PASS_REGULAR_EXPRESSION "check: ok")

set_tests_properties(smash2_cpp_dump_ast_tiny PROPERTIES
  PASS_REGULAR_EXPRESSION "File")
