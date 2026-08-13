# adl2_interp — reference interpreter (Rust adl-interp / SPEC_LANGUAGE §4).
# Parallel to formula after sema. Executable semantics oracle for later verify.
adl2_add_library(adl2_interp
  SOURCES
    src/event.cpp
    src/eval.cpp
    src/cutflow.cpp
    src/histo.cpp
    src/bridges.cpp
    src/sample.cpp
    src/provenance.cpp
  PUBLIC_DEPS adl2_sema
)
