# Full review — 2026-07-24 (13-agent swarm)

Method: 4 independent review dimensions (efficiency, feature gaps, code
quality, test/robustness), each adversarially re-verified by a second agent
instructed to refute; concurrently 5 agents exercised every CLI feature and
judged real output. 161 commands run. 2.15M tokens.

Tally: **46 review findings** — 29 CONFIRMED, 17 OVERSTATED, 0 refuted.
35 novel, 9 still-open from the July log, 2 worse than logged.
**84 exercise problems** (20 high / 36 medium / 28 low) and 50 feature gaps.

Items marked ✅ below were reproduced by the main agent directly, not only
by the swarm.

---

## 1. SOUNDNESS — a real false PROVEN DISJOINT on the cross-file path ✅

> **FIXED 2026-07-24.** `ElemPredInterner` (hir.rs) now carries the one
> interning discipline for both the resolver and the merger. It removed
> **two false PROVEN DISJOINT verdicts from the real CMS corpus** — see
> "Live on the real corpus" below. Regression pins:
> `merge::tests::merge_never_unifies_unsupported_cuts`,
> `merge_still_unifies_identical_supported_cuts_across_units`, and the
> golden-cross case `examples/golden/cross/opaque-cut-collision/`.

**This is the only finding that threatens the tool's core claim.**

`Merger::remap_pred` (crates/adl-sema/src/merge.rs:276) interns element
predicates by render-key dedup **without** the fail-closed branch that
`Resolver::intern_elem_pred` (resolve.rs:1097) has. The resolver explicitly
refuses to share an `ElemPredId` when `node.has_unsupported()`, with a
comment naming it as a reproduced false-PROVEN factory (soundness review S1).
The merge path has no such guard.

Several unsupported render strings discard the differing substructure
entirely — e.g. resolve.rs:2411 emits ``sum` body must reference exactly one
collection once (found N plural references)`, keeping only the reducer kind
and a count. Two physically different opaque cuts therefore render
identically, unify to one `ElemPredId` → one `Filtered` CollectionId → one
size quantity in the merged table.

Reproduction (confirmed directly):

```
a.adl: object o1 / take Jet / select sum(pt(Muon) + pt(Electron)) > 5
       region RA / select size(o1) >= 3
b.adl: object o2 / take Jet / select sum(eta(Photon) * eta(Tau)) > 5
       region RB / select size(o2) <= 1

$ smash2 verify --cross a.adl b.adl
  a.adl::RA vs b.adl::RB: PROVEN DISJOINT — size(o1): [3, inf] vs [-inf, 1]
                                            ^^^^^^^^ ONE quantity for BOTH regions

# identical content in ONE file, where the resolver guard holds:
$ smash2 verify single.adl
  RA vs RB: POSSIBLY OVERLAPPING
```

The regions are not disjoint: `o1` can hold 3 jets while `o2` holds 0.

**Why every net missed it.** The proof came from the **interval fast path**,
not the solver — so no Farkas certificate was ever requested (`certified` is
absent, no `--combine` bundle is emitted). The sampling gate could not refute
it either: the opaque cut makes interpreter membership Unknown, so no sampled
event can be shown to be in both regions. The three nets all sit downstream
of an identity error that happens before any of them run.

**Certification does NOT catch this — routing everything through the
certifier makes it worse.** Forced onto the solver path (contradiction
needing two quantities so the interval layer cannot decide it), the same
collision yields `proven_disjoint` with `certified: true`, and
`smash2-recheck` replays the exported bundle as `OK`. The certificate is
*valid*: the bundle's asserts are `q1 >= 0`, `q0 - q1 >= 3`, `q0 <= 1`,
which is genuinely UNSAT with Farkas multipliers (1,1,1). The error is that
`q0` denotes two different collections. This is the encoder/identity residual
from the July trust-surface review, biting for real: the certificate proves
the *formula* is contradictory and cannot know the formula is not what the
analyses said.

### Live on the real corpus (not just synthetic)

The fix removed exactly two verdicts from the 3-file CMS merge
(042 + 043 + 049): 15 → 13 proven disjoint.

