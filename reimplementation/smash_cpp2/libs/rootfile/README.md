# `adl2_rootfile`

Pure-C++ writer for ROOT files containing TH1D/TH2D histograms and a
TNamed provenance object. smash2_cpp `cpp/libs/rootfile` is the
algorithm. smash3 `run --histos` `out.root` is the layout oracle.

Does not link CERN ROOT. Small-format TFile, TKey v4, uncompressed
records, vendored StreamerInfo blobs. The TNamed key is
`smash2_provenance` so `hadd` still finds it. The title is the
provenance JSON; the tool string inside is `smash_cpp2 0.1.0`.

Pin `pack_datime(2026, 6, 12, 0, 0, 0)` and zero UUIDs for byte-stable
output (same pins smash3 uses).

Headers: `libs/rootfile/include/adl2/rootfile/`
