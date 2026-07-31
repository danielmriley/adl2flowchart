# ADL2 presence (definedness) model — Phase B

Status: DESIGN v1.0, 2026-07-30. Closes `SOUNDNESS_PROOF_2026-07-25.md`
§8 item 1 ("the absent-property seam") in both arms and **deletes** its
Phase-A interim hedge. Written for the post-M3 world (`Rat` events,
`NumVal{Exact,Approx}` interpreter — see the `adl2-rational-numeric`
skill). Paths are relative to `reimplementation/adl2/` unless absolute.

Reading order: `SPEC_ANALYSIS.md` §1–§4 (what the verifier claims),
`SOUNDNESS_PROOF_2026-07-25.md` §3–§5 (the premises this spec discharges),
then this file.

---

## 1. Problem statement

The reference interpreter is the meaning (proof §1). Its numeric result
type is `NumRes = Result<NumVal, NonValue>` (`adl-interp/src/eval.rs:221`),
and a **soft non-value** — `MissingProperty` / `MissingElement` /
`NonFinite` / `EmptyReduction` (`eval.rs:60-73`) — makes the *enclosing
comparison decidably false* (SPEC_LANGUAGE §4.4; "soft non-value is
ABSORBING", `eval.rs:1054`, `1131`, `1252`). Absence is not Unknown: it
is a decided `false`. The prover, meanwhile, reasons over *total*
rational valuations, while `⟦q⟧ : E ⇀ ℚ` is partial (proof §1) — for a
property-less element both `q ≥ 0` and `q < 0` are false, so no rational
value reproduces the interpreter. Two demonstrated consequences:

**Arm 1 — negation (interim-fixed, precision withdrawn).**
`reject q ⋈ k` HOLDS on absence (inner comparison soft-falses, the reject
is a no-op) while the classical `¬(q ⋈ k)` constrains a total valuation.
Two complementary rejects over the same possibly-absent property both
accept a property-less event, yet their classical encodings are
contradictory ⇒ **false PROVEN DISJOINT via the interval path**
(no axiom, no solver: `IntervalMap::disjoint_with`,
`adl-analysis/src/interval.rs:192`). Verified shipping instance:
complementary rejects on `BTag(jets[0])` — `proven_disjoint`,
`refutations: 0`, because the gate battery always writes `btag` on jets
(`adl-interp/src/sample.rs:81-87,161-163`) so the sampler is blind, while
the interpreter accepts a legal btag-less-jet JSONL event into both.

Phase A patched this with `Encoder::guarded_not` (`adl-formula/src/encode.rs:570`),
which replaces an unsafe negation with `Dual{plus: widen_unsafe(¬f),
minus: ¬f}` where `widen_unsafe` (`:610`) rewrites every atom over a
non-total quantity to `true`, and `negation_safe` (`:636`) whitelists only
`Quantity::Size` and `EventScalar(MetProp(_))`. Placement is canonicalized
first (`reject not X → select X` at `:689-696`; `not not c → c` at
`:736-739`). Measured cost: **18 of 834 corpus PROVEN DISJOINT withdrawn**,
and three *truly* disjoint/empty complement-of-one-predicate results
re-pinned fail-closed with restore markers (§9).

**Arm 2 — positive cut + axiom (STILL OPEN).** Subset and bin coverage
consume L1-lower over the inner region's `Under`. An axiom can discharge
an atom over a possibly-absent property with **no negation involved**:

```adl
region A   select size(jets) >= 2
region B   select BTag(jets[-1]) >= 0
```

TAG (`Emit::tag`, `adl-axioms/src/lib.rs:877-895`, exact-name keys
`["btag","ctag","tautag"]` set at `:524`) asserts `Or([q == 0, q == 1])`
**unguarded**, which entails `q ≥ 0`; `¬B⁻` then has no satisfiable
disjunct and the engine ships `subset A-in-B: true`
(`Engine::subset`, `adl-analysis/src/engine.rs:1079`), while the
interpreter *rejects* a btag-less-jet event from B. Today this is held
only where the sampling gate (`Engine::gate_pair`, `engine.rs:1363`;
`sample_subset` `:1422`) happens to draw the right absence pattern.

Both arms are one defect: **the encoding has no way to say "this quantity
has a value".**

### The running examples

| tag | ADL | today | after Phase B |
|---|---|---|---|
| **N1** | `reject BTag(jets[0]) >= 1` vs `reject BTag(jets[0]) < 1` | POSSIBLY (was: false PROVEN DISJOINT) | POSSIBLY — correctly, both admit a btag-less jet |
| **N2** | `select BTag == 1` vs `select not BTag == 1` (`not_tag.adl`) | POSSIBLY (sound, unprovable) | **PROVEN DISJOINT** (restored) |
| **N3** | `features-angular_06.adl`: `reject dR(jets,eles) < 0.4` vs `select dR(jets,eles) < 0.4` | POSSIBLY | **PROVEN DISJOINT** (restored) |
| **N4** | CE-7 `empty_ra`: `select d0 < 2` + `reject d0 < 2` | `EmptyStatus::NotProven` | **`Proven`** (restored) |
| **P1** | `size(jets) >= 2` vs `select BTag(jets[-1]) >= 0` | **false `subset A-in-B`** (gate-held) | subset NOT proven |

---

## 2. Semantics

### 2.1 Presence-extended valuation

For every quantity `q` define the **presence indicator** `p_q`. Its
meaning is fixed against the interpreter, not against the data model:

> `p_q = 1` at event `e` iff `Interp::eval_quantity(q, e)` yields
> `NumOutcome::Value` (`eval.rs:455`); `p_q = 0` iff it yields
> `NumOutcome::NonValue(_)`.

This covers all four `NonValue` constructors uniformly — a missing
property, an out-of-range element, non-finite arithmetic inside an opaque
scalar, and an empty min/max reduction are all "no usable value". A hard
`EvalError` (missing MET / event scalar / trigger, `eval.rs:25-27`) is
*not* absence: the event is then `In` no region at all and cannot witness
anything, which is why those quantities need no indicator (§3.4).

**The presence-extended valuation** of `e ∈ E` is
`v̂(e) : (QuantityId ∪ PresenceId) → ℚ`:

```
p̂_q(e) = 1                      if ⟦q⟧(e) is defined
       = 0                      otherwise
q̂(e)   = ⟦q⟧(e)                 if defined
       = 0  (arbitrary)         otherwise      ← "junk"
```

**Lemma E (Embedding).** Every loader-valid event embeds into a total
valuation `v̂(e)`, and the junk components are **unconstrained by any
asserted formula**.

*Proof.* Totality is by construction. Unconstrainedness is a consequence
of two encoding invariants proven in §3.6 and §4.1:
 (E-i) every atom mentioning a possibly-absent `q` occurs only in the
 shapes `p_q ≥ 1 ∧ φ` (positive) or `p_q < 1 ∨ ψ` (negated), and
 (E-ii) every axiom instance mentioning a possibly-absent `q` is a
 disjunction containing `p_q < 1`.
Under `p̂_q(e) = 0` every such occurrence is discharged by its presence
literal alone, so no constraint mentions `q̂(e)`. ∎

Lemma E is what the current design lacks. Today the junk value is
*shared* between the two regions of a pair and between the axioms: two
regions that both accept an absence event may impose contradictory
constraints on the same junk variable, and the resulting UNSAT is
reported as DISJOINT. That is exactly arm 1.

### 2.2 The revised projection lemma

**L1′ (Projection sandwich under presence).** With `R⁺`/`R⁻` as in proof
§4 and encoding rules §3, for every `e ∈ E`:

- *(upper)* `I(R,e) = In ⇒ R⁺` true under `v̂(e)`.
- *(lower)* `R⁻` true under `v̂(e) ⇒ I(R,e) = In`, on the exact fragment.

*Proof sketch.* Structural induction as in proof §4; only the leaf case
changes, and it now holds in **both** directions without a definedness
side-condition.

*Positive leaf* `p_q ≥ 1 ∧ (q ⋈ k)`. (⇒) If `I` decides the cut true,
the interpreter obtained a `NumVal` for `q`, so `p̂_q(e)=1` and
`q̂(e) = ⟦q⟧(e)` satisfies the atom by P2. (⇐) If the conjunction holds
at `v̂(e)` then `p̂_q(e)=1`, so `q` is defined and the atom's truth is the
interpreter's (P2), so the cut is true.

*Negated leaf* `p_q < 1 ∨ ¬(q ⋈ k)`, obtained by `Formula::not`
(`adl-formula/src/formula.rs:108`) with `Rel::Ge.negated() = Rel::Lt`
(`lin.rs:25-33`). (⇒) If `I` decides `reject`/`not` true, either `q` was
absent (⇒ `p̂_q(e)=0 < 1`, first disjunct) or present and failing the
inner comparison (⇒ second disjunct by P2). (⇐) Symmetric: `p̂_q(e) < 1`
means `p̂_q(e) = 0`, i.e. `q` is absent, i.e. the inner comparison
soft-falses, i.e. the negation holds. Multi-quantity leaves conjoin one
presence literal per possibly-absent quantity; De Morgan gives the
corresponding disjunction, which is again exact because the interpreter's
absorbing rule fires on *any* absent operand. ∎

**Consequence: the negated leaf is `is_exact()`.** `Formula::Dual`
vanishes from the negation path (`formula.rs:139-145`), so `reject` no
longer poisons the `Under` projection. This is what restores subset
inners, bin coverage, and region-emptiness through complements (N2/N3/N4).

### 2.3 Which proof-document premises become theorems

| Premise | Today | After Phase B |
|---|---|---|
| **P2 (leaf faithfulness)** — "the quantities the atom mentions are **defined** under `v(e)`" | an *assumption* about the event, empirically audited by `adl-difftest`; **provably false** for absent properties | **discharged by construction**: definedness is no longer a side-condition but a conjunct of the leaf. P2 reduces to the residual numeric claim "when `p_q = 1`, the atom's truth equals the interpreter's" — the exact-rational claim already discharged by the `Rat` core |
| **P3 (axiom truth)** — "every catalog instance holds of `v(e)` for every `e`" | quantifies over a *partial* `v`; the current justification is the informal "the canonical pad-with-0 extension satisfies the fact" (`adl-axioms/src/lib.rs:24-33`), which is **false for ORD** on an event whose leading element lacks `pt` (the loader's skip-without-reset, `adl-interp/src/event.rs:538`, admits it) | **restated over `v̂` and discharged**: guarded instances are vacuously true when `p_q = 0`, and the pad-with-0 premise is deleted outright (§4.1). No axiom family retains a definedness obligation |
| **L1 (projection sandwich)** | leaf case carries P2's definedness side-condition; the negation case needs the Phase-A `Dual` hedge | **L1′ above** — leaf case exact in both directions, no hedge |
| **L5 (spine entailment)** | unchanged | unchanged, and *strengthened*: presence literals are additional necessary conditions on the And-spine (§7) |

P1, P4, P5, A1, L2, L3, L4 are untouched.

---

## 3. Encoding rules

### 3.1 Where `p_q` lives

**Decision: a first-class IR variant.**

```rust
// crates/adl-sema/src/quantity.rs, in `pub enum Quantity` (:180)
/// The definedness indicator of another quantity: 1 when the interpreter
/// obtains a `NumVal` for `inner` at this event, 0 on any soft non-value
/// (missing property/element, non-finite, empty reduction).
Present(QuantityId),
```

Rejected alternatives: a side-table `BTreeMap<QuantityId, QuantityId>`
(loses structural interning, so P1/P1m have to be re-argued on the merge
path); an `ExternalFn { name: "presence", args: [Quantity(q)] }`
(gets swept up by the exact-name NNEG/TRIG rules at `adl-axioms/src/lib.rs:806,845`
and by opaque-arg namespacing in `Merger::remap_key`).

The variant buys: structural interning gives `p_q ≡ p_q′ iff q ≡ q′`
for free (P1 preserved); `Merger::remap_quant` (`adl-sema/src/merge.rs:307`)
needs one recursive arm; the IR census (`adl-sema/tests/ir_census.rs:53`)
**forces** every future `Quantity` arm to answer the presence question at
compile time — the same discipline that made `existence_floor` a
chokepoint.

`existence_floor` (`quantity.rs:448`) must return the **inner** quantity's
floor for `Present(q)`, so the size guards on a presence literal match
those on the atom it guards.

### 3.2 Presence literals

| literal | encoding | rationale |
|---|---|---|
| "q is present" | `LinAtom::single(p_q, Rel::Ge, 1)` | plain inequality: Farkas-friendly (§5), interval-friendly (§7) |
| "q is absent" | `LinAtom::single(p_q, Rel::Lt, 1)` | exactly `Formula::not` of the above (`lin.rs:25-33`) — no special case |
| domain (family **PRES**) | `And([p_q ≥ 0, p_q ≤ 1])` | **not load-bearing**; keeps witness models tidy and lets the realizer read `p` directly |

**No equalities, no `Rel::Ne`, no integrality dependence.** Over ℚ the two
literals partition the domain into `{1}` and `[0,1)`; models with
`p ∈ (0,1)` are extra *absent-like* models, which only ever ADD models
(UNSAT harder — sound) and realize as omission (§6). `QSort::Real`
(`adl-solver/src/lib.rs:82`); the `engine.rs:243-249` declare loop needs
no new arm, `Present` falls through to `_ => QSort::Real`.

**Family PDEF (presence ⇒ existence).** For every interned
`Present(ElemProp{coll, index, prop})` emit
`Or([p ≤ 0 …, size(coll) ≥ floor+1])` with `floor` from `existence_floor`.
True by construction (a property of a non-existent element is a
`MissingElement` soft non-value). Keep the existing `guard_existence`
size conjuncts (`encode.rs:845`) as well — belt and braces, zero cost.

### 3.3 The encoder chokepoint

Every construction of an atom over a possibly-absent quantity goes
through **one** new function, mirroring `Emit::guarded`:

```rust
/// THE presence chokepoint. Wraps an exact leaf in the presence literals
/// of every possibly-absent quantity it mentions. Non-exact leaves pass
/// through untouched (an `Unknown` is already an honest refusal).
fn presence_guarded(&mut self, terms: &BTreeMap<QuantityId, Rat>, inner: Formula) -> Formula
```

Called from `Encoder::atom_of` (`encode.rs:1950`) — the single site where
`Formula::Atom` is constructed from a term map — so coverage is
structural, not per-callsite. `atom_of`'s constant-fold early return
(`:1951-1957`) and its Int-size coercion (`:1961-1977`) are unchanged;
the wrap happens on the final `Formula::Atom` only.

**Two arms bypass `atom_of` and need the wrap applied by hand**, both
because they fold to a constant *before* linearization: `abs_cmp`'s
`c < 0` fold (`:1829-1834`) and any future constant-fold that returns
`Formula::True` from a node that mentions quantities. Invariant I-1
(§11) is what catches a missed one; it is a *checked* invariant, not a
convention, precisely because these two exist.

An invariant test (§11 I-1) asserts `Formula::Atom` is constructed at
exactly one place in `adl-formula` and that no `Over`/`Under` projection
of any region contains an atom over a possibly-absent quantity that is
not conjoined with (or disjoined against) its presence literal.

### 3.4 Total quantities — no indicator

| kind | total? | why |
|---|---|---|
| `Quantity::Size(_)` | **yes** | `Event::collections` absent ⇒ empty (`adl-interp/src/event.rs:91`); `size` is defined on every loader-valid event |
| `EventScalar(MetProp(_))` | **yes** | missing MET is a hard `EvalError`, not a soft non-value (`eval.rs:25-27`) — such an event is `In` no region |
| `EventScalar(EventVar(_))` | **yes** | same hard-error rule. *Widens the current `negation_safe` whitelist* (`encode.rs:642` admits only `MetProp`) — a small precision gain, justified by the same argument |
| `EventScalar(Trigger(_))` | **yes** | same |
| `Present(_)` | **yes** | an indicator is always 0 or 1 |
| `ElemProp{..}` | **no** | `object_prop` → `Err(MissingProperty)` (`eval.rs:1401-1409`); `elem_position` → `Err(MissingElement)` (`eval.rs:95`) |
| `AngularSep{..}` | **no** | any absent `eta`/`phi` leg soft-non-values (`eval.rs:1443-1466`, `1490`, `1514`) |
| `ExternalFn{..}` | **no** (conservative) | includes `reduce.*` (`intern_reduce`, `encode.rs:1999`) and `opaque.scalar` (`:2033`), whose bodies may reference absent leaves, and `EmptyReduction`. A genuinely total external loses only what `widen_unsafe` already discards today, so this is a strict improvement in every case |

The classifier lives **once**, next to the enum:

```rust
// crates/adl-sema/src/quantity.rs
impl QuantityTable {
    /// Can `q` be a soft non-value on a loader-valid event? THE single
    /// source for the encoder's presence chokepoint and the axiom
    /// emitter's presence guards (same discipline as `existence_floor`).
    pub fn may_be_absent(&self, q: QuantityId) -> bool { ... }
}
```

### 3.5 Per-construct table

`q̄` = the possibly-absent quantities of the term map; `P(q̄)` =
`⋀ p_q ≥ 1`. Everything below is what `atom_of`'s chokepoint produces
automatically unless noted.

| construct (`encode.rs`) | today | after |
|---|---|---|
| linear comparison, `cmp` `:1605` / `cmp_node_const` `:1739` | `Σcᵢqᵢ ⋈ k` | `P(q̄) ∧ Σcᵢqᵢ ⋈ k` |
| element-existence guard, `guard_existence` `:845` | `size(C) > i ∧ leaf` | unchanged (kept; PDEF makes it derivable) |
| band `[]`/`][`, `band` `:1885` | `And`/`Or` of two bounds | `In` = `P ∧ lo ∧ hi`, `Out` = `P ∧ (lo ∨ hi)`. Both bounds share one term map, so `(P∧lo) ∨ (P∧hi)` is *logically* equivalent — but **hoist `P` above the `Or` anyway**, or the whole disjunction leaves the And-spine and the interval layer loses the presence bound (§7b) |
| ratio `L/D ⋈ c`, `ratio` `:1759` | `(D>0 ∧ L⋈cD) ∨ (D<0 ∧ L⋈̄cD)` | `P(q̄_L ∪ q̄_D) ∧ ( … )` — hoisted for the same spine reason; `D = 0` and an absent leg both fail the cut (§4.4) |
| constant-denominator non-pow2 ratio `:1786-1793` | one `opaque.scalar` atom | `p_O ≥ 1 ∧ (O ⋈ c)` |
| `abs_cmp` `:1819` | `And`/`Or` of upper/lower | `P(q̄)` hoisted above the `And`/`Or`. **The `c < 0` constant folds at `:1829-1834` are the one arm the `atom_of` chokepoint cannot reach** — they return before any linearization (`inner` may be opaque), so `q̄` must come from `collect_quantities` (`:227`) over `inner`'s subtree. `Lt/Le/Eq ⇒ False` is unchanged; `Gt/Ge/Ne ⇒ True` must become `P(q̄)`, because absence falsifies `abs(x) > -1` in the interpreter |
| scalar `min`/`max` vs const, `pattern` `:1662-1721` | monotone `Or`/`And` of arg comparisons + unconditional `needs_guards` | each arg comparison carries its own `P`; **additionally** hoist `⋀_args P(q̄_arg)` to the `And` level, exactly as the existing existence guards are hoisted at `:1702-1710` — the interpreter's fold makes a soft non-value in *any* arg absorbing (`eval.rs:1350-1367`) |
| ternary `g ? a : b`, `boolean` `:747-755` | `(g∧a) ∨ (¬g∧b)` | unchanged in shape; `¬g` now comes from plain `Formula::not`, and each leaf carries its own `P`. **Do not** hoist: a missing else-branch is `true` (§4.4) and the guard's absence genuinely selects the else arm |
| boolean reducers `any`/`all`, `encode_reduce` `:979` + `build_dual` `:912` | per-index `Dual` expansion with size guards | unchanged; each instance leaf carries its own `P` at index `i`. The `size ≤ i` guards and the `P` literals are independent (an element can exist and lack the property) |
| unindexed collection cut, `leaf` `:810-826` | `Dual` bounded expansion, `OPEN1_BOUND = 3` (`:37`) | unchanged shape; instances carry `P` |
| unindexed angular (`dR(jets, eles)`, OPEN-1 min-pair) | one `AngularSep` over `Whole(coll)` legs | `p ≥ 1 ∧ atom`. This is what restores **N3** |
| composite existence, `try_comb_existence` `:1276` | size atoms only | unchanged (`Size` is total) |
| opaque scalar / reducer, `opaque_atom` `:1729` | `O ⋈ c`, `O` free | `p_O ≥ 1 ∧ O ⋈ c` |
| trigger, `simple_atom` via `boolean` `:757-762` | `trig(t) = 1` | unchanged (total) |
| negation, `guarded_not` `:570` | `Dual{plus: widen_unsafe(¬f), minus: ¬f}` | **DELETED.** `HirRegionStmt::Reject` (`:689`) and `HKind::Not` (`:740`) call `Formula::not` directly. Keep both peephole canonicalizations (`reject not X → select X`, `not not c → c`) — they are cheap and keep `guarded_negation_is_rewrite_invariant` meaningful |
| `widen_unsafe` `:610`, `negation_safe` `:636` | — | **DELETED** (no other caller) |

### 3.6 Invariant E-i (mechanically checkable)

> In any `Formula` produced by `encode_region`, every `LinAtom` whose term
> set contains a `may_be_absent` quantity `q` is either (a) a conjunct of
> an `And` that also contains `p_q ≥ 1`, or (b) a disjunct of an `Or` that
> also contains `p_q < 1`.

`Formula::not` preserves (a)↔(b) by De Morgan; `fand`/`forr`
(`encode.rs:132,150`) preserve it because they only flatten same-kind
nodes and drop `True`/`False`. Checked by test I-1 (§11).

---

## 4. Axiom catalog changes

### 4.1 The guard chokepoint

`Emit::guarded` (`adl-axioms/src/lib.rs:574-598`) already builds
implications as `QFormula::Or([¬guard…, fact])` over element-existence
floors. Extend it with presence:

```rust
// after the existence-floor loop (:577-586), before `parts.push(fact)`
for &(_, q) in terms {
    if !self.hir.table.may_be_absent(q) { continue; }
    let p = self.hir.table.intern_quantity(Quantity::Present(q));
    parts.push(Self::atom(&[(1.0, p)], Rel::Lt, 1.0));   // p < 1  ⇒  fact vacuous
}
```

and **route every element-touching family through it**. This is the step
that makes Lemma E's clause (E-ii) hold and deletes the "pad-with-0
satisfies the fact" premise (`lib.rs:24-33`) outright.

| family | today | after | note |
|---|---|---|---|
| **ORD** `:674-727` | `guarded` (size only) | `guarded` (size **+ presence** of both `pt` legs) | closes the latent hole: the loader's skip-without-reset (`event.rs:538`, and the skill's "do not fix this into a reset") admits `[no-pt, pt=100]`, where pad-0 violates `pt(C[0]) ≥ pt(C[1])` |
| **IDOM** `:950-979` | `guarded` (size only) | `guarded` (+ presence of `pt(F[i])`, `pt(P[i])`) | same |
| **EPRED** `:911-947` | hand-rolled `Or([size(F) ≤ i, pred])` at `:937-939` | same, **plus** the derived presence facts of §4.2 | the biggest precision lever |
| **NNEG** (`ElemProp` arm `:809-812`) | **unguarded** bare atom | route through `guarded` | pad-0 satisfied `q ≥ 0`, so this was "sound by accident"; under §2 the junk value must be free |
| **TAG** (`ElemProp` arm `:880-882`) | **unguarded** `Or([q==0, q==1])` | route through `guarded` | **this is running example P1.** Note the fix does *not* depend on it: `p_q ≥ 1` in B's cut is undischargeable by any axiom, since no axiom mentions `p`. Guarding TAG is defence in depth for Lemma E |
| **DPHI** `:857-874`, **TWIN** `:898-908` | unguarded | route through `guarded` | `AngularSep` legs are element-dependent (`existence_floor` `quantity.rs:465-472`) |
| **SZ0, SUB, UNI, SZSLICE, SZPERM, COMBSIZE** | bare | **unchanged** | `Quantity::Size` only — total |
| **TRIG** `:839-854` | bare on `ExternalFn` cos/sin | **unchanged** | `−1 ≤ cos ≤ 1` holds for any junk too; and no cut can use it without asserting `p` |
| **NNEG** (MET / ht / dR / ext arms) | bare | dR and the `NNEG_EXTFN_KEYS` (`:806`) arms get presence guards; MET/ht arms unchanged (total) | |
| **XSUB / XEQ** (`engine.rs:1179-1215`) | `derived_size_le` over sizes | **unchanged** | sizes are total; see §8 for the derivation frame |
| **PRES** (new) | — | `And([p ≥ 0, p ≤ 1])` per interned `Present(_)` | non-load-bearing normalization |
| **PDEF** (new) | — | `Or([p ≤ 0, size(coll) ≥ floor+1])` | presence ⇒ element existence |

`AxiomId` (`:51-74`), `AxiomId::ALL` (`:100`), `as_str` (`:78`) and
`catalog()` (`:139-292`) gain two rows; `catalog_is_complete_and_audited`
(`:1688`) then forces a written justification + assumption tag for each.

**Precision claim.** Guarding costs (almost) nothing, because every *cut*
over `q` asserts `p_q ≥ 1`, which discharges the guard by unit
propagation in the very frame where the axiom is needed. Concretely,
`select pt(jets[0]) < 0` still proves EMPTY: `A⁺ = size>0 ∧ p ≥ 1 ∧ pt<0`,
and `p ≥ 1` kills the `p < 1` disjunct of guarded NNEG, leaving
`pt ≥ 0 ∧ pt < 0` ⇒ UNSAT. What is lost is exactly the case where an
axiom relates a pinned quantity to an **un**pinned one (e.g. ORD across a
`reject`ed index) — and that loss is the *correction*, not a regression.

### 4.2 EPRED-derived presence (the precision recovery)

EPRED already says "elements of a filtered `F` satisfy `F`'s predicate"
(`Or([size(F) ≤ i, predF(F[i])])`, `:937-939`). If the predicate is
false on every element lacking property `π`, then membership in `F`
**proves** `π` is present:

```
size(F) > i  ⇒  p_{ElemProp{F, FromFront(i), π}} ≥ 1
```

encoded as `Or([size(F) ≤ i, p ≥ 1])`, family **EPRES**, emitted
alongside each EPRED instance.

Soundness rests on a conservative syntactic predicate:

```rust
/// Is `pred` false on every element that lacks `prop`? Fail-closed:
/// `false` for anything not in the recognized shape.
fn requires_present(pred: &HNode, prop: PropId) -> bool
```

- `HKind::And(v)` ⇒ **any** conjunct requires it.
- `HKind::Or(v)` ⇒ **every** disjunct requires it.
- `HKind::Not(_)`, `HKind::Ternary{..}` ⇒ **`false`** (absence makes a
  negation *hold*; a missing ternary branch is `true`, `eval.rs:1271-1278`).
- `HKind::Cmp{..}` / `HKind::Band{..}` ⇒ `true` iff `prop` occurs in the
  expression tree **and** that tree consists only of
  `Num | Bool | ElemSelfProp | Neg | Abs | Binary{Add,Sub,Mul,Div}`.
  Justification: on that grammar the interpreter's arithmetic is
  soft-non-value **absorbing** (`eval.rs:1131-1152`), so an absent `prop`
  forces the comparison false. `Reduce`, `ScalarMinMax`, `RegionPred`,
  `CollProp`, `Quantity`, opaque externals ⇒ `false`.
- everything else ⇒ `false`.

This recovers the corpus: `object jets take Jet select pT > 30` yields
`size(jets) > i ⇒ p_{pt(jets[i])} ≥ 1`, so ORD/IDOM over `jets` discharge
their presence guards exactly where they used to fire, and every
downstream `pt(jets[i])` cut is free.

Emitted only for `ElemIndex::FromFront(i)` (mirroring EPRED's own
restriction at `:913-926`), and never for the generic element (§8).

### 4.3 Linearization: implication via a 0/1 indicator

**Every guarded fact is a plain `QFormula::Or` — there is no big-M
anywhere in this design.** This is deliberate:

```
guard g ⇒ fact F      encodes as      Or([¬g, F])
```

with `¬g` materialized as a literal (`size(C) ≤ i` for existence,
`p_q ≤ 0`… — written `p_q < 1` for uniformity with §3.2 — for presence).
Big-M (`F + M(1−p) ≥ …`) would require a finite bound on every quantity's
range; `pt`, `ht`, `MET` and every opaque scalar are unbounded, so any
big-M constant would be either unsound (too small) or numerically
useless. The disjunctive form is exact, needs no constant, and is already
the shape `Emit::guarded` produces, `adl-solver` emits, and
`adl-certify`'s `CertNode::Split` (`adl-certify/src/certificate.rs:107`)
replays.

Cost is one extra disjunct per guarded fact per possibly-absent quantity —
typically 1, at most 2 (ORD/IDOM relate two element properties).

---

## 5. Solver and certifier

**Solver.** `Present(_)` falls through `engine.rs:243-249` to
`QSort::Real`. All presence atoms are single-term `LinAtom`s with
`Rel::Ge`/`Rel::Lt` and constant `1` — inside the existing QF_LRA
fragment, no new theory, no new sort. The subprocess backend's
undeclared-defaults-to-Real path (`adl-solver/src/subprocess.rs:93`) is a
harmless second net.

**Farkas certificates: no structural impact.** `certify_unsat`
(`adl-certify/src/lib.rs:186`) consumes `&[QFormula]` and knows nothing
about quantity provenance; a presence literal is one more row
`1·p ≥ 1` / `1·p < 1` in a Fourier–Motzkin leaf, and a contradiction
between them is the trivial Farkas combination `λ = (1,1)` yielding
`0 > 0`. Because we chose inequalities over `Rel::Eq`/`Rel::Ne` (§3.2)
there is no equality-splitting and no disequality case.

**The real risk is branch count, not shape.** Each guarded axiom and each
negated leaf is a disjunction, so `Searcher::refute` sees more `Split`
nodes. `Budget::default()` is `max_branches: 100_000, max_atoms: 128`
(`adl-certify/src/lib.rs:118-137`); over-budget is `Uncertified`, which
demotes `ProvenDisjoint → CandidateDisjoint` (`engine.rs:646-657`) —
fail-closed, never wrong, but a visible regression if it fires.
Mitigations, in order: (1) unit-propagate presence literals before the
split search — a presence literal appearing positively in the frame
immediately kills the matching `p < 1` disjunct of every guarded axiom,
which is a pure simplification in `adl-certify/src/saturate.rs`;
(2) if (1) is insufficient, raise `max_branches`. Acceptance criterion
for the migration: **the `certified: true` count over the corpus must not
fall** (§9 step 6).

**Bundle schema.** `CombineBundle` carries `(Vec<(String, QFormula)>,
Certificate)` (`engine.rs:639-644`); presence atoms ride inside the
`QFormula`s with no schema change. Two presentation changes:
`adl_axioms::quantity_label` (`adl-axioms/src/lib.rs:444`) gains a
`Present(inner)` arm rendering `defined(<label of inner>)`, and the JSON
schema version bumps because verdict *reasons* and unsat cores can now
name a presence fact. `AxiomId` gains `Pres`, `Pdef`, `Epres` in the
axioms-used list (`engine.rs:354-362`).

---

## 6. Witness realization

`build_event` (`adl-analysis/src/witness.rs:324-629`) currently makes
presence decisions by *cardinality only* — the `Vec` length at `:435-438`
— and then **unconditionally** fills a fixed 7-key standard property set:
the pT fill at `:477-490` (defaulting to `Rat::from_i64(50)`) and the
`const_defaults` array at `:538-545` (`eta`, `phi`, `m`, `btag`, `ctag`,
`tautag`, all `0`). Consequence: **the realizer can never produce an
absence event**, which is precisely why the Phase-A restore markers had
to be pinned instead of validated.

Required change, in `build_event`:

1. **Read the presence decisions first.** Before the model-pin loop
   (`:442-454`), collect `absent: BTreeSet<(CollectionId, u32, PropId)>`
   from every `Quantity::Present(ElemProp{..})` in `mentioned` whose
   model value is `< 1`; and `required` from those with value `≥ 1`.
2. **`p ≥ 1` ⇒ write the property** with the model value for the inner
   quantity if pinned, else the existing default/fill. Unchanged behavior.
3. **`p < 1` ⇒ OMIT the key**, and mark it in the `pinned` set
   (`:435-438`) so the repair pass (`:458-468`), the pT fill (`:477-490`),
   `realize_angulars` (`:517`) and the `const_defaults` loop (`:546-557`)
   all treat it as *taken* and never write it back. This is the one place
   where "pinned" must mean "decided", not "has a value".
4. **Conflict is a realization failure, not a silent fix.** If step 3
   would omit `pt` on an element that the pT-descending sort (`:560-581`)
   or a `p ≥ 1` pin also needs, return `Err` from `build_event`; the
   caller's retry loop (`engine.rs:739-786`, `MAX_WITNESS_ATTEMPTS = 6`
   at `:94`) then adds a blocking clause and re-solves, and a persistent
   failure demotes to POSSIBLY. Never fabricate.
