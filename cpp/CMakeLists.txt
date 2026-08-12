# ADL2 C++ port — modular libs mirroring the Rust crate map (ADR-010).
# There is NO monolithic libadl2_cpp: each Rust crate is its own CMake
# target under libs/ (see MODULES.md). 3.20+: ctest --test-dir.
cmake_minimum_required(VERSION 3.20)
project(adl2 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(Adl2Module)

# --- module targets (spine order) ---
# syntax → sema → {interp ∥ formula} → axioms → solver → analysis → certify
# viz → sema ; cli wires libs
add_subdirectory(libs/util)
add_subdirectory(libs/syntax)
add_subdirectory(libs/sema)
add_subdirectory(libs/formula)
add_subdirectory(libs/interp)
add_subdirectory(libs/axioms)
add_subdirectory(libs/solver)
add_subdirectory(libs/analysis)
add_subdirectory(libs/certify)
add_subdirectory(libs/viz)
add_subdirectory(libs/cli)

enable_testing()
add_subdirectory(tests)
