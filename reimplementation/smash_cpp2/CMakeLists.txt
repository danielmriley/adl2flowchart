# smash_cpp2 — C++ production tree (smash3 ideas, smash2_cpp parse_*).
# U01: syntax + run-first CLI + check --dump-ast.
# 3.20+: ctest --test-dir. Stock g++. No Flex, no Bison, no libz3.
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
add_subdirectory(libs/cli)
