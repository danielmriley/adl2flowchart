# ADL2 architecture — explainer

Companion to `architecture_slide.png` / `.pdf`. ADL2 (binary: `smash2`)
is a from-scratch, pure-Rust toolchain for ADL analysis files: it
**parses** them, **proves** disjointness/overlap relations between
selection regions, **runs** them over event data, and **writes ROOT
histograms** — with no dependency on ROOT or CutLang at runtime.

12 crates · ~26,600 lines of Rust · 510 tests green.

## The one idea that holds it together

Everything funnels through a single resolved representation — the **HIR**
(High-level Intermediate Representation, in `adl-sema`). The parser
produces a raw syntax tree; resolution turns it into the HIR, where every
event quantity (`MET.pt`, `jets[0].pt`, `dPhi(a,b)`) is a typed, interned
value with structural identity, and every node is tagged `InFragment` or
`Unsupported(reason)`. Three consumers then each read the *same* HIR:

- the **verifier** (does this region overlap that one?),
- the **interpreter** (does this event pass this region?),
- the **visualizer** (draw the flowchart/AST).

Because they share one resolved truth, they cannot disagree — the diagram
shows exactly what was verified, and the verifier's proofs are checked
against the same interpreter that runs real events.

## The layers (left-to-right on the slide)

**Inputs.** An ADL analysis file, plus event data in either of two forms:
a Delphes `.root` file, or line-delimited JSON (`events.jsonl`).

**Front end.**
- `adl-syntax` — hand-written lexer + recursive-descent parser → spanned
  AST with readable diagnostics.
- `adl-ingest` — reads a Delphes `.root` input and maps it to canonical
  events (see "Two ROOT directions" below).
- `adl-sema` — resolution → HIR + the typed quantity table. This is the
  spine; everything downstream depends on it.

**Verify (proofs).** `adl-formula` encodes each region into two
polarity-typed formulas (an over- and an under-approximation); the type
system forbids feeding the wrong one to a proof. `adl-axioms` supplies the
physics facts (pT ordering, size relations, tag domains — each with a
written justification). `adl-solver` runs Z3 (native bindings, or a
subprocess fallback). `adl-analysis` produces the verdicts —
PROVEN DISJOINT / OVERLAPPING / SUBSET, vacuous-region detection, bin
partition checks — each with an explanation (an unsat core mapped to
source lines, or a witness event). Overlap witnesses are **re-validated
through the interpreter** before they are ever shown (the green dotted
arrow): the tool never displays a witness it cannot reproduce.

**Run (semantics).** `adl-interp` is the reference interpreter — the
executable definition of what a region *means*. It evaluates regions over
events (streaming input, parallel loop, deterministic merge), filling
histograms and cutflows. Its accumulators feed `rootfile`.

**Visualize.** `adl-viz` emits the flowchart and AST Graphviz diagrams
from the HIR.

**CLI & harness.** `adl-cli` is the thin `smash2` binary
(`check | verify | run | dot | objects | ingest`). `adl-difftest` is the
test harness that property-checks the encoder against the interpreter —
the mechanism that has caught real soundness bugs at every stage.

**Outputs.** Verdict reports (human / `--json` / `--explain`); ROOT
histograms with cutflows and embedded provenance; the bridge formats
(`histos.json`, a ROOT C++ macro, a uproot Python script, CSV, SVG); the
DOT diagrams; and the object-attribute table.

## Two ROOT directions (this is the part worth being precise about)

ADL2 touches `.root` files at **both ends of the real-data run, and both
are Rust** — but they are different crates doing different jobs:

- **Reading input** — `adl-ingest` *parses* a Delphes `.root` to extract
  events, using the `oxyroot` crate (a third-party Rust ROOT reader). It
  re-chunks leaf data by Delphes' per-collection size counters, validates
  canonical-model invariants (pT-ordering, tag domains) and refuses rather
  than silently fixing bad input.
- **Writing output** — `rootfile` is our own from-scratch, pure-Rust
  writer of the ROOT file format. It produces `out.root` containing the
  TH1D/TH2D histograms and cutflows. No Rust crate could write ROOT
  histograms before this; ours is validated byte-for-byte against uproot.

So the full real-data chain is: **input `.root` → (adl-ingest, read) →
events → (adl-interp, run) → histograms → (rootfile, write) → output
`.root`.** ROOT in, ROOT out, pure Rust throughout.

Python/uproot appears **only in tests** — as an independent oracle that
reads back what our writer produced (and a generated `to_jsonl.py` script
that cross-checks the reader). Neither runtime path needs Python, ROOT, or
CutLang installed.

## Proven on real data

A real Delphes stop-signal sample (T2tt 700/50, 71 MB, 20k events) runs
end-to-end: a physically sensible MET spectrum, a monotonic cutflow, 2-D
and variable-bin histograms, all written to an `out.root` that uproot
reads back — with provenance (tool version, input + ADL SHA-256s, event
count, profile) embedded in the file.
