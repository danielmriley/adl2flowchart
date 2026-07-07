# Implementation Plan: Semantic Analysis of ADL Composites, Reducers, Sort/Slice, and 4-Vector Arithmetic in smash2

## Goal & guardrails

"Analyzed" here is two distinct, layered capabilities — never conflate them:

- **Interpreted (`run`)**: `smash2 run` evaluates the construct on a concrete event to a concrete value, three-valued (True / False / Unknown, plus soft `NonValue` ⇒ comparison-false). This is the *highest-value, lowest-risk* deliverable: today `run` hard-errors on the dominant corpus pattern `reject any(dR(this,X)) < 0.4` (34 uses) because every non-`sqrt` `ExternalFn` hits `"... has no reference interpretation"` (eval.rs:1017) and `Collection::Combination` errors in `materialize_uncached` (eval.rs:1213).
- **Analyzed (`verify` PROVEN)**: the encoder produces a sound over-approximation (superset, drives UNSAT → PROVEN DISJOINT/SUBSET/EMPTY) and under-approximation (subset, drives SAT → PROVEN OVERLAPPING). A construct is "analyzable" only insofar as we can emit a *provably-true* fact about it; otherwise it stays a free/opaque quantity yielding POSSIBLY/Unknown.

**Load-bearing soundness invariants the entire effort must preserve.** Every new line of analyzer code is measured against these:

1. **Over = superset, Under = subset.** The over-approx (UNSAT side) may *never* exclude a real accepted event; the under-approx (SAT side) may never include one the interpreter would reject. The UNSAT side is the dangerous one — it has *no witness re-validation safety net*. A single wrong universal fact fabricates a false PROVEN DISJOINT with nothing to catch it.
2. **No pT-order on anything not provably pT-descending.** ORD/IDOM/EPRED ride on the pT-descending input invariant (validated only on *base* input collections, never on derived/materialized ones). They must fire ONLY through the existing `pt_ordered` walk (adl-axioms/lib.rs:450), which today returns `false` for `Union`/`Combination`. Sorted-by-non-pt, ascending-pt, and Combination candidate collections must never emit an index-ordering fact.
3. **Irrational ⇒ opaque.** Anything whose value is `sqrt`/`atan2`/`asinh`/trig of event kinematics (candidate mass, pt-of-sum, eta/phi-of-sum, dR) cannot enter the exact-rational linear core. It stays a free interned `ExternalFn` quantity with at most a sign/range axiom (the proven-sound `dR` posture). Linearizing it is unsound.
4. **Structural interning is the only identity.** Two constructs share a `QuantityId`/`CollectionId` iff they intern structurally identical (after canonical operand sort). This is what lets `mass(l1+l2) > 106` in region A cancel against `mass(l1+l2) < 106` in region B as pure linear UNSAT over one free var — and what guarantees two *different* sums never falsely cancel.
5. **Interpreter-first ordering is mandatory.** Every PROVEN OVERLAPPING witness is re-validated through the interpreter (witness.rs). A construct the interpreter cannot evaluate caps verdicts at POSSIBLY (Candidate). Analyzer-before-interpreter would emit SAT verdicts no witness can confirm. So interpret support always lands before the matching analyzer encoding.

A sixth, emergent invariant from the 4-vector work: **making the interpreter able to compute mass(sum) only ever makes witness re-validation *stricter*** (it can now recompute and reject), never looser — so it cannot fabricate a false OVERLAPPING.

## What becomes analyzable vs interpret-only

Legend: **full** = exact both directions; **size/existence-only** = SIZE facts + bounded existence, no per-index value; **opaque** = free quantity, sign/range axiom at most, POSSIBLY/Unknown.

