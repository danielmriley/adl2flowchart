# `adl2_rootfile`

Pure-C++ writer for ROOT files containing TH1D/TH2D histograms and TNamed
provenance (Rust `rootfile` crate / SPEC_ROOT_WRITER.md). Small-format TFile,
TKey v4, uncompressed records, vendored uproot StreamerInfo blobs.

Does **not** link CERN ROOT. The writer is the native `out.root` path for
`smash2_cpp run --histos`. The reader is a verification parser for files
this library emits — not a general TTree ingester (`adl2_ingest` owns that).

Pin `pack_datime(2026, 6, 12, 0, 0, 0)` and zero UUIDs for byte-stable output
(same pins smash2 uses).

Headers: `libs/rootfile/include/adl2/rootfile/`
