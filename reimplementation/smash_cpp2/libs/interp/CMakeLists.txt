# adl2_interp — reference interpreter (smash3 adl-interp / LANGUAGE.md).
# Event → region membership + cutflow + histogram fills.
# Native ROOT bytes live in adl2_rootfile (CLI writes out.root).
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
