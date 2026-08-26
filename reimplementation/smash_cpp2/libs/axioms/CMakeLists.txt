# adl2_axioms — audited axiom catalog + emitters.
# smash2_cpp cpp/libs/axioms is the algorithm reference.
# smash3 check --dump-axioms is the dump oracle. No verify, no certify.
adl2_add_library(adl2_axioms
  SOURCES
    src/catalog.cpp
    src/emit.cpp
    src/elem_pred.cpp
  PUBLIC_DEPS adl2_formula
)
