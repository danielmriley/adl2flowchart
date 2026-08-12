# adl2_viz — flowchart / DOT from HIR (Rust adl-viz). P1: stub.
# Reads HIR only — depends on sema, not raw AST-only paths for meaning.
adl2_add_library(adl2_viz STUB
  SOURCES src/stub.cpp
  PUBLIC_DEPS adl2_sema
)
