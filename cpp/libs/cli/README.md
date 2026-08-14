# `adl2_cli` / `smash2_cpp`

Wires libraries into the `smash2_cpp` binary. **Does not own core logic.**

Commands: `check`, `run`, `dot`, `verify`, `objects`, `ingest`. Links
syntax + sema + formula + interp + axioms + viz + analysis + rootfile +
ingest (certify is PUBLIC through analysis). Recheck is `smash2_cpp-recheck`.

Bare `check` always resolves. stdout is empty on
success; diagnostics go to stderr. `--dump-ast` prints the AST dump then
still resolves. `--dump-hir` / `--dump-quantities` / `--dump-formula` /
`--dump-axioms` print the corresponding dump. `run` prints smash2-style
event lines plus per-region cutflow tables (`--json` is compact JSONL +
cutflow; `--histos DIR` writes histos.json + cutflow.json + make_histos.C +
to_root.py + native `out.root` unless `--no-root`; `--csv`/`--svg` add
per-histogram files; `--profile NAME` ingests ROOT then the same JSONL
loader). Provenance `tool` is `smash2_cpp 0.1.0`. `dot` emits HIR
flowchart/AST DOT. `objects` prints `adl2::sema::object_table`. `verify`
defaults to solver + certify on; `--no-certify` skips Farkas replay
(uncertified solver-UNSAT is CANDIDATE, not PROVEN); `--dump-verdicts` is
the compact `A vs B: KIND` form; default stdout is the human report
(`--json` / `--explain` / `--matrix` / `--human=short` / `--fail-on`).
`--human=short` prints DISJOINT / OVERLAPS / NOT PROVED; JSON is unchanged.
`--demote-uncertified-interval` is opt-in (default off): interval PD that
fails Farkas becomes CANDIDATE DISJOINT.
`ingest --profile`
writes JSONL and/or `to_jsonl.py`.