5. **The loader must accept the result.** `validate_pt_descending`
   (`adl-interp/src/event.rs:538`) skips pt-less elements without
   resetting `prev`, so an omitted `pt` is loader-legal — but the emitted
   JSON must simply lack the key, which `EventObject { props: BTreeMap }`
   (`event.rs:39-41`) represents natively. `event_to_json` (`witness.rs:167`)
   and the diagnostic serializer at `:583-628` iterate the map, so
   omission is already the correct behavior once the key is never inserted.

**Rat interaction (post-M3).** Nothing changes: presence is a *structural*
decision (key present / absent), orthogonal to the value type. Values
still flow model → `Rat` → `EventObject::from_props` with no f64
round-trip (`witness.rs:602`); the model's `p` value is compared against
`Rat::one()` and then discarded.

**Closing the loop.** `validate_witness` (`witness.rs:57`) re-runs
`eval_region_membership_idx` (`:87`) on the realized event. An absence
witness that the interpreter rejects is `Rejected` and retried — so the
SAT side keeps its full safety net *and* gains the ability to witness the
absence-driven overlaps that today can only be POSSIBLY. This is the
mechanism that converts running example **N1** from "fail-closed
POSSIBLY" into "POSSIBLY with a real, printable near-witness".

---

## 7. Interval layer

