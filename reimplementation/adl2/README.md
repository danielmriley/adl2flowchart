# ADL2 (`smash2`) — analysis toolchain for ADL

ADL2 parses [ADL](https://cern.ch/adl) (Analysis Description Language)
files, **interprets** them over event records, **verifies** relations
between selection regions with solver-backed proofs that are then
**independently certified** with exact-rational arithmetic, and
**visualizes** them as Graphviz diagrams. It is the from-scratch successor
to the legacy tool in `../../legacy_parser/`, built so that the soundness
properties the legacy tool earned through two audits hold here *by
construction*.

New here? Start with [docs/QUICKSTART.md](docs/QUICKSTART.md) — a 5-minute
tour from `check` to the solver-backed `verify` proofs.

Status: all spec phases through the parity gate draft are built and green,
plus Phase 9 histogram production (`run --histos`, native pure-Rust
`out.root`) and the full Phase 10 event pipeline: Delphes ingestion
(`ingest` / `run --profile delphes`), per-region cutflows, TH2D +
variable-bin histograms in per-region `TDirectory`s, embedded provenance,
and a streaming chunked-parallel run loop that is byte-deterministic at any
`--jobs`. The numeric core of the analyzer is **exact rational** (`0.3` is
`3/10`, not an f64); merged-unit **cross-file** verdicts (`verify --cross`)
with same-base collection reconciliation are shipped; and every
disjointness proof is **independently re-checked** by a self-contained
exact-rational certifier (`adl-certify`) that replays a Farkas certificate
against a trusted kernel — a proof the solver's word alone never has to be
taken for. End-to-end validated on the real 20k-event T2tt Delphes sample
against independent uproot/numpy oracles (see
[`PIPELINE_REPORT.md`](PIPELINE_REPORT.md)) — 751 tests across 71 suites, a
136-file corpus (68 base + 58 pinned-verdict golden + 10 cross-file golden),
the full legacy golden battery on both solver backends, a 100k-case
property oracle against the interpreter, and a verdict-parity comparison
against the legacy tool with zero legacy-better differences
(`../PARITY_DRAFT.md`).

---

## Quick start

Requirements: stable Rust (≥ 1.93). The default build links **nothing** —
it uses the SMT-LIB subprocess backend and needs only a solver *binary* on
PATH at runtime: `apt install z3`. With no solver at all,
verdicts degrade honestly to POSSIBLY.

```bash
cd reimplementation/adl2
cargo build --release           # no libz3 needed; subprocess backend
alias smash2=$PWD/target/release/smash2

smash2 --version                # smoke test
smash2 verify ../../examples/tutorials/ex01_selection.adl   # first real run
```

The one external dependency is the SMT solver binary, and only at runtime:
`z3` (any recent build) must be on `PATH` for the `verify` proofs. The
independent certifier is pure Rust and always compiled in — it needs no
solver and no extra flag. `dot` visualization output additionally wants
Graphviz (`apt install graphviz`) to rasterize, but `smash2 dot` itself
only emits DOT text and has no build-time dependency on it.

**Solver backends.** The subprocess backend (default) shells out to the
`z3` binary per check — zero link burden, the right default for a stock
machine. For heavier workloads (e.g. the 100k-case property battery) the
faster **in-process** backend is an opt-in:

```bash
# in-process libz3 — needs system libz3 (apt install libz3-dev):
cargo build --release -p adl-cli --features native
# in-process libz3, built automatically from vendored source (no system
# libz3; needs a C++ toolchain + cmake):
cargo build --release -p adl-cli --features bundled
```

Both backends are conformance-tested to return identical verdicts; the
choice is purely performance. (Verdicts can still differ between *z3
versions* on the SAT side — which witness a model returns — so pin one z3
for reproducible witness output.)

### The subcommands

**`check` — parse + resolve, report diagnostics**

```bash
smash2 check analysis.adl              # exit 1 on errors; stdout stays clean
smash2 check --dump-ast analysis.adl   # canonical AST text dump to stdout
smash2 check --json analysis.adl       # diagnostics as a JSON array (editors/CI)
```

Diagnostics carry spans, labels, and help ("`selct` is not a keyword; did
you mean `select`?"), report *every* error in a file (the parser
resynchronizes at statement boundaries), and go to stderr.

**`verify` — the analysis (legacy `smash -r` equivalent)**

```bash
smash2 verify analysis.adl
smash2 verify --explain analysis.adl                # full proof chains: unsat cores, witness values, per-axiom statements
smash2 verify --json analysis.adl > report.json     # versioned schema (v3)
smash2 verify --no-solver analysis.adl              # interval heuristic only
smash2 verify --no-certify analysis.adl             # skip the independent certifier (see below)
smash2 verify --fail-on=overlap,empty analysis.adl  # CI gating on findings
smash2 verify a.adl b.adl                           # each analyzed independently (per-unit reports)
smash2 verify --cross a.adl b.adl                   # merged unit: cross-FILE overlap matrix
smash2 verify --cross analyses/                     # a directory expands to its *.adl files
```

`--fail-on` kinds are `overlap`, `gap`, `empty`, `non-exact` (comma-
separated); the exit code is nonzero if any listed finding is present, for
CI gating.

A directory argument contributes its `*.adl` files (sorted, non-recursive,
deduped against the other inputs). Without `--cross`, several files are
each analyzed on their own — a per-unit report in human mode, a JSON array
under `--json` (also whenever a directory was given, regardless of its
file count). With `--cross` the files are merged into one analysis unit
and regions are namespaced `file::region` (same-named regions across files
are never falsely unified; colliding basenames are qualified by path),
producing the sound cross-analysis overlap matrix — the identity model was
built for exactly this (design notes in
[`MULTIFILE_PLAN.md`](MULTIFILE_PLAN.md)). Cross runs additionally
reconcile same-base filtered collections across files: when one file's
element predicate provably implies the other's, the derived
`size(A) ≤ size(B)` fact (axiom XSUB, or XEQ for both directions) links
the two analyses' object counts — under the documented residual
assumption that the same detector-base name means the same input.

Per region: encoding coverage with named dropped cuts, vacuity check.
Per pair: PROVEN DISJOINT / CANDIDATE DISJOINT / PROVEN OVERLAPPING (with
witness) / CANDIDATE OVERLAPPING / PROVEN SUBSET / POSSIBLY / UNKNOWN, each
with an explanation (unsat core mapped to source lines for disjointness; a
validated witness event for overlap). Per `bin` set: pairwise disjointness
and region coverage with gap witnesses. Output is deterministic — two runs
are byte-identical.

**Two independent nets sit behind every PROVEN verdict**, on by default:

- **Certification** (`adl-certify`). When the solver returns UNSAT for a
  disjointness/emptiness query, the unsat core is handed to a self-
  contained exact-rational checker that must produce a replayable Farkas
  certificate — a proof re-checked by a small trusted kernel, in exact
  arithmetic, with no dependence on the solver. A pair the checker cannot
  certify (budget, shape, or an integrality-only refutation) is reported as
  **CANDIDATE DISJOINT** — an honest "the solver says so but we could not
  independently prove it" rather than a bare PROVEN. Certified pairs carry
  `certified: true` in `--json`. Turn it off with `--no-certify`.
- **Sampling gate.** Every PROVEN pair is additionally checked against a
  deterministic battery of boundary events pushed through the reference
  interpreter; a sampled event that the interpreter accepts into both
  regions of a "disjoint" pair is an internal contradiction, so the verdict
  fails closed to POSSIBLY and a bug diagnostic is filed. (No real event
  should ever trip this — it is a live self-audit of the encoder/axioms.)

**`run` — interpret regions over events**

```bash
smash2 run analysis.adl events.jsonl              # per-region pass/fail + bin assignment
smash2 run analysis.adl events.jsonl --histos out/  # + fill histograms (see below)
smash2 run analysis.adl events.root --profile delphes  # straight off a Delphes file
```

Events are JSONL: per-collection ordered object lists with properties,
plus event scalars and trigger flags. `run` is the *reference semantics* —
the verifier is property-tested against it, and every overlap witness is
re-validated through it before being shown.

`--histos DIR` additionally accumulates the file's `histo` statements and
writes four outputs into `DIR` — `histos.json`, a native `out.root`, and
two ROOT bridge scripts — covered in [Histograms](#histograms) below.

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

| Verdict | Matrix | Claim | Sound because |
|---|---|---|---|
| PROVEN DISJOINT | `D` | no event can pass both regions | checked on an over-approximation of each region: if even the supersets cannot intersect, the regions cannot — and the unsat core is independently certified in exact arithmetic |
| CANDIDATE DISJOINT | `d` | the solver reports the regions disjoint, but the proof could not be independently certified — **not a certified proof** | the uncertified tier is reported separately instead of overclaiming PROVEN; disable the certifier with `--no-certify` to collapse it back into PROVEN |
| PROVEN OVERLAPPING | `O` | a concrete event passes both | checked on under-approximations; the realized witness event is accepted by the reference interpreter in both regions (`witness_validated = true`) |
| CANDIDATE OVERLAPPING | `c` | a joint model exists, but it rests on an opaque quantity the interpreter cannot decide — **not a proof of overlap** | the unvalidated tier is reported separately instead of overclaiming PROVEN; conservative for combination studies |
| PROVEN SUBSET A⊆B | `s` | every event passing A passes B | UNSAT(A⁺ ∧ ¬B⁻) |
| region EMPTY | `E` | the region's cuts contradict physical axioms | UNSAT(R⁺ ∧ axioms) |
| POSSIBLY / UNKNOWN | `?` / `U` | no claim | — |

Anything the tool cannot encode faithfully becomes an explicit `Unknown`
with a reason you can read in the report — it can weaken a verdict to
POSSIBLY, never flip it. Overlap verdicts are always printed with their
model caveat; a PROVEN OVERLAPPING witness's displayed values are read
back from the interpreter-validated event. **CI note:**
`--fail-on=overlap` fires on both PROVEN and CANDIDATE OVERLAPPING
(fail-closed — an unvalidated candidate may still be a real overlap).

---

## Histograms

ADL `histo` statements declare per-region histograms; `smash2 run
--histos DIR` fills them while it streams events and writes the results to
`DIR`. Histograms are auxiliaries (not part of the analysis algorithm), so
they only ever affect the `--histos` outputs — the per-event table and the
verifier are untouched.

**Fill semantics.** A histogram fills once per event, on **full
acceptance** of the selection region that declares it. The fill value is
the statement's expression (`histo hmet, "MET (GeV)", 40, 0, 1000, MET`)
evaluated through the same reference interpreter `run` uses. Binning is
ROOT's `TH1::Fill`: `x < lo` → underflow, `x >= hi` → overflow, the top
edge is open. `entries` is the **raw fill count** (ROOT `fEntries`),
including flow-bin fills. A `histoList` block is a template instantiated
into each region that references it (filled once on that region's full
acceptance); plain region inheritance does **not** import histograms. An
event whose fill expression has no value (missing element/property,
non-finite arithmetic) or hits a hard evaluation error is counted and
summarized on stderr — no entry is recorded. 1-D, **variable-bin 1-D**, and
**2-D** histograms are all supported (`histo h, "t", nx, xlo, xhi, ny, ylo,
yhi, xexpr, yexpr` for 2-D; `histo h, "t", e0 e1 … en, expr` for variable
bins); each carries the matching ROOT fill-time stats moments (four for 1-D,
seven for 2-D).

**Weights.** Each fill is weighted by the **product of the region's own
numeric `weight` statements** (`weight lumi 2.0`). A non-numeric weight
argument yields a diagnostic and a weight of 1.0; inherited regions'
weights do not apply. The accumulator tracks ROOT `Sumw2` errors (per-bin
Σw and Σw², including the flow bins) so `GetBinError` is `sqrt(Σw²)`, and
it accumulates the four fill-time stats moments **at fill time, in-range
fills only** (Σw, Σw², Σw·x, Σw·x² — ROOT's `GetStats` convention) so
`GetMean`/`GetStdDev` and `hadd`-merged stats stay exact. In `out.root`
histograms live in **per-region `TDirectory`s** keyed by bare name
(`baseline/hmet`, `singlelepton/hlep1pt`); `--flat-names` keeps the v1 flat
region-prefixed layout (`baseline_hmet`) that the bridges use and that some
`hadd` workflows prefer. Both layouts are `hadd`-mergeable.

**Cutflows.** Every `run --histos` also writes a per-region **cutflow**: an
ordered list of steps (step 0 `all`, then one step per membership-affecting
statement — `select`/`reject`/`trigger`, and parent inheritance as a single
step), each with `raw`/`sumw`/`sumw2`/`errors`. A hard evaluation error at a
step counts the event as *failing* that step and increments `errors` (a
faithful diagnostic, never a guessed pass). Cutflows emit three ways from
one accumulator: the canonical `cutflow.json`, a stdout table, and a TH1D
pair per region in `out.root` (`<region>__cutflow_raw` Poisson + `__cutflow_wt`
Sumw2) whose bins are **labeled with the verbatim statement text**, sitting
in the region's `TDirectory`.

**Provenance.** Every output carries one canonical `provenance` object,
byte-identical across them: tool version, ADL file + sha256, input file +
sha256 + event count + profile, and the per-run `[DECIDE]` choices. In
`out.root` it is a `smash2_provenance` TNamed (read it with
`rfile.Get("smash2_provenance")->GetTitle()`); the JSON outputs embed it
verbatim.

**The outputs** (all written into `DIR`, all byte-deterministic across runs):

| File | What it is |
|---|---|
| `histos.json` | the canonical record (name, title, region, `type` h1/h1var/h2, edges/axes, per-bin Σw/Σw², under/overflow, entries, the stats moments) + `provenance`. Single source of truth — every other format is a renderer of it. |
| `cutflow.json` | the canonical per-region cutflow (steps with raw/sumw/sumw2/errors, bin appendix) + `provenance`. |
| `out.root` | a native ROOT file — one `TH1D`/`TH1D`(varbin)/`TH2D` per histogram and the cutflow TH1D pair, in per-region `TDirectory`s, plus the provenance TNamed — written in pure Rust by the [`rootfile`](#the-rootfile-crate) crate, no ROOT or Python on your machine. Opt out with `--no-root`. |
| `make_histos.C` | a self-contained ROOT macro (`root -l -b -q make_histos.C` → `histos.root`); zero dependencies beyond ROOT itself. |
| `to_root.py` | an uproot 5 + numpy script (`python3 to_root.py` → `histos.root`) for Python-side collaborators. |

`--csv` and `--svg` add a per-histogram CSV (`bin_lo,bin_hi,content,error`
for the in-range bins) and a hand-rolled step-plot SVG quick-look (no
plotting dependency). All four formats agree on the flat region-prefixed
names, so a `.root` from any path is `hadd`-mergeable with the others.

```bash
# Fill ex02's histograms over the committed toy-event fixture, all formats
# (paths relative to this directory, reimplementation/adl2):
smash2 run ../../examples/tutorials/ex02_histograms.adl \
  crates/adl-difftest/tests/fixtures/ex02_events.jsonl \
  --histos out/ --csv --svg

# Skip the native .root (keep histos.json + the two bridge scripts):
smash2 run analysis.adl events.jsonl --histos out/ --no-root
```

**What a collaborator does with the output.** Four equivalent paths to the
same histograms, pick whichever your environment has:

- **ROOT, directly:** `root -l out/out.root`, then `baseline->cd();
  hmet->Draw()` (or `new TBrowser` and click into the region directories) —
  the native file opens with no extra step.
- **ROOT, via the macro:** `cd out && root -l -b -q make_histos.C` builds
  `histos.root` from scratch (useful when you want the build script in
  version control rather than a binary, or to tweak titles/styling).
- **Python/uproot:** `python3 out/to_root.py` writes a byte-equivalent
  `histos.root`, or open the native file directly:
  `uproot.open("out/out.root")["baseline/hmet"].values(flow=True)`.
- **No ROOT at all:** the `--csv` tables drop straight into a spreadsheet
  or pandas, and the `--svg` quick-looks open in any browser.

### The `rootfile` crate

`out.root` is produced by **`rootfile`** — to our knowledge the first
pure-Rust writer of ROOT `TH1` histogram files (the existing Rust ROOT
crates read/write `TTree`s only; none write `TH1`). It is a standalone,
zero-dependency workspace member: small-format `TFile` container, `TKey`
records, a single root `TDirectory`, uncompressed `TH1D` v3 objects with
`Sumw2`, and a vendored uproot `TStreamerInfo` blob so the files are fully
self-describing. Its API is independent of the rest of the toolchain —
`RootFile::create().add_th1d(name, &H1Spec { … })?.finish(path)` — so it is
usable on its own for any Rust program that needs to emit ROOT histograms.

Validation is layered and runs in CI: the serialized `TH1D` payload is
asserted **byte-identical** to one uproot writes for the same histogram
(against a checked-in reference, plus an env-gated test that regenerates
the reference with a pinned uproot at test time), a strict in-crate reader
re-parses every file and checks the framing/key invariants, and an uproot
read-back test confirms values, variances, axis edges, `fEntries`, and the
stats array round-trip. The `out.root` `smash2` writes here is read back
exactly by uproot in the wiring tests.

---

## Event ingestion (Delphes)

`smash2` reads Delphes ROOT files directly — no external converter
(SPEC_EVENT_PIPELINE §1). Experiment specifics live in **converter
profiles** (a pure data table in `adl-ingest`: branch names → canonical
keys, tag-derivation rules, weight source); the core event model never
sees experiment names. Two profiles ship: `delphes` and `nanoaod` (CMS
NanoAOD — `Events` tree, `n<Coll>` counters, underscored leaves, flat
per-event `MET_pt`/`MET_phi`/`genWeight` scalars, and the continuous
`btagDeepB` discriminant as the `btag` property).

```bash
# Run an analysis straight off a Delphes or NanoAOD file (native read):
smash2 run analysis.adl events.root --profile delphes
smash2 run analysis.adl nano.root  --profile nanoaod

# Materialize canonical JSONL (byte-deterministic) for debugging/fixtures:
smash2 ingest events.root --profile nanoaod -o events.jsonl

# Generate the independent uproot oracle script (also the no-Rust path):
smash2 ingest --profile nanoaod --emit-script out/
python3 out/to_jsonl.py events.root events.jsonl   # byte-identical output
```

Both profiles are validated against their generated uproot oracle on a real
sample (the Delphes T2tt file; the committed CMS Open Data ttbar NanoAOD
fixture) — the native reader and the independent uproot path must produce
byte-identical JSONL.

The mapping (branch → canonical): `Jet`/`FatJet` pt/eta/phi/m + `btag`/
`tautag` flags from the Delphes bitmasks (bit 0 = the card's default
working point; other set bits are *diagnosed*, never folded in),
`Electron`/`Muon` pt/eta/phi/q with PDG masses as profile constants,
`Photon` pt/eta/phi/e, `MissingET` → `MET.pt`/`MET.phi`, `ScalarHT.HT` →
`HT`, `Event.Weight` → the event weight (JSONL top-level `"weight"`,
absent = 1.0). Anything the profile cannot map faithfully is a stderr
**diagnostic** — LHE multiweights, unmapped leaves, unknown branches,
MET multiplicity anomalies — and `--verbose` adds the profile's
per-`[DECIDE]` choices and full dropped-leaf lists. Invariants are
enforced at ingest with hard refusals, never silent fixes: collections
must arrive pT-descending (no re-sort) and non-finite values are
rejected.

The native reader (oxyroot, pinned `=0.1.25`) is validated against the
generated uproot script — an independent reader implementation — byte
for byte: continuously on the committed fixtures
(`crates/adl-ingest/fixtures/`, env-gated `SMASH2_RUN_UPROOT_ORACLE=1`)
and on the full 20 000-event T2tt tutorial sample (env-gated
`SMASH2_RUN_DELPHES_E2E=1`; fetch it with
`scripts/fetch_delphes_sample.sh`).

### Real-data quickstart

CutLang's own tutorial sample is the blessed reference (their notebook runs
`exHistos` ≡ our `ex02_histograms.adl` over it):

```bash
# 1. fetch the pinned 20k-event Delphes sample (sha256-checked, ~71 MB)
scripts/fetch_delphes_sample.sh        # -> ~/.cache/smash2/delphes_T2tt_700_50.root

# 2. run the full pipeline straight off the ROOT file
./target/release/smash2 run examples/tutorials/ex02_histograms.adl \
  ~/.cache/smash2/delphes_T2tt_700_50.root --profile delphes --histos out/ --json

# out/ now holds histos.json, cutflow.json, out.root (per-region TDirectories
# with TH1D/TH1D-varbin/TH2D + the labeled cutflow pair + provenance TNamed),
# and the make_histos.C / to_root.py bridges. Determinism is byte-exact for
# any --jobs.
```

This run is validated end-to-end against independent oracles (uproot
ingestion fidelity, uproot+numpy cutflow recompute, distribution sanity,
out.root round-trip, `--jobs 1`≡`--jobs 8` byte identity) — see
[`PIPELINE_REPORT.md`](PIPELINE_REPORT.md).

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
diagnostic. Every disjointness UNSAT is then handed to **`adl-certify`**, a
self-contained exact-rational checker independent of the solver: it
searches for a Farkas certificate over the rationals and replays it through
a small trusted kernel, so a PROVEN DISJOINT never rests on the solver's
word alone — an uncertifiable core is reported as CANDIDATE DISJOINT
instead. On the SAT side, a deterministic **sampling gate** pushes boundary
events through the interpreter and fails any PROVEN pair closed to POSSIBLY
if a sampled event lands in both regions.

The numeric core is **exact rational** (`adl_sema::Rat`, a `BigRational`
newtype with shortest-round-trip decimal semantics — `0.3` is exactly
`3/10`), so boundary folding never invents an f64 seam that the legacy
tool's stepwise floats once turned into false PROVEN verdicts. Where the
analyzer's flattened canonical form *could* diverge from the interpreter's
stepwise f64 — an additive expression that is not f64-faithful (more than
one add/sub, or a non-dyadic additive constant) — an **f64-faithfulness
guard** interns the operand as a structure-keyed opaque scalar instead of
a shared linear atom, so two regions that round differently can never
unify into a false disjoint. The encodable fragment now covers ratio cuts
and ratio-bands (exact denominator clearing; nonlinear denominators stay
opaque), inclusive/excluded bands (`[]`/`][`), scalar n-ary `min`/`max`,
`abs`, bare and back-indexed elements (`jets[-1]`), static slices, and
operator-scoped unindexed angular cuts (`dR(A,B)` as a single min-pair
quantity). The pT-ordering axioms include front ORD, back-index ORD, and a
**front-to-back ORD** fact (`pt(C[i]) >= pt(C[-k])`, emitted only when
`i == 0` or `k == 1`, the size-invariant cases) — each proven sound
against the interpreter's accepted-event domain.

**Why trust it**: beyond unit tests, the encoder is property-tested
against the interpreter (random regions × sampled events; any PROVEN
verdict that contradicts sampling is a release-blocking bug — this
battery caught and fixed a real missing-element soundness bug during the
build, see `COUNTEREXAMPLES.md`), a metamorphic suite checks invariances
(`reject c` ≡ `select not c`, rename invariance, …), the entire legacy
golden battery — every historical false-verdict bug from two audits of the
old tool — runs as integration tests, and a hand-authored **golden verdict
corpus** (`../../examples/golden/`, 58 single-file + 10 cross-file across 5
merge groups) pins fully-known disjoint/overlapping/empty ground truth:
each file declares its expected verdict in a `# GOLDEN` / `# GOLDEN-CROSS`
header and `golden_regions.rs` / `golden_cross.rs` assert the analyzer
reproduces it exactly. Paired with the property oracle (which guarantees no
false PROVEN) and the independent certifier, a green golden run means those
PROVEN headers are real. Every confirmed counterexample the batteries have
ever found is regression-locked in `COUNTEREXAMPLES.md` + `regressions.rs`.

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
| `adl-ingest` | converter-profile event ingestion (Delphes ROOT → canonical events; native oxyroot reader + generated uproot oracle) |
| `adl-formula` | polarity-typed formula IR + projections; HIR→formula encoder |
| `adl-axioms` | audited axiom catalog (+ prohibited-axiom regressions) |
| `adl-solver` | `Solver` trait; native-z3 and SMT-LIB subprocess backends |
| `adl-certify` | self-contained exact-rational DPLL(Farkas) checker; replays a certificate against a trusted kernel to certify solver UNSAT results independently |
| `adl-analysis` | pairwise verdicts, vacuity, subset, bins, witnesses, sampling gate, certification, reports/JSON |
| `adl-viz` | flowchart + AST DOT from HIR |
| `rootfile` | pure-Rust ROOT file writer (`TH1D`, variable-bin `TH1D`, `TH2D`, labeled cutflow pairs, per-region `TDirectory`s, `TNamed` provenance — the `run --histos` native `out.root`); standalone, zero-dependency |
| `adl-difftest` | event generator, property/metamorphic batteries, legacy harness |
| `adl-cli` | the `smash2` binary |

Specs and design records live one directory up: `../SPEC_LANGUAGE.md`,
`../SPEC_ARCHITECTURE.md`, `../SPEC_ANALYSIS.md`, `../TESTING.md`,
`../DECISIONS.md` (ADRs tied to the legacy bugs that motivated them),
`../PHASE0_RESOLUTIONS.md` (current answers to the open semantic
questions), `../PARITY_DRAFT.md`, `../SPEC_EVENT_PIPELINE.md` (Phase 10).
Build history: `BUILD_NOTES.md`, `BUILD_REPORT.md`, `COUNTEREXAMPLES.md`,
`PIPELINE_REPORT.md` (Phase 10 real-sample e2e).

```bash
cargo test --workspace          # full battery (751 tests / 71 suites, subprocess backend)
scripts/corpus_gate.sh          # all 136 example files parse + resolve
cargo test -p adl-analysis --test golden_regions # single-file golden verdict corpus (needs a solver)
cargo test -p adl-analysis --test golden_cross    # cross-file (merged/reconciled) golden corpus
cargo test -p adl-certify        # the independent exact-rational certifier (kernel, replay, tamper)
cargo test --workspace --features native         # same battery, in-process libz3 backend
cargo test -p adl-difftest --features deep        # 100k-case property oracle (use --features native too)

# env-gated oracles (need .venv-uproot on PATH; see BUILD_NOTES.md):
ROOTFILE_REQUIRE_UPROOT=1 cargo test -p rootfile --test uproot_oracle
SMASH2_RUN_DELPHES_E2E=1 cargo test -p adl-cli --test ingest
```

## Known limits / open items

- Some semantic questions remain pinned to convention-neutral defaults
  pending a project decision (Daniel + collaborators) — the dPhi/dEta sign
  convention, `~=`, and size aliases — see `../PHASE0_RESOLUTIONS.md`;
  deciding them upgrades several POSSIBLY verdicts to exact. (Negative
  indices and the quantifier reading of unindexed angular cuts `dR(A,B)`
  are now resolved, operator-scoped.)
- The per-event scalar model caveat applies to overlap witnesses
  (opaque external-function values are free variables). A back-indexed
  element is a sound free leaf on the disjoint/empty side, but the witness
  builder cannot realize it, so an overlap that depends on it caps at
  POSSIBLY.
- Known residual soundness boundary (out of corpus, monitored by the
  property oracle): single-subtraction catastrophic cancellation `q1 - q2`
  with `q1 ≈ q2` huge passes the f64-faithfulness guard.
- Legacy feature not yet ported: the object-pair disjointness printout
  (still in `../../legacy_parser/`). The object-attributes listing *is*
  ported — `smash2 objects`.
