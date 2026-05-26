# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

ADL2Flowchart is a compiler that parses Analysis Description Language (ADL) files — a DSL for describing high-energy physics event selection — and generates Graphviz DOT visualizations of the AST and flowchart.

## Build & Run

```bash
make                # Builds everything (runs make in adl/, copies smash binary to root)
make clean          # Cleans generated files

./smash <FILE>      # Parse an ADL file, generates ast.dot and fc.dot
dot -Tpdf ast.dot -o ast.pdf   # Convert to PDF
```

**Requirements:** flex, bison, clang++ (C++17), graphviz

There is no test suite or linter configured.

## Architecture

The compiler follows a standard pipeline: **Lex → Parse → Semantic Analysis → DOT Output**.

### Compiler Pipeline (adl/)

1. **scanner.l** — Flex lexer. Tokenizes ADL keywords (`define`, `region`, `object`, `select`, `take`, `reject`, `weight`, `trigger`, `histo`), operators, identifiers, and literals.

2. **parser.y** — Bison LALR(1) grammar. Builds the AST from tokens. Each grammar rule constructs AST nodes defined in `ast.hpp`.

3. **ast.hpp** — AST node hierarchy. Base class `Expr` with subtypes: `BinNode`, `VarNode`, `NumNode`, `FunctionNode`, `DefineNode`, `astObjectNode`, `RegionNode`, `CommandNode`, `HistoNode`, `ITENode`. Each node has a unique ID for graph output and a `clone()` method.

4. **driver.h / driver.cpp** — Orchestrates parsing. Manages symbol tables (`objectTable`, `regionTable`, `definitionTable`, `typeTable`, `dependencyChart`). Loads external library files and performs table setup.

5. **semantic_checks.h / semantic_checks.cpp** — Type checking (`typeCheck`), declaration checking (`checkDecl`), dependency graph analysis, and DOT file generation for both AST (`visitAST`/`printAST`) and flowchart (`printFlowChart`).

6. **cutlang_declares.h / cutlang_declares.cpp** — CutLang integration types for mapping ADL constructs to CutLang data structures.

7. **main.cpp** — Entry point. Loads `property_vars.txt`, creates Driver, runs parse → setTables → checkDecl → typeCheck → visitAST → printFlowChart.

### External Data Files (adl/)

These text files define the ADL standard library loaded at startup:
- **ext_objs.txt** — Predefined particle objects (Electron, Muon, Jet, MissingET, etc.)
- **ext_lib.txt** — Built-in functions (dR, dPhi, dEta, abs, sqrt, ht, met, etc.)
- **property_vars.txt** — Maps particle property names to internal function names (pt→Ptof, eta→Etaof, mass→Mof, etc.)

### Key Design Details

- The root `Makefile` delegates to `adl/Makefile` then copies the binary. All source compilation happens in `adl/`.
- Bison generates `parser.cpp`/`parser.hpp`; Flex generates `scanner.cpp`. These are gitignored.
- The executable resolves its own path at runtime to find the external data files relative to the binary location.
- AST nodes use a global counter for unique IDs used in DOT graph node naming.
