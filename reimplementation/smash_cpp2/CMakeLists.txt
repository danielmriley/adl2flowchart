# smash_cpp2 — C++ production tree (smash3 ideas, smash2_cpp parse_*).
# U09: U08 plus objects + dot / dot --ast (libs/viz).
# No Flex, no Bison, no libz3.
cmake_minimum_required(VERSION 3.20)

# Prefer stock g++. Some images default `c++` to clang without libstdc++.
if(NOT DEFINED CMAKE_CXX_COMPILER AND NOT DEFINED ENV{CXX})
  find_program(_SMASH_CPP2_GXX g++)
  if(_SMASH_CPP2_GXX)
    set(CMAKE_CXX_COMPILER "${_SMASH_CPP2_GXX}")
  endif()
endif()

project(smash_cpp2 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

list(APPEND CMAKE_MODULE_PATH "${CMAKE_CURRENT_SOURCE_DIR}/cmake")
include(Adl2Module)

add_subdirectory(libs/syntax)
add_subdirectory(libs/sema)
add_subdirectory(libs/interp)
add_subdirectory(libs/formula)
add_subdirectory(libs/axioms)
add_subdirectory(libs/solver)
add_subdirectory(libs/certify)
add_subdirectory(libs/analysis)
add_subdirectory(libs/viz)
add_subdirectory(libs/cli)
