# ADL Flowchart Generation

Repository layout:

- **`legacy_parser/`** — the original flex/bison C++ tool (`smash`):
  parsing, DOT visualization, and the dual-encoding region
  disjointness/overlap analysis. Build and run from inside that folder.
- **`reimplementation/`** — the ADL2 from-scratch re-implementation:
  specs, plan, and (eventually) the Rust workspace.
- **`examples/`** — the shared ADL corpus used by both.
- **`docs/`** — audits, reports, and plans.

## Dependencies

`flex`, `bison`, `graphviz`, and `make` are required.

For linux systems run:

```bash
apt install flex bison graphviz make
```

## To compile

Run `make` at the repo root (delegates to `legacy_parser/`) or inside
`legacy_parser/`; the executable `legacy_parser/smash` will be generated.

To run:

```bash
cd legacy_parser
./smash <FILE>
./smash -r <FILE>   # also run object/region disjointness analysis (stdout)
```

Two files will be made.
`ast.dot` and `fc.dot`

Run:

```bash
dot -Tpdf ast.dot -o ast.pdf
dot -Tpdf fc.dot -o fc.pdf
```

to create the PDFs.
