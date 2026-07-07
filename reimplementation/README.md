# ADL2 — from-scratch re-implementation of the ADL compiler/interpreter

This folder holds the specification and plan for a clean re-implementation
of the ADL toolchain ("ADL2", working binary name `smash2`). The
implementation now lives in [`adl2/`](adl2/) and is built and green through
the parity gate; the legacy `adl/` tool is retained as the reference
oracle. The documents here are the design contract the implementation was
written against.

> **To build and run `smash2`, and for the full feature reference, see
> [`adl2/README.md`](adl2/README.md).** This document explains *why* the
> re-implementation exists and the principles it holds to.

## Why a re-implementation

The legacy tool reached a sound, useful state (see
`../docs/DUAL_ENCODING_REPORT.md`), but every major defect we fixed there
traces to four root causes that were *architectural*, not incidental:

1. **No soundness contract.** The original analysis silently strengthened
   or weakened region formulas depending on which extraction path fired;
   false "PROVEN" verdicts followed. The dual-encoding contract had to be
   retrofitted by rewriting the whole extraction layer.
2. **String-keyed identity.** Event quantities were synthesized as strings
   and "canonicalized" — index loss, alias over-merging, case bugs, and
   the MET/MET.pt unification hack all came from this. The eventual goal
   (cross-file disjointness) is *precisely* an identity problem.
3. **Accreted grammar.** A LALR grammar with (originally) 87 conflicts,
   signed-literal lexing hacks, identifiers swallowing hyphens, a NOT
   token the lexer never produced, and no written language spec.
4. **No oracle.** There was no executable definition of what a region
   *means*, so every soundness bug had to be found by adversarial
   hand-crafting instead of mechanical comparison.

ADL2 inverts these: contract first, typed identity, specified grammar,
and a reference interpreter as the test oracle — all from commit one.

## Goals (same goals as the legacy tool, plus its destination)

- Parse ADL analysis files; produce diagnostics worth reading.
- **Interpret**: evaluate regions/objects over event records (the
  reference semantics, usable standalone and as the test oracle).
- **Verify**: prove pairwise region relations — disjoint, overlapping,
  subset — plus vacuous regions and bin-partition correctness, with
  sound verdicts and human-readable explanations.
- Visualize: DOT flowchart/AST output derived from the semantic IR.
- **Cross-file**: the quantity-identity model is designed so that
  cross-analysis overlap matrices are an extension, not a rewrite.

## Document map

| Doc | Contents |
|---|---|
| [SPEC_LANGUAGE.md](SPEC_LANGUAGE.md) | Lexical rules, EBNF grammar, semantics, the checked fragment, open semantic questions |
| [SPEC_ARCHITECTURE.md](SPEC_ARCHITECTURE.md) | Crate layout, pipeline, the Quantity model, polarity-typed formula IR, solver interface |
| [SPEC_ANALYSIS.md](SPEC_ANALYSIS.md) | Verdict definitions, encoding rules, axiom catalog, outputs, cross-file design |
| [TESTING.md](TESTING.md) | Oracle strategy, property-based/differential/metamorphic testing, CI |
| [PLAN.md](PLAN.md) | Phases, exit criteria, parity gate, risks, estimates |
| [DECISIONS.md](DECISIONS.md) | ADRs: language, parser, identity, solver, fragment posture — each tied to the legacy bug that motivated it |

## Principles (non-negotiable)

- **Soundness direction is a type.** Code that proves disjointness can
  only consume over-approximations; overlap/subset proofs only
  under-approximations. The compiler enforces what review used to.
- **Identity must be proven, never assumed.** Two quantities unify only
  by construction (same definition) or by proof (solver-verified
  equivalence). Names are labels, not identities.
- **"I don't know" is a first-class value.** Anything outside the checked
  fragment is an explicit `Unknown` with a reason the user sees; it can
  weaken a verdict to POSSIBLY, never flip it.
- **Axioms are a catalog, not code.** Every background fact asserted to
  the solver lives in one audited table with its justification and its
  assumptions, and each has a test.
- **The interpreter is the meaning.** If the verifier and the interpreter
  ever disagree on a satisfying event, that is a release-blocking bug.
