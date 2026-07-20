# smash2 quickstart

A 5-minute tour of the ADL2 toolchain (`smash2`), from "does my file parse"
to the thing no other ADL tool does: **machine-checked, solver-backed proofs
about the relationships between your selection regions**, each with a witness.

Paths below are relative to `reimplementation/adl2/`. The example corpus is at
`../../examples/`.

## 0. Build

```bash
cd reimplementation/adl2
cargo build --release            # default: subprocess solver (uses a z3 on PATH)
alias smash2=$PWD/target/release/smash2
```

The default build links no libz3 and uses the SMT-LIB subprocess backend
(needs a `z3` binary on `PATH` — `apt install z3`). For the faster
in-process libz3 backend add `--features native` (system libz3, `apt install
libz3-dev`) or `--features bundled` (libz3 built from vendored source); see the
README. With no solver at all, proofs degrade honestly to `POSSIBLY` — they
never lie.

## 1. `check` — does it parse and resolve?

```bash
smash2 check ../../examples/tutorials/ex01_selection.adl     # silent + exit 0 = clean
smash2 check --json my_analysis.adl                          # diagnostics as JSON (editors/CI)
```

Diagnostics carry spans, labels, and "did you mean" help; every error in the
file is reported (the parser resynchronizes), and they go to **stderr** so
stdout stays machine-clean. `--json` emits a stable array
(`file, severity, line, col, start, end, message, label, help`) for tooling.

## 2. `run` — interpret regions over events

`run` is the **reference semantics** — the executable definition of what your
regions mean. Feed it JSONL, or read a detector file natively with `--profile`:

```bash
# straight off a real CMS Open Data NanoAOD file (no temp files):
smash2 run my_analysis.adl events.root --profile nanoaod
smash2 run my_analysis.adl events.root --profile delphes      # Delphes too

# per-region pass/fail + bin assignment over a JSONL event stream:
smash2 run ../../examples/tutorials/ex01_selection.adl events.jsonl

# fill histograms → histos.json + a native out.root + ROOT/uproot bridges:
smash2 run my_analysis.adl events.root --profile nanoaod --histos out/
```

You get per-region cutflows (raw + weighted), bin assignments, and — with
`--histos` — a native pure-Rust `out.root` plus `make_histos.C` / `to_root.py`.

## 3. `verify` — the part that's unique: prove region relations

This is the legacy `smash -r`, but **sound**. For every pair of regions it
proves one of: `PROVEN DISJOINT` / `PROVEN OVERLAPPING` / `PROVEN SUBSET` /
`POSSIBLY` / `UNKNOWN`; it flags vacuous (`EMPTY`) regions and checks that
`bin` sets actually partition their observable.

```bash
smash2 verify ../../examples/Examples/CMS-SUS-16-032.adl    # verdict matrix + findings
smash2 verify --explain my_analysis.adl                     # full proof chains
smash2 verify --json my_analysis.adl > report.json          # versioned schema
smash2 verify --fail-on=overlap,empty my_analysis.adl       # gate CI on findings
```

What makes the verdicts trustworthy:

- **PROVEN DISJOINT** is checked on *over-approximations* of both regions — if
  even the supersets can't intersect, the regions can't. `--explain` prints the
  unsat core mapped back to your source lines.
- **PROVEN OVERLAPPING** comes with a concrete **witness event** that is
  *re-validated through the interpreter* (step 2's reference semantics) before
  it's ever shown — if the verifier and interpreter disagreed, that's a
  release-blocking bug, not a verdict.
- Anything outside the checked fragment becomes an explicit `Unknown` with a
  reason you can read; it can weaken a verdict to `POSSIBLY`, never flip it.

`CMS-SUS-16-032` is a good first read: `verify` proves several of its compressed
regions `EMPTY` (their cuts contradict the physical axioms) — a real bug class
this tool catches mechanically.

## 4. `dot` / `objects` — see the structure

```bash
smash2 dot my_analysis.adl       | dot -Tpdf -o flowchart.pdf   # flowchart from the resolved IR
smash2 dot --ast my_analysis.adl | dot -Tpdf -o ast.pdf
smash2 objects my_analysis.adl                                  # one row per collection
```

Both diagrams are generated from the *resolved* representation, so they show
exactly what the verifier analyzed.

## Where to go next

- `README.md` — the full subcommand/flag reference and the histogram pipeline.
- `SPEC_ANALYSIS.md` — verdict definitions, the soundness contract, the axiom
  catalog.
- `docs/ADL_LHC_RESEARCH_AND_ROADMAP.md` — where the tool fits in the LHC
  ecosystem and what's planned next (cross-analysis overlap matrices).
- The `adl2-build-test`, `adl2-soundness`, and `adl2-corpus-sweep` skills —
  build/test recipes, the soundness contract, and the corpus regression sweep.
