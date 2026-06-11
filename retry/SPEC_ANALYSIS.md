# ADL2 analysis specification (verifier semantics)

Status: DRAFT v0.1. This fixes *what the verifier claims and why the
claims are sound*. It is the successor of the legacy dual-encoding design
(`../docs/DUAL_ENCODING_REPORT.md`) with the June-2026 audit fixes folded
in as requirements, not patches.

## 1. Region encoding

Each region R compiles (adl-sema HIR → adl-formula) to an **exact**
formula over per-event quantities, where:

| HIR construct | Formula |
|---|---|
| `select c` | encode(c) |
| `reject c` | ¬encode(c) (NNF; exact) |
| region inheritance | inline the referenced region's formula (cycle ⇒ Unknown) |
| `trigger t` | atom `trig(t) = 1` |
| `bin …` | not part of membership (captured separately, §5) |
| comparison over linear arith | `LinAtom` (sums/diffs/const-mults; defines inlined; Int sizes coerced) |
| ratio `L/D ⋈ c`, D non-const | `(D>0 ∧ L ⋈ cD) ∨ (D<0 ∧ L ⋈̄ cD)`; D=0 fails the cut (per SPEC_LANGUAGE §4.4) |
| ternary `g ? a : b` | `(g∧a) ∨ (¬g∧b)` |
| `[]` / `][` bands | conjunction / disjunction of bounds |
| unindexed collection cut | OPEN-1-dependent: exact ∀ or ∃ once resolved; until then `Dual{plus,minus}` with the **empty-collection case in plus** (audit Bug 1) |
| anything Unsupported | `Unknown(diag)` |

Non-finite constants cannot construct atoms (audit Bug 5). Numeric
defines are HIR-inlined before encoding (audit Bug 2). Oriented angular
quantities never merge across argument order (audit Bug 3).

## 2. Verdicts and their soundness arguments

With R⁺ = over-projection (Unknown→⊤, Dual→plus) and R⁻ = under-
projection (Unknown→⊥, Dual→minus), and Ax = the axiom set (§4):

| Verdict | Definition | Sound because |
|---|---|---|
| **PROVEN DISJOINT** | UNSAT(Ax ∧ A⁺ ∧ B⁺) | A ⊆ A⁺, B ⊆ B⁺, Ax true of every event ⇒ no event in A∩B |
| **PROVEN OVERLAPPING** | SAT(Ax ∧ A⁻ ∧ B⁻), shared dimension, no convention-ambiguous twin pair (§4) | model satisfies real cuts of both regions, within the scalar event model |
| **PROVEN SUBSET A⊆B** | UNSAT(Ax ∧ A⁺ ∧ ¬(B⁻)) | A∧¬B ⊆ A⁺∧¬B⁻ |
| **REGION EMPTY** | UNSAT(Ax ∧ R⁺) | no physical event can satisfy a superset of R |
| **POSSIBLY OVERLAPPING** | everything weaker | not a claim |
| **UNKNOWN** | solver inconclusive | not a claim |

Stated model caveat (printed with every PROVEN OVERLAPPING): "a model
exists in the per-event scalar fragment" — opaque external-function
values and padded out-of-range element variables are free; the witness is
a candidate, not a simulated event. PROVEN DISJOINT carries no such
caveat (free variables only make UNSAT harder).

Pipeline per pair: cheap interval heuristic on the unconditional And-spine
of A⁺/B⁺ (sound fast path; also the no-solver fallback) → solver checks
batched in one incremental session → witness/core extraction for proven
verdicts.

## 3. Explanations (new requirement, not in legacy)

Every solver-proven verdict must answer "why": UNSAT verdicts report the
unsat core mapped back to source spans — e.g. "disjoint because
`region A line 12: select HT [] 200 450` cannot hold together with
`region B line 9: select HT >= 500`". The core names the minimal
conflicting cut set, so incidental cuts never appear in the explanation.
SAT verdicts report the witness with quantities in source notation and
axiom-derived values marked. Cores/witnesses are part of the JSON schema.

## 4. Axiom catalog (normative list at bootstrap)

| Axiom | Statement | Assumption tag |
|---|---|---|
| ORD | `pt(C[i]) ≥ pt(C[j])` for i<j, same C | collections pT-ordered |
| SZ0 | `size(C) ≥ 0` | — |
| SUB | `size(F) ≤ size(P)` for *single-source* filtered F of P | take = filter |
| UNI | `size(U) ≥ size(part)` each, `≤ Σ parts` | union = concat/dedup |
| NNEG | `pt, m, e, ht, dR, abs(·) ≥ 0` | — |
| DPHI | `−π ≤ Δφ ≤ π` | both sign conventions (OPEN-2) |
| TAG | exact-name `btag/ctag/tautag`, `trig(·)` ∈ {0,1} | tags boolean; discriminants excluded by exact-name rule |
| TWIN | oriented twins: `x = y ∨ x = −y` | either convention (OPEN-2) |
| EPRED | elements of filtered F satisfy F's predicate: `size(F)>i ⇒ predF(F[i])` | take = filter (new vs legacy: element-fact propagation) |
| IDOM | `pt(F[i]) ≤ pt(P[i])` for filtered F⊆P | ORD + SUB (new vs legacy) |

Prohibited-by-history: "referencing C[i] implies size(C)>i" (false under
guards — removed in legacy after a false empty-region proof). Pairs whose
combined quantities contain an oriented twin pair cap at POSSIBLY for the
SAT direction until OPEN-2 is resolved.

## 5. Bin partition checks

Per region, per bin set: pairwise `UNSAT(Ax ∧ R⁺ ∧ Bᵢ⁺ ∧ Bⱼ⁺)` ⇒ bins
disjoint; `UNSAT(Ax ∧ R⁺ ∧ ⋀ᵢ ¬Bᵢ⁻)` ⇒ bins cover the region; SAT
coverage check reports the gap witness. Boundary bins use real-valued
edges (SPEC_LANGUAGE divergence 5).

## 6. Outputs

- Human report: per-region coverage (`encoded leaves n/m`, `exact`,
  dropped reasons with spans), region-empty warnings, bin checks,
  pairwise verdict lines with explanations, summary counts.
- JSON (versioned schema): regions[], pairwise[] (kind, reason, witness,
  core, subset flags, exact, shared_dimensions), bin_checks[], axioms
  used, fragment diagnostics. Stable ordering.
- Exit code reflects parse/sema errors only; verdicts never fail the run
  by default. `--fail-on=overlap|gap|empty|non-exact` lets CI pipelines
  gate on physics findings explicitly.

## 7. Cross-file (forward design — Phase 8)

`AnalysisUnit` per file; quantities scoped per unit. A `CrossLink` pass,
under an explicit `--assume-same-events` banner, may add only:
(1) Base↔Base unification, (2) Filtered↔Filtered unification when element
predicates are solver-proven equivalent, (3) subset facts from proven
implication, (4) trigger-name unification. Everything else stays
namespaced ("private dimensions"). The pairwise machinery then runs
unchanged over the union — by construction, because verdict functions
take quantities, not names. Outputs add the identity report (what
unified, what got subset facts, what stayed private) and an M×N matrix
export (CSV/JSON) for combination workflows.
