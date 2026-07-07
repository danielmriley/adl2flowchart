# Soundness review — the disjointness/overlap validation system

**Date:** 2026-07-01 · **Scope:** the full verdict path that produces PROVEN
DISJOINT / PROVEN OVERLAPPING / PROVEN SUBSET / REGION EMPTY in the ADL2
analyzer (`reimplementation/adl2`), audited at commit `774cbd4`.
**Method:** seven parallel audit lenses (polarity/projection, interval fast
path, solver layer, witness validation, axiom catalog, engine control flow +
interpreter seam, quantity identity), each with live repro attempts against
the built `smash2`; every serious claim was then **independently re-reproduced
by a second run** before entering this document. Repro files referenced below
live outside the repo (session scratchpad) — each is small enough to recreate
from the inline ADL.

---

## 1. Executive summary

The **architecture** of the validation system is sound and repeatedly proved
it under attack: the `Over`/`Under` polarity split is enforced by types and
verified monotone at every constructor; the interval fast path is an exact-
rational proof layer (zero defects found across an adversarial boundary
battery); the solver layer cannot desync (stateless per-check replay), maps
every abnormal output to Unknown, and has fully balanced frames; witness
validation could not be made to produce a `witness_validated = true` that is
not genuine two-region membership of a loader-accepted event; and the engine's
verdict constructors all carry their full precondition chains.

The **failures are all at the seams**, and they cluster into two root causes:

- **RC-A — the canonical-render identity escape hatch.** Quantity/predicate
  identity falls back from structural keys to rendered text. Two renders
  discard exactly the information that made things different: `<unsupported:
  reason>` drops the differing substructure, and context-relative leaves
  (`this.pt`, binder-as-scalar) drop the owning collection. Result: two
  *physically different* values share one solver variable, and contradictory
  cuts on them prove UNSAT. **Two critical, reproduced, diagnostic-free false
  PROVEN DISJOINT paths — both triggered by the corpus-standard lepton-cleaning
  / overlap-removal idiom** (`reject dR(j, leptons) < 0.4`).
- **RC-B — absent-element extension conflicts in the axiom catalog.** The
  catalog's implicit contract is that every axiom stays satisfiable when
  absent elements are padded with 0. The front-to-back ORD family (`k==1,
  i>=1`) breaches it and, jointly with IDOM, renders the base frame
  unsatisfiable on ordinary events: **a reproduced PROVEN DISJOINT for a pair
  the tool's own interpreter passes both regions on.**

Plus three narrower encoder-boundary defects (a false PROVEN via region-level
`sort` × ORD, a false PROVEN SUBSET via a skipped existence guard on mixed-
exactness `min()`, and a stack-overflow crash), one fail-open token parse, and
a set of low-severity honesty/hardening items.

**Interim caution until the RC-A fixes land:** PROVEN DISJOINT verdicts on
files whose object blocks contain out-of-fragment cuts (reducers, binder-arg
externals such as `dR(j, X)`) or function-wrapped element properties
(`sqrt(pt)` inside a cut) should be treated as unverified.

---

## 2. Verified findings

Severity legend: **critical** = reproduced false PROVEN on corpus-realistic
input with no diagnostic; **high** = reproduced false PROVEN on legal but
rarer input; **medium** = crash, verdict-vs-intent divergence, or metamorphic
instability; **low/info** = honesty, display, hardening.

### RC-A: render-keyed identity escape hatch

#### S1 — CRITICAL: `<unsupported: reason>` render masks differing substructure → two different collections intern as one → false PROVEN DISJOINT
`adl-sema/src/dump.rs:161-165` (render), `adl-sema/src/resolve.rs:897-906`
(`intern_elem_pred` keys on render), `resolve.rs:~2233` (`opaque_arg`).

Element predicates intern by canonical render text; `HKind::Unsupported`
renders as `<unsupported: {reason}>`, discarding the differing sub-expression.
Two objects over the same parent whose cuts collapse to the same reason string
share one `ElemPredId` → one `CollectionId` → one `Size` variable.

Reproduced (twice): `cleanjetsA` (`reject any(dR(this, eles) < 0.2 …)`) and
`cleanjetsB` (`reject any(dR(this, muons) < 0.4 …)`) — both reducer bodies
collapse to the same reason — then `size(cleanjetsA) >= 4` vs
`size(cleanjetsB) <= 1` → `PROVEN DISJOINT — size(cleanjetsA): [4, inf] vs
[-inf, 1]` (the report even prints A's name for B's cut). A 5-jet event near
muons and far from electrons passes both. Zero diagnostics; regions reported
"exact yes". Same root reproduced through `QuantityArg::Opaque` merging two
different `sqrt(min(dR(…)))` quantities.

