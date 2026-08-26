# `adl2_ingest`

Native ROOT TTree → canonical JSONL. smash2_cpp `cpp/libs/ingest` is
the algorithm. smash3 `ingest` is the byte oracle.

Does not link CERN ROOT. Reads Delphes- and NanoAOD-layout TTrees with
a TKey/TTree/TBranch/TBasket parser. Decompresses `ZL` (zlib) baskets
only. Converter profiles (`delphes`, `nanoaod`) are the same table that
generates `to_jsonl.py`.

The interpreter never sees experiment names. `read_root` emits JSONL
and `smash_cpp2 run --profile` feeds those bytes to `read_jsonl`.
Input provenance hashes the original ROOT bytes, never the JSONL
intermediate.

Headers: `libs/ingest/include/adl2/ingest/`
