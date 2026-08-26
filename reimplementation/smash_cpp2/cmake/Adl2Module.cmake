# Helpers for smash_cpp2 library targets.
#
# U05 spine: syntax → sema → interp → formula → cli.
# Later units add: axioms → solver
#                            ↘ certify ↗ analysis
# Include policy: each library PUBLIC-exports only
#   ${CMAKE_CURRENT_SOURCE_DIR}/include
# i.e. `libs/<module>/include/adl2/<module>/…`.

function(adl2_add_library name)
  cmake_parse_arguments(ARG "STUB" "" "SOURCES;PUBLIC_DEPS;PRIVATE_DEPS" ${ARGN})
  add_library(${name} STATIC ${ARG_SOURCES})
  target_include_directories(${name} PUBLIC
    $<BUILD_INTERFACE:${CMAKE_CURRENT_SOURCE_DIR}/include>
  )
  target_compile_options(${name} PRIVATE -Wall -Wextra -Wpedantic)
  if(ARG_PUBLIC_DEPS)
    target_link_libraries(${name} PUBLIC ${ARG_PUBLIC_DEPS})
  endif()
  if(ARG_PRIVATE_DEPS)
    target_link_libraries(${name} PRIVATE ${ARG_PRIVATE_DEPS})
  endif()
  if(ARG_STUB)
    target_compile_definitions(${name} PUBLIC ADL2_MODULE_STUB=1)
  endif()
endfunction()
