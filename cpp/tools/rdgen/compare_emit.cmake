# Compare adl2_rdgen --emit-expr output to the committed golden.
if(NOT GEN OR NOT EBNF OR NOT HPP OR NOT MAP OR NOT GOLDEN OR NOT OUT)
  message(FATAL_ERROR "compare_emit.cmake: missing -DGEN/-DEBNF/-DHPP/-DMAP/-DGOLDEN/-DOUT")
endif()
execute_process(
  COMMAND "${GEN}"
          --ebnf "${EBNF}"
          --parser-hpp "${HPP}"
          --map "${MAP}"
          --check
          --emit-expr "${OUT}"
  RESULT_VARIABLE rc
)
if(rc)
  message(FATAL_ERROR "adl2_rdgen emit failed (exit ${rc})")
endif()
execute_process(
  COMMAND "${CMAKE_COMMAND}" -E compare_files "${GOLDEN}" "${OUT}"
  RESULT_VARIABLE rc2
)
if(rc2)
  message(FATAL_ERROR
    "generated expression ladder differs from ${GOLDEN}\n"
    "Re-run adl2_rdgen --emit-expr and update libs/syntax/generated/parser_expr.inc.hpp")
endif()
