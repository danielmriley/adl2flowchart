# `adl2_ingest`

Native ROOT TTree → canonical JSONL (Rust `adl-ingest`, SPEC_EVENT_PIPELINE §1).

Does **not** link CERN ROOT. Reads Delphes- and NanoAOD-layout TTrees with a
purpose-built TKey/TTree/TBranch/TBasket parser; decompresses `ZL` (zlib)
baskets only. Converter profiles (`delphes`, `nanoaod`) are the same data
table that generates `to_jsonl.py`.

The interpreter never sees experiment names: `read_root` emits JSONL and
`smash2_cpp run --profile` feeds those bytes to the one `read_jsonl` loader.
Input provenance hashes the **original ROOT bytes**, never the JSONL
intermediate.

Headers: `libs/ingest/include/adl2/ingest/`
