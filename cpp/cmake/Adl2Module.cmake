# Helpers for adl2_* CMake targets (mirror Rust crate map).
#
# Spine (dependency direction only this way):
#   syntax → sema → {interp ∥ formula} → axioms → solver → analysis → certify
#   viz → sema
#   cli wires libs; does not own core logic.

function(adl2_add_library name)
  cmake_parse_arguments(ARG "STUB" "" "SOURCES;PUBLIC_DEPS;PRIVATE_DEPS" ${ARGN})
  add_library(${name} STATIC ${ARG_SOURCES})
  target_include_directories(${name} PUBLIC ${PROJECT_SOURCE_DIR}/include)
  target_compile_options(${name} PRIVATE -Wall -Wextra -Wpedantic)
  if(ARG_PUBLIC_DEPS)
    target_link_libraries(${name} PUBLIC ${ARG_PUBLIC_DEPS})
  endif()
  if(ARG_PRIVATE_DEPS)
    target_link_libraries(${name} PRIVATE ${ARG_PRIVATE_DEPS})
  endif()
  if(ARG_STUB)
    # Keep stubs linkable and visible in `cmake --build` graphs.
    target_compile_definitions(${name} PUBLIC ADL2_MODULE_STUB=1)
  endif()
endfunction()
