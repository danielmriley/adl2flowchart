# ADL2 implementation plan (v1.1 — post spec review)

Phases are strictly gated: a phase is done when its **exit criteria pass
in CI**, not when its code exists. Estimates assume one focused developer
with agent assistance. The spec defects found in the June 2026 review
(identifier/underscore lexing, missing EBNF productions, index typing,
newline termination, unsat-core example) are already fixed in the specs;
Phase 0 re-validates the corrected grammar as a whole.

Dependency spine: P0 → P1 → P2 → {P3, P4} → P5 → {P6, P7} → P8.
P3 (interpreter) and P4 (formula/encoder) can proceed in parallel after
P2; everything downstream of P5 depends on both.

---

## Phase 0 — Spec ratification (≈ 1 week)

Freeze SPEC_LANGUAGE v1.0. No production code; throwaway tooling allowed.

Work items:
- **Grammar validation harness**: a quick parser skeleton (or an
  instrumented run of the legacy parser) that checks the corrected EBNF
  against all 68 corpus files and produces the divergence lints:
  mixed `and`/`or` chains without parentheses (expected: none),
  bare path tokens, fractional bin edges, identifier/underscore splits
  (`goodJets_1`-style — expected in ex04/ex10/SUS-16-033), scientific
  notation (expected: none).
- **Resolve OPEN-1…OPEN-5 by collaborator decision** (Daniel + ADL
  collaborators), informed by the corpus and the reference interpreter:
  quantifier reading of unindexed collection cuts, dPhi/dEta sign
  convention and range, index base + negative indices, `~=` semantics,
  size/`Size`/`count` aliases. Each decision lands in the spec as the
  recorded answer; until decided, items keep an explicit Dual/diagnostic
  strategy.
- **Collaborator decisions** (Daniel + ADL stakeholders): case
  sensitivity of resolution, `~=`, index base, and whether the
  CMS-SUS-16-032 vacuous-region finding should be fixed in the corpus
  (it is currently a *useful* acceptance test for vacuity detection).

Exit: spec tagged v1.0; corpus parse report attached; zero unresolved
[DECIDE] markers or each explicitly deferred with its named strategy.

## Phase 1 — adl-syntax (≈ 1.5 weeks)

Cargo workspace bootstrap with CI from day one (stable Rust, clippy
`-D warnings`, rustfmt, insta, cargo-fuzz). Lexer (incl. the
underscore-split rule and NEWLINE handling for greedy productions),
recursive-descent parser (one function per EBNF nonterminal), spanned
AST, multi-error diagnostics with statement resynchronization,
`--dump-ast` canonical form.

Exit: corpus gate green (68/68, zero errors); AST snapshots for 15
corpus files + all legacy goldens reviewed; fuzzer clean for 1 CPU-hour;
error-message review on 10 deliberately broken files (incl. `selct`
typo, stray `;`, unterminated string, `not not` — which must now parse).

## Phase 2 — adl-sema: Quantity model + HIR (≈ 2 weeks)

Symbol resolution; `CollectionId`/`QuantityId` interning; define
resolution with cycle errors; fragment tagging (`InFragment` /
`Unsupported(reason)`); HIR. Pure-alias unification as a resolution
fact; ext_objs/ext_lib/property_vars ingested into typed declarations
at load time. `ElemIndex::FromBack` enabled or diagnosed per OPEN-3's
Phase-0 answer.

Exit: HIR snapshots for the golden suite; quantity-table dumps
hand-reviewed for three real files (SUS-16-032, SUS-16-033 Delphes,
SUS-21-006); identity unit battery green (pure rename ≡ source,
filtered ≢ parent, `jets[0].x` ≢ `jets[1].x`, oriented angular pairs
distinct, define resolves to body).

## Phase 3 — adl-interp: the executable spec (≈ 1.5 weeks, parallel with P4)

Event model (JSONL records), evaluator for the checked fragment
implementing SPEC_LANGUAGE §4 exactly (ordering, union, ternary, bands,
div-by-zero per the verified semantics, bin assignment), `smash2 run`.
Toy event generator in adl-difftest.

Exit: every SPEC §4 clause has a unit test; bin assignment tested
against boundary edges; differential run on the Phase-0 decision suite
against legacy `smash` with zero unexplained disagreements.

## Phase 4 — adl-formula + encoder (≈ 2 weeks, parallel with P3)

Formula IR with `Over`/`Under` type-enforced polarity; NNF negation
(Dual branch swap); `LinAtom` construction rejecting non-finite
constants; HIR→Formula encoder per SPEC_ANALYSIS §1 (ratio branches,
defines pre-inlined by sema, OPEN-1 strategy as resolved, Dual with the
empty-collection case in plus if OPEN-1 stayed ambiguous).

Exit: encoder-vs-interpreter property tests green at 10⁵ random
region/event cases (this requires P3 — schedule the joint week
accordingly); metamorphic battery green (swap symmetry, reject ≡ select
not, double negation, inline-vs-named define, inherit-vs-paste, pure
rename); Unknown/coverage notes human-reviewed across the corpus.

## Phase 5 — axioms, solver, analysis (≈ 2.5 weeks)

Axiom catalog with per-axiom tests (incl. the prohibited-axiom
regression tests); z3-native backend; SMT-LIB subprocess backend;
backend conformance battery; pairwise engine (interval fast path on the
And-spine of Over, batched incremental solving, vacuous regions, subset,
bin partition checks); witnesses with **interpreter re-validation**
(failed validation downgrades the verdict — production behavior, not
test-only); unsat-core explanations mapped to source spans; human
report + versioned JSON + `--fail-on`.

