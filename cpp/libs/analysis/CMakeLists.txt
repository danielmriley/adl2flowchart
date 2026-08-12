# adl2_analysis — verify / pairwise analysis (Rust adl-analysis). P1: stub.
# Consumes solver (+ interp for membership/oracle paths). Does NOT parse.
adl2_add_library(adl2_analysis STUB
  SOURCES src/stub.cpp
  PUBLIC_DEPS adl2_solver adl2_interp
)