| Construct | Interpreter (run) | Analyzer (verify PROVEN) | One-line soundness reason |
|---|---|---|---|
| `any(P)` / `all(P)`, boolean body P over one collection | full (Kleene fold) | **full** (per-kind Dual, tighter than today's hedged OPEN-1) | keyword resolves ∃/∀; same audited bounded expansion with the `size>k` escape. |
| `min(e)⋈c` / `max(e)⋈c`, monotone pairing (`min` with `>,≥`; `max` with `<,≤`) | full | **full** via desugar to `all`/`any` of `e⋈c` | logical equivalence over a finite non-empty fold; empty-collection boundary agrees (both ⇒ cut-false). |
| `min`/`max` with anti-monotone pairing (`min<`, `max>`) or `==`/`!=` | full | **opaque** (Unknown) | needs ∃ over the extremum's identity / simultaneous ∀∧∃; deferred to avoid subtle under-approx. |
| `min`/`max(e)⋈c` over a **static slice** `coll[:n]` | full | **full** (fixed k=n conjunction, no Dual, no bound approx) | slice has exactly n elements; existence guards handle size<n. The `min(dphi(jets[:4],met[0]))>0.5` win (4+ NPS files). |
| `sum(e)` over a collection | full (f64 fold) | **opaque**, `≥0` only when body is an NNEG-named magnitude | unbounded element count ⇒ no upper bound; sign-indefinite bodies (`pt·cos φ`) get nothing. |
| `sort(coll, pt(coll), descend)`, `coll` pt-ordered | full (identity no-op) | **full** — canonicalized to an *alias* of `coll` | stable descending-pt sort of a pt-descending list is the identity permutation. ORD/IDOM fire on the source. |
| `sort` by any other key / ascending / over a Union | full (real re-sort) | **size/existence-only** (SZPERM: `size=size(src)`) | permutation preserves count; per-index identity is event-dependent ⇒ no ORD/IDOM. |
| `coll[a:b]`, static bounds (`[:4]`, `[2:]`) | full (clamped sub-slice) | **full** — indexed access rebased `slice[i]≡src[a+i]`; SZSLICE for size | half-open contiguous range; subsequence of a sorted list stays sorted ⇒ ORD comes free. |
| `coll[-n]` / `[:-1]` (FromBack) | blocked (reserved) | opaque (Unsupported, OPEN-3) | negative-index ↔ missing-element interaction not yet modeled. |
| composite `size(X->cand)` / candidate count, 2-binder disjoint/cartesian | full (tuple count) | **size/existence-only** (COMB-* axioms; positive lower bound gated on `size≥2`/both-nonempty) | tuple-counting is pure integer combinatorics over the materialized sources. |
| composite per-tuple cut existence (`a.charge+b.charge==0`, `deta(a[i],b[j])`) | full | **full** via 2D dual-expansion over index pairs | linear/angular atoms over two indexed elements; same proof shape as 1D OPEN-1. |
| `mass(cand)` / `pt(cand)` where `cand=l1+l2` (4-vector sum) | full (f64 Lorentz) | **opaque**, `≥0` only (cross-region mass-window cancellation works) | irrational `sqrt` of a quadratic form; same interned-free-var posture as dR. |
| `eta(sum)` / `phi(sum)` / candidate eta/phi/charge-sum-as-value | full (atan2/asinh) | **opaque**, *no* sign axiom | unbounded/convention-dependent range; any bound risks a false PROVEN. |
| `cos/sin/tan/log/sqrt` in defines (HardMET leaves) | full (f64) | **opaque**; `sqrt≥0`; optional `cos/sin∈[-1,1]` | transcendental/irrational; range axioms universally true, low corpus value. |

## Phases

Each phase is independently shippable and verifiable. Run the corpus sweep (`adl2-corpus-sweep`) before and after each phase; **no pair may flip to a NEW false PROVEN**. Build/test via the `adl2-build-test` skill (no system libz3).

### P1 — Interpreter support for all five families (run works end-to-end)

Highest value (unblocks 34+ hard-erroring corpus files), lowest analyzer risk (touches no UNSAT-side fact). Ship this whole phase before any P2 analyzer encoding.

**Files touched.** `adl-sema/src/hir.rs` (new `HKind::Reduce`), `adl-sema/src/quantity.rs` (`Collection::Sorted`, `Collection::Slice`; `ParticleRef::Sum`), `adl-sema/src/resolve.rs` (reducer interception, sort/slice/Member/binary-add arms, parser-gap fix coordination), `adl-syntax/src/parser.rs` (slice-as-take-source), `adl-interp/src/eval.rs` (the bulk: `LV`/`FourVec` type, reducer folds, sort/slice/Combination materialize arms, `ExternalFn` getter + trig arm, binder/sum environment), `adl-difftest/src/casegen.rs` (generators).

**New quantity-model / AST / eval additions.**
- `HKind::Reduce { kind: ReduceKind(Any/All/Sum/Min/Max), coll, body, slice: Option<(u32,Option<u32>)> }`. `resolve_call` intercepts `any/all/sum/min/max` *before* the generic `ExternalFn` fallthrough (mirroring how `size` is intercepted), pushing an element context so the body's bare `pt`/`mass`/`dR(this,…)` resolve against the implicit element exactly as filter-predicate cuts do (the `elem: Option<&EventObject>` thread + `ElemSelfProp` already exist). Hoist the surrounding comparison into the body for boolean reducers: `any(dR(this,X)) < 0.4` ⇒ `any over X of (dR(this,X) < 0.4)`. Resolve-time, never eval-time.
- Eval folds (reuse `truth3`/`num3`/`materialize`): **Any** Kleene-OR, empty⇒false; **All** Kleene-AND, empty⇒true (vacuous); **Sum** f64 add, empty⇒0; **Min/Max** f64 fold, empty⇒new `NonValue::EmptyReduction`⇒comparison-false. Any element ⇒ `Tri::Unknown` poisons the fold to Unknown.
- `Collection::Sorted { source, key: SortKey(PtDesc|Opaque), dir }` and `Collection::Slice { source, start, end: Option<u32> }`, both structurally interned. Materialize: Sorted = stable-sort a clone by the per-element key (PtDesc on already-ordered source is an identity no-op); Slice = clamped half-open `src[lo..hi]`. Neither passes back through `validate_pt_descending` (it only iterates base `ev.collections`).
- `FourVec`/`LV { px,py,pz,e: f64 }` lifted from `(pt,eta,phi,m)`: `px=pt cos φ`, `py=pt sin φ`, `pz=pt sinh η`, `E=√(px²+py²+pz²+m²)`. `+` adds components; `mass=√(max(0, E²−|p|²))` (clamp tiny-negative to 0), `pt=hypot(px,py)`, `phi=atan2`, `eta=asinh(pz/pt)`. Missing `m` when mass requested ⇒ `NonValue::MissingProperty` (do NOT assume massless). MET summand: `pz=0, E=pt`. All `fin()`-wrapped, same posture as `angular()`.
- `ParticleRef::Sum(Vec<ParticleRef>)`, canonically sorted at intern (`ParticleRef` already derives `Ord`), with a flatten step so `l0+l1+l2` (left-assoc nested `Binary`) interns identically regardless of association. `resolve_target` gains a `Binary{Add}` arm: when both sides resolve to a `ParticleRef`, produce `Sum`. Getters `mass/pt/eta/phi/e(Sum)` intern as `ExternalFn{name, args:[Particle(Sum)]}`. Extend the eval `ExternalFn` arm (currently `sqrt`-only) with these getters and single-arg `cos/sin/tan/log`.
- Composite materialization (largest piece, interpret-only): enumerate tuples (cartesian = ordered product incl. cross-collection repeats; disjoint = unordered distinct by `(collection,index)` identity, strictly-increasing key for same-source); bind `n_i` per tuple; eval `candidate cand=expr` via FourVec; run per-tuple `select`/`reject` with a binder environment threaded through `Ev` (a `Symbol→EventObject` map alongside `elem`). `X->cand`/`X->a` resolve to the candidate/member axis; `[0]` reuses the element-index path. Member access must stop hard-coding `Unsupported` at resolve.rs (~1059).

**Soundness argument.** P1 emits *zero* UNSAT-side facts, so it cannot create a false PROVEN DISJOINT. The only soundness surface is the SAT side via witness re-validation: a construct the interpreter now evaluates makes re-validation *stricter*, never looser. The empty-collection conventions (All⇒true, Any⇒false, Min/Max⇒NonValue) are chosen to match what the P2 encoder will assert — pinning them now is the load-bearing metamorphic contract.

**Test strategy.** (a) **Encoder-vs-interpreter difftest** (`prop_encoder_vs_interp`): not yet relevant for analyzer (P1 is interp-only) but extend `casegen` to *generate* any/all/min/max/sort/slice/`mass(elem+elem)⋈k` cases now, so P2 inherits coverage. (b) **Metamorphic**: `all` over empty = true, `any` over empty = false, `min/max` over empty = cut-false; `sort(coll,pt,descend)[i]` ≡ `coll[i]` on pt-ordered input; `slice[i]` ≡ `src[a+i]`. (c) **Golden**: cutflow/histo goldens on the NPS files that previously hard-errored now produce values. (d) **Exhaustive-match discipline**: adding `HKind::Reduce` and two `Collection` variants makes every match non-exhaustive — a compile error is the forcing function; a stray `_ => Unsupported` is acceptable (sound) but must be deliberate.

**Effort/risk.** Reducers **M**; slice **S**; sort **S-M**; 4-vector getters **S-M** (~120 LOC mirroring `angular`/`angles`); composite tuple-materialization **L** (the single largest addition). Riskiest: the resolver's iteration-collection / body-shape classification (wrongly accepting a 2-collection self-cross body like `dR(leptons,leptons)` must ⇒ Unsupported) and the composite binder environment. Both are interpret-only risks (caught by difftest), not soundness risks.

### P2 — The cleanly-analyzable analyzer wins

The fully-sound, high-value encoder additions. Each reuses audited machinery.

**Files touched.** `adl-formula/src/encode.rs` (`dual_expand` → per-kind routing; slice-index rebasing), `adl-axioms/src/lib.rs` (SZPERM, SZSLICE, COMB-* size axioms, `sqrt` into `NNEG_EXTFN_KEYS`; `pt_ordered` arms for Sorted/Slice/Combination), `adl-sema/src/resolve.rs` (PtDesc-sort alias canonicalization via `pure_alias_of`; min/max→any/all desugar), `adl-formula/src/formula.rs` (verify `not()` swaps the new Duals correctly), witness.rs (Slice/Sorted/Sum realizers).

**New axiom / quantity-model additions.**
- **Reducers (any/all):** refactor `dual_expand` to take `ReduceKind`. `any`-plus = `P(0)∨P(1)∨P(2)∨size>k`; `any`-minus = `⋁ᵢ(size>i ∧ P(i))`. `all`-plus = today's dual-plus (`⋀ᵢ(size≤i ∨ P(i))`, admits `size>k`); `all`-minus = `size=0 ∨ (1≤size≤k ∧ ⋀ᵢ(size≤i ∨ P(i)))`. The size=0 disjunct goes to **All-minus** (vacuous true), NOT Any. `subst()` is reused verbatim to instantiate `P(i)`. Negation falls out: `¬any(P)=all(¬P)` via the existing `Formula::not()` Dual swap.
- **min/max desugar (sema-side):** `max(e)>c ⇔ any(e>c)`, `max(e)<c ⇔ all(e<c)`, `min(e)<c ⇔ any(e<c)`, `min(e)>c ⇔ all(e>c)` (and `≥/≤`). Only the monotone pairings; `==/!=` stay Unknown. Static-slice min/max emits a fixed k=n conjunction (exact equivalence, no Dual). The interpreter keeps the *real* fold (P1) AND the desugar feeds analysis — they must share the body and provably not drift (difftest pins it).
- **Sort alias:** at intern, `Sorted{src,PtDesc,Descend}` with `pt_ordered(src)` canonicalizes to `src` (reuse `pure_alias_of`). Everything else ⇒ opaque Sorted with **SZPERM** (`size(Sorted)=size(src)`) and `pt_ordered(Sorted{non-PtDesc})=false`.
- **Slice:** canonicalize indexed access `slice[a:b][i] → src[a+i]` at resolve (concrete bounds only), bringing ORD/IDOM/EPRED for free. Keep the Slice id for `size`/reducer cases with **SZSLICE** (linear core: `size≥0`, `size≤b−a`, `size≤size(src)`, `size≤size(src)−a` clamped; exact ITE-clamp deferred unless the sweep needs it).
- **Composite SIZE:** COMB-MEMBER-SIZE (`size(K->axis)=size(K)`); COMB-CARTESIAN-2 / COMB-DISJOINT-2-cross (`both-nonempty ⇒ size(K)≥1`, `either-empty ⇒ size(K)=0`); COMB-DISJOINT-2 single-source (`size(C)<2 ⇒ size(K)=0`, `size(C)≥2 ⇒ size(K)≥1`, `size(K)≥0`). The positive lower bound is asserted on the **pre-cut** Combination only; the post-cut `size(X->cand)` is a SUB-like `≤` of it (model pre/post-cut as parent/Filtered, reusing SUB). The 2D dual-expansion handles per-candidate cut existence.
- **4-vector / mass:** add `sqrt` to `NNEG_EXTFN_KEYS`; confirm `mass/pt/e` getters over `Particle(Sum)` inherit NNEG by exact-name. Cross-region mass-window cancellation works automatically via structural interning. No eta/phi sign axiom.

**Soundness argument.** any/all is the audited OPEN-1 expansion with the ∃/∀ hedge *resolved by the keyword* (strictly tighter, never looser); the `size>k` escape is preserved so no real witness beyond the bound is excluded. min/max desugar is a logical equivalence over a finite non-empty fold with the empty boundary matching the interpreter. SZPERM/SZSLICE/COMB-* are integer facts true on every event (permutation bijection / half-open range / combinatorial tuple-count). The PtDesc alias is gated on the *exact same* `pt_ordered` predicate that already guards ORD/IDOM. Mass stays a free var with `≥0`; non-overlapping mass bands on the *identical* interned quantity are sound DISJOINT precisely because both regions constrain the same free var.

**The false-PROVEN inventory P2 must prevent** (each gated to Unknown/false on doubt): (1) ORD/IDOM on a non-PtDesc Sorted — prevented by `pt_ordered(Sorted)=false` unless exact-pt-descending-on-ordered-source (exact-match, never substring — the Bug-6 TAG lesson); (2) UNI/SUB size facts on Combination — emit only the COMB-* family, never a positive lower bound below the `size≥2`/both-nonempty gate; (3) min/max ∃/∀ direction confusion — only monotone pairings; (4) slice rebasing with a non-static bound — concrete-`a` only; (5) All-minus dropping the size=0 case — but this weakens only (SAT side), not a false PROVEN; (6) two structurally-different sums interning together — prevented by canonical-sorted structural interning.

**Test strategy.** (a) **Encoder-vs-interpreter difftest**: the central guard — generate cuts over each construct and assert `over ⊇ interp ⊇ under` on random events; the encoder must never be *more* restrictive than the interpreter on opaque masses (trivially holds since the var is free). (b) **`axioms_hold.rs`**: enumerate small events and check every emitted SZPERM/SZSLICE/COMB-* instance holds — *critical* for the COMB lower bounds. (c) **Metamorphic**: mass-window UNSAT pair becomes PROVEN DISJOINT; `size(sortedX)≥2 ≡ size(X)≥2`. (d) **Corpus sweep** before/after.

**Effort/risk.** Reducer kind-routing **S-M**; min/max desugar **S**; slice rebasing + SZSLICE **S**; SZPERM/COMB-* **S**; sort alias with exact-key gate **M** (small code, soundness-critical). 4-vector mass **S** (largely already works once P1 lowers the sum to `Particle(Sum)`). **Riskiest:** the exact-pt-key gate on the sort alias — a single wrong key match fabricates a false PROVEN via ORD on the UNSAT side with no witness net.

### P3 — Harder / opaque-with-axioms pieces

Lower value, higher subtlety, or explicitly deferred.

**Files touched.** `adl-formula/src/encode.rs` (2D dual-expansion for k≥2 binders), witness.rs (Combination realizer fallback), `adl-axioms/src/lib.rs` (optional `cos/sin∈[-1,1]`).

**Additions.** (a) The full 2D dual-expansion for composite per-candidate cuts (Tier 2) generalized to a binder environment and index pairs `(i,j)` with `i<j` (disjoint) or all (cartesian), at a possibly-reduced bound to cap `k²` blowup. (b) Combination witness realizer: build sources from the model, enumerate tuples, re-run the interpreter; fall back to Candidate when mass/pt opacity is load-bearing. (c) Optional `cos/sin∈[-1,1]` range axiom (one corpus use, defer unless cheap). (d) min/max `==/!=` via `all(≥c)∧any(≤c)` — corpus scan shows only strict `</>`, so defer.

**Soundness argument.** The 2D expansion is identical in shape and proof to 1D OPEN-1, sound only when `P` is built from analyzable per-element quantities (indexed ElemProp, sizes, tags, angular seps). The Combination realizer never returns a false Validated — it falls to Candidate when opacity bites.

**Test strategy.** Quantify encoder size on the NPS files first (k=3 ⇒ 27 instances/cut may blow up). `axioms_hold.rs` for the 2D witnesses; corpus sweep.

**Effort/risk.** 2D dual-expansion **M**; Combination realizer **M** (depends on P1 materialization); range axioms **S**. Risk: encoder-size blowup; measure before raising the bound.

## Explicit opacity decisions

Deliberately kept Unknown. Each is sound (it only ever weakens to POSSIBLY) and acceptable (no corpus cross-region proof depends on it):

- **Candidate invariant mass / pt-of-sum** (`mass(l1+l2)`, `pt(l1+l2)`, eta/phi/energy of a sum): irrational `√` of a quadratic form in the members' angles/momenta. No exact-rational bound exists; linearizing is unsound. Free interned `ExternalFn` with `≥0` (mass/pt) or nothing (eta/phi). The *only* analyzable leverage — cross-region cancellation of the identical interned quantity — is exact and kept.
- **Sum over a variable-size collection**: unbounded element count ⇒ no upper bound; lower bound `≥0` only when every body element is an NNEG-named magnitude. Sign-indefinite bodies (`pt·cos φ` for HardMET px/py) get nothing; HardMET is doubly opaque (free sum inside an irrational `√`).
- **Sorted-per-index properties for non-pt / ascending / Union-rooted sorts**: the permutation depends on event-specific (often irrational) keys. `Sorted[i].prop` is a fresh free element property; only SZPERM (size) is recovered. ORD/IDOM correctly NOT asserted.
- **Trig / sqrt / log of any argument**: transcendental/irrational; opaque free var, at most `sqrt≥0` and optional `cos/sin∈[-1,1]`. No constant-folding even on constant arguments (an irrational result is not an exact `Rat`).
- **Combination cardinality for k≥3 binders or mixed combinators**: high-degree combinatorial (products / n-choose-k), non-linear; only loose 2-binder bounds are soundly assertable.
- **`eta(sum)` / `phi(sum)` / candidate eta/phi/charge-sum-as-continuous-value**: free, *no* sign axiom (eta unbounded, phi convention-dependent — a bound here is the most tempting false-PROVEN trap).
- **`[-n]` / `[:-1]` back-indexed slices**: reserved under OPEN-3; Unsupported (interpret-blocked too).

## Risks & open questions

**Gravest false-PROVEN hazards** (UNSAT side, no safety net — each gated to Unknown-on-doubt):
1. **SORT exact-pt-key gate.** Aliasing a sort over a non-pt-ordered source (Union), an ascending sort, or an opaque-key sort asserts `sorted[0]≡src[0]` — false. Defense: alias ONLY when `key==PtDesc ∧ dir==Descend ∧ pt_ordered(src)`, via the *exact same* `pt_ordered` walk; structural (not syntactic) key-quantity equality, defaulting to false. This is the single gravest trap in the family.
2. **COMB size lower bound.** Asserting `size(K)≥1` when only one element exists (single-source disjoint) is a direct false-PROVEN. Defense: positive lower bound gated strictly on `size(C)≥2` / both-sources-nonempty, asserted on the **pre-cut** Combination only, with `axioms_hold.rs` enumeration.
3. **min/max ∃/∀ direction.** Only monotone pairings (`min` with `>,≥`; `max` with `<,≤`); the rest Unknown.
4. **eta/phi(sum) sign axiom.** Must NOT exist — assert nothing.
5. **`pt_ordered` divergence.** It lives only in adl-axioms/lib.rs:450 today (verified — no second copy), but if encode.rs ever grows its own, a Sorted/Slice arm added to one and not the other is a divergence bug.

**Unresolved design decisions needing the user / corpus confirmation:**
- **`any(scalar) ⋈ c` author shorthand** (`reject any(dR(this,electrons)) < 0.4`, 6+ NPS files): malformed, `any(dR<0.4)`, or `min(dR)<0.4`? Reinterpreting is unsound guessing. **Recommend escalating to corpus authors; v1 keeps it Unsupported** — but this blocks real coverage, so it needs a decision.
- **ADL empty-collection quantifier convention** (any=false, all=true): the empty boundary is soundness-load-bearing (interpreter and encoder must agree). Confirm against the legacy C++ parser / a CMS reference.
- **Candidate-collection order:** does any analysis index `->cand[0]` as "highest-pt candidate", or only ever after a `size==1` cut (order-irrelevant)? Confirm across all 68 files before deciding whether the interpreter must re-sort the candidate collection.
- **DISJOINT identity** (by `(collection,index)` vs by kinematic value): value-distinctness would change tuple counts and the COMB axioms. Confirm.
- **OPEN1_BOUND for reducers / composites:** raise k for `size(jets)≥4 ∧ all(pt(jets)>30)` tightness vs solver cost and k² blowup? Per-construct bound or shared?
- **`mass`/`m` name normalization:** does the `ExternalFn`-name path canonicalize `mass→m` (per property_vars.txt)? If not, `mass(ll)` and `m(ll)` are distinct quantities and a real cancellation is missed (not unsound, just lost).

## Recommended first slice

**P1, reducers only — `any`/`all`/`min`/`max`/`sum` interpreter support, plus the `min(scalar)⋈c` desugar groundwork.** Smallest high-value, low-risk increment:

- It unblocks `run` on the single most common corpus pattern (`reject any(dR(this,X)) < 0.4`, 34 uses) that currently hard-errors at eval.rs:1017 — immediate, demonstrable, end-to-end value.
- It emits **zero** UNSAT-side facts, so it cannot fabricate a false PROVEN — the entire soundness risk reduces to the SAT-side witness path, which only gets stricter.
- It reuses existing `truth3`/`num3`/`materialize`/`elem` plumbing — no new physics (defer the 4-vector `LV` type and composite tuple-materialization, the two large/risky pieces).
- It seeds `casegen` with reducer generators that P2's encoder-vs-interpreter difftest battery will depend on.

Concretely: add `HKind::Reduce` + `ReduceKind` + `NonValue::EmptyReduction`, the `resolve_call` interception with the boolean-reducer comparison-hoist, and the five eval folds — gating multi-collection / `dR(this,X)`-element-as-particle bodies to Unsupported (sound). Verify with the empty-collection metamorphic cases and a cutflow golden on a previously-erroring NPS file. Defer the desugar's analyzer emission to P2; land only the interpreter fold here.

Relevant files (all absolute): `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-interp/src/eval.rs` (folds, getter/trig arm, sort/slice/Combination materialize), `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-sema/src/resolve.rs` (reducer interception ~1338, sort 457-470, Member ~1059, Slice ~1103, `pure_alias_of` 552), `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-sema/src/quantity.rs` (Collection/ParticleRef enums), `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-sema/src/hir.rs` (`HKind::Reduce`), `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-formula/src/encode.rs` (`dual_expand` 478, `OPEN1_BOUND` 36), `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-axioms/src/lib.rs` (`pt_ordered` 450, `NNEG_EXTFN_KEYS` 570), `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-syntax/src/parser.rs` (slice-as-take-source), witness.rs and `/home/daniel/Projects/adl2flowchart/reimplementation/adl2/crates/adl-difftest/src/casegen.rs`.