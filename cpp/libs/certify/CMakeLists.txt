# adl2_certify — small trusted certificate kernel (Rust adl-certify).
# Depends on formula only. Analysis calls this crate; do not invert.
adl2_add_library(adl2_certify
  SOURCES src/lib.cpp
  PUBLIC_DEPS adl2_formula
)
