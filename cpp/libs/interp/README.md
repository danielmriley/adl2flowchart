# `adl2_interp`

Reference interpreter: Event → bool/values (Rust `adl-interp`, SPEC_LANGUAGE §4).

JSONL loader enforces pT-descending collections and the NNEG/TAG domain
the axioms assume. Two-valued `run_event` matches smash2 `run` event lines
(`PASS` / `fail` / `ERROR:` + bins). Kleene three-valued membership
(`region3` / `eval_region_membership`) prefers a decidable False over
Unknown and is the witness-validation entry point.

Headers: `libs/interp/include/adl2/interp/`

Oracle: `smash2_cpp run` vs `smash2 run` on pinned pairs
(`cpp/tests/interp_gate_pairs.txt`); compared lines start with `event `
(cutflow/histo tables deferred).
