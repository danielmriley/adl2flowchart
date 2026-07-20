# adl2flowchart / smash2

Tooling for [ADL](https://cern.ch/adl) (Analysis Description Language —
a declarative DSL for HEP event selection): parse analyses, run them over
events, visualize them, and **prove relations between selection regions**
— disjoint, overlapping, subset — within one analysis and **across
analyses**, with independently certified proofs.

## Repository layout

| Directory | Contents | Canonical doc |
|---|---|---|
| [`reimplementation/`](reimplementation/) | **ADL2 / `smash2`** — the from-scratch Rust toolchain: interpreter, certified prover, cross-analysis engine, ROOT-file pipeline, visualizer. This is the active tool. | [`reimplementation/README.md`](reimplementation/README.md) |
| [`legacy_parser/`](legacy_parser/) | the original flex/bison C++ tool (`smash`), retained as the reference oracle | [`legacy_parser/README.md`](legacy_parser/README.md) |
| [`examples/`](examples/) | the shared ADL corpus (tutorials, real CMS/ATLAS analyses, pinned-verdict golden files) | — |
| [`docs/archive/`](docs/archive/) | design specs, plans, audits, and reports (historical record; the READMEs above are the entry points) | — |

## Quick start (the Rust tool)

```bash
cd reimplementation/adl2
cargo build --release            # stable Rust >= 1.93; no linking deps
alias smash2=$PWD/target/release/smash2
# runtime: a z3 binary on PATH enables proofs (apt install z3)

smash2 verify ../../examples/tutorials/ex01_selection.adl            # region-relation proofs
smash2 verify --cross a.adl b.adl                                    # cross-analysis overlap matrix
smash2 run analysis.adl events.root --profile delphes --histos out/  # events -> histograms/out.root
smash2 dot analysis.adl | dot -Tpdf -o flowchart.pdf                 # the flowchart
```

Full build/run/feature reference: [`reimplementation/README.md`](reimplementation/README.md).

## The legacy tool

Requirements: `flex`, `bison`, `graphviz`, `make` (`apt install flex bison graphviz make`).

```bash
make                       # at the repo root (delegates) or in legacy_parser/
cd legacy_parser
./smash <FILE>             # writes ast.dot and fc.dot
./smash -r <FILE>          # + region disjointness analysis (stdout)
dot -Tpdf fc.dot -o fc.pdf
```

Details: [`legacy_parser/README.md`](legacy_parser/README.md).
