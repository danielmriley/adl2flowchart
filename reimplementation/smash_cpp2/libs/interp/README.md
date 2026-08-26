# `adl2_interp`

Reference interpreter: Event → bool/values. smash3 `run` is the oracle.
smash2_cpp `cpp/libs/interp` is the algorithm reference.

JSONL loader enforces pT-descending collections and the NNEG/TAG domain
the axioms assume. Two-valued `run_event` matches smash3 `run` event lines
(`PASS` / `fail` / `ERROR:` + bins). `run_event_traced` + `CutflowSet`
emit smash3 cutflow tables (step / raw / abs% / rel% / errors / sumw ± err).

This unit does not write ROOT, histos.json, or ingest a converter profile.

Headers: `libs/interp/include/adl2/interp/`

Oracle: `smash_cpp2 run` vs smash3 `run` on
`examples/tutorials/ex01_selection.adl` and
`examples/tutorials/ex02_histograms.adl` with
`reimplementation/adl2/crates/adl-difftest/tests/fixtures/ex02_events.jsonl`.
