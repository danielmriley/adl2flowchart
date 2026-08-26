# `adl2_interp`

Reference interpreter: Event → bool/values. smash3 `run` is the oracle.
smash2_cpp `cpp/libs/interp` is the algorithm reference.

JSONL loader enforces pT-descending collections and the NNEG/TAG domain
the axioms assume. Two-valued `run_event` matches smash3 `run` event lines
(`PASS` / `fail` / `ERROR:` + bins). `run_event_traced` + `CutflowSet`
emit smash3 cutflow tables. `HistoSet` accumulates `histo` fills with
ROOT TH1/Sumw2 semantics. `histos.json` is schema version 2.

`run --histos DIR` writes `histos.json`, `cutflow.json`, `make_histos.C`,
`to_root.py`, and native `out.root` (skip with `--no-root`). The TNamed
key is `smash2_provenance`. The tool string is `smash_cpp2 0.1.0`.

Headers: `libs/interp/include/adl2/interp/`

Oracle: `smash_cpp2 run` vs smash3 `run` on
`examples/tutorials/ex01_selection.adl` and
`examples/tutorials/ex02_histograms.adl` with
`reimplementation/adl2/crates/adl-difftest/tests/fixtures/ex02_events.jsonl`.
`--histos` JSON matches after substituting the tool string.
