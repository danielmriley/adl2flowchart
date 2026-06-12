# ADL2 (`smash2`) — analysis toolchain for ADL

ADL2 parses [ADL](https://cern.ch/adl) (Analysis Description Language)
files, **interprets** them over event records, **verifies** relations
between selection regions with solver-backed proofs, and **visualizes**
them as Graphviz diagrams. It is the from-scratch successor to the legacy
tool in `../../legacy_parser/`, built so that the soundness properties the
legacy tool earned through two audits hold here *by construction*.

Status: all spec phases through the parity gate draft are built and green
— 356 tests, 68/68 corpus files, the full legacy golden battery on both
solver backends, and a verdict-parity comparison against the legacy tool
with zero legacy-better differences (`../PARITY_DRAFT.md`).

---

## Quick start

Requirements: stable Rust (≥ 1.93). Optional but recommended: `libz3-dev`
(native solver backend) or a `z3`/`cvc5` binary on PATH (subprocess
backend). With no solver at all, verdicts degrade honestly to POSSIBLY.

```bash
cd reimplementation/adl2
cargo build --release
alias smash2=$PWD/target/release/smash2
```

### The five subcommands

**`check` — parse + resolve, report diagnostics**

```bash
smash2 check analysis.adl              # exit 1 on errors; stdout stays clean
smash2 check --dump-ast analysis.adl   # canonical AST text dump to stdout
```

Diagnostics carry spans, labels, and help ("`selct` is not a keyword; did
you mean `select`?"), report *every* error in a file (the parser
resynchronizes at statement boundaries), and go to stderr.

**`verify` — the analysis (legacy `smash -r` equivalent)**

```bash
smash2 verify analysis.adl
smash2 verify --json analysis.adl > report.json     # versioned schema
smash2 verify --no-solver analysis.adl              # interval heuristic only
smash2 verify --fail-on=overlap,empty analysis.adl  # CI gating on findings
```

Per region: encoding coverage with named dropped cuts, vacuity check.
Per pair: PROVEN DISJOINT / PROVEN OVERLAPPING (with witness) / PROVEN
SUBSET / POSSIBLY / UNKNOWN, each with an explanation (unsat core mapped
to source lines for disjointness; a validated witness event for overlap).
Per `bin` set: pairwise disjointness and region coverage with gap
witnesses. Output is deterministic — two runs are byte-identical.

**`run` — interpret regions over events**

```bash
smash2 run analysis.adl events.jsonl   # per-region pass/fail + bin assignment
```

Events are JSONL: per-collection ordered object lists with properties,
plus event scalars and trigger flags. `run` is the *reference semantics* —
the verifier is property-tested against it, and every overlap witness is
re-validated through it before being shown.

**`dot` — Graphviz diagrams**

```bash
smash2 dot analysis.adl        | dot -Tpdf -o flowchart.pdf
smash2 dot --ast analysis.adl  | dot -Tpdf -o ast.pdf
```

Both diagrams are generated from the *resolved* representation (HIR), so
they always show exactly what the verifier analyzed.

**`objects` — object-attribute summary**

```bash
smash2 objects analysis.adl   # one aligned row per collection (also in `verify --explain`)
```

One row per declared collection: name, base chain (`bjets <- jets <- Jet`,
pure renames collapsed with `=`), element cuts (`pt > 25, |eta| < 2.4`),
fragment status, and derived size facts (subset of parent, union bounds).

### Reading verdicts

| Verdict | Claim | Sound because |
|---|---|---|
| PROVEN DISJOINT | no event can pass both regions | checked on an over-approximation of each region: if even the supersets cannot intersect, the regions cannot |
| PROVEN OVERLAPPING | a concrete event candidate passes both | checked on under-approximations; the witness satisfies fully-encoded real cuts and is re-validated by the interpreter |
| PROVEN SUBSET A⊆B | every event passing A passes B | UNSAT(A⁺ ∧ ¬B⁻) |
| region EMPTY | the region's cuts contradict physical axioms | UNSAT(R⁺ ∧ axioms) |
| POSSIBLY / UNKNOWN | no claim | — |

Anything the tool cannot encode faithfully becomes an explicit `Unknown`
with a reason you can read in the report — it can weaken a verdict to
POSSIBLY, never flip it. PROVEN OVERLAPPING is always printed with its
model caveat: the witness is a candidate in the per-event scalar
fragment, not a simulated event.

---

## How it works

```
source ──lex/parse──▶ AST(spans) ──resolve──▶ HIR + QuantityTable
                                       │
         ┌─────────────────────────────┼──────────────────────────┐
         ▼                             ▼                          ▼
    adl-interp                   adl-analysis                  adl-viz
  (run: events in,        (encode → Formula, project R⁺/R⁻,   (DOT out)
   pass/fail out)          axioms, solver, verdicts)
         ▲                             │
         └────── witness re-validation ┘
```

**Syntax** (`adl-syntax`): hand-written lexer and recursive-descent
parser — one function per grammar rule in `../SPEC_LANGUAGE.md` §3, so
code audits against the spec. Spanned AST as plain owned enums.

**Semantic resolution** (`adl-sema`): produces the **HIR** — the resolved
meaning of the file — and the **QuantityTable**, where every event
quantity is a typed, interned value (`MET.pt` an event scalar,
`jets[0].pt` an element property, `dPhi(a,b)` an *oriented* angular
separation). Identity is structural: a pure rename (`object MHT take
MissingET`, no cuts) IS its source collection; a filtered collection is
NEVER its parent; `jets[0].x` can never alias `jets[1].x`. Every node
carries an `InFragment`/`Unsupported(reason)` tag — the single shared
definition of what the tool understands, obeyed identically by the
interpreter and the verifier.

**Verification** (`adl-formula`, `adl-axioms`, `adl-solver`,
`adl-analysis`): each region compiles to an exact boolean formula in
which un-encodable parts are explicit `Unknown` leaves (and genuinely
ambiguous constructs are polarity-split `Dual` nodes). Two projections
with opposite soundness directions are derived: R⁺ (Unknown→true, a
superset of the region) and R⁻ (Unknown→false, a subset). **The type
system enforces the proof discipline**: `prove_disjoint` only accepts
`Over` values, `prove_overlap` only `Under` — feeding the wrong
approximation to a proof does not compile. Checks run with a catalog of
background axioms (pT ordering, size relations, physical ranges, tag
booleans — each with a written justification, an assumption tag, and a
test), against the native libz3 backend or a conformance-equivalent
SMT-LIB subprocess backend. Every overlap witness is converted to a
synthetic event and re-validated through the interpreter; a witness the
interpreter rejects downgrades the verdict and files an internal
diagnostic.

**Why trust it**: beyond unit tests, the encoder is property-tested
against the interpreter (random regions × sampled events; any PROVEN
verdict that contradicts sampling is a release-blocking bug — this
battery caught and fixed a real missing-element soundness bug during the
build, see `COUNTEREXAMPLES.md`), a metamorphic suite checks invariances
(`reject c` ≡ `select not c`, rename invariance, …), and the entire
legacy golden battery — every historical false-verdict bug from two
audits of the old tool — runs as integration tests.

---

## Extending it

ADL2 is built as passes over shared representations; adding analysis or
tooling means writing another pass.

**Write a pass over the HIR** (recommended). The HIR is the resolved,
typed view: names resolved, defines linked to bodies, quantities
interned, fragment tags everywhere. A new analysis is a function:

```rust
use adl_sema::{Hir, HirRegionStmt};

pub fn count_rejects(hir: &Hir) -> Vec<(String, usize)> {
    hir.regions.iter().map(|r| {
        let n = r.stmts.iter()
            .filter(|s| matches!(s, HirRegionStmt::Reject { .. }))
            .count();
        (hir.symbols.text(r.name).to_string(), n)
    }).collect()
}
```

Get a `Hir` with `adl_sema::analyze_str(&src, name, &ExtDecls::legacy())`.
Convenience lookups exist (`hir.collection_of("jets")`,
`hir.define("HT")`), iteration order is deterministic, and the
`Fragment` tags tell your pass which parts of the file it can honestly
claim to cover. The interpreter, the verifier, and the viz are themselves
three such passes — use them as templates. Exhaustive `match` on the HIR
enums means the compiler points at every pass that needs updating when a
node kind is added.

**Other extension points:**

- **CLI subcommand**: add a variant in `adl-cli/src/main.rs` and a module
  under `adl-cli/src/cmd/` (each existing subcommand is ~50 lines of
  plumbing around a library call).
- **A new axiom**: one entry in `adl-axioms` — emitter + justification
  ("true of every physical event because …") + assumption tag + a test
  that it holds on generated events. The prohibited-axiom list in the
  same crate records patterns that look sound and aren't; read it first.
- **A new encodable construct**: extend the encoder in `adl-formula`
  following the table in `../SPEC_ANALYSIS.md` §1. The rule: encode
  exactly or emit `Unknown` — the type system prevents the historical
  failure mode (silent approximation in the wrong direction), and the
  property battery will catch semantic mistakes against the interpreter.
- **A new solver**: implement the `Solver` trait in `adl-solver` and add
  it to the conformance battery.

Tests are part of the definition of done here: snapshot tests for
anything with stable output (`insta`), the corpus gate for anything
touching syntax/sema, and a golden or property test for anything
touching verdicts.

---

## Workspace layout

| Crate | Role |
|---|---|
| `adl-syntax` | lexer, recursive-descent parser, AST, spans, diagnostics, canonical dump |
| `adl-sema` | name resolution, Quantity/Collection identity model, fragment tagging, HIR |
| `adl-interp` | reference interpreter (the executable spec) |
| `adl-formula` | polarity-typed formula IR + projections; HIR→formula encoder |
| `adl-axioms` | audited axiom catalog (+ prohibited-axiom regressions) |
| `adl-solver` | `Solver` trait; native-z3 and SMT-LIB subprocess backends |
| `adl-analysis` | pairwise verdicts, vacuity, subset, bins, witnesses, reports/JSON |
| `adl-viz` | flowchart + AST DOT from HIR |
| `adl-difftest` | event generator, property/metamorphic batteries, legacy harness |
| `adl-cli` | the `smash2` binary |

Specs and design records live one directory up: `../SPEC_LANGUAGE.md`,
`../SPEC_ARCHITECTURE.md`, `../SPEC_ANALYSIS.md`, `../TESTING.md`,
`../DECISIONS.md` (ADRs tied to the legacy bugs that motivated them),
`../PHASE0_RESOLUTIONS.md` (current answers to the open semantic
questions), `../PARITY_DRAFT.md`. Build history: `BUILD_NOTES.md`,
`BUILD_REPORT.md`, `COUNTEREXAMPLES.md`.

```bash
cargo test --workspace          # full battery (~356 tests)
scripts/corpus_gate.sh          # all 68 example files parse + resolve
cargo test -p adl-solver --no-default-features   # subprocess-backend job
```

## Known limits / open items

- Five semantic questions (quantifier reading of unindexed collection
  cuts, dPhi/dEta sign convention, negative indices, `~=`, size aliases)
  are pinned to convention-neutral defaults pending CutLang probing —
  see `../PHASE0_RESOLUTIONS.md`. Resolving them upgrades several
  POSSIBLY verdicts to exact.
- The per-event scalar model caveat applies to overlap witnesses
  (opaque external-function values are free variables).
- Cross-file analysis (`--cross`) is designed (`../SPEC_ANALYSIS.md` §7)
  but not yet implemented — the quantity-identity model was built for it.
- Legacy features not yet ported: the object-pair disjointness printout
  and the object-attributes listing (both still available in
  `../../legacy_parser/`).