#### S2 — CRITICAL: context-relative leaves leak into context-free identity; EPRED weaponizes the shared id → false PROVEN DISJOINT
`adl-sema/src/resolve.rs:1631-1640, ~2213-2250` (elem-self/binder → fixed
context-free strings), `adl-sema/src/dump.rs:108-109` (`this.{prop}` render),
amplified by `adl-axioms/src/lib.rs:866-903` (EPRED) → `lin_pred:1474-1477`
(accepts any in-fragment quantity leaf).

Inside an object block, an element-self or binder reference used as an
argument to a *declared* external (`dR(j, leptons)`) degenerates to a
context-free opaque key — the SAME string for every block — so every object's
`dR(<own element>, leptons)` interns to one `QuantityId`. EPRED then asserts
`size(A)>0 ⇒ q > c1` and `size(B)>0 ⇒ q < c2` over the one shared `q` that
physically denotes different per-element values.

Reproduced (three independent shapes): `sqrt(pt)` cuts on Jet vs Muo blocks →
PROVEN DISJOINT (EPRED×2); `dR(j, eles) > 0.4` (Jet binder) vs `dR(p, eles) <
0.1` (Pho binder) → PROVEN DISJOINT; and the exact corpus spelling
`reject dR(j, leptons) < 0.4` vs `select dR(k, leptons) < 0.3` → PROVEN
DISJOINT. This is the field-standard overlap-removal idiom — **real
cross-analysis runs are exposed today.** Note: reconciliation's
`references_binder_or_reduce` guard does not catch this shape either (the
collapsed arg is `Opaque`, not `Particle`), though reconcile is coincidentally
safe (same-base grounding).

**Proposed changes (S1+S2, layered fail-closed):**
1. *Sema, identity layer:* `intern_elem_pred` mints a fresh never-shared id
   when the node `has_unsupported()`; `opaque_arg`/`quantity_arg` refuse to
   build a context-free key from any node containing `ElemSelfProp`,
   `ReduceProp`, `ThisElem`, `ReduceElem`, or an elem-alias fallback — either
   qualify the key with the owning collection/predicate context or tag the
   enclosing call `Fragment::Unsupported`.
2. *Sema, propagation:* when any argument of an external call resolves to an
   Unsupported node, propagate `Fragment::Unsupported` onto the call node —
   the encoders then reject it at the existing tag checks, and EPRED's
   conjunct-dropping is its sound weakening.
3. *Axioms, second net:* `lin_pred` (and `encode_pred_exact`) reject
   `HKind::Quantity` leaves whose quantity is context-dependent — add an
   "element-dependent" bit to `QuantityTable` set at intern time — so the
   encoder fails closed even when sema misses a future leak path.
