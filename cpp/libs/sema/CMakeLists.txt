# adl2_sema — resolve / HIR (Rust adl-sema). P2 fills this module.
# Spine: syntax → sema. Syntax is PRIVATE so parser headers do not leak to
# formula / analysis / viz (analysis must not parse; viz reads HIR only).

set(ADL2_LEGACY_ADL "${CMAKE_SOURCE_DIR}/../legacy_parser/adl")
set(ADL2_EXT_EMBED "${CMAKE_CURRENT_BINARY_DIR}/ext_embedded.cpp")

function(adl2_embed_legacy_file varname path)
  file(READ "${path}" _content)
  set(${varname} "${_content}" PARENT_SCOPE)
endfunction()

adl2_embed_legacy_file(ADL2_EXT_OBJS "${ADL2_LEGACY_ADL}/ext_objs.txt")
adl2_embed_legacy_file(ADL2_EXT_LIB "${ADL2_LEGACY_ADL}/ext_lib.txt")
adl2_embed_legacy_file(ADL2_EXT_PROPS "${ADL2_LEGACY_ADL}/property_vars.txt")
adl2_embed_legacy_file(ADL2_EXT_ALIASES "${ADL2_LEGACY_ADL}/object_aliases.txt")

set(_embed_src
"namespace adl2 { namespace sema { namespace embedded {
extern const char kExtObjs[] = R\"ADL2(${ADL2_EXT_OBJS})ADL2\";
extern const char kExtLib[] = R\"ADL2(${ADL2_EXT_LIB})ADL2\";
extern const char kPropertyVars[] = R\"ADL2(${ADL2_EXT_PROPS})ADL2\";
extern const char kAliases[] = R\"ADL2(${ADL2_EXT_ALIASES})ADL2\";
}}}")
file(WRITE "${ADL2_EXT_EMBED}" "${_embed_src}")

adl2_add_library(adl2_sema
  SOURCES
    src/quantity.cpp
    src/hir.cpp
    src/ext.cpp
    src/dump.cpp
    src/rat.cpp
    src/num.cpp
    src/resolve.cpp
    src/resolve_expr.cpp
    src/objects.cpp
    src/merge.cpp
    ${ADL2_EXT_EMBED}
  PRIVATE_DEPS adl2_syntax
)
# Private resolver.hpp lives next to the .cpp files.
target_include_directories(adl2_sema PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/src")
# Static-lib footgun: PRIVATE deps do not propagate at link time. Downstream
# (formula / viz / cli) must link syntax without seeing parser includes.
# LINK_ONLY re-exports the archive, not INTERFACE_INCLUDE_DIRECTORIES.
target_link_libraries(adl2_sema INTERFACE $<LINK_ONLY:adl2_syntax>)
