# smash_cpp2 CLI — wires libraries; owns no parse logic.
# Binary name is smash_cpp2. Recheck is smash_cpp2-recheck.
add_executable(smash_cpp2 src/main.cpp)
target_link_libraries(smash_cpp2 PRIVATE adl2_axioms adl2_formula adl2_interp adl2_sema adl2_syntax)
target_compile_options(smash_cpp2 PRIVATE -Wall -Wextra -Wpedantic)
set_target_properties(smash_cpp2 PROPERTIES
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)

add_executable(smash_cpp2-recheck src/recheck.cpp)
target_compile_options(smash_cpp2-recheck PRIVATE -Wall -Wextra -Wpedantic)
set_target_properties(smash_cpp2-recheck PROPERTIES
  RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}"
)
