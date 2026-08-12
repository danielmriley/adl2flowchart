# adl2_sema — resolve / HIR (Rust adl-sema). P1: stub.
# Spine: syntax → sema. Syntax is PRIVATE so parser headers do not leak to
# formula / analysis / viz (analysis must not parse; viz reads HIR only).
adl2_add_library(adl2_sema STUB
  SOURCES src/stub.cpp
  PRIVATE_DEPS adl2_syntax
)
