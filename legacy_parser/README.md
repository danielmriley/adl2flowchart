# Legacy parser — `smash` (the original adl2flowchart tool)

The original C++ implementation: a flex/bison front end for ADL (Analysis
Description Language) files with Graphviz DOT visualization and the
dual-encoding region disjointness/overlap analysis that was retrofitted
onto it. It is retained as the **reference oracle** for the from-scratch
Rust reimplementation in [`../reimplementation/`](../reimplementation/),
which supersedes it for all new work.

## Build & run

Requirements: `flex`, `bison`, `clang++` (C++17), `graphviz`, `make`.

```bash
make                # here, or at the repo root (which delegates)
make test           # golden verdict suite + corpus sweep + z3 spike
make clean

./smash <FILE>      # parse an ADL file; writes ast.dot and fc.dot
./smash -r <FILE>   # + dual-encoding region disjointness analysis
dot -Tpdf fc.dot -o flowchart.pdf
```

Run `smash` from inside this directory: it locates its data files by
searching ancestor directories for `adl/`.

## Layout

- `adl/` — all sources: `scanner.l` (lexer), `parser.y` (LALR grammar),
  `ast.hpp`, `driver.{h,cpp}`, `semantic_checks.{h,cpp}` (type/decl
  checks + DOT output), `region_formula.h` /
  `constraint_encoder.{hpp,cpp}` / `region_analysis.{hpp,cpp}` (the
  dual-encoding disjointness engine), and the external data files
  (`ext_objs.txt`, `ext_lib.txt`, `property_vars.txt`) that define the
  ADL standard library — these are also embedded by the reimplementation.
- `tests/golden/` — the pinned-verdict golden suite; every historical
  false-verdict bug from the tool's two audits is locked here (and runs
  as an integration battery in the reimplementation too).

## History & analysis records

The audits, verification reports, and per-analysis notes that used to
live in this directory are archived in
[`../docs/archive/legacy/`](../docs/archive/legacy/); the dual-encoding
design report is at
[`../docs/archive/reports/DUAL_ENCODING_REPORT.md`](../docs/archive/reports/DUAL_ENCODING_REPORT.md).
