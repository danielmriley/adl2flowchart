# adl2_rootfile — pure-C++ ROOT histogram file writer.
# smash2_cpp cpp/libs/rootfile is the algorithm. No CERN ROOT link.
# Uncompressed small-format TFile. Vendored StreamerInfo blobs.
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
