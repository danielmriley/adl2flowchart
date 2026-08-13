# adl2_formula — polarity-aware Formula IR + HIR encoder (Rust adl-formula).
# Parallel to interp after sema. Soundness polarity is a type (Over / Under).
adl2_add_library(adl2_formula
  SOURCES
    src/lin.cpp
    src/formula.cpp
    src/encode.cpp
    src/dump.cpp
  PUBLIC_DEPS adl2_sema
)
