# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ADL2Flowchart is a compiler that parses Analysis Description Language (ADL) files — a DSL for describing high-energy physics event selection — and generates Graphviz DOT visualizations of the AST and flowchart.

## Repository layout

- `legacy_parser/` — the original C++ tool (everything below describes it)
- `reimplementation/` — ADL2 / `smash2`, the from-scratch Rust toolchain
  (canonical doc: reimplementation/README.md)
- `examples/` — shared ADL corpus; `docs/archive/` — specs, plans,
  audits, and reports (historical record)

## Build & Run (legacy tool)

```bash
make                # at repo root (delegates) or in legacy_parser/
make test           # golden suite + corpus sweep + z3 spike
make clean

cd legacy_parser
./smash <FILE>      # Parse an ADL file, generates ast.dot and fc.dot
./smash -r <FILE>   # + dual-encoding region disjointness analysis
dot -Tpdf ast.dot -o ast.pdf   # Convert to PDF
```

Note: `smash` resolves its data files by searching ancestor directories
for `adl/`, so run it from within `legacy_parser/`.

**Requirements:** flex, bison, clang++ (C++17), graphviz

Tests: `make test` (golden verdict suite in legacy_parser/tests/golden, corpus sweep over examples/, z3 spike).

## Architecture

The compiler follows a standard pipeline: **Lex → Parse → Semantic Analysis → DOT Output**.

### Compiler Pipeline (legacy_parser/adl/)

1. **scanner.l** — Flex lexer. Tokenizes ADL keywords (`define`, `region`, `object`, `select`, `take`, `reject`, `weight`, `trigger`, `histo`), operators, identifiers, and literals.

2. **parser.y** — Bison LALR(1) grammar. Builds the AST from tokens. Each grammar rule constructs AST nodes defined in `ast.hpp`.

3. **ast.hpp** — AST node hierarchy. Base class `Expr` with subtypes: `BinNode`, `VarNode`, `NumNode`, `FunctionNode`, `DefineNode`, `astObjectNode`, `RegionNode`, `CommandNode`, `HistoNode`, `ITENode`. Each node has a unique ID for graph output and a `clone()` method.

4. **driver.h / driver.cpp** — Orchestrates parsing. Manages symbol tables (`objectTable`, `regionTable`, `definitionTable`, `typeTable`, `dependencyChart`). Loads external library files and performs table setup.

5. **semantic_checks.h / semantic_checks.cpp** — Type checking (`typeCheck`), declaration checking (`checkDecl`), dependency graph analysis, and DOT file generation for both AST (`visitAST`/`printAST`) and flowchart (`printFlowChart`).

6. **region_formula.h / constraint_encoder.{hpp,cpp} / region_analysis.{hpp,cpp}** — the dual-encoding disjointness engine: polarity-aware formula IR, region encoder, axioms, batched Z3 analysis (see docs/archive/reports/DUAL_ENCODING_REPORT.md).

7. **main.cpp** — Entry point: parse → setTables → checkDecl → typeCheck → DOT output → optional `-r` region analysis.

### External Data Files (legacy_parser/adl/)

These text files define the ADL standard library loaded at startup:
- **ext_objs.txt** — Predefined particle objects (Electron, Muon, Jet, MissingET, etc.)
- **ext_lib.txt** — Built-in functions (dR, dPhi, dEta, abs, sqrt, ht, met, etc.)
- **property_vars.txt** — Maps particle property names to internal function names (pt→Ptof, eta→Etaof, mass→Mof, etc.)

### Key Design Details

- The `legacy_parser/Makefile` delegates to `adl/Makefile` then copies the binary; the repo-root Makefile delegates to `legacy_parser/`. All source compilation happens in `adl/`.
- Bison generates `parser.cpp`/`parser.hpp`; Flex generates `scanner.cpp`. These are gitignored.
- The executable resolves its own path at runtime to find the external data files relative to the binary location.
- AST nodes use a global counter for unique IDs used in DOT graph node naming.
