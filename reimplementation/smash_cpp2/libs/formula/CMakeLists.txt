# adl2_formula — polarity-aware Formula IR + HIR encoder.
# smash2_cpp cpp/libs/formula is the algorithm reference.
# smash3 check --dump-formula is the dump oracle. No axioms, no verify.
adl2_add_library(adl2_formula
  SOURCES
    src/lin.cpp
    src/formula.cpp
    src/encode.cpp
    src/dump.cpp
  PUBLIC_DEPS adl2_sema
)
