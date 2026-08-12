# adl2_cli — wires libraries; owns no core logic.
# Binary name remains smash2_cpp (oracle-facing). Links syntax only for P1.
add_executable(smash2_cpp src/main.cpp)
add_executable(adl2_cli ALIAS smash2_cpp)
target_include_directories(smash2_cpp PRIVATE ${PROJECT_SOURCE_DIR}/include)
# Layering: cli must NOT link analysis/parser stacks wholesale — P1 needs syntax.
target_link_libraries(smash2_cpp PRIVATE adl2_syntax)
target_compile_options(smash2_cpp PRIVATE -Wall -Wextra -Wpedantic)
# Keep the binary at the build root for scripts/docs (`cpp/build/smash2_cpp`).
set_target_properties(smash2_cpp PROPERTIES
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)

# Build stub modules in the default graph without linking them into cli
# (proves the spine compiles; does not blur cli → analysis).
add_custom_target(adl2_module_stubs DEPENDS
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
add_dependencies(smash2_cpp adl2_module_stubs)
