# P4 tests — no external test framework required.
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

add_test(NAME smash2_cpp_check_stdout_empty
  COMMAND bash ${CMAKE_CURRENT_SOURCE_DIR}/check_silent.sh ${SMASH2_CPP} ${FIXTURE_TINY})

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

add_test(NAME axioms_gate_allowlist_count
  COMMAND bash ${CMAKE_SOURCE_DIR}/scripts/dump_axioms_corpus_gate.sh)
set_property(TEST axioms_gate_allowlist_count PROPERTY ENVIRONMENT
  "COUNT_ONLY=1"
  "SKIP_BUILD=1")

add_test(NAME objects_gate_allowlist_count
  COMMAND bash ${CMAKE_SOURCE_DIR}/scripts/dump_objects_corpus_gate.sh)
set_property(TEST objects_gate_allowlist_count PROPERTY ENVIRONMENT
  "COUNT_ONLY=1"
  "SKIP_BUILD=1")

add_executable(adl2_viz_unit viz.cpp)
target_link_libraries(adl2_viz_unit PRIVATE adl2_viz adl2_sema)
target_compile_options(adl2_viz_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_viz_unit
  COMMAND adl2_viz_unit)

add_test(NAME dot_gate_allowlist_count
  COMMAND bash ${CMAKE_SOURCE_DIR}/scripts/dump_dot_corpus_gate.sh)
set_property(TEST dot_gate_allowlist_count PROPERTY ENVIRONMENT
  "COUNT_ONLY=1"
  "SKIP_BUILD=1")

add_executable(adl2_solver_test solver.cpp)
target_link_libraries(adl2_solver_test PRIVATE adl2_solver)
target_compile_options(adl2_solver_test PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_solver_test
  COMMAND adl2_solver_test)

add_executable(adl2_region3 region3.cpp)
target_link_libraries(adl2_region3 PRIVATE adl2_sema adl2_syntax adl2_interp)
target_compile_options(adl2_region3 PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_region3
  COMMAND adl2_region3)

add_executable(adl2_interval_unit interval.cpp)
target_link_libraries(adl2_interval_unit PRIVATE adl2_analysis)
target_compile_options(adl2_interval_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_interval_unit
  COMMAND adl2_interval_unit)

add_executable(adl2_analysis_encode analysis_encode.cpp)
target_link_libraries(adl2_analysis_encode PRIVATE adl2_analysis)
target_compile_options(adl2_analysis_encode PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_analysis_encode
  COMMAND adl2_analysis_encode)

add_executable(adl2_analysis_solver analysis_solver.cpp)
target_link_libraries(adl2_analysis_solver PRIVATE adl2_analysis)
target_compile_options(adl2_analysis_solver PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_analysis_solver
  COMMAND adl2_analysis_solver)

add_executable(adl2_witness witness.cpp)
target_link_libraries(adl2_witness PRIVATE adl2_analysis)
target_compile_options(adl2_witness PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_witness
  COMMAND adl2_witness)

add_test(NAME layering_analysis_cannot_include_syntax
  COMMAND ${CMAKE_COMMAND}
    -DCOMPILER=${CMAKE_CXX_COMPILER}
    -DSOURCE=${CMAKE_CURRENT_SOURCE_DIR}/layering/cannot_include_syntax.cpp
    "-DINCLUDES=$<TARGET_PROPERTY:adl2_analysis,INTERFACE_INCLUDE_DIRECTORIES>"
    -P ${CMAKE_CURRENT_SOURCE_DIR}/layering/expect_compile_fail.cmake)

add_test(NAME layering_certify_cannot_include_analysis
  COMMAND ${CMAKE_COMMAND}
    -DCOMPILER=${CMAKE_CXX_COMPILER}
    -DSOURCE=${CMAKE_CURRENT_SOURCE_DIR}/layering/cannot_include_analysis.cpp
    "-DINCLUDES=$<TARGET_PROPERTY:adl2_certify,INTERFACE_INCLUDE_DIRECTORIES>"
    -P ${CMAKE_CURRENT_SOURCE_DIR}/layering/expect_compile_fail.cmake)

set_tests_properties(smash2_cpp_help PROPERTIES
  PASS_REGULAR_EXPRESSION "objects")

