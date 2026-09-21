# smash3

Production ADL toolchain. Run analyses over events first. Prove region
relations after the interpreter agrees with the file.

`smash3` is the product name. The libraries keep the `adl-*` crate names.
The certified cores (HIR identity, polarity formulas, Farkas replay,
Delphes/NanoAOD ingest, ROOT histos) are the smash2 algorithms. This
tree is a copy of those crates, not a shared library — soundness fixes
must land in both. smash2 stays the CI oracle. smash3 is the run-first
tool with closed language decisions and a grammar file collaborators
can edit.

## Install

Rust 1.93 (this tree pins it). A `z3` binary on PATH enables proofs.

```bash
cd reimplementation/smash3
cargo build --release -p adl-cli
alias smash3=$PWD/target/release/smash3
```

No libz3 link. `--features native` is optional.

## Daily loop

```bash
# 1. parse + resolve
smash3 check analysis.adl

# 2. run over events (the product)
smash3 run analysis.adl events.jsonl
smash3 run analysis.adl events.root --profile delphes --histos out/

# 3. prove region relations
smash3 verify analysis.adl
smash3 verify --cross a.adl b.adl

# also
smash3 objects analysis.adl
smash3 dot analysis.adl
smash3 ingest events.root --profile nanoaod -o events.jsonl
```

`run` is the meaning. `verify` is property-tested against it. A witness
the interpreter rejects is not a proof.

## Features

Same surface as smash2 and smash2_cpp.

| Area | What you get |
|---|---|
| Syntax | Recursive-descent parser. `grammar.ebnf`. Multi-error diagnostics. |
| Sema | Typed Quantity/Collection identity. Exact `Rat`. Fragment tags. |
| Run | JSONL or ROOT (`delphes`, `nanoaod`). Cutflows. Histos. `out.root`. |
| Verify | Pairwise disjoint / overlap / subset. Vacuity. Bins. Certify on. |
| Gates | Sampling + refute. Fail-closed to POSSIBLY on contradiction. |
| Cross | `--cross`, XSUB/XEQ ledger, `--combine`, `smash3-recheck`. |
| Viz | Flowchart and AST DOT from HIR. |

## Grammar edits

1. Edit `grammar.ebnf`.
2. Add or update a dump-ast test.
3. Change the `parse_*` named in `BISON_MAP.md`.

Language meaning is in `LANGUAGE.md`. Those items are closed.

## Tests

Same battery smash2 CI runs, plus the corpus ledger pin. Needs a `z3`
binary on PATH (CI pins 4.12.2).

```bash
cargo test --release --workspace --no-fail-fast
scripts/corpus_gate.sh
scripts/verify_corpus_gate.sh
scripts/parity_dump_ast.sh      # smash2 is the dump-ast oracle
```

Landing-discipline extras (native libz3 optional; subprocess works):

```bash
cargo test -p adl-analysis --release --test golden_regions --test golden_cross
cargo test -p adl-difftest --release --test prop_encoder_vs_interp
cargo test -p adl-difftest --release --test metamorphic
cargo test -p adl-difftest --release --test cross_oracle --test prop_reconcile_oracle
cargo test -p adl-cli --release --test cli --test ingest --test adversarial
```

`--features native` and `--features deep` (100k-case oracle) stay opt-in.