`IntervalMap::spine` (`adl-analysis/src/interval.rs:128-171`) needs
**no structural change**. Three observations make that precise.

**(a) A presence literal is an ordinary single-quantity atom.** The
slice pattern `let [(c, q)] = a.terms() else { return }` (`:141`) matches
it; `Rel::Ge` → `tighten_lo(1, false)` (`:162`), `Rel::Lt` →
`tighten_hi(1, true)` (`:159`). So `p ≥ 1` records `[1, ∞)` and `p < 1`
records `(−∞, 1)`. `Iv::disjoint_from` (`:58-93`) then reports the pair
disjoint on the `lo == hi && (lo_strict || hi_strict)` branch (`:90`).
Meaning: *"one region requires the quantity present, the other requires
it absent"* is a sound, solver-free disjointness proof — a **new**
capability, and one that cannot be wrong (an event either has the value
or it does not).

**(b) A region that does not commit to presence contributes no bound —
automatically.** Under §3.5 a `reject`/`not` leaf becomes
`Or([p < 1, ¬atom])`, and `spine` skips `QFormula::Or` entirely
(`:139`, "ignoring it is sound"). So the negated region records neither
`p` nor `q`. There is no way for a `q`-bound to reach the spine without
its `p ≥ 1` sibling, because §3.6's invariant E-i puts them in the same
`And`.

