# ADL2 architecture specification

Status: DRAFT v0.1. Language: **Rust** (2024 edition) — see DECISIONS ADR-001;
the architecture is language-portable, but type-enforced soundness polarity
and ownership safety are first-class requirements that Rust meets natively.

## 1. Workspace layout

```
retry/                      (this spec)
adl2/                       (cargo workspace — created in Phase 1)
├── crates/
│   ├── adl-syntax      lexer, recursive-descent parser, AST, spans, snapshots
│   ├── adl-sema        name resolution, Quantity model, fragment check, HIR
│   ├── adl-interp      reference interpreter: Event in → bool/values out
│   ├── adl-formula     polarity-aware formula IR + projections (no solver dep)
│   ├── adl-axioms      the axiom catalog (data + per-axiom tests)
│   ├── adl-solver      SolverBackend trait; z3-native + smtlib-subprocess impls
│   ├── adl-analysis    pairwise verdicts, subset, vacuous, bins; reports/JSON
│   ├── adl-viz         DOT output (consumes HIR, not raw AST)
│   ├── adl-difftest    generators, sampling oracle, CutLang/legacy harnesses
│   └── adl-cli         the `smash2` binary
```

Dependency rule: arrows point left-to-right only; `adl-interp` and
`adl-analysis` share `adl-sema`'s HIR and **must not** have private
re-interpretations of semantics. `adl-viz` reads HIR so the flowchart can
never disagree with what was verified.

## 2. Pipeline

```
source ──lex/parse──▶ AST(spans) ──resolve──▶ HIR + QuantityTable
                                      │
        ┌─────────────────────────────┼──────────────────────────┐
        ▼                             ▼                          ▼
   adl-interp                    adl-analysis                 adl-viz
 (event evaluator)        (encode→Formula, project ±,        (DOT out)
        ▲                  axioms, solver, verdicts)
        └────────── differential testing ─────────┘
```

## 3. adl-syntax

- Hand-written lexer (logos optional) and recursive-descent parser, one
  function per EBNF nonterminal, Pratt-style expression parsing with the
  precedence table from SPEC_LANGUAGE §3.
- Every node carries a `Span`; diagnostics via `miette`-style reports
  (span + label + help), multiple errors per run, statement-level
  resynchronization.
- AST is plain owned Rust enums (`Box`ed children) — no clone()-on-
  construct, no manual ownership: the entire legacy UB class
  (REVIEW_NOTES C10) is unrepresentable.
- `--dump-ast` emits a canonical, stable text form; snapshot tests (insta)
  from day one. Parser fuzzing (cargo-fuzz) target in-tree.

## 4. adl-sema: the Quantity model (the core idea)

The legacy engine synthesized string keys and "canonicalized" them; every
identity bug (index loss, alias over-merge, MET/MET.pt, case folding,
dphi arg order) lived there. ADL2 makes event quantities **typed,
interned values** whose identity is structural:

```rust
pub struct QuantityId(u32);            // interned
pub enum Quantity {
    EventScalar(ScalarSource),         // MET.pt, scalarHT, user define (numeric)
    Size(CollectionId),
    ElemProp { coll: CollectionId, index: ElemIndex, prop: PropId },
    //        ElemIndex = FromFront(u32) | FromBack(u32)  — FromBack
    //        gated on OPEN-3; until resolved the parser diagnoses [-n]
    AngularSep { kind: AngKind,        // DPhi | DEta | DR
                 a: ParticleRef, b: ParticleRef,
                 oriented: bool },     // DR unoriented; DPhi/DEta oriented
    ExternalFn { name: Symbol, args: Vec<QuantityArg> }, // opaque but interned
}

pub enum Collection {
    Base(Symbol),                                   // detector-level: Jet, Muon…
    Filtered { parent: CollectionId, pred: ElemPredId }, // object block w/ cuts
    Union(Vec<CollectionId>),
    Combination { … },
}
```

Consequences, by construction:

- A **pure rename** (`object MHT take MissingET`, no cuts) resolves to the
  *same* `CollectionId` — unification is a fact of resolution, not a
  string table (legacy `object_aliases.txt` becomes a small base-name
  spelling map only).
- A **filtered** collection is a *different* `CollectionId` than its
  parent forever; relations between them (size monotonicity, per-index
  pt domination, element-predicate inheritance) are derived *facts*
  attached to the pair, produced by the axiom layer or by solver proof —
  never by name merging. (Legacy bugs 1g, audit Bug 4.)
- `Quantity` identity is exact: `jets[0].btag` vs `jets[1].btag` cannot
  alias (legacy 1e); oriented angular pairs cannot silently merge
  (audit Bug 3).
- Numeric defines resolve to their **body expression** (HIR), so the
  analyzer inlines by construction (audit Bug 2 unrepresentable);
  cyclical defines are a resolution error.
