# Compare adl2_rdgen --emit-keywords output to the committed golden.
if(NOT GEN OR NOT EBNF OR NOT HPP OR NOT MAP OR NOT GOLDEN OR NOT OUT)
  message(FATAL_ERROR "compare_keywords.cmake: missing required -D variables")
endif()
execute_process(
  COMMAND "${GEN}"
          --ebnf "${EBNF}"
          --parser-hpp "${HPP}"
          --map "${MAP}"
          --check
          --emit-keywords "${OUT}"
  RESULT_VARIABLE rc
)
if(rc)
  message(FATAL_ERROR "adl2_rdgen --emit-keywords failed (exit ${rc})")
endif()
execute_process(
  COMMAND "${CMAKE_COMMAND}" -E compare_files "${GOLDEN}" "${OUT}"
  RESULT_VARIABLE rc2
)
if(rc2)
  message(FATAL_ERROR
    "generated keyword synonyms differ from ${GOLDEN}\n"
    "Update libs/syntax/generated/keyword_synonyms.inc.hpp")
endif()
