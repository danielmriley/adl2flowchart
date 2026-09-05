# adl2_ingest — converter profiles + native ROOT TTree reader.
# No interp dependency; adl2_sema only for the shared ryu number text
# (adl2/sema/ryu_f64.hpp). zlib is PRIVATE (ZL baskets only).
# smash2_cpp cpp/libs/ingest is the algorithm. smash3 ingest is the oracle.
find_package(ZLIB REQUIRED)
adl2_add_library(adl2_ingest
  SOURCES
    src/profile.cpp
    src/ttree.cpp
    src/reader.cpp
    src/script.cpp
  PUBLIC_DEPS adl2_sema
  PRIVATE_DEPS ZLIB::ZLIB
)