**(c) L5 still holds and gets stronger.** Every recorded bound is a
conjunct of `R⁺`, hence a necessary condition of membership; the presence
bounds are necessary for the same reason (L1′-upper). Nothing is added to
the map that is not entailed.

**Belt-and-braces guard (specified, cheap, zero measured cost).** Amend
`IntervalMap::disjoint_with` (`:192-201`) so that when the witnessing
quantity `q` satisfies `may_be_absent(q)`, the pair is accepted **only
if both maps also record `p_q` with `lo ≥ 1`**:

```rust
for (q, a) in &self.by_quantity {
    let Some(b) = other.by_quantity.get(q) else { continue };
    if !a.disjoint_from(b) { continue; }
    if table.may_be_absent(*q) && !(self.pins_present(*q) && other.pins_present(*q)) {
        continue;                       // E-i violated somewhere: refuse the shortcut
    }
    return Some((*q, a.clone(), b.clone()));
}
```

This turns invariant E-i from a code-review property into a runtime
check on the *one* path with the narrowest premise set (P1+P2 only,
proof §5.1) and no certificate. It costs nothing in precision: by (b), a
region that fails the check contributes no `q`-bound in the first place.
`IntervalMap` gains a `&QuantityTable` (or a precomputed
`BTreeSet<QuantityId>` of absent-capable ids) — `disjoint_with` is called
from exactly one place, `engine.rs:580-591`.

