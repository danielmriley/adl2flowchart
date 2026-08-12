# adl2_sema — resolve / HIR (Rust adl-sema). P1: stub.
# Depends on syntax only (spine: syntax → sema → …).
adl2_add_library(adl2_sema STUB
  SOURCES src/stub.cpp
  PUBLIC_DEPS adl2_syntax
)