```
CMS-SUS-16-042::zerob vs CMS-SUS-16-043::preselection   was PROVEN DISJOINT
CMS-SUS-16-042::zerob vs CMS-SUS-16-043::signal         was PROVEN DISJOINT
  reason: "intervals cannot intersect on size(bjets):
           zerob requires [0,0], preselection requires [2,2]"
```

Both files declare a **byte-identical** `jets` block:

```
object jets
  take Jet j
  select pT > 30
  select abs(Eta) < 2.4
  reject dR(j, leptons) < 0.4      <- opaque; render discards `leptons`
```

but `leptons` differs between them — 042's electrons are `pt > 25`, 043's are
`pt > 30`. The two `jets` (hence `bjets`) collections are therefore
physically different, and `size(042.bjets)==0` with `size(043.bjets)==2` can
both hold on one event. The old unification made them one quantity and the
interval layer "proved" a contradiction. Every golden-cross pin and the 2-file
042/049 run were unchanged, so nothing legitimate was lost.

Fix (applied): one `ElemPredInterner` in `hir.rs` owning `preds` +
`by_render`, carrying both the render dedup and the `has_unsupported`
fresh-id branch; `Resolver::intern_elem_pred` and `Merger::remap_pred` both
go through it, so the two paths cannot drift again.

Still worth doing: an interval-fast-path PROVEN over a merge-path collection
deserves the same scrutiny as a solver one — note both real false verdicts
came through the interval layer, where no certificate is ever requested.

---

## 2. PERFORMANCE — 90% of cross-analysis runtime is z3 process startup

`SubprocessSolver` implements the incremental `Solver` trait but is not
incremental: `check()` rebuilds the **entire** SMT-LIB script and spawns a
fresh `z3 -in` child that re-parses and re-solves from scratch. All push/pop
state is discarded between queries.

Measured (PATH shim logging every exec):

| run | wall | z3 spawns | piped SMT-LIB |
|---|---|---|---|
| `--cross` 032+033 (2 files) | 1m00.6s | 3,508 | — |
| `--cross examples/CMS/` (13 files) | **9m05s** | **23,182** | **603 MB** |

Same 100 real scripts, replayed three ways:

```
COLD, one z3 spawn each ..................... 21.5 ms/query
WARM, push/pop frames in ONE process ........  3.1 ms/query   (same answers)
PERSISTENT + model retrieval ................  1.58 ms/query
bare `z3 -in` on "(check-sat)" .............. 17.2 ms  <- the startup floor
```

23,182 × 21.5 ms = 498 s against 545 s measured. The whole 9 minutes is
process startup. smash2's own RSS is 40 MB; the axiom fixpoint and the entire
non-solver pipeline for the 13-file merge run in **37 ms**.

Three separable fixes, largest first:

1. **Persistent child process** (no libz3 needed): spawn one `z3 -in` per
   Engine, write `(push)`/`(assert)`/`(check-sat)` to stdin, read one answer
   line. Verified z3 answers interactively over a pipe. Keep the fail-closed
   rules (`(error ...)`/`unsupported` → Unknown) and restart the child on I/O
   failure so `spawn_failures` accounting is unchanged. Use
   `(set-option :timeout N)` instead of the `-t:`/`-T:` CLI flags.
   Projected: 9 min → well under 1 min.
2. **Stop re-solving for model/core** (29% of all spawns, ~160 s of the
   13-file run): `model()` and `unsat_core()` each run a *second* complete z3
   process on the identical assertion set to read back what the first process
   computed and threw away. Append the getters to the check-sat script and
   cache the output. Model retrieval measured at zero marginal cost in-process.
3. **Drop the eager `refined_model` base fetch** (engine.rs:897, ~14% of
   spawns, ~79 s): `let base = s.model();` runs before any wish layer, but
   `base` is only consumed by the final `.or(base)` after all four layers
   fail — and a wish layer succeeds essentially always. Logged 2026-07-16,
   unchanged; now quantified.

Measured non-issues, contradicting earlier suspicion: certification is free
(60.6 s vs 61.3 s with `--no-certify`), the axiom fixpoint is negligible, and
the new `near_misses()` pass does **zero** work on the real corpus.

