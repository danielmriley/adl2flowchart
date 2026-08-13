# adl2_analysis — verify / pairwise analysis (Rust adl-analysis).
# P4: interval + encode + solver pairwise. P5: witness + (later) certify call.
# Consumes solver (+ interp for membership/oracle paths). Does NOT parse.
adl2_add_library(adl2_analysis
  SOURCES
    src/interval.cpp
    src/encode.cpp
    src/report.cpp
    src/lib.cpp
    src/witness.cpp
  PUBLIC_DEPS adl2_solver adl2_interp
)
