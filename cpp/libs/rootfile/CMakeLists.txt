# adl2_rootfile — pure-C++ ROOT histogram file writer (Rust `rootfile`).
# Zero CERN ROOT / uproot dependency. Uncompressed small-format TFile.
# Vendored StreamerInfo blobs live in fixtures/ (copied from the Rust crate).
adl2_add_library(adl2_rootfile
  SOURCES
    src/blobs.cpp
    src/datime.cpp
    src/th1d.cpp
    src/th2d.cpp
    src/file.cpp
    src/lib.cpp
    src/reader.cpp
)
