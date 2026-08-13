# adl2_analysis — verify / pairwise analysis (Rust adl-analysis).
# P4: interval + encode + solver pairwise. P5: witness. P6: certify call +
# default human report. Consumes solver + interp + certify. Does NOT parse.
# Spine: analysis calls certify; do not invert.
adl2_add_library(adl2_analysis
  SOURCES
    src/interval.cpp
    src/encode.cpp
    src/report.cpp
    src/render.cpp
    src/lib.cpp
    src/witness.cpp
    src/refute.cpp
  PUBLIC_DEPS adl2_solver adl2_interp adl2_certify
)
