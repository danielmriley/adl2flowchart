# C++ module map (mirrors Rust crates)

Locked design constraint: **real separation of concerns** — not a single
smash-shaped blob. Each Rust crate maps to a CMake static library + header
tree under `libs/<module>/include/adl2/<module>/`.

## Targets

| CMake target | Rust crate | Status | Depends on |
|---|---|---|---|
| `adl2_syntax` | `adl-syntax` | **filled** (P1) | — |
| `adl2_sema` | `adl-sema` | **filled** (P2; Rat/NumVal in P3) | `adl2_syntax` (**PRIVATE** includes; `LINK_ONLY` at link) |
| `adl2_formula` | `adl-formula` | **filled** (P3) | `adl2_sema` (PUBLIC) |
| `adl2_interp` | `adl-interp` | **filled** (P3) | `adl2_sema` (PUBLIC) |
| `adl2_axioms` | `adl-axioms` | **filled** (P3) | `adl2_formula` (PUBLIC) |
| `adl2_solver` | `adl-solver` | **filled** (P4; subprocess only) | `adl2_formula` (PUBLIC; not axioms) |
| `adl2_analysis` | `adl-analysis` | **filled** (P4 pairwise + P5 witness + P6 certify/report) | `adl2_solver`, `adl2_interp`, `adl2_certify`, `adl2_axioms` (PUBLIC; **not** parser) |
| `adl2_certify` | `adl-certify` | **filled** (P5 kernel + P6 bundles / SHA-256) | `adl2_formula` (PUBLIC; **not** analysis) |
| `adl2_viz` | `adl-viz` | **filled** (P4: flowchart/AST DOT) | `adl2_sema` (PUBLIC; HIR only) |
| `adl2_rootfile` | `rootfile` | **filled** (P6: native `out.root`) | — |
| `adl2_ingest` | `adl-ingest` | **filled** (P6: TTree → JSONL) | zlib PRIVATE |
| `smash2_cpp` / alias `adl2_cli` | `adl-cli` | wiring only | syntax + sema + formula + interp + axioms + viz + analysis + rootfile + ingest |
| `smash2_cpp-recheck` | `smash2-recheck` | **filled** (`smash2-combine/2` replay) | `adl2_certify` |
| `adl2_util` | _(optional)_ | stub | — |

There is **no** `libadl2_cpp` / monolithic static blob. The CMake `project()`
name is `adl2`; libraries are the `adl2_*` targets above.

## Dependency spine

```
syntax → sema → {interp ‖ formula} → axioms → solver
                                    ↘ certify ↗ analysis
rootfile is a leaf (histogram TFile writer); ingest is a leaf (ROOT TTree → JSONL, zlib for ZL baskets).
viz reads HIR only; cli wires modules.
```

`adl2_certify` is a leaf over formulas (Rust `adl-certify`). Analysis *calls*
it; certify must not depend on analysis. The P0 stub edge
`certify → analysis` was inverted in the P5 API lock.

### Non-negotiable layering

1. **analysis must not parse** — no `adl2_syntax` parser calls inside analysis.
2. **viz reads HIR only** — depends on `adl2_sema`, not AST-only paths for flowchart meaning.
3. **cli wires modules** — no core logic in the executable.
4. **certify stays a small trusted kernel** — no parser/analysis sprawl.
5. **No string-keyed identity** sneaking across modules (ADR-007 spirit).
6. Prefer **one reviewable PR/phase per module boundary** when filling stubs.
   P4 fills **solver / viz / analysis (interval+pairwise)** plus region3 and
   EPRED/EPRES. P5 fills **certify** (Farkas replay) and **witness**
   (region3 re-validation).   P6 wires certify into `analyze_hir` and ports
  the default verify report + `objects`. Cutflow tables and histogram
  accumulation live in `adl2_interp`.

## Include policy

Each `adl2_*` library **PUBLIC**-exports only its own include root:

```
libs/<module>/include/adl2/<module>/…
```

`#include "adl2/<module>/foo.hpp"` resolves if and only if the TU’s target
links that module (directly, or via a **PUBLIC** dependency). There is **no**
workspace-wide `cpp/include/` dump on every target.

- `adl2_sema` links `adl2_syntax` **PRIVATE**, so formula / analysis / viz /
  certify cannot see parser/AST headers unless they link `adl2_syntax`
  themselves (cli does; analysis must not). The syntax *archive* is
  re-exported with `$<LINK_ONLY:adl2_syntax>` so static-lib consumers can
  link without inheriting parser includes.
- ctest `layering_analysis_cannot_include_syntax` compiles a probe against
  `adl2_analysis`’s interface includes and **expects failure**.
- ctest `layering_certify_cannot_include_analysis` compiles a probe against
  `adl2_certify`’s interface includes and **expects failure**.

## Layout

```
cpp/
  libs/<module>/include/adl2/<module>/   public headers (seams by path + CMake)
  libs/<module>/src/                     implementation
  libs/cli/                              smash2_cpp executable only
  cmake/Adl2Module.cmake                 shared add_library helper
  grammar.ebnf                           collaborator EBNF (owned by syntax)
  BISON_MAP.md                           collaborator map (owned by syntax)
```

Namespaces follow modules: `adl2::syntax`, `adl2::sema`, `adl2::formula`,
`adl2::interp`, `adl2::axioms`, …
