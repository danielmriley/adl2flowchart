# adl2_cli — wires libraries; owns no core logic.
# Binary name remains smash2_cpp (oracle-facing). P4: + adl2_viz (`dot` / `dot --ast`).
add_executable(smash2_cpp src/main.cpp)
add_executable(adl2_cli ALIAS smash2_cpp)
target_link_libraries(smash2_cpp PRIVATE
  adl2_sema
  adl2_syntax
  adl2_formula
  adl2_interp
  adl2_axioms
  adl2_viz
)
target_compile_options(smash2_cpp PRIVATE -Wall -Wextra -Wpedantic)
set_target_properties(smash2_cpp PROPERTIES
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)

# Remaining stub modules stay in the default graph without linking into cli.
add_custom_target(adl2_module_stubs DEPENDS
  adl2_util
  adl2_solver
  adl2_analysis
  adl2_certify
)
add_dependencies(smash2_cpp adl2_module_stubs)