Second-tier cost centre is the event loader: a String-allocating property
canonicalizer (3 Strings per property, 2 immediately dropped) makes it 2.2×
slower than CPython's `json`, and composite tuple enumeration deep-clones the
binder env twice per tuple (12.8× the cost of the analysis it belongs to).

---

## 3. The `--combine` trust surface is thinner than advertised

The Farkas replay itself is solid — every multiplier/branch/constant mutation
tested fails closed. The gaps are all *around* it:

- **`note` and `verdict` are unauthenticated.** `replay()` checks only
  `schema` and the certificate. A forged bundle can replace the honest-scope
  sentence with an arbitrary over-claim, and `smash2-recheck` prints `OK` and
  then prints *"see each bundle's `note` for scope"* — actively directing a
  skeptical third party to attacker-written prose. Cheap fix: pin
  `note == SCOPE_NOTE && verdict == "PROVEN DISJOINT"` in `replay()`, or have
  recheck print `SCOPE_NOTE` from its own binary instead of echoing the file.
  (README:279 "a tampered bundle fails replay" is unqualified and wrong.)
- **`region_a`/`region_b` are free text echoed raw to stdout** — a bundle can
  inject newlines/ANSI and fabricate result lines in recheck's output.
- **Coverage is silently partial.** Only pairs proven *by the solver* are
  certified and bundled; interval-fast-path proofs are not. Measured: 58 of
  219 corpus PROVEN DISJOINT get a bundle. On 042+049 I confirmed 10 proven
  disjoint → 9 `certified: true`, 1 with no `certified` key at all ✅. A user
  handing over `--combine bundles/` is handing over a fraction of their claims
  with no indication the rest are missing. (Also: the JSON omits `certified`
  rather than setting it false, so filtering `certified == true` silently
  discards valid proofs.)
- **Not self-describing enough to publish.** Quantities are bare `q0`/`q22`
  with no dictionary; asserts are `R1S0`/`AX31`/`XR0` with no statement text;
  no region source, no analysis-file hash, no version stamp, no timestamp, no
  axiom statements/assumptions — all of which `--explain` already renders in
  the same run. The Farkas multiplier vector's alignment to constraints is
  defined only by `saturate.rs` behaviour and is stated nowhere in the file.
  Two different analyses can produce byte-identical bundles.
- **`parse_uint` is superquadratic** (certificate.rs:64): the digit fold
  `acc = acc*10 + d` over `BigRational` does a gcd per digit. An 11 KB bundle
  with one 10,000-digit numeral costs **18 s**; measured ~n^2.7, and it's all
  in serde deserialization *before* any fail-closed logic. A self-DoS on a
  checker you point at a file you were handed. Longest literal in real
  bundles: 5 chars. Fix: reject numerals over ~4096 chars, or accumulate in
  `BigInt`.
- Stale bundles accumulate in the output dir across runs and recheck counts
  them as passing; unreadable files are counted as "checked"; the summary
  always asserts replay proved something.

---

## 4. A bug in the near-miss advisory I shipped this week

`lower()` creates a fresh `DiagTable::default()` per call, so `DiagId`
restarts at 0 each time and every opaque conjunct becomes
`Formula::Unknown(DiagId(0))`. `near_misses()` compares with `fa == fb` and
`Formula` derives `PartialEq` — so **two completely different opaque cuts
compare equal** and get advertised as "identical cut structure":

```
a.adl: object sel1 / take Tracks / select bdt(this) > 0.5
b.adl: object sel2 / take Hits   / select aplanarity(this) > 0.9
→ note: C1#sel1 and C3#sel2 have identical cut structure but different bases
```

Soundness is intact (advisories derive nothing — verified: no XSUB/XEQ
emitted), but the *advice* is wrong. Fix: require both formulas `is_exact()`
before comparing — an opaque conjunct means the structure is *unknown*, which
is not the same as identical. Add the negative test; the current suite only
covers byte-identical cuts.

