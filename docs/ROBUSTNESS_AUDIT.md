# Robustness audit of the single-file disjointness checks

June 2026. Pre-cross-file hardening pass: an independent adversarial audit
(code inspection + ~20 crafted attack files) against the dual-encoding
engine, plus hand-verified probes. Companion to
`docs/DUAL_ENCODING_REPORT.md` (architecture) — this records what the audit
found, what was fixed, and what is accepted as a documented limitation.

## Found and fixed (commit 13d2239; each locked by a golden)

| # | Hole | Wrong verdict it produced | Fix |
|---|------|---------------------------|-----|
| 1 | Bounded quantifier R⁺ omitted the empty-collection case (∀ over zero elements passes vacuously) | false PROVEN DISJOINT vs a `size==0` region; mirrored false overlap via `reject` | R⁺ gains a `size == 0` disjunct |
| 2 | Defines in arithmetic comparisons became opaque free scalars, dropping the link to their body | false PROVEN OVERLAPPING with a witness violating `halfmet = MET/2`, labeled "exact" | `linearize()` inlines define bodies; un-linearizable defines → Unknown |
| 3 | `dphi(x,y)` key-merged with `dphi(y,x)` though they are negatives under the signed convention | false PROVEN DISJOINT (and false overlap for dEta) | order-sensitive keys (only symmetric dR sorts); convention-neutral twin axiom `x=y ∨ x=−y`; overlap verdicts with reversed twins downgrade to POSSIBLY |
| 4 | `size(child) ≤ size(parent)` asserted per parent of a `union(...)` take — false whenever both species present | realizable regions "provably empty" + false DISJOINT | subset axiom only for single-source takes |
| 5 | `100/0` → `inf` reached SMT as an invalid literal; z3 dropped the assert and the weakened R⁻ still answered SAT | false PROVEN OVERLAPPING with witness violating the cut | non-finite constants rejected at `numericValueOf` (→ Unknown); batch parser invalidates blocks containing z3 `(error)` lines |
| 6 | `{0,1}` tag axiom matched "btag" as a substring, hitting continuous discriminants (`btagDeepB`) | overlapping regions "provably empty" + false DISJOINT | exact-name match (btag/ctag/tautag) only |

Pattern worth noting for cross-file work: 4 of 6 holes (1, 3, 4, 6) were in
the *assumption layers* — quantifier expansion, key identity, background
axioms — not in the core dual-encoding logic, which the audit verified
clean (fNot/project/Dual polarity, subset formula, ratio branch split,
Int rounding directions, batch protocol, heuristic restricted to the
And-spine all confirmed correct). Identity and axioms are exactly the
layers cross-file analysis multiplies.

## Verified robust (attacks that failed)

Sign-indefinite ratio denominators, mixed Int/Real linear atoms,
fractional size comparisons, inheritance combined with reject, reject of
ITE, defines under boolean negation, pure-alias chains over filtered
objects, `[]`/`][` desugaring, boundary-inclusivity at touching intervals,
Dual + explicit-index interplay, reject of OR with an opaque branch,
no-z3 degradation (always POSSIBLY, never a false PROVEN), JSON validity,
echo-tag protocol injection and timeout handling.

## Accepted limitations (documented, not bugs)

- **Scalar event model**: PROVEN OVERLAPPING means a witness exists over
  the modeled per-event scalars; opaque function values (aplanarity, MVA
  scores) in the witness are not checked for realizability against their
  unmodeled inputs. In-model dependencies (linear, defines) ARE enforced.
- **Ratio at D == 0** is treated as failing the cut; if the runtime uses
  IEEE ±inf semantics instead, a D==0 event could behave differently.
- **pT-ordering axiom** constrains padding variables for elements beyond
  an event's actual collection size (consistent assignments always exist;
  affects witness realizability only).
- Object names compare case-insensitively (ADL practice); bin boundary
  lists truncate to integers (parser, plan item G1).

## Verification inventory

- 37 golden checks (`scripts/run_golden_tests.sh`), including 11
  soundness-regression cases from the two audits, each encoding a
  ground-truth verdict computed by hand.
- 68-file corpus parse+analysis sweep; Delphes-033 spike; `make test`
  runs all three.
- Planned next (docs/PLAN_GRAMMAR_AND_CROSS_FILE.md, D4): property-based
  random-region testing against a sampling oracle, in CI — the systematic
  version of this audit.
