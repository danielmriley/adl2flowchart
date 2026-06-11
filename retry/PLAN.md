# ADL2 implementation plan

Phases are strictly gated: a phase is done when its exit criteria pass in
CI, not when its code exists. Estimates assume one focused developer +
agent assistance; they are calendar-honest, not optimistic.

## Phase 0 — Spec ratification (≈ 1 week)

Freeze SPEC_LANGUAGE v1.0.

- Throwaway grammar validation: a quick parser skeleton (or instrumented
  legacy parser) checks the EBNF against all 68 corpus files; divergence
  lint (mixed and/or chains, bare path tokens, fractional bin edges).
- Resolve OPEN-1…OPEN-5 against CutLang (probe ADL files + small event
  samples; read CutLang source where probing is ambiguous). Each answer
  lands in the spec with a citation.
- Decide with collaborators: case sensitivity, `~=`, index base.
- **Exit:** spec marked v1.0; corpus parse report attached; zero
  unresolved [VERIFY] markers (or explicitly deferred with a Dual/
  diagnostic strategy named per item).

## Phase 1 — adl-syntax (≈ 1.5 weeks)

Workspace bootstrap, CI day one (clippy, fmt, tests). Lexer, parser, AST,
spans, diagnostics with recovery, `--dump-ast`, snapshots, fuzz target.

- **Exit:** corpus gate green; snapshot suite reviewed; fuzzer clean for
  1 CPU-hour; error-message review on 10 deliberately broken files.

## Phase 2 — adl-sema (≈ 2 weeks)

Symbol resolution, CollectionId/QuantityId interning, define resolution
(cycle errors), fragment tagging, HIR. Pure-alias unification as a
resolution fact. ext_objs/ext_lib/property_vars ingested into typed
declarations (no runtime file scans per lookup).

- **Exit:** HIR snapshots for goldens; quantity-table dumps reviewed for
  three real CMS files (032, 033, SUS-21-006); identity unit tests
  (rename≡, filtered≢parent, index distinctness, oriented angulars).

## Phase 3 — adl-interp (≈ 1.5 weeks)

Event model (JSONL records), evaluator for the fragment, bin assignment,
`smash2 run`. Toy event generator in adl-difftest.

- **Exit:** semantics unit battery green (every SPEC §4 clause has a
  test); if CutLang available: differential run on probe suite with zero
  unexplained disagreements.

## Phase 4 — adl-formula + encoder (≈ 2 weeks)

Formula IR, NNF negation, projections with type-enforced polarity,
LinAtom construction (finite-only), encoder HIR→Formula per SPEC_ANALYSIS
§1, including ratio branches and the OPEN-1 strategy.

- **Exit:** encoder property tests vs interpreter (sampling) green at
  10⁵ cases; metamorphic battery green; coverage notes (Unknown reasons)
  human-reviewed on the corpus.

## Phase 5 — adl-axioms, adl-solver, adl-analysis (≈ 2.5 weeks)

Axiom catalog with per-axiom tests; z3-native backend; subprocess
backend; conformance battery; pairwise engine (heuristic fast path,
batched incremental checks, vacuous regions, subset, bins); witnesses,
unsat-core explanations, witness re-validation through the interpreter;
human report + versioned JSON.

- **Exit:** all ported goldens green (dual-encoding suite + audit suite);
  explanation output reviewed on 033; determinism check; no-solver
  degradation test (verdicts cap at POSSIBLY).

## Phase 6 — adl-viz + CLI polish (≈ 1 week)

DOT flowchart/AST from HIR; `smash2 check|verify|run|dot` subcommands;
`--json`; quiet/verbose modes (machine output clean by default — the
legacy stdout soup is a non-goal to reproduce).

- **Exit:** DOT renders for corpus; CLI snapshot tests.

## Phase 7 — Parity gate & switchover (≈ 1.5 weeks)

Verdict-matrix differential vs legacy `smash -r` across the corpus and
golden suites. Classify every difference: ADL2-better (cite spec/audit),
legacy-better (fix ADL2), or spec change (document). Performance check:
≤ 2× legacy wall-clock on 033 (expect faster via native incremental
solving).

- **Exit:** signed-off parity report in `retry/PARITY.md`; legacy
  pipeline marked deprecated in README; `make test` switched to ADL2
  with the legacy run kept as a nightly comparison for one release.

## Phase 8 — Cross-file foundation (≈ 2 weeks, then iterate)

AnalysisUnit scoping, CrossLink pass (Base unification, proven-equivalent
filtered collections, implication⇒subset facts, trigger unification),
`--cross a.adl b.adl`, identity report, M×N matrix export, banner for the
same-events assumption. Goldens: duplicated-file self-test (everything
overlapping + mutually subset), complementary-windows disjoint pair,
same-name-different-filter must-not-unify, different-name-same-filter
must-unify. Acceptance: 032 × 033 matrix reviewed with collaborators.

**Total ≈ 13 weeks** of phased, individually-shippable work.

## Risks

| Risk | Mitigation |
|---|---|
| CutLang unavailable / ambiguous for OPEN items | spec marks the item convention-neutral (Dual / capped verdicts), exactly as legacy does today; revisit when pinned |
| and/or precedence change alters a real file's meaning | Phase-0 lint enumerates affected files; decision recorded before any code |
| z3 crate linking friction in HEP environments | subprocess backend is a hard requirement with its own CI job, not a stretch goal |
| Scope creep into CutLang re-implementation | the fragment is declared; `Unsupported` + diagnostic is the designed answer for everything else |
| Parity gate reveals semantic drift late | differential harness is built in Phase 3/4, not Phase 7 — the gate is a formality if the layers below stayed green |
