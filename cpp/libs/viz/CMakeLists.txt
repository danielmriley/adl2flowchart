# adl2_viz — flowchart / AST DOT from resolved HIR (Rust adl-viz).
# Reads HIR only — depends on sema, not raw AST-only paths for meaning.
adl2_add_library(adl2_viz
  SOURCES
    src/label.cpp
    src/dot.cpp
  PUBLIC_DEPS adl2_sema
)
