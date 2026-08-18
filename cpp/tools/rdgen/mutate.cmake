# Mutate grammar.ebnf: add xor to the or-expr group (identity, not inherit)
# and emit both the expression ladder and the (empty extra) keyword map.
# Invoked as:
#   cmake -DGEN=... -DEBNF=... -DHPP=... -DMAP=...
#         -DOUT_EBNF=... -DOUT_KWS=... -DOUT_EXPR=... -P mutate.cmake
if(NOT GEN OR NOT EBNF OR NOT HPP OR NOT MAP OR NOT OUT_EBNF OR NOT OUT_KWS OR NOT OUT_EXPR)
  message(FATAL_ERROR
    "mutate.cmake: missing -DGEN/-DEBNF/-DHPP/-DMAP/-DOUT_EBNF/-DOUT_KWS/-DOUT_EXPR")
endif()

set(_from_or [==[("or"|"||")]==])
set(_to_or [==[("or"|"||"|"xor")]==])

file(READ "${EBNF}" _ebnf)
string(REPLACE "${_from_or}" "${_to_or}" _ebnf "${_ebnf}")
get_filename_component(_ebnf_dir "${OUT_EBNF}" DIRECTORY)
if(_ebnf_dir)
  file(MAKE_DIRECTORY "${_ebnf_dir}")
endif()
file(WRITE "${OUT_EBNF}" "${_ebnf}")

get_filename_component(_kws_dir "${OUT_KWS}" DIRECTORY)
if(_kws_dir)
  file(MAKE_DIRECTORY "${_kws_dir}")
endif()
get_filename_component(_expr_dir "${OUT_EXPR}" DIRECTORY)
if(_expr_dir)
  file(MAKE_DIRECTORY "${_expr_dir}")
endif()

execute_process(
  COMMAND "${GEN}"
          --ebnf "${OUT_EBNF}"
          --parser-hpp "${HPP}"
          --map "${MAP}"
          --check
          --emit-keywords "${OUT_KWS}"
          --emit-expr "${OUT_EXPR}"
  RESULT_VARIABLE rc
  OUTPUT_VARIABLE out
  ERROR_VARIABLE err
)
if(rc)
  message(FATAL_ERROR
    "adl2_rdgen mutate failed (exit ${rc})\n${out}${err}")
endif()