`Rel::Ne` continues to be dropped (`:167`) and multi-term atoms continue
to be skipped (`:141`); presence literals are never either.

---

## 8. Reconciliation and generic-element lowering

`reconcile::lower` (`adl-analysis/src/reconcile.rs:241-257`) grounds each
filter predicate onto one shared generic element via
`encode_elem_pred_generic` (`adl-axioms/src/lib.rs:1212`) at
`GENERIC_INDEX = u32::MAX` (`:1192`). Three requirements:

1. **The generic frame gets presence literals too.** `φ_A(g)` and
   `φ_B(g)` are built by the same predicate encoder, so a cut
   `select pT > 30` lowers to `p_{pt(g)} ≥ 1 ∧ pt(g) > 30`. The XSUB
   query `UNSAT(φ_A(g) ∧ ¬φ_B(g))` is unaffected in the common case:
   `¬φ_B = p < 1 ∨ pt ≤ 25`, and `φ_A`'s `p ≥ 1` kills the first
   disjunct. Derivations that survive today survive unchanged.
2. **It fixes a real asymmetry.** `A: select BTag == 1` vs
   `B: select pT > 30` currently compares two predicates over *different*
   properties with no notion of which one an element must have. With
   presence, `φ_A ⇒ φ_B` correctly fails (`p_{pt} ≥ 1` is underivable
   from `p_{btag} ≥ 1`), and the previously-silent case
   `A: select pT > 30 and BTag == 1` ⇒ `B: select pT > 25` still proves.
