# P0 smoke tests — no external test framework required.

set(SMASH2_CPP $<TARGET_FILE:smash2_cpp>)
set(FIXTURE "${CMAKE_CURRENT_SOURCE_DIR}/fixtures/tiny.adl")

add_test(NAME smash2_cpp_help
  COMMAND ${SMASH2_CPP} --help)

add_test(NAME smash2_cpp_check_tiny
  COMMAND ${SMASH2_CPP} check ${FIXTURE})

# --help must mention the check subcommand.
set_tests_properties(smash2_cpp_help PROPERTIES
  PASS_REGULAR_EXPRESSION "check")

# Tiny fixture should parse without harness errors.
set_tests_properties(smash2_cpp_check_tiny PROPERTIES
  PASS_REGULAR_EXPRESSION "check: ok")
