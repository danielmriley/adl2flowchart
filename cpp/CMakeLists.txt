# 3.20+: ctest --test-dir (used by CI and local smoke).
cmake_minimum_required(VERSION 3.20)
project(adl2_cpp LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

add_library(adl2_cpp STATIC
  src/syntax/diag.cpp
  src/syntax/dump.cpp
  src/syntax/lexer.cpp
  src/syntax/parser.cpp
)
target_include_directories(adl2_cpp PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_compile_options(adl2_cpp PRIVATE -Wall -Wextra -Wpedantic)

add_executable(smash2_cpp src/main.cpp)
target_link_libraries(smash2_cpp PRIVATE adl2_cpp)
target_compile_options(smash2_cpp PRIVATE -Wall -Wextra -Wpedantic)

enable_testing()
add_subdirectory(tests)
