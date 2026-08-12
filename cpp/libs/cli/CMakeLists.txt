# adl2_cli / smash2_cpp — wires libraries; owns no core logic.
# P1: check --dump-ast uses adl2_syntax only.
add_executable(smash2_cpp src/main.cpp)
target_include_directories(smash2_cpp PRIVATE ${PROJECT_SOURCE_DIR}/include)
target_link_libraries(smash2_cpp PRIVATE adl2_syntax)
target_compile_options(smash2_cpp PRIVATE -Wall -Wextra -Wpedantic)
# Keep the binary at the build root for scripts/docs (`cpp/build/smash2_cpp`).
set_target_properties(smash2_cpp PROPERTIES
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)

# Ensure stub modules are built in the default graph (layout exists / links).
add_dependencies(smash2_cpp
  adl2_util
  adl2_sema
  adl2_formula
  adl2_interp
  adl2_axioms
  adl2_solver
  adl2_analysis
  adl2_certify
  adl2_viz
)