- **Cross-file ready**: `QuantityId`s are scoped per `AnalysisUnit`;
  cross-unit identity is a separate explicit pass that may only (a) unify
  `Base` collections/event scalars under the same-events assumption,
  (b) unify `Filtered` collections whose element predicates are proven
  equivalent, (c) emit subset relations for proven implication. Exactly
  the PLAN_GRAMMAR_AND_CROSS_FILE.md X1 design, but as typed operations.

`adl-sema` also performs the **fragment check**: each HIR node is tagged
`InFragment` or `Unsupported(reason)`. Both the interpreter and the
verifier key off the same tag — one diagnosis, two consumers.

## 5. adl-formula: polarity in the type system

```rust
pub enum Formula {            // exact; may contain Unknown/Dual
    True, False,
    Atom(LinAtom),            // Σ cᵢ·Quantityᵢ ⋈ k   (⋈ ∈ <,≤,>,≥,=,≠)
    And(Vec<Formula>), Or(Vec<Formula>),
    Unknown(DiagId),          // explicit ignorance, with its diagnostic
    Dual { plus: Box<Formula>, minus: Box<Formula>, why: DiagId },
}

pub struct Over(QFormula);    // Unknown→true,  Dual→plus   (R⁺ ⊇ R)
pub struct Under(QFormula);   // Unknown→false, Dual→minus  (R⁻ ⊆ R)
// QFormula: Unknown/Dual-free by type (separate enum), directly emittable.

impl Formula {
    pub fn not(self) -> Formula;        // NNF; Dual swaps branches
    pub fn over(&self) -> Over;
    pub fn under(&self) -> Under;
}

// The ONLY constructors of proven verdicts:
pub fn prove_disjoint(a: &Over,  b: &Over,  ax: &AxiomSet, s: &mut dyn Solver) -> Tri;
pub fn prove_overlap (a: &Under, b: &Under, ax: &AxiomSet, s: &mut dyn Solver) -> Tri;
pub fn prove_subset  (a: &Over,  b: &Under, ax: &AxiomSet, s: &mut dyn Solver) -> Tri; // a ∧ ¬b
```

A contributor *cannot* feed an under-approximation to a disjointness
proof; the signature rejects it. This is the legacy dual-encoding
contract promoted from convention to type. Non-finite constants are
rejected at `LinAtom` construction (audit Bug 5 layer 1).

## 6. adl-axioms

One table; each entry = emitter + justification string + assumption tag +
unit test. Initial catalog (carried from legacy, with audit fixes):
pt-ordering of indexed elements; `size ≥ 0`; `size(filtered) ≤
size(parent)` (single-source only); `size(union) ≥ each part` and `≤ sum`;
nonneg `pt/m/e/ht`, `dR`, `abs(·)`; `|Δφ| ≤ π` (until OPEN-2 resolves);
exact-name tags/triggers ∈ {0,1}; oriented-twin disjunction `x=y ∨ x=−y`
(until OPEN-2). Adding an axiom requires: justification ("true of every
physical event because …"), the assumption it rides on, and a test that
would have caught the legacy "C[i] ⇒ size>i" mistake (guarded references
do not imply existence).

## 7. adl-solver

```rust
pub trait Solver {
    fn push(&mut self); fn pop(&mut self);
    fn assert(&mut self, f: &QFormula, name: Option<AssertName>);
    fn check(&mut self, timeout: Duration) -> SatResult;   // Sat/Unsat/Unknown — no text parsing
    fn model(&self) -> Option<Model>;
    fn unsat_core(&self) -> Option<Vec<AssertName>>;       // explanations
}
```

Primary impl: **z3 crate (native bindings)** — typed terms, incremental
push/pop, models and cores without string protocols (kills the legacy
echo-tag parsing and the dropped-assert hazard, audit Bug 5 layer 2).
Secondary impl: SMT-LIB2 subprocess (z3/cvc5 on PATH) for environments
where linking is impractical; it must pass the same conformance test
suite as the native backend. No solver ⇒ heuristic interval layer only,
verdicts capped at POSSIBLY (same conservative degradation as legacy).

## 8. adl-interp

Evaluates HIR over an `Event` (JSON event records; generator in
adl-difftest). Semantics are exactly SPEC_LANGUAGE §4 — this crate is the
executable spec. Used as: (1) a user tool (`smash2 run file.adl
events.jsonl` → per-region pass/fail and bin assignment), (2) the oracle
for property-based verification testing, (3) the CutLang differential
anchor.

## 9. Determinism & reporting

All iteration orders deterministic (BTreeMap/sorted interning); two runs
produce byte-identical reports. Outputs: human report, versioned JSON
(`schema_version` field from day one), DOT. Every PROVEN verdict carries
either a witness (models) or an unsat-core explanation ("disjoint
because: HT bins [200,500) vs [500,1000)").
