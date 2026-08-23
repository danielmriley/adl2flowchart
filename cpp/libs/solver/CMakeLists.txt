# adl2_solver — SMT backend facade (Rust adl-solver). P4: subprocess backend.
find_package(Threads REQUIRED)
adl2_add_library(adl2_solver
  SOURCES
    src/classify.cpp
    src/sexp.cpp
    src/subprocess.cpp
  PUBLIC_DEPS adl2_formula
)
target_link_libraries(adl2_solver PUBLIC Threads::Threads)
