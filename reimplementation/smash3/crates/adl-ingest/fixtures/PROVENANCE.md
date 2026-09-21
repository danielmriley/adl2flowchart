# adl-ingest fixture provenance

All `.root` fixtures are Delphes-shaped TTrees written by
`make_fixtures.py` (uproot 5.7.4 / awkward 2.9.1, the repo's
`.venv-uproot`), with the exact Delphes 3.4.x naming the profile maps:
dotted leaf branches (`Jet.PT`) and one `<collection>_size` int32 counter
per collection. Regenerate with:

```bash
../../.venv-uproot/bin/python make_fixtures.py [path/to/delphes_T2tt_700_50.root]
```

Layout note: uproot writes per-leaf baskets, not split TClonesArrays, and
omits basket entry offsets when every entry has the same length. The
reader's counter-authoritative re-chunking (`src/reader.rs`) makes both
layouts read identically; these fixtures pin that behavior.

| file | sha256 | contents |
|---|---|---|
| `delphes_mini.root` | `ae327efdc11c94a9ab3e8d0d3061e2acdcb3853efbd216fd7af619698c9eba0b` | 13 real events from the CutLang tutorial sample `T2tt_700_50.root` (sha256 `04fae8b1d94809f799741af8351f9448b84370122b780ccf03df3b74531b89fc`, 20 000 events; SPEC_EVENT_PIPELINE header): tree entries [0..8] + [23, 27, 28, 337] — the first nine events plus the earliest electron events and the earliest 2-muon event. Includes unmapped `Jet.T`, known-dropped `Event.ProcessID` / `MissingET.Eta`, and the LHE `Weight.Weight` vector. |
| `delphes_synth.root` | `faf080034ca367578f6f61d222a35b1d24bcc46817870b45f221340ae5b537dc` | 4 synthetic events: BTag masks {1, 2, 3, 0} (multi-bit → [DECIDE-I1] diagnostics), TauTag, ±1 charges, Event.Weight {0.5, −1.5, 1.0, 2.0}, 3 LHE weights/event, unknown `Track` collection, MissingET multiplicities 1/1/2/0, an all-empty event, equal-pT jets. |
| `delphes_badorder.root` | `6087737bc5209d926ca9f6e791091f0e2dd3b50883efc968c96abba0426d5e46` | 2 events; entry 1 has ascending `Jet.PT`. Ingest must refuse (`NotPtDescending`). |
| `delphes_nan.root` | `d43a6df76e9cfad0254baf23d035b7d303ec4b412cce5f778849d700a890d3d9` | 1 event with `Jet.Eta = [NaN]`. Ingest must refuse (`NonFinite`). |

## Goldens

`delphes_mini.expected.jsonl` and `delphes_synth.expected.jsonl` are the
canonical JSONL of the native reader, frozen 2026-06-12 after verifying:

- byte-identical to the generated `to_jsonl.py` uproot oracle on both
  fixtures **and** on the full 20 000-event T2tt sample (21 003 410
  bytes, `cmp` clean);
- spot values against uproot directly (entry 0: `Jet.PT[0]` =
  719.5091552734375, `MissingET.MET[0]` = 653.098876953125).
