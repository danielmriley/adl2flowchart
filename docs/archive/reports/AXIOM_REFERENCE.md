# The axiom catalog — reference definitions

Every background fact smash2 may assert to the solver lives in one audited
table (`crates/adl-axioms/src/lib.rs`). Each family below is quoted from
that catalog: the **statement** (the fact schema, instantiated per
quantity), the **justification** (why it is true of every physical event),
and the **assumption tag** (the modeling reading it rests on — printed in
every report that uses the family). Every family also carries a test that
its instances hold on generated events (`axioms_hold.rs`), and a
prohibited-axiom list records plausible-looking facts that are FALSE and
must never be added.

Notation: `C` a collection, `F` a filtered collection of parent `P`,
`C[i]` the i-th element (0-based, pT-descending), `size(C)` its count.

---

## Ordering & element facts

**ORD — pT ordering.**
`pt(C[i]) ≥ pt(C[j])` for `i < j` (front-front, unconditional); the
back-index families (back-back, and front-to-back with `k = 1` or
`i = 0`) are guarded by `size(C)` so they go vacuous when the deep
element is absent.
*Justification:* detector collections are delivered pT-descending and
filtering preserves order.
*Assumes:* collections pT-ordered. (The ingest layer refuses non-descending
input, so the assumption is enforced at the door.)

**EPRED — element-predicate propagation.**
`size(F) > i ⟹ pred_F(F[i])` (for the exactly-encodable conjuncts of F's
filter).
*Justification:* every element of a filtered collection passed the filter;
the size guard keeps the fact vacuous for absent elements — a guarded
reference never implies existence (this guard is the CE-1 fix class).
*Assumes:* take = filter.

**IDOM — filtered pT dominance.**
`pt(F[i]) ≤ pt(P[i])`.
*Justification:* `F[i]` equals some `P[j]` with `j ≥ i` and P is
pT-descending; satisfiable for absent elements under the canonical
pad-with-0 extension.
*Assumes:* ORD + SUB (it is the composition of the two readings).

## Cardinality facts

**SZ0 — size non-negativity.** `size(C) ≥ 0`.
*Justification:* a collection is a finite list. *Assumes:* none.

**SUB — filtered subset size.**
`size(F) ≤ size(P)` for single-source filtered F of P.
*Justification:* an object block keeps a subset of its single take source.
NEVER emitted for unions (a past audit bug class).
*Assumes:* take = filter.

**UNI — union size bounds.**
`size(U) ≥ size(part)` for each part; `size(U) ≤ Σ parts`.
*Justification:* true under BOTH the concatenation and deduplication
readings of union — deliberately weak enough to need no choice.
*Assumes:* union = concat/dedup.

**SZSLICE — slice size bounds.**
`0 ≤ size(C[a:b]) ≤ size(C)`, and `≤ b − a` for a concrete end.
*Justification:* a half-open contiguous slice is a sub-list.
*Assumes:* slice = clamped half-open sub-range.

**SZPERM — sort is a permutation.**
`size(sort(C, key, dir)) = size(C)`.
*Justification:* a sort is a bijection on the list; cardinality is
preserved regardless of the (event-dependent) key. NO per-index ordering
fact rides on this — ORD/IDOM stay off for a non-pT/ascending sort.
*Assumes:* sort = permutation.

**COMBSIZE — composite tuple combinatorics.**
`size(K→axis) = size(K)`; same-source disjoint pair over C:
`size(C) < 2 ⟹ size(K) = 0` and `size(K) ≥ 0`; cartesian/cross-source:
any part empty ⟹ `size(K) = 0`, all parts non-empty ⟹ `size(K) ≥ 1`.
*Justification:* tuple combinatorics; note the positive lower bound for
the same-source case is DELIBERATELY OMITTED — distinctness is by
kinematic value, so two value-equal elements may form zero pairs.
*Assumes:* comb = tuple enumeration; disjoint distinctness by value.

## Range facts (physics of the quantities)

**NNEG — magnitudes are non-negative.**
`pt, m, e`, HT-family scalars, `MET.pt`, `dR ≥ 0`; also opaque external
calls named EXACTLY pt/m/mass/e/energy/dr/sqrt (case-insensitive).
*Justification:* magnitudes by definition — m and E of any summed
four-vector by the timelike/lightlike condition, dR a metric distance,
sqrt the non-negative root. The exact-name rule keeps unrelated opaque
functions (bdt, aplanarity, …) free, and eta/phi-of-sum get NO sign axiom.
*Assumes:* none.

**DPHI — azimuthal wrap.**
`−π ≤ Δφ ≤ π`, the bound widened by one ulp for soundness.
*Justification:* azimuthal differences are wrapped into one period under
EITHER sign convention.
*Assumes:* both sign conventions (OPEN-2 — the community decision pending).

**TWIN — oriented reversals.**
For reversed-argument dphi/deta pairs: `x = y` or `x = −y`.
*Justification:* reversing the arguments either preserves or negates the
separation, whichever convention holds.
*Assumes:* either convention (OPEN-2).

**TAG — boolean tags.**
Exact-name `btag/ctag/tautag` element properties and `trig(...)` are in
`{0, 1}`.
*Justification:* tags and trigger flags are booleans; the exact-name rule
keeps continuous discriminants (`btagDeepB`, …) OUT (a past audit bug).
*Assumes:* tags boolean; discriminants excluded by exact-name rule.

**TRIG — circular-function bounds.**
`−1 ≤ cos(x) ≤ 1`, `−1 ≤ sin(x) ≤ 1` for opaque cos/sin calls.
*Justification:* bounded for every real argument, regardless of the
(opaque) argument. NOT applied to tan/asin/… and never constant-folded
(an irrational cosine is not an exact rational).
*Assumes:* none.

## Cross-file facts (derived, not assumed)

These two are not schema instantiations — each instance is emitted only
after a solver PROOF, and each also replays through the independent
certifier like any other disjointness fact.

**XSUB — proven cross-collection refinement.**
`size(A) ≤ size(B)` when A and B filter the SAME base collection and A's
element predicate PROVABLY implies B's (proven on the subset side over a
shared generic element).
*Justification:* emitted ONLY when the solver reports UNSAT for
(pred_A-over ∧ ¬pred_B-under) over one shared base element: the weakest
reading of A's cut already forces the strongest reading of B's, so in any
event every element A keeps, B keeps too. An opaque conjunct in B
under-approximates to false (never dropped) — it can only SUPPRESS the
fact, never fabricate it; composite/reduce binders abort the pair.
*Assumes:* same base name = same base input (the documented cross-file
residual — the implication is proven; the shared-input premise is the
convention the provenance roadmap replaces).

**XEQ — proven cross-collection equality.**
`size(A) = size(B)` when each element predicate implies the other (both
XSUB directions proven).
*Justification:* both directions are the XSUB proof run each way; each is
individually sound, so `size(A) ≤ size(B) ≤ size(A)`.
*Assumes:* same base name = same base input.

---

## How to read `== axioms used ==` in a report

- `FAMILY×n` = n concrete instances of that schema were asserted for the
  quantities in your files — the complete list of background facts the
  proofs were ALLOWED to use; nothing else was asserted.
- The `assuming:` line is the union of assumption tags across the families
  used: the modeling readings your verdicts may rest on. Untagged families
  are unconditional.
- Under `--explain`, each family is listed with its instance count and
  assumption individually; the JSON report carries the same.
