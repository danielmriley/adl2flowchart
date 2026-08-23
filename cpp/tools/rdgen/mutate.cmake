# Mutate grammar.ebnf with sibling-synonym literals and emit the extra
# lexer-map entries. Invoked as:
#   cmake -DGEN=... -DEBNF=... -DHPP=... -DMAP=...
#         -DOUT_EBNF=... -DOUT_KWS=... -P mutate.cmake
if(NOT GEN OR NOT EBNF OR NOT HPP OR NOT MAP OR NOT OUT_EBNF OR NOT OUT_KWS)
  message(FATAL_ERROR
    "mutate.cmake: missing -DGEN/-DEBNF/-DHPP/-DMAP/-DOUT_EBNF/-DOUT_KWS")
endif()

set(_from_or [==[("or"|"||")]==])
set(_to_or [==[("or"|"||"|"xor")]==])
set(_from_sel [==[("select"|"cut"|"cmd"|"command")]==])
set(_to_sel [==[("select"|"cut"|"cmd"|"command"|"sel")]==])

file(READ "${EBNF}" _ebnf)
string(REPLACE "${_from_or}" "${_to_or}" _ebnf "${_ebnf}")
string(REPLACE "${_from_sel}" "${_to_sel}" _ebnf "${_ebnf}")
get_filename_component(_ebnf_dir "${OUT_EBNF}" DIRECTORY)
if(_ebnf_dir)
  file(MAKE_DIRECTORY "${_ebnf_dir}")
endif()
file(WRITE "${OUT_EBNF}" "${_ebnf}")

get_filename_component(_kws_dir "${OUT_KWS}" DIRECTORY)
if(_kws_dir)
  file(MAKE_DIRECTORY "${_kws_dir}")
endif()

execute_process(
  COMMAND "${GEN}"
          --ebnf "${OUT_EBNF}"
          --parser-hpp "${HPP}"
          --map "${MAP}"
          --check
          --emit-keywords "${OUT_KWS}"
  RESULT_VARIABLE rc
  OUTPUT_VARIABLE out
  ERROR_VARIABLE err
)
if(rc)
  message(FATAL_ERROR
    "adl2_rdgen mutate failed (exit ${rc})\n${out}${err}")
endif()
