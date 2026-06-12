# rootfile fixtures — provenance

All three files were generated on 2026-06-12 with **uproot 5.7.4** /
numpy (Python 3.12.3, `.venv-uproot` at the workspace root; see
BUILD_NOTES.md) and are validated against a freshly regenerated reference
by the env-gated test
`tests/uproot_oracle.rs::vendored_fixtures_match_freshly_generated_uproot_reference`.

To regenerate (only needed if the pinned uproot version changes):

```bash
.venv-uproot/bin/python crates/rootfile/tools/make_reference.py /tmp/reference.root
.venv-uproot/bin/python crates/rootfile/tools/extract_streamerinfo.py /tmp/reference.root crates/rootfile/fixtures/
cp /tmp/reference.root crates/rootfile/fixtures/reference.root
```

| file | what | sha256 |
|---|---|---|
| `streamerinfo_th1d.bin` | data bytes of the `StreamerInfo` TList record (TKey stripped) from the uproot reference file: the pre-serialized TStreamerInfo set for TH1D (TObject, TString, TCollection v3, TSeqCollection v0, TList v5, THashList v0, TAttLine v2, TAttFill v2, TAttMarker v2, TAttAxis v4, TAxis v10, TNamed v1, TH1 v8, TH1D v3). Compiled into the crate via `include_bytes!` (SPEC_ROOT_WRITER.md §2: vendor the blob, no checksum algorithm implemented). | `eaa2bb516bb79b53853b48d147141b8477994d18bcb304b1b61f358b07afe981` |
| `reference_th1d_payload.bin` | data bytes (TKey stripped) of the `h_met` TH1D record uproot wrote for the pinned spec in `tools/make_reference.py`. Gold standard for `src/th1d.rs` (offline unit test asserts byte equality). | `6a3edcc03f0e32283ffe730b28d1309ca775a6ac99ef79578f31a938ada4cc0c` |
| `reference.root` | the complete uproot reference file (datime `0x7d9902ed` = 2026-06-12 16:11:45 UTC), kept for manual byte-level archaeology with `tools/dissect.py`. Not read by any test. | `b5d5d3f5b5611b2b9cbdf05416ff3585a456c08ceb688d56624ac2347fff0c3a` |

The pinned histogram (identical constants in `tools/make_reference.py`,
`tools/check_with_uproot.py`, `tests/structure.rs`, `tests/uproot_oracle.rs`):
`h_met`, title `MET [GeV]`, 4 uniform bins on [0, 100); contents
(flow-inclusive) `[1.5, 2.0, 0.0, 3.25, 4.0, 0.5]`; Sumw2
`[2.25, 4.0, 0.0, 5.0625, 8.0, 0.25]`; fEntries 11; stats
(Σw, Σw², Σwx, Σwx²) = (9.25, 17.0625, 300.5, 20000.25).
