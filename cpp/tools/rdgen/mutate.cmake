# Mutate grammar.ebnf:
#   - xor in or-expr (identity, not inherit)
#   - sel in cut-stmt (dispatch reaches Ident + lowercase keyword)
#   - foo-stmt = "foo" condition, inserted before region-ref
# Emit expr + dispatch + keywords into MUTATE_DIR.
# Invoked as:
#   cmake -DGEN=... -DEBNF=... -DHPP=... -DMAP=...
#         -DOUT_EBNF=... -DOUT_KWS=... -DOUT_EXPR=... -DOUT_DISPATCH=...
#         -P mutate.cmake
if(NOT GEN OR NOT EBNF OR NOT HPP OR NOT MAP OR NOT OUT_EBNF OR NOT OUT_KWS
    OR NOT OUT_EXPR OR NOT OUT_DISPATCH)
  message(FATAL_ERROR
    "mutate.cmake: missing -DGEN/-DEBNF/-DHPP/-DMAP/-DOUT_EBNF/-DOUT_KWS/"
    "-DOUT_EXPR/-DOUT_DISPATCH")
endif()

set(_from_or [==[("or"|"||")]==])
set(_to_or [==[("or"|"||"|"xor")]==])
set(_from_cut [==[("select"|"cut"|"cmd"|"command")]==])
set(_to_cut [==[("select"|"cut"|"cmd"|"command"|"sel")]==])
set(_from_ref [==[| region-ref ;]==])
set(_to_ref [==[| foo-stmt | region-ref ;
foo-stmt        = "foo" condition ;]==])

file(READ "${EBNF}" _ebnf)
string(REPLACE "${_from_or}" "${_to_or}" _ebnf "${_ebnf}")
string(REPLACE "${_from_cut}" "${_to_cut}" _ebnf "${_ebnf}")
string(REPLACE "${_from_ref}" "${_to_ref}" _ebnf "${_ebnf}")
get_filename_component(_ebnf_dir "${OUT_EBNF}" DIRECTORY)
if(_ebnf_dir)
  file(MAKE_DIRECTORY "${_ebnf_dir}")
endif()
file(WRITE "${OUT_EBNF}" "${_ebnf}")

foreach(_out IN ITEMS "${OUT_KWS}" "${OUT_EXPR}" "${OUT_DISPATCH}")
  get_filename_component(_dir "${_out}" DIRECTORY)
  if(_dir)
    file(MAKE_DIRECTORY "${_dir}")
  endif()
endforeach()

execute_process(
  COMMAND "${GEN}"
          --ebnf "${OUT_EBNF}"
          --parser-hpp "${HPP}"
          --map "${MAP}"
          --check
          --emit-keywords "${OUT_KWS}"
          --emit-expr "${OUT_EXPR}"
          --emit-dispatch "${OUT_DISPATCH}"
  RESULT_VARIABLE rc
  OUTPUT_VARIABLE out
  ERROR_VARIABLE err
)
if(rc)
  message(FATAL_ERROR
    "adl2_rdgen mutate failed (exit ${rc})\n${out}${err}")
endif()
