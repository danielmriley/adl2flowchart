# adl2_axioms — audited axiom catalog + emitters (Rust adl-axioms / ADR-008).
adl2_add_library(adl2_axioms
  SOURCES
    src/catalog.cpp
    src/emit.cpp
  PUBLIC_DEPS adl2_formula
)