set_tests_properties(adl2_sema_identity PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=128 FAIL=0")

set_tests_properties(adl2_p3_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=123 FAIL=0")

set_tests_properties(adl2_viz_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

set_tests_properties(adl2_solver_test PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

set_tests_properties(adl2_region3 PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=30 FAIL=0")

set_tests_properties(adl2_interval_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=46 FAIL=0")

set_tests_properties(adl2_analysis_encode PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_certify_unit certify.cpp)
target_link_libraries(adl2_certify_unit PRIVATE adl2_certify)
target_compile_options(adl2_certify_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_certify_unit
  COMMAND adl2_certify_unit)

set_tests_properties(adl2_analysis_solver PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

set_tests_properties(adl2_certify_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=79 FAIL=0")

set_tests_properties(adl2_witness PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_objects_unit objects.cpp)
target_link_libraries(adl2_objects_unit PRIVATE adl2_sema adl2_syntax)
target_compile_options(adl2_objects_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_objects_unit
  COMMAND adl2_objects_unit)

set_tests_properties(adl2_objects_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=9 FAIL=0")

add_executable(adl2_cutflow_unit cutflow.cpp)
target_link_libraries(adl2_cutflow_unit PRIVATE adl2_interp adl2_sema adl2_syntax)
target_compile_options(adl2_cutflow_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_cutflow_unit
  COMMAND adl2_cutflow_unit)

set_tests_properties(adl2_cutflow_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=76 FAIL=0")

add_executable(adl2_histo_unit histo.cpp)
target_link_libraries(adl2_histo_unit PRIVATE adl2_interp adl2_sema adl2_syntax)
target_compile_options(adl2_histo_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_histo_unit
  COMMAND adl2_histo_unit)

set_tests_properties(adl2_histo_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=34 FAIL=0")

add_executable(adl2_bridges_unit bridges.cpp)
target_link_libraries(adl2_bridges_unit PRIVATE adl2_interp adl2_sema adl2_syntax)
target_compile_options(adl2_bridges_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_bridges_unit
  COMMAND adl2_bridges_unit)

set_tests_properties(adl2_bridges_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_sample_unit sample.cpp)
target_link_libraries(adl2_sample_unit PRIVATE adl2_interp adl2_sema adl2_syntax)
target_compile_options(adl2_sample_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_sample_unit
  COMMAND adl2_sample_unit)

set_tests_properties(adl2_sample_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_refute_unit refute.cpp)
target_link_libraries(adl2_refute_unit PRIVATE adl2_analysis)
target_compile_options(adl2_refute_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_refute_unit
  COMMAND adl2_refute_unit)

set_tests_properties(adl2_refute_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_analysis_gates analysis_gates.cpp)
target_link_libraries(adl2_analysis_gates PRIVATE adl2_analysis)
target_compile_options(adl2_analysis_gates PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_analysis_gates
  COMMAND adl2_analysis_gates)

set_tests_properties(adl2_analysis_gates PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_merge_unit merge.cpp)
target_link_libraries(adl2_merge_unit PRIVATE adl2_sema adl2_syntax)
target_compile_options(adl2_merge_unit PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_merge_unit
  COMMAND adl2_merge_unit)

set_tests_properties(adl2_merge_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_analysis_cross analysis_cross.cpp)
target_link_libraries(adl2_analysis_cross PRIVATE adl2_analysis)
target_compile_options(adl2_analysis_cross PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_analysis_cross
  COMMAND adl2_analysis_cross)

set_tests_properties(adl2_analysis_cross PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_analysis_combine analysis_combine.cpp)
target_link_libraries(adl2_analysis_combine PRIVATE adl2_analysis)
target_compile_options(adl2_analysis_combine PRIVATE -Wall -Wextra -Wpedantic)

add_test(NAME adl2_analysis_combine
  COMMAND adl2_analysis_combine)

set_tests_properties(adl2_analysis_combine PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=53 FAIL=0")

add_executable(adl2_rootfile_unit rootfile.cpp)
target_link_libraries(adl2_rootfile_unit PRIVATE adl2_rootfile)
target_compile_options(adl2_rootfile_unit PRIVATE -Wall -Wextra -Wpedantic)
target_compile_definitions(adl2_rootfile_unit PRIVATE
  ROOTFILE_FIXTURES_DIR="${CMAKE_SOURCE_DIR}/libs/rootfile/fixtures")

add_test(NAME adl2_rootfile_unit
  COMMAND adl2_rootfile_unit)

set_tests_properties(adl2_rootfile_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_executable(adl2_ingest_unit ingest.cpp)
target_link_libraries(adl2_ingest_unit PRIVATE adl2_ingest)
target_compile_options(adl2_ingest_unit PRIVATE -Wall -Wextra -Wpedantic)
target_compile_definitions(adl2_ingest_unit PRIVATE
  INGEST_FIXTURES_DIR="${CMAKE_SOURCE_DIR}/../reimplementation/adl2/crates/adl-ingest/fixtures")

add_test(NAME adl2_ingest_unit
  COMMAND adl2_ingest_unit)

set_tests_properties(adl2_ingest_unit PROPERTIES
  PASS_REGULAR_EXPRESSION "PASS=.* FAIL=0")

add_test(NAME smash2_cpp_recheck_usage
  COMMAND bash -c "$<TARGET_FILE:smash2_cpp-recheck>; test $? -eq 2")

add_test(NAME smash2_cpp_ingest_unknown_profile
  COMMAND bash -c "$<TARGET_FILE:smash2_cpp> ingest --profile atlas -o /tmp/smash2_cpp_never.jsonl; test $? -eq 2")
