# `adl2_cli` / `smash2_cpp`

Wires libraries into the `smash2_cpp` binary. **Does not own core logic.**

P3: `check [--dump-ast|--dump-hir|--dump-quantities|--dump-formula]`
and `run <adl> <jsonl>`. Links syntax + sema + formula + interp + axioms.

**Bare `check` (no dump flag) is parse-only.** It does not run name
resolution. Rust `smash2 check` always resolves. This is an intentional
contract, not silent under-parity: help text and a stderr note say so.
`--dump-ast` is also parse-only so a sema bug cannot break the P1 corpus
gate. `--dump-hir` / `--dump-quantities` run sema. `--dump-formula` runs
sema + encode. There is no `--dump-axioms` until a Rust smash2 oracle dump
exists (library `dump_axioms` is unit-test only). `run` prints smash2-style
event lines only (no cutflow/histo tables).