3. **The generic element must still receive NO axioms.** Guaranteed by
   the existing phase ordering (`emit_axioms` at `adl-analysis/src/lib.rs:212`
   runs before `Engine::reconcile` at `engine.rs:257`, normatively stated
   at `reconcile.rs:115-118`), which now also means the generic element
   gets no PRES/PDEF/EPRES facts. `ReconEnc::quantities()`
   (`reconcile.rs:94-103`) already walks `formula_quantities(phi_a/phi_b)`,
   so `Present(...)` ids are declared automatically — **no change needed**.
4. **`references_concrete_peer` must recurse.** `adl-axioms/src/lib.rs:1236-1248`
   rejects `ElemProp | AngularSep | Size(_)` leaves because a concrete
   peer's id carries guarded base-frame axioms the subset frame never
   replays. Add a `Quantity::Present(inner) => recurse on inner` arm:
   `p_{pt(Jet[1])}` is exactly as much of a leak as `pt(Jet[1])`.

XSUB/XEQ facts themselves (`derived_size_le`, `adl-axioms/src/lib.rs:301`)
relate `Quantity::Size` only and are unchanged.

---

## 9. Migration plan

Each step is independently green (full workspace suite + corpus gate) and
independently committable. No step may be skipped or reordered: steps 1
and 2 make the change *observable* before it is *made*.

