# Expect this compile to FAIL (layering: analysis must not see syntax headers).
# Invoked as: cmake -DCOMPILER=... -DSOURCE=... -DINCLUDES=... -P expect_compile_fail.cmake
if(NOT COMPILER OR NOT SOURCE)
  message(FATAL_ERROR "COMPILER and SOURCE are required")
endif()

set(inc_flags)
foreach(d IN LISTS INCLUDES)
  if(d)
    list(APPEND inc_flags "-I${d}")
  endif()
endforeach()

execute_process(
  COMMAND ${COMPILER} -fsyntax-only -std=c++17 ${inc_flags} ${SOURCE}
  RESULT_VARIABLE rc
  OUTPUT_VARIABLE out
  ERROR_VARIABLE err
)
if(rc EQUAL 0)
  message(FATAL_ERROR
    "layering violation: analysis include path compiled ${SOURCE}\n${out}${err}")
endif()
