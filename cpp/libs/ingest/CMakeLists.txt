# adl2_ingest — converter profiles + native ROOT TTree reader
# Leaf module: no interp/sema dependency. zlib is PRIVATE (ZL baskets only).
find_package(ZLIB REQUIRED)
adl2_add_library(adl2_ingest
  SOURCES
    src/profile.cpp
    src/ttree.cpp
    src/reader.cpp
    src/script.cpp
  PRIVATE_DEPS ZLIB::ZLIB
)