**Step 1 — absence-visible gate (≈120 LOC, no verdict change intended).**
Extend the probe/battery generators so the gate can see absence:
`adl-interp/src/sample.rs` — make `COLLS` (`:81-87`) tag emission
probabilistic in `event_json` (`:161-163`) and add an
`obj_absence_json(coll, missing_key)` builder alongside
`obj_boundary_json` (`:203`); `adl-analysis/src/refute.rs` — add an
absence family to `probe_events` (`:126`) covering, per collection,
one event per {`pt`, `eta`, `phi`, `btag`, `ctag`, `tautag`}-omitted
element, budgeted under `MAX_REFUTE_PROBES` (`:23`).
*Expected:* running example **P1** flips from silently-shipped to
`refutations: 1` and the corpus PROVEN DISJOINT count may **drop** —
every drop here is a previously-invisible false PROVEN and must be
individually recorded in the commit message.

**Step 2 — IR plumbing (≈250 LOC, zero verdict change).**
`Quantity::Present(QuantityId)` + `QuantityTable::may_be_absent` +
`existence_floor` arm (`adl-sema/src/quantity.rs`); `ir_census` arm;
`Merger::remap_quant` arm (`adl-sema/src/merge.rs:307`);
`adl_axioms::quantity_label` arm (`:444`); `Interp::quantity` arm
(`adl-interp/src/eval.rs:1634`) returning `1`/`0` from the inner
`NumRes`; `build_event` presence handling (§6). **Nothing interns a
`Present` yet**, so every verdict is bit-identical. Verify: full suite,
corpus tuple unchanged.

**Step 3 — encoder chokepoint (≈200 LOC).** `presence_guarded` in
`atom_of` (`encode.rs:1950`) + the hoists of §3.5 (band-`Out`, ratio,
abs, scalar min/max). `guarded_not` stays. *Expected:* over-sides
strengthen (`p ≥ 1` is a genuine necessary condition), so PROVEN DISJOINT
may rise slightly; **`subset` results may fall** — that is running example
**P1** being fixed at the derivation level rather than by the gate.

**Step 4 — axiom presence guards (≈180 LOC).** `Emit::guarded` extension;
route NNEG/TAG/DPHI/TWIN element arms through it; add PRES + PDEF with
`AxiomId` rows and catalog justifications. *Expected:* small precision
dip, recovered by step 5.

**Step 5 — EPRED-derived presence (≈150 LOC).** `requires_present` +
EPRES emission (§4.2). *Expected:* the step-4 dip closes; filtered
collections regain full ORD/IDOM strength.

**Step 6 — delete the Phase-A hedge and flip the pins (≈80 LOC deleted).**
Remove `guarded_not` (`encode.rs:551-603`), `widen_unsafe` (`:605-630`),
`negation_safe` (`:632-650`); `Reject`/`Not` call `Formula::not`. Then
flip **the three restore markers**, each of which names this phase in
its own comment:

| # | file | current pin | after |
|---|---|---|---|
| R1 | `examples/golden/features-angular_06.adl` header (lines 1, 7-14) | `# GOLDEN RCleaned ROverlapReq POSSIBLY` | `PROVEN DISJOINT`; delete the "flip the pin back" paragraph |
| R2 | `crates/adl-analysis/tests/golden_battery.rs:189-216` `not_tag_complementary_regions_fail_closed_pending_definedness` | `assert_eq!(p.kind, VerdictKind::PossiblyOverlapping)` | rename to `not_tag_complementary_regions_are_disjoint`; assert `ProvenDisjoint`; keep the `assert_ne!(ProvenOverlapping)` guard |
| R3 | `crates/adl-difftest/tests/regressions.rs:238-241` (CE-7) | `assert_eq!(s1.empty_ra, EmptyStatus::NotProven)` | `EmptyStatus::Proven`, and re-assert the disjoint-tier kind the comment says was suspended |

Also re-run the **18-proof recovery**: the withdrawn set is the pairs
that moved to POSSIBLY on 2026-07-25. Baseline arithmetic — corpus sweep
(June 2026) recorded **844** PROVEN DISJOINT over 1 832 pairs
(`docs/archive/specs/CORPUS_SWEEP_REPORT.md`); the Phase-A note records
**18 of 834** withdrawn, i.e. ≈**816** today. Target after step 6:
**≥ 834**, with every non-returning pair individually justified.

*Expected verdict-tuple movement, and the one surprise to plan for:*
several of the 18 will return **via the solver rather than the interval
fast path**, because the negated region now contributes an `Or` and
`spine` skips it (§7b). Those pairs gain a Farkas certificate they did
not have before (a strict improvement), but they will **not** return
under `--no-solver`, where the interval fallback caps at POSSIBLY
(`engine.rs:603-609`). Record the split (interval-proven vs
solver-proven) in the A/B, and check the `certified: true` count did not
fall (§5).

**Step 7 — reconciliation + docs (≈60 LOC).**
`references_concrete_peer` recursion (§8.4); update `SPEC_ANALYSIS.md` §1
and §4 with the presence rows; update `SOUNDNESS_PROOF_2026-07-25.md` §3
(P2/P3 boxes), §4 (L1′), §8 item 1 → CLOSED; update the
`adl2-soundness` skill's negation section.

Total ≈ 1 040 LOC added, ≈ 130 deleted, across 9 files in 6 crates.

---

## 10. Risks and rejected alternatives

**R-a. Unknown-for-absent in `eval.rs` — REJECTED.** Evaluating a
comparison over an absent property to `Tri::Unknown` would align the
interpreter with a total prover, but (i) it changes *the meaning* —
SPEC_LANGUAGE §4.4 and the ROOT/C++ NaN convention, against the explicit
2026-07-25 owner decision to keep the NaN semantics; (ii) it introduces a
large new source of Unknown in exactly the position the opaque-masking
invariants police ("prefer a decidable False over Unknown", `eval.rs`
`region3`/`truth3`/`num3` — five verification rounds); (iii) it downgrades
every absence-touching verdict to Candidate tier rather than proving
anything.

**R-b. Input-completeness assumption — REJECTED as the closure.**
Assuming every element carries every referenced property shrinks `E`,
changing what *every* PROVEN verdict means; it is not checkable from ADL
text (so it would need an A1-style banner on every report) and is false
for the reachable inputs the proof document identifies (hand-written and
partially-populated JSONL). It may later be offered as an off-by-default
`--assume-complete-properties` precision knob; it is not the soundness
mechanism.

**R-c. Granularity — CHOSEN: per-`QuantityId`, i.e. per
(collection, index, property).** `Quantity::ElemProp{coll, index, prop}`
already *is* that triple, and it is exactly the interpreter's
granularity: `object_prop` (`eval.rs:1401`) looks up one key in one
object's `BTreeMap`.