Exit: full ported golden battery green — the legacy dual-encoding
regression suite AND the June audit suite (empty-∀, define-arith,
angular order, union size, non-finite constants, btag discriminant);
explanations reviewed on SUS-16-033; determinism check (byte-identical
reruns); no-solver degradation capped at POSSIBLY; subprocess-backend CI
job green with the native feature disabled.

## Phase 6 — viz + CLI (≈ 1 week)

DOT flowchart/AST emitted from HIR (cannot disagree with the verifier by
construction); `smash2 check | verify | run | dot` subcommands; `--json`;
clean machine output by default (the legacy stdout soup is a non-goal).

Exit: DOT renders for the corpus; CLI snapshot tests; `--fail-on`
behavior tests.

## Phase 7 — Parity gate & switchover (≈ 1.5 weeks)

Verdict-matrix differential vs legacy `smash -r` across corpus + golden
suites. Every difference classified and signed off: ADL2-better (cite
spec/audit), legacy-better (fix ADL2 before the gate), or spec change
(document in PARITY.md). Performance: ≤ 2× legacy wall-clock on
SUS-16-033 (expect faster via native incremental solving).

Exit: signed-off `reimplementation/PARITY.md`; legacy marked deprecated in the
README; `make test` switched to ADL2 with the legacy run kept as a
nightly comparison for one release cycle.

## Phase 8 — Cross-file foundation (≈ 2 weeks, then iterate)

`AnalysisUnit` scoping; `CrossLink` pass under the explicit
`--assume-same-events` banner (Base unification, solver-proven
filtered-collection equivalence, implication ⇒ subset facts, trigger
unification); `--cross a.adl b.adl`; identity report; M×N matrix export
(JSON/CSV). Goldens: duplicated-file self-test (all overlapping +
mutually subset), complementary-windows disjoint pair,
same-name-different-filter must-NOT-unify, different-name-same-filter
must-unify.

Exit: 032 × 033 matrix reviewed with collaborators; identity report
audited; cross-file goldens green.

## Phase 9 — Histogram production (≈ 1.5 weeks; replaces external runtimes)

The project produces histograms itself; design per the June-2026 research
report (no Rust crate writes ROOT TH1 files today — oxyroot is TTree-only
and dormant, root-io is dead — so we accumulate natively and emit bridges).

- **Accumulator** (adl-interp): `ndhistogram 0.13` `Hist1D<Uniform,
  WeightedSum>` — uniform bins, under/overflow, sum-of-weights and
  sum-of-weights² (ROOT `Sumw2` semantics). ADL `histo h, "title", n, lo,
  hi, expr` statements become fills during `smash2 run`, gated on region
  membership, weighted by the region's `weight` product (event weights
  multiply; table weights deferred). 2-D histo form if the corpus needs it.
- **Canonical output**: `histos.json` — name/title/region path/edges/
  contents/sumw2/under/overflow/entries. Single source of truth; every
  other format is a renderer of it.
- **ROOT bridges, both generated next to the JSON**:
  `make_histos.C` (primary; collaborator runs `root -l -b -q
  make_histos.C` → real `.root` with TH1Ds in per-region TDirectories,
  Sumw2 + errors + entries intact; zero dependencies on our side or
  theirs beyond ROOT itself) and `to_root.py` (secondary; uproot 5.x +
  hist for Python-side collaborators; byte-equivalent histograms).
- **No-ROOT path**: `--csv` per histogram and `--svg` quick-look step
  plots (hand-rolled SVG, no plotting dependency).
- CLI: `smash2 run file.adl events.jsonl --histos out/` writes all of the
  above; `run --json` gains a histogram section.

Exit: interpreter unit tests for fill/weight/under-overflow/Sumw2
semantics; golden JSON for a corpus file with histos (ex02_histograms);
generated `.C` macro validated by running it under ROOT where available
(env-gated CI job), else by a checked-in expected-output fixture; uproot
script validated the same way; determinism (byte-identical reruns).

---

**Total ≈ 14.5 weeks calendar** (Phase 9 added) (P3/P4 overlap saves ~1 week vs serial;
the joint encoder-vs-interpreter week is the schedule pinch point).

## Standing decision points for Daniel

| When | Decision |
|---|---|
| Phase 0 | case sensitivity; `~=`; index base; OPEN-1/2 acceptance (project decision); fix 032's vacuous cut upstream? |
| Phase 5 | JSON schema review (downstream consumers; TACO-style matrix needs) |
| Phase 7 | parity sign-off — every classified difference |
| Phase 8 | same-events assumption wording; collaborator review of the 032×033 matrix |
| Phase 9 | histogram output formats to standardize with collaborators (.C macro vs uproot vs both); weight-table semantics |

## Risks

| Risk | Mitigation |
|---|---|
| OPEN items undecided by collaborators | spec keeps the convention-neutral strategy (Dual / capped verdicts), exactly as the legacy tool ships today; revisit when the project decides |
| and/or precedence alters a real file | Phase-0 lint enumerates affected files (corpus pre-scan found none — all mixed chains are parenthesized); decision recorded before code |
| Underscore-split lexing surprises a file the pre-scan missed | corpus gate + the lexer's split-note diagnostic make every occurrence visible in the Phase-1 report |
| z3 crate linking friction in HEP environments | subprocess backend is a hard requirement with its own CI job |
| Scope creep toward implementing the full ADL language | the fragment is declared; `Unsupported` + diagnostic is the designed answer |
| Parity gate reveals semantic drift late | the differential harness exists from P3/P4; the gate is a formality if lower layers stayed green |
| Interpreter itself wrong (oracle drift) | the interpreter is the authoritative spec; [DECIDE] items are settled by collaborator review and property tests pin encoder-vs-interpreter agreement; interpreter/verifier disagreement is release-blocking in *both* directions |