Related, milder: the gate suppresses advice only when **both** bases are in
`ext_objs.txt`, so `Jet` vs `GenJet` and `Jet` vs `AK15jet` get an advisory
(the table doesn't know those names). The wording is conditional and derives
nothing, and the arm is the documented design — but the advisory reads
identically whether the unknown side is a private analysis collection or a
name a physicist would recognise as a different detector-level object. Worth
splitting the wording. Neither arm has a test.

---

## 5. The safety nets pass vacuously without z3

Every solver-dependent suite in adl-analysis short-circuits on
`report.solver == "none"` with `continue`/`return` — no `#[ignore]`, no skip
count, no marker. 0.07 s of green looks identical to 4.59 s of green:

```
PATH with z3:    golden_regions → "51 pair pins + 13 empty pins matched"  4.59s
PATH without:    golden_regions → "ok. 1 passed"                          0.07s   ← 0 pins
```

Evaporating suites: golden_regions (51+13 pins), golden_cross (7),
reconciliation_ledger (5, including `jet_vs_electron_is_never_advised`),
combine (2), soundness_review_regressions (10), analysis_behaviors (20).
Full-workspace CI still goes red (golden_battery/report_rendering/cross_file/
cli fail hard), so this is not a silent-CI hole — but the *targeted* commands
developers are told to run give confident green while verifying nothing.

Fix: `assert!(std::env::var_os("SMASH2_ALLOW_NO_SOLVER").is_some(), ...)`
before the early return — `report_rendering.rs:31` already does exactly this
with `assert_ne!(r.solver, "none")`. Also assert the pin count so a corpus
file losing its header can't quietly shrink the net.

Other test gaps: no corpus-wide `verify` sweep in the suite (68 real analyses
CI-unguarded); the smash2-recheck **binary** has zero tests; `--combine` has
no CLI test; the two fail-closed reconciliation skip guards have no test; no
JSON snapshot for a cross run, so the new ledger/advisory keys are unpinned;
CI runs no clippy, no fmt, no MSRV, no `--locked`.

---

## 6. Physics-correctness bugs in the event pipeline

- **A `define`d constant used as a weight silently becomes 1.0.**
  `define run2lumi = 138` + `weight lumiWeight run2lumi` — which is exactly
  what the shipped `examples/tutorials/ex10_tableweight.adl` does at lines
  83/90 — logs a stderr note and weights by 1.0. Measured sumw
  `[0.0123, …]` instead of `[1.6974, …]`: **138× too small**, exit 0. With no
  `--lumi`/`--xsec` flag anywhere in the CLI, this *is* the normalization
  path, and it fails silently on idiomatic ADL.
- **`bin` yields never reach the ROOT deliverable.** The per-signal-region
  counts that *are* the result of a CMS SUSY-style analysis appear only in
  `cutflow.json` — not in the printed cutflow, not in `out.root`, not in
  `make_histos.C`/`to_root.py`. `histos.json`'s histogram array is `[]`.
- **Panic on a non-ROOT input file.** `--profile delphes` on a JSONL (the
  single most likely user mistake) aborts with a raw oxyroot assertion,
  exit 101, no user-facing message.
- `--csv`/`--svg` silently drop underflow/overflow while still labelling the
  plot with the full entry count.
- A misspelled collection name in an event record is silently absorbed as an
  unused collection → empty selection, no report. Contrast the strict,
  line-numbered error for a wrong-typed *property*.
- Ingest knows which leaves it dropped and the resolver knows which
  identifiers are unresolved; the two are never cross-checked, so an analysis
  needing a dropped branch yields all zeros with two disconnected warnings.
- ROOT_PRODUCTION_AUDIT re-check: systematics **still open**, TTree/event-list
  output **still open**, mid-selection histoList fill points **still open**,
  σ/lumi scaling **worse than logged** (see the weight bug), negative weights
  **half-open** (behaviour verified correct by hand, still zero test coverage).

---

## 7. Robustness / crash surface

- **Parser stack overflow → SIGABRT (exit 134), not a diagnostic.** 8,000
  nested parens in an *unused* `define` (a 16 KB file) aborts `smash2 check`.
  Bisection: parens ≥3,000 abort; a 10,000-term `+` chain aborts; the same
  depth inside an `info` line (raw tokens) is fine — so it's the recursive
  expression parser and the downstream HNode walkers. No depth guard exists
  in parser.rs or resolve.rs. For a tool whose purpose is ingesting
  third-party files, an uncatchable abort is the worst failure mode.
- **SIGPIPE panic** ✅ — `smash2 verify --explain <file> | head` exits 101
  with a Rust backtrace note. Reproduced; correction to the swarm's claim:
  it needs output above the 64 KB pipe buffer (`--explain` at 51 KB panics;
  the default report doesn't).
- UTF-8 BOM is a hard parse error whose message contains only the invisible
  BOM character. Column numbers are byte offsets, so carets misalign on any
  line with a non-ASCII character.

---

## 8. `check` says "ok" too easily

The command whose entire job is "is my file understood?" is silent about
everything that would make a verdict meaningless:

- 99 `partial: cut out of fragment` rows exist across the corpus; `check -v`
  reports `ok … 0 warnings` for every one of them. The information exists —
  `objects` prints it.
- An object whose `take` source is unsupported has its input replaced by the
  empty set, and `check` still says `ok` (3 corpus files, including
  `fmegajets(...)` in CMS-SUS-16-017 where megajets are the central variable).
- `select pt + 1` — a non-boolean expression, an obvious typo for
  `select pt > 1` — is accepted with zero diagnostics and the fragment is then
  reported **exact**. The repo's own `examples/bad/bad_objects.adl` passes clean.
- `1_000` misparses to `1[0]` with no diagnostic.
- `expect_tok` discriminant bug (July log #1) **still open**: a `TokKind::Kw(x)`
  pattern matches any keyword, so a table block with three wrong keywords
  parses silently — and now produces a diagnostic naming a keyword the user
  never typed.

There is no `--deny-warnings` on `check`, so CI cannot gate on any of it.

---

## 9. Reporting / UX (highest-frequency complaints)

- **Certification is invisible in both human modes.** The headline trust
  claim appears only as `certified: true` in `--json`. Likewise the sampling
  gate (`sampling: {events: 64, refutations: 0}`) — the strongest trust
  signal the tool computes — is JSON-only.
- **`--explain` files ordinary POSSIBLY downgrades under
  "== INTERNAL DIAGNOSTICS (bugs, please report) =="**, though the default
  report's own footer correctly calls them "POSSIBLY downgrades, no verdict
  changed".
- `--help` promises `--explain` gives "per-axiom statements"; no human mode
  ever prints an axiom statement.
- **The verdict matrix is silently suppressed above 20 regions** — i.e. it
  disappears exactly at the scale the "cross-analysis overlap matrix" exists
  for, with no note. And matrix row labels ellipsize at 24 chars, which for
  `file.adl::region` truncates the region name away entirely, so every region
  of a file renders as the same string.
- **Ledger rows carry no file attribution**, and `C1#name` is an internal,
  non-contiguous collection index explained nowhere — not in the output, not
  in `--help`, not in the README, with no command that resolves it. Biggest
  gap in the feature I just shipped. (Also: `--explain` omits the ledger and
  advisories entirely; 231 of 267 rows on the CMS run are "neither cut set
  implies the other" with no filtering or ranking.)
- The default report inlines full witness-event JSON into pairwise lines
  (>1,200 chars) while its own footer tells you to use `--explain`/`--json`
  for that detail.
- A systematically failing solver produces a fully green all-UNKNOWN report at
  exit 0, with a header that still names z3.
- 9 minutes of silence on a 13-file cross run; `-v` adds one line at startup.

Top missing affordances: `objects --json`; a ledger→source legend; an
analysis-level N×N rollup (the useful question at 13 analyses, vs the 62×62
region view that isn't printed); verdict filtering (`--only=disjoint`);
`--fail-on=unknown` and a gate on solver absence; a `--combine` manifest;
dimensional/plausibility warnings on kinematic cuts.

Coverage note: **CANDIDATE DISJOINT and UNKNOWN occur in none of the 136
corpus files** — both tiers are reachable only by hand-built inputs (an
integrality-only contradiction; a stub z3 answering `unknown`). Neither is
pinned by a golden file.