Coarser granularities are **unsound**, not merely imprecise. A
per-(collection, property) indicator asserts "either every element of `C`
has `π` or none does" — a constraint the data does not satisfy. It
*removes models*, which makes UNSAT easier, which fabricates PROVEN
DISJOINT. Same for per-collection. Finer than the triple is meaningless.

*Cost.* One extra quantity per possibly-absent quantity, i.e. the
declared-variable count grows by `|ElemProp| + |AngularSep| + |ExternalFn|`.
The formula, for a unit:

```
|Present| = Σ_{C} Σ_{π on C} |indices(C, π)|  +  |AngularSep|  +  |ExternalFn|
indices(C, π) ⊇ {explicit source indices} ∪ {0 … OPEN1_BOUND-1}   (OPEN1_BOUND = 3)
                ∪ {reduce-slice rebased indices} ∪ {IDOM parent mirrors}
```

Static estimate for `examples/CMS/CMS-SUS-16-033_Delphes.adl` (161 lines,
8 object blocks, 13 regions, exactly 2 explicitly-indexed property
references — the rest are unindexed cuts that go through the OPEN-1
expansion): element-accessed collections ≈ 10 (six object blocks plus the
`Jet`/`Muon`/`Electron`/`jets` parents IDOM interns at
`adl-axioms/src/lib.rs:960-969`); distinct element properties ≈ 4
(`pt`, `eta`, `btag`, `d0`), averaging ≈ 2.5 per collection; ≈ 3 indices
each ⇒ **≈ 75 `ElemProp`**, plus a handful of `AngularSep`/`ExternalFn`
⇒ **≈ 80–100 new `Present` variables** against a current declared set
dominated by the same ~80 plus ~20 `Size`. Roughly a **1.8× growth in
declared variables and ~2× in asserted axiom instances** for the largest
realistic file. QF_LRA at that scale is not a concern; per-pair solve
time (3–8 ms today, per the corpus sweep) is expected to stay in the
same order. The one metric to watch is certificate branch count (§5).

**R-d. Certificate branch blow-up.** See §5. Fail-closed (demotion to
CandidateDisjoint), mitigated by presence unit-propagation in
`saturate.rs`, gated by the `certified: true` count in step 6.

**R-e. `may_be_absent` drifting from the interpreter.** The classifier
(§3.4) and `Interp::quantity` (`eval.rs:1634`) must agree about which
quantities can soft-non-value. Enforced by test I-3 (§11) and by the
`ir_census` arm added in step 2 — the same "single source of definedness"
discipline `existence_floor` already carries (`quantity.rs:439-447`,
`adl-axioms/src/lib.rs:566-570`).

**R-f. `guarded_not`'s canonicalization must not be deleted with it.**
`reject not X → select X` (`encode.rs:689-696`) and `not not c → c`
(`:736-739`) predate the guard conceptually and keep
`guarded_negation_is_rewrite_invariant` (and the metamorphic battery)
meaningful. Keep both; rename the test.

---

## 11. Test plan

**New invariants (structural, solver-free):**

- **I-1 (chokepoint).** For every corpus region and every projection, no
  `LinAtom` mentioning a `may_be_absent` quantity `q` occurs outside the
  §3.6 shapes. Implemented as a walker over `Formula` in
  `adl-formula/tests/encoder.rs`; failure names the offending span.
- **I-2 (single construction site).** `Formula::Atom(` appears at exactly
  one place in `adl-formula/src/encode.rs` (grep-style test, mirroring the
  existing `.intern()`-caller-count discipline of P1m).
- **I-3 (classifier/interpreter parity).** For every `Quantity`
  constructor, `may_be_absent(q) == false` implies the interpreter never
  returns `NonValue` for it on any battery event. Property test in
  `adl-interp`.
- **I-4 (negation exactness).** For every corpus region, `encode(reject c)`
  `.is_exact()` iff `encode(c).is_exact()`. Pins the disappearance of the
  `Dual` hedge.
- **I-5 (presence ⇒ existence).** Every emitted PDEF instance evaluates
  true on every battery event (extends `adl-axioms/tests/axioms_hold.rs`,
  which already evaluates instances on generated events).
- **I-6 (EPRES conservatism).** `requires_present` returns `false` for
  every `Not`/`Ternary`/`Reduce`/opaque shape — a table-driven test with
  a `Reject`-shaped row per constructor, so a new `HKind` cannot silently
  become "requires present".

**Oracle / sampler work (lands in step 1, exercised by every later step):**

- `adl-difftest` `casegen.rs`: add an **absence axis** to the generated
  events — per generated case, a deterministic subset of
  `{pt, eta, phi, btag, ctag, tautag}` omitted from a deterministic subset
  of elements. `prop_encoder_vs_interp.rs` then compares encoder and
  interpreter over absence patterns, which is where the whole seam lives.
- `cross_oracle.rs`: same absence axis on the cross-file samplers, so
  XSUB/XEQ derivations are exercised against absence (§8).
- `adl-analysis/src/refute.rs`: the absence probe family (step 1).
- `adl-interp/src/sample.rs`: probabilistic tags in `event_json`; the
  `obj_absence_json` builder.
- Deep gate: `cargo test -p adl-difftest --features deep` (100k cases)
  must be run for steps 3, 4 and 6 — this is the layer that catches an
  encoder/interpreter divergence the unit tests will not.

**Golden pins:**

- The three restore markers R1–R3 (§9 step 6).
- A new golden `examples/golden/presence_*.adl` set pinning: (a) N1 —
  complementary rejects over `BTag(jets[0])` ⇒ **POSSIBLY** (the
  fabricated proof must stay dead); (b) N2/N3 — complement-of-one-predicate
  ⇒ **PROVEN DISJOINT**; (c) P1 — `size(jets) >= 2` vs
  `select BTag(jets[-1]) >= 0` ⇒ **subset NOT proven**, with a comment
  naming this spec; (d) a presence-vs-absence pair proving DISJOINT
  through the interval path alone (§7a), pinned as `solver-less` in the
  same style as the existing S1 pin.
- Corpus A/B captured before step 1 and after each of steps 3–6, per the
  **adl2-corpus-sweep** skill, with the PROVEN DISJOINT / OVERLAPPING /
  POSSIBLY / UNKNOWN tuple, the subset count, and the `certified: true`
  count recorded in each commit message. A rise in PROVEN DISJOINT that
  is not attributable to a named restored pair is a red flag under the
  **adl2-soundness** regression invariant and must be investigated before
  committing.

**Adversarial pass (mandatory, per `adl2-soundness` §(d)).** At least two
rounds of independent skeptics after step 6, tasked with (i) constructing
an ADL input that still fabricates a PROVEN over absence, and (ii)
constructing a genuinely-disjoint pair that the presence model
now downgrades. Both are failures. Every "false PROVEN" claim must first
be run through `smash2 run` on the counterexample event — a non-pT-descending
event is rejected by the loader and is not a valid counterexample.
