# ADL2 corpus sweep report

June 2026. `smash2` run over all 68 corpus files × four subcommands
(`check`, `verify`, `dot`, `dot --ast`), every DOT output rendered
through Graphviz, plus a `run` smoke test over hand-written events.
Qualitative triage of diagnostics, diagrams, and all 1,832 pairwise
verdicts; 5 overlap witnesses hand-verified against sources; two files
re-cross-checked against the legacy tool. Raw outputs: `/tmp/adl2_sweep/`.

## Headline

No crashes, no solver errors, **zero UNKNOWN verdicts** across 1,832
pairs, all 136 diagrams render, zero diagnostic false positives found,
zero unsound witnesses found, no verdict contradictions vs legacy.
Total verify time 11.5 s (3–8 ms/pair, linear in pair count).

| Verdict | Count |
|---|---|
| PROVEN DISJOINT | 844 (46%) |
| PROVEN OVERLAPPING | 370 (20%) |
| POSSIBLY | 618 (34%) |
| UNKNOWN | 0 |
| PROVEN SUBSET annotations | 233 |
| Bin sets: coverage proven / gap-flagged | 20 / 38 |

## Real findings in the corpus (show collaborators)

1. **CMS-SUS-16-032 `compressednbnc0`: genuine bin coverage gap** —
   region floor is MET > 250 but the first bin edge is 300; gap witness
   MET = 251. The 250–300 sliver is unbinned as written.
2. **CMS-SUS-16-042 `multib`: incomplete boolean binning** (witness:
   leptons[0].pt = 251, 2 b-jets falls in no bin).
3. **CMS-SUS-16-041: dangling region reference** — six regions
   `select preselection`, but no region of that name exists in the file
   (regions are `baseline`, `onZ`, …). Source bug, correctly diagnosed.
4. The 38 "coverage not proven" bin sets include genuinely unprovable
   ones (bin variables outside the linear fragment) — conservative, not
   wrong; each lists its dropped reason.

## Tool gaps found (prioritized)

1. **NNEG axiom misses opaque `pt(...)`-named external calls** — the
   known CMS-SUS-16-032 vacuous-region transcription bug is NOT caught
   (legacy caught it): the opaque `pT(jets[0] jets[1])` scalar gets no
   ≥ 0 axiom, and the model assigns it −126.5 to satisfy the ratio cut.
   Sound (no false claim made) but a regression in finding power.
   Fix: extend NNEG to `ExternalFn` quantities named pt/m/e/energy/dR —
   physically valid for any particle-list argument.
2. **Flowchart inheritance edges missing for the `select <region>`
   form** — bare-name inheritance draws dashed edges; `select baselineHad`
   renders as label text only. The largest analysis (SUS-21-006, 24
   inheritance refs) loses its whole inheritance graph.
3. **AST diagrams degenerate on large files** — the AST DOT emits a
   forest with no layout direction/packing; SUS-21-006's AST renders
   110,175–356,776 pt wide. Needs `pack`/array mode or a synthetic root.
4. **Underscore-indexing note far too chatty** — 130 occurrences (half
   of all diagnostic volume) on valid idiomatic ADL (`METLV_0`).
   Collapse to once-per-file like the bare-path warning.
5. Minor: `pi` should be a stdlib constant (currently "unresolved
   identifier" on `pi / 4`); "tagged Unsupported in semantic analysis"
   is compiler jargon; empty cluster boxes render; `histo (no
   membership)` label unclear; TreeMaker2 variant 2.2× slower than its
   twin (cost concentrated in a 24-bin disjointness block).

## Conservatism difference vs legacy (design choice, for PARITY.md)

On CMS-SUS-16-042, legacy reports PROVEN OVERLAPPING from a witness
containing opaque function values; smash2 downgrades to POSSIBLY because
it refuses to certify witnesses that depend on opaque externals. On
CMS-SUS-16-033, smash2 is strictly stronger (encodes comparisons legacy
drops; proves overlap + subset where legacy says POSSIBLY). Neither
direction is a contradiction; the opaque-witness policy is the one open
judgment call to ratify with collaborators.

## Coverage

23 files have at least one region below 100% encoded leaves; every drop
has an honest named reason (nonlinear products/powers, non-constant-
denominator ratios beyond the supported form, `sort`, OPEN-3 negative
indices, opaque bases). Worst real file: CMSSUS16048_adl2tnm at 11/16
leaves in one region. 12 files are all-POSSIBLY, each traceable to one
un-encodable cut — no systematic encoding failure.
