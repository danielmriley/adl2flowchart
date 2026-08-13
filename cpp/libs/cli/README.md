# `adl2_cli` / `smash2_cpp`

Wires libraries into the `smash2_cpp` binary. **Does not own core logic.**

Commands: `check`, `run`, `dot`, `verify`, `objects`. `ingest` exits 2
(not ported: no ROOT). Links syntax + sema + formula + interp + axioms +
viz + analysis (certify is PUBLIC through analysis).

Bare `check` always resolves (Rust smash2 parity). stdout is empty on
success; diagnostics go to stderr. `--dump-ast` prints the AST dump then
still resolves. `--dump-hir` / `--dump-quantities` / `--dump-formula` /
`--dump-axioms` print the corresponding dump. `run` prints smash2-style
event lines plus per-region cutflow tables (`--json` is compact JSONL +
cutflow; no provenance / `--histos` / ROOT). `dot` emits HIR
flowchart/AST DOT. `objects` prints `adl2::sema::object_table`. `verify`
defaults to solver + certify on; `--no-certify` skips Farkas replay;
`--dump-verdicts` is the compact `A vs B: KIND` form; default stdout is
the human report (`--json` / `--explain` / `--matrix` / `--fail-on`).