4. Fix the falsified doc invariant at `dump.rs:3-7` ("identical text always
   means identical resolution") to state the taint rule.

### RC-B: absent-element extension conflicts (axiom catalog)

#### S3 — HIGH: front-to-back ORD × IDOM jointly unsatisfiable on reachable events → false PROVEN DISJOINT (interpreter passes both regions)
`adl-axioms/src/lib.rs:660-683` (F2B ORD) × `:905-931` (IDOM); breaches the
module's pad-with-0 contract at `lib.rs:24-28`.

`pt(C[i]) >= pt(C[-k])` is emitted unconditionally for `i==0 || k==1`. For
`k==1, i>=1` on an event with `1 <= size(C) <= i`, the absent front element
`pt(C[i])` is a free variable that F2B-ORD **lower-bounds by a real value**
while IDOM **upper-bounds it by another real value** — no single extension
satisfies both, so the solver derives the ghost fact `pt(F[-1]) <= pt(P[i])`,
false whenever the filter drops `P[i]` but keeps a higher-pT earlier element.

Reproduced: `goodjets = Jet | eta<2`; A: `size>=2 ∧ pt(goodjets[-1])>=30`;
B: `pt(Jet[2])<=15`; a third region mentions `pt(goodjets[2])` (any region's
quantities poison every pair — the axiom set is the union). `verify` →
**PROVEN DISJOINT** citing exactly the two axioms; `run` on Jet pts
[100,40,10] with etas [0,0,3] → **A PASS, B PASS**. The tool contradicts
itself.

Latent sibling: back-back ORD (`lib.rs:640-659`) also breaches pad-0 for
`k1 <= size < k2`; no current partner axiom exploits it, but two of three
back-index ORD variants now violate the stated contract.

**Proposed change:** emit the `k==1, i>=1` family in guarded EPRED shape —
`Or(size(C) <= i, pt(C[i]) >= pt(C[-1]))` — and the same size guard for
back-back ORD; keep `i==0` unconditional (pad-0 consistent). Extend the
`axioms_hold` vocabulary with back-index selects so the existing pad-0
evaluator locks the fix (it would fail today's unconditional instances).
Re-verify `examples/golden/empty_10.adl` and `features-num_03.adl` pins.

### Encoder-boundary defects

#### S4 — HIGH: region-level `sort` encoded as a pure Unknown while ORD still binds the region's elements → false PROVEN EMPTY / PROVEN DISJOINT
`adl-sema/src/resolve.rs:1055` (sort → NonMembership + Unsupported),
`adl-formula/src/encode.rs:500-506` (Unknown leaf, over→True),
`adl-axioms/src/lib.rs:625-637` (ORD unaware of region-local sorts).

An `Unknown` hedge (over→True) is sound only for *membership predicates*; a
region-level `sort` is an environment mutation that re-binds what `jets[i]`
means for subsequent statements. The encoder keeps the canonical
(pT-descending) element quantities, ORD constrains them, and the region's
over-projection is no longer a superset of its real semantics.

Reproduced: `sort pt(jets) ascend` + `jets[0].pt < 30` + `jets[1].pt > 100` →
"EMPTY — provably selects no events" + trivially PROVEN DISJOINT, while under
ascending semantics a `[150, 25]` event re-sorts to `[25, 150]` and passes
both cuts. Corpus-rare (region sorts appear only commented out) — hence high,
not critical.

**Proposed change:** cascade the sort's Unsupported tag onto every subsequent
statement of the same region that references an element-indexed or ordered
quantity (mirroring the info-line strictness cascade), or demote those
statements' formulas to `Unknown` in `encode_unit`. The under side is already
safe.

#### S5 — HIGH: existence guards skipped for mixed-exactness `min()`/`max()` → unguarded element atom on the under side → false PROVEN SUBSET
`adl-formula/src/encode.rs:629-631` (`guard_existence` early-returns unless
`is_exact()`) × the ScalarMinMax expansion at `:1401-1432`; consumed by
`negated_under` + `subset` (engine).

`min(a, b) ⋈ c` expands to a disjunction whose existence guards are conjoined
only when the whole formula is exact. One opaque argument (e.g. a
value-position ternary) makes it non-exact → guards skipped → the under
contains a bare `pt(jets[0]) < 50` with no `size(jets) > 0`. `¬(B⁻)` is then
too small and, closed by IDOM, fabricates PROVEN SUBSET.

Reproduced: A: `Jet[0].pt < 50`; B: `min(jets[0].pt, MET > 100 ? MET : 7) <
50` → "subset: A within B". Counterexample: one Jet at pt 10 (in A; `jets`
empty → B's min-comparison false → not in B). The control with both args
exact correctly withholds the claim. The same too-big under feeds
`bin_coverage` (false "bins cover region").

**Proposed change:** in the ScalarMinMax arm, conjoin the existence guards
unconditionally (they are necessary conditions of the comparison — sound on
both projections), and conjoin each non-exact argument's `Unknown` beside the
disjunction (over unchanged; under honestly false).

#### S6 — MEDIUM: `subst` has no ScalarMinMax arm → infinite recursion → stack-overflow crash
`adl-formula/src/encode.rs:950-993` (falls to `other.clone()`), driven by the
OPEN-1 leaf path (`collect_collprops` *does* see min/max args).

Reproduced: `select min(jets.pt, MET) < 50` → `fatal runtime error: stack
overflow` (core dump). Denial of analysis, not a wrong verdict.

**Proposed change:** add `HKind::ScalarMinMax` arms to `subst`,
`subst_reduce`, and `subst_binders`, mapping over args.

### Language-fidelity / intent

#### S7 — MEDIUM: sort-direction token parse fails open into the alias gate
`adl-sema/src/resolve.rs:674-681, 699-704`. Any token other than literally
`ascend` — `ascending`, `asc`, typos — silently means Descend, and the
descend+pt alias gate then unifies the "sorted" collection with its
pT-descending source. Reproduced: `sort(jets, pt(jets), ascending)` → ORD
proves a DISJOINT that is false under the ascending intent. Internally
consistent (interpreter shares the alias) but a soundness-critical gate
should not fail open on an unrecognized token.
**Proposed:** accept exactly `ascend`/`descend`; anything else → diagnostic +
opaque `Sorted` (no alias, `pt_ordered = false`).

#### S8 — MEDIUM: duplicate `object`/`define`/`region` names are silently first-binding-wins
`adl-sema/src/resolve.rs:144-155, 1071-1074`. The code comment claims "a
duplicate is diagnosed when resolved" — no such diagnostic exists. Two
`object jets` blocks: every reference silently binds to the first; `check`
exits 0 with no output. Verdicts are internally consistent but may be about
the wrong object with zero signal; duplicate region names additionally make
the report ambiguous (two indistinguishable `SR vs ref` lines).
**Proposed:** error (or at minimum warning) on duplicate keys; disambiguate
report labels (`SR#2`) when a unit carries duplicates; fix the phantom comment.

### Completeness / stability (sound, but should be fixed)

- **S9 — realizer interning-order instability** (`witness.rs:611-698`): dR
  realized before a same-anchor dEta equality burns the phi budget in the
  wrong plane; the identical pair flips PROVEN OVERLAPPING ↔ POSSIBLY under
  region declaration order and re-opens INTERNAL-diagnostic spam. *Proposed:*
  two-pass realization — DEta/DPhi kinds first, DR last.
- **S10 — trigger rounding** (`witness.rs:472`): `v ≥ 0.5 → 1` starves
  `not <trigger>` witnesses (model picks 0.7 for `trig ≠ 1`). *Proposed:*
  wish `trig = 0` when a negated trigger atom is present.

### Honesty / display / hardening (low)

- **S11** bin-coverage "gap witness" is an unvalidated over-approximation
  model printed with witness vocabulary (two lenses independently flagged
  it). *Proposed:* rename to "gap hint (not validated)" or interpreter-check
  the point before display.
- **S12** default renderer name-substitution corrupts prose when regions are
  named `A`/`B` — "POSSIBLY" renders as "POSSI§ALY" (byte-verified).
  *Proposed:* identifier-boundary replacement or single-pass simultaneous
  substitution.
- **S13** `OVERLAP_CAVEAT` tail ("the witness is a candidate, not a simulated
  event") contradicts the adjacent "[witness validated by interpreter]" on
  PROVEN rows. *Proposed:* split the constant (base caveat + candidate-only
  tail).
- **S14** `spawn_failures` undercounts: the witness-retry re-check and
  `refined_model::try_with` call `s.check` directly, bypassing the counter
  (found independently by two lenses). *Proposed:* route through
  `Engine::check`.
- **S15** solver-layer hardening asserts (all currently by-construction
  invariants; convert to checked): `debug_assert` on pop-at-base-frame in the
  subprocess backend; `classify` returns Unknown when a script yields ≠ 1
  answer lines; panic on conflicting sort re-declaration in `declare`; extend
  `all_q` with `lookup_size` results so wish-frame size hints declare as Int.
- **S16** interval `human()` renders exact bounds through f64 (reason strings
  can misstate a boundary); print the exact rational.
- **S17** SPEC_ANALYSIS §2 should say explicitly that "loader-accepted" is a
  weaker event space than "physical": a validated witness may be physically
  impossible (`jets[0].pt = 501, ht = 0` — nothing couples ht-family scalars
  to jet pTs). Conservative direction, but worth one sentence; a future
  `ht >= Σ pt` axiom class would also strengthen the UNSAT side.
- **S18** composite binder identity `(coll, name)` is shared across distinct
  composite blocks — latent only (composites are P1-opaque today); make
  `Binder` carry the composite's identity before P2 encodes composites.
- **S19** `> MAX_REALIZED` size rejections file an INTERNAL diagnostic for
  expected behavior (no quiet-downgrade substring match).

---

## 3. Areas verified sound (and the invariant that holds them)

| Area | Verdict | Load-bearing invariant |
|---|---|---|
| `Formula` projection/negation | sound | monotone `project` at every constructor; NNF-at-call `not()` flips Dual as `plus' = ¬minus`; `Over`/`Under` unforgeable (private field) |
| Interval fast path | sound (0 findings) | exact `Rat` end-to-end; harvests only the unconditional And-spine of `Over`s; boundary logic verified on all four strict/closed touching combos |
| Solver layer | sound (0 soundness) | stateless per-check replay (desync structurally impossible); error/timeout/garbage → Unknown, never Unsat; all engine frames balanced incl. blocking clauses; verdicts never depend on cores or model precision |
| Witness validation | sound (0 soundness) | index-based region resolution; genuinely non-short-circuiting Kleene walk (decidable False beats preceding opaque); `Validated` carries the final artifact itself; caps precede every upgrade; could not construct a lying `witness_validated=true` |
| Engine control flow | sound | exactly four PROVEN constructor sites, each with its full precondition chain; every solver None/Unknown gate is a non-proof; canonical swap maps flags/indices back correctly |
| Interpreter seam | sound | D=0/NaN/missing-element/pT-order conventions agree encoder↔interpreter; loader validates pT-descent on every collection on all ingest paths |
| Axiom emitters (11/14) | sound | exact-name rules, guard shapes, deliberate omissions verified against interpreter reference semantics; SZSLICE/SZPERM/COMBSIZE math checked against `eval.rs` |
| Identity structural core | sound | literal keying is raw-text (no f64 round-trip); Sum flatten/sort Add-only; AngularSep orientation preserved except symmetric dR; slice rebase exactly half-open; sort-alias gate structurally exact (module the S7 token parse) |

---

## 4. Why the existing test batteries missed these

1. **`axioms_hold` evaluates instances individually under one extension.**
   S3 is a *joint* inconsistency between two instances that are individually
   satisfiable under different extensions. Also its vocabulary has no
   back-index quantities (the pad-0 evaluator would have failed today's
   back-ORD instances outright) and cannot contain opaque externals (the
   harness panics on evaluation errors).
2. **The difftest generator's vocabulary is too clean.** No reducer cuts, no
   binder-arg externals, no function-wrapped element properties, no region
   sorts, no min/max over mixed-exactness args — so the encoder-vs-interpreter
   oracle (the designated anti-false-PROVEN net) never sees any RC-A/S4/S5
   shape. Every finding above would have been caught by `check_sound` had the
   generator produced the shape.
3. **The corpus sweep counts verdicts, it doesn't check them.** False
   DISJOINTs on corpus-realistic idioms just raise the disjoint count.

**Proposed test-infrastructure changes (highest leverage, in order):**
- **T1** Extend the difftest case generator with: object cuts containing
  `dR(<binder>, <other coll>)` and reducer rejects; `sqrt(pt)`-style wrapped
  element props; back-index region selects; min/max with a ternary arg;
  region-level sort. Each RC-A/S3/S4/S5 shape becomes oracle-reachable.
- **T2** Add back-index vocabulary to `axioms_hold` + a per-event
  joint-satisfiability check of the whole emitted axiom set (pad-0 evaluator
  over the conjunction, not per-instance).
- **T3** Pin each finding in this document as a golden file once fixed
  (`examples/golden/` + `golden/cross/` where relevant).
- **T4** A fuzz target over object-block cut grammar feeding `verify
  --no-solver` (catches the S6 crash class without z3 in the loop).

---

## 5. Recommended fix order

1. **S1 + S2 (critical, one shared fix surface):** the fail-closed taint rule
   for Unsupported/context-dependent renders at the three layers (intern,
   opaque_arg, lin_pred). Ship with T1's generator extension + regression
   tests for all five reproduced shapes. *Blast radius: verdicts can only
   weaken (fewer merges → fewer PROVENs); corpus sweep before/after.*
2. **S3 (high):** guard the F2B/back-back ORD families; T2's battery
   extension locks it.
3. **S5 (high):** unconditional existence guards in the ScalarMinMax arm.
4. **S4 (high):** sort-Unsupported cascade.
5. **S6 (medium):** the three `subst*` ScalarMinMax arms (one-line each).
6. **S7 + S8 (medium):** strict sort tokens; duplicate-name diagnostics.
7. **S9–S19:** completeness/display/hardening batch.

All proposed changes are local (no architecture rework); the projection core,
solver protocol, and witness pipeline need no changes.
