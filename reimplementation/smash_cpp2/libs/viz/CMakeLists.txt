# adl2_viz — flowchart / AST DOT from resolved HIR (smash2_cpp libs/viz).
# smash3 `dot` / `dot --ast` is the stdout oracle. Reads HIR only.
adl2_add_library(adl2_viz
  SOURCES
    src/label.cpp
    src/dot.cpp
  PUBLIC_DEPS adl2_sema
)
