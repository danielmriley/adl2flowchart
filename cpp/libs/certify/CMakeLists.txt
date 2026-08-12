# adl2_certify — small trusted certificate kernel (Rust adl-certify). P1: stub.
# Must stay thin; no parser/analysis sprawl.
adl2_add_library(adl2_certify STUB
  SOURCES src/stub.cpp
  PUBLIC_DEPS adl2_analysis
)
