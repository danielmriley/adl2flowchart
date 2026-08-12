# C++ module map (mirrors Rust crates)

Locked design constraint: **real separation of concerns** — not a single
smash-shaped blob. Each Rust crate maps to a CMake static library + header
tree under `include/adl2/<module>/`.

## Targets

| CMake target | Rust crate | P1 status | Depends on (PUBLIC) |
|---|---|---|---|
| `adl2_syntax` | `adl-syntax` | **filled** | — |
| `adl2_sema` | `adl-sema` | stub | `adl2_syntax` |
| `adl2_formula` | `adl-formula` | stub | `adl2_sema` |
| `adl2_interp` | `adl-interp` | stub | `adl2_sema` |
| `adl2_axioms` | `adl-axioms` | stub | `adl2_formula` |
| `adl2_solver` | `adl-solver` | stub | `adl2_axioms` |
| `adl2_analysis` | `adl-analysis` | stub | `adl2_solver`, `adl2_interp` |
| `adl2_certify` | `adl-certify` | stub | `adl2_analysis` |
| `adl2_viz` | `adl-viz` | stub | `adl2_sema` |
| `smash2_cpp` (`adl2_cli`) | `adl-cli` | wiring only | `adl2_syntax` (P1) |
| `adl2_util` | _(optional)_ | stub | — |

## Dependency spine

```
syntax → sema → formula → axioms → solver → analysis → certify
              ↘ interp ─────────────────────↗
              ↘ viz
cli wires the libs; does not own core logic.
```

### Non-negotiable layering

1. **analysis must not parse** — no `adl2_syntax` parser calls inside analysis.
2. **viz reads HIR** — depends on `adl2_sema`, not AST-only paths for flowchart meaning.
3. **certify stays a small trusted kernel** — no parser/analysis sprawl.
4. **No string-keyed identity** sneaking across modules (ADR-007 spirit).
5. Prefer **one reviewable PR/phase per module boundary** when filling stubs.

## Layout

```
cpp/
  include/adl2/<module>/…   public headers (seams obvious by path)
  libs/<module>/            sources + CMakeLists + README
  libs/cli/                 smash2_cpp executable only
  cmake/Adl2Module.cmake    shared add_library helper
  grammar.ebnf              collaborator EBNF (owned by syntax)
  BISON_MAP.md              collaborator map (owned by syntax)
```

Namespaces follow modules: `adl2::syntax`, `adl2::sema`, …
