# A soundness argument for the smash2 validation engine

*2026-07-25. Companion artifacts: the emission-site audit this instantiates
(17 sites, independently verified against the tree), the cross-file
differential oracle, and the merge-identity invariant suite added the same
day. Status of every claim: PROVEN means proven **given the premise set of
§3**; the premises themselves are discharged by proof where possible and by
named, running test batteries where not. §7 states exactly what is assumed.*

---

## 0. What this document is

A soundness *proof* for a 37k-line analyzer would be a lie if it claimed to
be unconditional. What can honestly be done — and is done here — is:

1. **Fix the semantics** (§1): the reference interpreter is the meaning.
2. **State the claims** (§2): what each PROVEN-tier verdict asserts.
3. **Isolate the premises** (§3): the exact facts the proofs consume.
4. **Prove the lemmas** (§4): the mathematical steps, actually proven.
5. **Prove each verdict sound given the premises** (§5): a case analysis
   over the *complete* enumeration of emission sites — completeness itself
   machine-checked (§5.0).
6. **Map every premise to its discharging argument or auditing net** (§6),
   and state the residual trusted base plainly (§7).

The structure mirrors how the system is built: the engine is small and
provable; the encoder and axiom catalog are empirical and *audited*; the
one previously-unaudited layer (cross-file identity) acquired its net the
same day this document was written.

## 1. Semantics

- **Events.** `E` is the set of loader-valid events: schema-valid records
  whose base collections are pT-descending
  (`adl-interp/src/event.rs::validate_pt_descending` rejects everything
  else, with no allowlist).
- **Meaning.** The reference interpreter `I` gives three-valued membership
  `I(R, e) ∈ {In, Out, Unknown}` (Kleene; `eval.rs` `region3/truth3/num3`).
  *Definition:* event `e` **is in** region `R` iff `I(R, e) = In`.
  The prover is judged against `I`; where they could disagree, `I` wins.
- **Quantity valuation.** Each interned `QuantityId q` denotes a partial
  function `⟦q⟧ : E ⇀ ℚ` (undefined where the event lacks the referenced
  structure). `v(e)` is the induced partial valuation.

## 2. The claims

| Verdict | Claim |
|---|---|
| PROVEN DISJOINT(A,B) | `¬∃e∈E: I(A,e)=In ∧ I(B,e)=In` |
| PROVEN OVERLAPPING(A,B) | `∃e∈E: I(A,e)=In ∧ I(B,e)=In` (the witness) |
| subset A⊆B | `∀e∈E: I(A,e)=In ⇒ I(B,e)=In` |
| EMPTY(R) | `¬∃e∈E: I(R,e)=In` |
| bin-pair disjoint | as DISJOINT for `R∧Bᵢ` vs `R∧Bⱼ` |
| bin coverage | `∀e: I(R,e)=In ⇒ e in some bin` |
| CANDIDATE / POSSIBLY / UNKNOWN | **no claim** (vacuously sound) |

For the candidate tiers, soundness additionally requires they are reachable
only by *demotion* from an attempted stronger claim — never by promotion.
Verified in §5.6.

## 3. The premise set

**P1 — Quantity denotation (structural identity).** Two syntactic
occurrences mapped to the same `QuantityId` denote the same function `⟦q⟧`.
Cross-file sub-premise **P1m**: merging units preserves P1. *This is the
premise violated by both real soundness bugs found this week* — the same
class each time: a **lossy per-unit render string used as cross-unit
interning identity**. (i) 2026-07-24: `<unsupported: …>` element-predicate
renders collided on the merge path — fixed by `ElemPredInterner`
(`adl-sema/src/hir.rs`), the **single** interning discipline (an unsupported
predicate always receives a fresh, never-shared id; exactly two `.intern()`
callers in the workspace). (ii) 2026-07-25, found by the invariant work for
this document and fixed the same day: take-level `take sort(…)` opaque keys
embed unit-local collection ids, so two units' physically different keys
rendered byte-identically and their sorted views fused (`sj[0].pt` became
one variable → false DISJOINT via the interval prefilter, solver-less).
Fixed by unit-ordinal namespacing in `Merger::remap_key`, the same
discipline `QuantityArg::Opaque` already had in `remap_arg`. The general
rule now holds uniformly: **no per-unit render string crosses the merge as
identity un-namespaced** — supported element predicates (whose renders are
faithful, not lossy) are the one deliberate exception, and they are exactly
what cross-file identity is *for*.

**P2 — Leaf faithfulness.** For every *exact*-tagged atom in a region's
encoding and every `e` where `I` decides the source cut, the atom's truth
value under `v̂(e)` equals the interpreter's evaluation of that cut.
(Includes the exact-rational literal semantics: `0.3` is `3/10` on both
sides; the f64-faithfulness guard interns non-faithful additive shapes as
opaque scalars rather than shared atoms.)

> **UPDATE 2026-08-01 (Phase B, presence model): the definedness half of
> P2 is now DISCHARGED BY CONSTRUCTION, not assumed.**
> P2 used to also require "the quantities the atom mentions are *defined*
> under `v(e)`" — an assumption about the event that was **provably false**
> for a property-less element, and the root of the shipping false-claim
> class below. It is no longer a side-condition: every atom over a
> possibly-absent quantity is emitted inside `p_q ≥ 1 ∧ …`, so definedness
> is a CONJUNCT of the leaf. Valuations are now the total
> `v̂ : (QuantityId ∪ PresenceId) → ℚ` with `p̂_q(e) = 1` iff
> `Interp::eval_quantity(q, e)` yields a value, and junk elsewhere.
> What remains of P2 is the residual numeric claim — "when `p_q = 1`, the
> atom's truth equals the interpreter's" — which the exact-rational `Rat`
> core already discharges. The structural side (that the encoder really
> emits those shapes) is machine-checked by invariant **E-i** over every
> corpus region (`adl-formula/tests/presence_invariants.rs`, 447 regions),
> not argued.
>
> The definedness footprint is the SOURCE's, not the folded atom's:
> `pT(j[0]) − pT(j[0]) < 25` folds to `0 < 25` but the interpreter's
> arithmetic is soft-non-value ABSORBING, so it is FALSE on a pt-less jet.
> `LinExpr.mentioned` never cancels, and that is what closed a live false
> `subset` in the shipping golden corpus (`features-num_09.adl`).

**P3 — Axiom truth.** Every catalog instance asserted into a solver frame
holds of `v(e)` for every `e ∈ E` (each family carries a written physical
justification and an assumption tag; the pT-ordering family is sound
because the loader *rejects* non-pT-descending events).

> **Discovered seam (2026-07-25, by the cross-oracle work). Broader than
> first recorded — see the escalation note at the end of this box.**
> P3 quantifies over `v(e)` — but `v` is *partial*, and the interpreter's
> semantics for an **absent property** is a decidable `false` for every
> comparison over it: for a btag-less electron, `BTag(e) ≥ 0` and
> `BTag(e) < 0` are both false, so excluded middle fails and no rational
> valuation models such an event. TAG (`btag ∈ {0,1}` for an existing
> element) is nonetheless instantiated there, and the prover can derive a
> false SUBSET from it (repro: `select BTag(eles[-1]) >= 0` as a region a
> `size ≥ 2` region is "proven" inside). Today the default-on sampling gate
> refutes and withdraws exactly this claim (`refutations: 1` observed; with
> `sample_gate: 0` it would ship). Closing it for real means either
> Unknown-for-absent in the three-valued layer (an `eval.rs` change gated
> on the full opaque-masking battery — that layer's invariants cost five
> verification rounds) or an explicit input-completeness assumption on `E`
> plus axiom instantiation guarded by property presence. §8 item 1.
>
> **RESOLVED 2026-08-01 by the presence model (Phase B).** P3 is now
> stated over `v̂` and discharged: every element-touching family (ORD,
> IDOM, EPRED, NNEG, TAG, DPHI, TWIN, TRIG) is emitted through
> `Emit::guarded`, which adds a `defined(q) < 1` disjunct per
> possibly-absent term, so a guarded instance is VACUOUSLY TRUE wherever
> the interpreter obtains no value. The "canonical pad-with-0 extension
> satisfies the fact" premise is **deleted**, not weakened — which also
> closes its known counterexample: the loader admits `[no-pt, pt=100]`
> (`validate_pt_descending` deliberately skips pt-less elements without
> resetting the chain) and pad-0 violates `pt(C[0]) ≥ pt(C[1])` there.
> Two new families carry their own justifications: **PRES**
> (`0 ≤ defined(q) ≤ 1`) and **PDEF** (`defined(C[i].x) ⇒ size(C) > i`), and
> **EPRES** derives presence from membership in a filtered collection
> through a fail-closed syntactic recogniser. `axioms_hold` now evaluates
> every instance over the absence battery as well.
>
> Measured cost of the guarding: ZERO corpus proofs — the spec's own
> precision argument (every cut over a possibly-absent quantity asserts its
> presence, discharging the guard by unit propagation in the very frame
> where the fact is needed) confirmed on 1898 pairs.
>
> **Escalation (2026-07-25, follow-up verification) — now CLOSED.** The seam is not
> limited to axiom instantiation, and it is not always gate-held. The
> absorbing-false is **anti-monotone under negation**: a `reject`/`not`
> over an absent property evaluates *true* (inner comparison false →
> reject no-op), while its classical encoding `¬(q ⋈ k)` constrains a
> total valuation. Two complementary rejects over the same possibly-absent
> property therefore produce a false PROVEN DISJOINT **with no axiom
> involved, through the interval path** — this breaks P2's definedness
> premise (and hence L1-upper) directly. Verified instances: on
> `BTag(eles[0])` the gate refutes it (battery electrons lack btag); on
> `BTag(jets[0])` it **ships** — `proven_disjoint`, `refutations: 0`
> (battery jets carry btag, so the gate is blind), while the interpreter
> accepts a legal btag-less-jet JSONL event into both regions. The gate's
> reach equals the synthetic battery's absence pattern; the CLI never
> exposes `sample_gate: 0`, but that only bounds who can *disable* the
> net, not what it can *see*. This is the first known shipping false
> PROVEN since the merge-path fixes, and item 1 of §8 accordingly rises
> from "decide the semantics" to "required for the soundness claim over
> partial events". Interim scope note: real Delphes-ingested events carry
> the mapped properties, so the reachable inputs are hand-written or
> partially-populated JSONL — but `E` as defined admits them.

**P4 — Refutation validity.** When the engine treats a frame as UNSAT, the
asserted conjunction is truly unsatisfiable over ℚ. Two independent
sources: the solver's verdict, and (default-on, pairwise disjointness) the
certifier's replayed Farkas/Motzkin certificate — the latter *proven* sound
in L2, machine-checked in exact arithmetic.

**P5 — Witness validity.** A PROVEN OVERLAPPING witness is a loader-valid
event that `I` accepts (`In`) in both regions.

**A1 — Documented assumption (not dischargeable from ADL text).** Same
canonical detector-base name = same physical input across files. Consumed
only by XSUB/XEQ; surfaced in every report that uses them.

## 4. Lemmas (proven)

**L1 (Projection sandwich).** For every region `R` with encoding `F` over
the three-valued formula IR, define `R⁺ = F[Unknown↦true, Dual↦plus]` and
`R⁻ = F[Unknown↦false, Dual↦minus]`. Then for every `e` where P1 and P2
hold:

- *(upper)* `I(R,e)=In ⇒ R⁺ true under v(e)`, and
- *(lower)* `R⁻ true under v(e) ⇒ I(R,e)=In` on the exact fragment.

*Proof sketch.* Structural induction over `F`. At exact leaves both
directions are P2. At `∧/∨/¬` the interpreter's Kleene tables agree with
classical evaluation whenever the classical value is decided; replacing
`Unknown` by `true` yields an upper bound of every completion (Kleene
monotonicity), by `false` a lower bound; `Dual` is by its defining
invariant `minus ⊆ R ⊆ plus`, and negation swaps branches
(`formula.rs::not`), preserving the invariant. The IR census makes the
induction total: every `HKind` is classified, and anything unclassifiable
is `Unknown`. ∎

The lower direction is used **only** for subset/coverage inner sides; where
any statement of the inner region is non-exact, its under contains `false`
and the implication is vacuous — the proof then simply cannot be produced
(fail-closed), which is the intended behavior, not a soundness cost.

> **L1′ (2026-08-01, presence model).** L1 is restated over the total
> presence-extended valuation `v̂`, and the LEAF CASE now holds in **both**
> directions with no definedness side-condition:
>
> - *positive leaf* `p_q ≥ 1 ∧ (q ⋈ k)` — (⇒) if `I` decides the cut true it
>   obtained a value, so `p̂_q(e) = 1` and the atom holds by P2; (⇐) the
>   conjunction forces `p̂_q(e) = 1`, so `q` is defined and the atom's truth
>   IS the interpreter's.
> - *negated leaf over a SOFT-absent quantity* `p_q < 1 ∨ ¬(q ⋈ k)`, which
>   is literally `Formula::not` of the above — (⇒) `I` decides the
>   `reject`/`not` true either because `q` was absent (`p̂_q(e) = 0 < 1`) or
>   because it was present and failed; (⇐) symmetric, because
>   `p̂_q(e) < 1` at a REAL event means `p̂_q(e) = 0`, i.e. absent, i.e. the
>   inner comparison soft-falses, i.e. the negation holds. Multi-quantity
>   leaves conjoin one literal per possibly-absent quantity and De Morgan
>   gives the disjunction — exact, because the interpreter's absorbing rule
>   fires on ANY absent operand.
> - *negated leaf over a HARD-absent quantity* (event MET / scalars /
>   trigger flags) is `p_q ≥ 1 ∧ ¬(q ⋈ k)`, NOT the De Morgan form: a
>   missing event datum raises an evaluation error, `¬Unknown` is Unknown,
>   and the event is `In` no such region in EITHER polarity. `Encoder::negate`
>   re-conjoins the presence literal on top of the negation for exactly this
>   reason, and a unit-propagation peephole then drops the contradicted
>   `p < 1` disjuncts (which is also what keeps `reject MET > 100`'s bound
>   on the And-spine for the interval layer).
>
> **Consequence: the negated leaf is `is_exact()`.** `Formula::Dual`
> vanishes from the negation path, so `reject` no longer poisons the
> `Under` projection — measured as 47 corpus regions regaining exactness,
> +28 subset claims, and the return of all 18 disjointness proofs withdrawn
> on 2026-07-25.
>
> **L5 is unchanged and strengthened**: presence literals are additional
> necessary conditions on the And-spine, and `IntervalMap::disjoint_with`
> now REFUSES a refutation over a possibly-absent quantity unless both
> regions pin it present — turning E-i into a runtime check on the one
> path with no certificate and no axiom behind it.

**L2 (Certificate validity).** If nonnegative rational multipliers λ over a
leaf's constraints cancel every variable and yield a contradictory combined
relation (`0 > c`, or `0 ≥ c` with `c > 0`, strictness propagated from any
positively-weighted strict participant — Motzkin's transposition form), the
leaf's constraint set is unsatisfiable over ℚ; and if every branch of a
`Split` over a disjunction refutes its case, the disjunction refutes.
Classical results; the replay kernel checks exactly these conditions in
exact arithmetic (no search), fails closed on shape/depth mismatch, and is
itself adversarially tested (forged-certificate fuzz: arbitrary multiplier
vectors, bogus nodes, wrong arities — 2000×5 cases; sat-by-construction
systems are never certified). ∎

**L3 (Int→ℝ relaxation).** Integer-valued quantities relaxed to ℝ: unsat
over ℝ ⇒ unsat over ℤ. Used only on the UNSAT side; can lose proofs, never
mint them. ∎

**L4 (Interval refutation).** If two atoms over the *same* `q` entail
`⟦q⟧(e) ∈ Iv₁` and `⟦q⟧(e) ∈ Iv₂` with `Iv₁ ∩ Iv₂ = ∅` (exact-rational
endpoints, strictness respected), the atoms are jointly unsatisfiable. ∎

**L5 (Spine entailment).** Every bound recorded by `IntervalMap::add_over`
is entailed by `R⁺`: bounds are read only from single-quantity atoms on the
And-spine (each a conjunct of `R⁺`, hence necessary); `Or`-structure is
skipped, which only *omits* constraints (a weaker map proves less — sound);
the bound `k/c` is the exact rational with the relation flipped on `c<0`
and `c=0` guarded. Since 2026-07-25, `add_over` takes the `Over` type
itself, so polarity here is compiler-enforced, not convention. ∎

## 5. Soundness of every emission site

**5.0 Completeness of the enumeration.** The audit verified by exhaustive
grep + independent sweep of all 14 crates that *every* verdict-setting
assignment lives in `engine.rs`: proven-tier `kind` writes at exactly 4
sites, candidate tiers at 2, `EmptyStatus::Proven` at 2, subset flags at 1,
bin counters at 2, coverage at 1, derived XSUB/XEQ facts at 1, plus 3
demotion writes (gate) — the S1–S15/G1–G3 table. No other crate writes a
verdict. The case analysis below is therefore total.

**5.1 PROVEN DISJOINT — interval path** (S1/S2: `disjoint_with`,
`self_empty`). By L5 both interval maps are entailed by `A⁺`, `B⁺`; by L4 a
disjoint pair of intervals on one `q` refutes `A⁺ ∧ B⁺`; by L1-upper an
event in both regions would satisfy both spines — contradiction. Premises:
**P1, P2 only** (this path consumes no axioms and requests no certificate —
the narrowest premise set, and therefore the path most exposed to a P1
violation; that is precisely where the 07-24 bug and both of its real-corpus
instances surfaced). Net: sampling gate.

**5.2 PROVEN DISJOINT — solver path** (S4). `UNSAT(Ax ∧ A⁺ ∧ B⁺)` with the
frame's asserted formulas; by P3 the axioms hold at any purported shared
event, by L1-upper its valuation satisfies `A⁺ ∧ B⁺` — contradicting P4.
P4 here is doubly sourced: solver UNSAT *and* (default) certifier replay
(L2) of the named core, with demotion to CANDIDATE on any replay failure
(S3). Premises: P1–P4 (+A1 iff an XSUB/XEQ fact appears in the core — the
report's assumption line then says so). Nets: certifier + gate.

**5.3 EMPTY** (S8 interval; S9 solver `UNSAT(Ax ∧ R⁺)`) and **bin-pair
disjoint / coverage** (S10–S13): same arguments with the respective frames;
coverage additionally uses L1-lower on the bins' unders (`⋀¬Bᵢ⁻` inner
side): an uncovered accepted event would satisfy `R⁺` and every `¬Bᵢ⁻`,
i.e. UNSAT proves every accepted event lands in some bin *provided* the
bins are exact — a non-exact bin makes `¬Bᵢ⁻` a tautology and the proof
unobtainable. Fail-closed.

**5.4 PROVEN SUBSET** (S7). Query `UNSAT(Ax ∧ A⁺ ∧ ⋁ᵢ¬uᵢ)` where `uᵢ` are
B's unders (correct De Morgan of `¬B⁻`). If `I(A,e)=In`, L1-upper gives
`A⁺` at `v(e)`; P3+P4 then force `B⁻` at `v(e)`; L1-lower gives
`I(B,e)=In`. Non-exact B ⇒ vacuous `B⁻` ⇒ unprovable, not unsound.

**5.5 PROVEN OVERLAPPING** (S5). Sound **by construction**: the verdict is
emitted only for `Validation::Validated`, i.e. the realized event passed
the loader (∈E) and `I` returned `In` for both regions — the claim of §2
verified directly against the semantics. The only premise is P5's event
validity; no encoder, axiom, or solver fact is load-bearing. This asymmetry
— overlap essentially premise-free, disjointness premise-heavy — is the
design's central trade and the reason the UNSAT side carries all the nets.

**5.6 Demotions cannot promote.** Verified exhaustively: `certify_disjoint`
has one call site, inside the UNSAT branch, and its only effect is
`ProvenDisjoint → CandidateDisjoint`; the gate's three writes are all
strictly weakening, with interpreter errors discarded (`.ok()`) so an error
neither confirms nor refutes; `CandidateOverlapping` (S6) arises only from
a failed validation of an attempted overlap. No code path assigns a
proven tier from a weaker state. ∎

**5.7 Derived XSUB/XEQ facts** (S14) — *the one fact-minting site.* A fact
`size(A) ≤ size(B)` is asserted only after `UNSAT(φ_A(g) ∧ ¬φ_B(g))` over a
shared generic element `g` whose helper quantities carry no axioms
(enforced by build order — verified). Given P1/P2/P4 and A1 (same base ⇒
same element domain), `φ_A ⇒ φ_B` pointwise, so A's filtered list is a
sublist of B's: the size fact follows. Degenerate frames are rejected
(`frame_sat` requires a decidable Sat), ungroundable predicates drop the
candidate, non-detector bases are excluded (the A1 guard). Two residuals
are **documented, not proven**: (i) facts accumulate at the base frame
mid-loop, so later derivations see earlier facts — argued non-interacting
(facts constrain `Size` quantities; derivation frames constrain
generic-element properties) but not machine-enforced; (ii) a certificate
that consumes an XSUB fact proves "UNSAT *given* XR_k", not XR_k itself —
certification does not extend to the fact's own derivation. Both are on
the hardening list (§8).

## 6. Premise → discharge map

| Premise | Discharged / audited by | Character |
|---|---|---|
| P1 (single-unit) | interning is structural by construction; census forces classification | construction |
| P1m (merge) | `ElemPredInterner` single discipline; opaque-arg AND opaque-sort-key namespacing; **merge-identity invariant suite** (20 tests, structural, solver-free, mutation-validated); **cross-file differential oracle**; goldens `opaque-cut-collision` + the sort-key verdict pin; strengthened S1 pin (asserts solver-less) | construction + test |
| P2 | **definedness half: by construction since Phase B** (presence literals in the leaf; invariant E-i machine-checked over 447 corpus regions); numeric half: `adl-difftest` encoder-vs-interpreter property oracle (100k deep gate, now with an ABSENCE AXIS) + exact-rational core by construction + f64-faithfulness guard | construction + empirical |
| P3 | **instantiation guarding: by construction since Phase B** (every element-touching family emits a `defined(q) < 1` disjunct, so the pad-with-0 premise is deleted); the STATEMENTS remain physics: per-axiom justifications + `axioms_hold` event tests (absence battery included) + prohibited list + sampling gate | construction + empirical |
| P4 | certifier replay (L2, proven; kernel adversarially fuzzed) for pairwise disjoint; **bare solver trust for empty/subset/bins** (§8) | proven / partial |
| P5 | the interpreter itself; difftest witness checks | construction |
| L1–L5 | this document; `projections_resolve_unknown_and_dual`; 3 compile-fail doctests (executed, still failing for the right reasons); interval unit tests; `Over`-typed `add_over` | proof + types |
| A1 | not dischargeable; surfaced per report; near-miss advisories point at it | assumption |

## 7. The residual trusted base (what a skeptic must still grant)

1. The **encoder** (P2) and **axiom catalog** (P3) are code and physics,
   audited by large randomized batteries — not proven. This is the honest
   boundary already stated on every `--combine` bundle.
2. **A1** for any cross-file verdict whose core uses XSUB/XEQ.
3. The **interpreter** is the semantics; a bug there moves the target
   itself (mitigated by the uproot/numpy pipeline oracles and the legacy
   golden battery). Since Phase B this includes its DEFINEDNESS behaviour:
   `QuantityTable::absence` must agree with `Interp::eval_quantity` about
   which quantities can fail to produce a value, pinned by invariant I-3
   (`adl-interp/tests/presence_parity.rs`) over a battery that reaches both
   absence kinds.
4. `rustc`, `BigRational`, and the kernel's ~1.1k lines (fuzzed).
5. Verdicts *not* backed by a certificate trust z3's UNSAT (70% of corpus
   PROVEN DISJOINT arrive via the certificate-free interval path — though
   that path's premise set is only P1+P2 — and empty/subset/bin claims are
   solver-trusted; see §8).

## 8. Known gaps this proof does NOT close (hardening list, in order)

1. **The absent-property seam** (P3 box above) — **CLOSED 2026-08-01.**
   Owner decision taken 2026-07-25: keep the interpreter's NaN semantics;
   model definedness on the prover side.
   - **Phase A — landed 2026-07-25, now DELETED.** `guarded_not` /
     `widen_unsafe` / `negation_safe` replaced any negation over a
     possibly-absent quantity with the polarity-split hedge
     `Dual{plus: widen(¬f), minus: ¬f}`. It killed the shipping
     complementary-reject false DISJOINT at the derivation level, at a cost
     of 18 of 834 corpus PROVEN DISJOINT and three re-pinned
     complement-of-one-predicate results. Superseded: the hedge could not
     tell "two complementary rejects, which really do overlap on an absence
     event" from "a complement of ONE predicate, which really is disjoint",
     because neither could be SAID. The presence model says both.
   - **Phase B — LANDED 2026-08-01** (`SPEC_PRESENCE_MODEL.md`).
     `Quantity::Present(QuantityId)` is a first-class IR variant, so
     interning gives `p_q ≡ p_q′ iff q ≡ q′` and P1/P1m carry over
     untouched. Every cut over a possibly-absent quantity encodes
     `p_q ≥ 1 ∧ atom`; the presence set comes from a definedness FOOTPRINT
     that survives algebraic cancellation. `QuantityTable::absence`
     classifies each quantity `Never | Soft | Hard` and is the single source
     for both the encoder chokepoint and the axiom guards, with an
     `ir_census` arm forcing every future variant to answer.
     Discharged: P2's definedness half, P3's pad-with-0 premise, L1's
     side-condition (→ L1′), and the 2026-07-25 escalation in both arms.
     Live false claims killed, each verified through `smash2 run` on the
     counterexample event before and after:
     | | shape | was |
     |---|---|---|
     | K1 | `size(jets) >= 2` vs `select BTag(jets[-1]) >= 0` | `subset` shipped (TAG entails `btag >= 0`) |
     | K2 | `size(eles) >= 1` vs `select abs(D0(eles[0])) >= -5` | `subset` shipped (`abs ⋈ negative` folded to `true`) |
     | K4 | `size(jets) >= 1` vs `select HT >= 0` | `subset` shipped (NNEG on an event scalar) |
     | K6 | `size(jets) >= 1` vs `select MET >= 0` | `subset` shipped (NNEG on MET) |
     | K7 | `features-num_09.adl`: `MET > 60` vs `MET + HT - HT > 50` | `subset` shipped (HT cancelled out of the atom) |
     All five are now UNDERIVABLE with `refutations: 0` — prevention, not
     gate reliance. K4/K6/K7 are a DEVIATION from the spec, which listed
     event scalars as total; the evidence is that they shipped, and that
     the gates provably cannot see them (the interpreter answers Unknown,
     never `false`, so a subset counterexample search can never fire).
   - **What is NOT closed by this.** The premises that remain empirical are
     unchanged in character: A1 (§3), the PHYSICAL truth of each axiom
     family's statement, and the structural claim that the encoder emits
     the shapes L1′ assumes — the last of which is now machine-checked
     (E-i) rather than reviewed.

1b. **The out-of-fragment analogue (NEW, OPEN — found 2026-08-01 while
   validating Phase B).** `Quantity::Size(C)` is classified total, but
   materializing `C` can raise a HARD evaluation error when `C`'s filter
   predicate is out of fragment, so the interpreter decides NOTHING for any
   region reading it. Repro (ships today, `refutations: 0`, both regions
   reported `exact`):

   ```adl
   object jets   take Jet
   object weird  take Jet
     select bdt > 0.5          # `bdt` is not a declared external
   region A  select size(jets) >= 1
   region B  select size(weird) >= 0
   ```

   `subset A within B` is claimed; `smash2 run` gives `A -> PASS`,
   `B -> ERROR: unresolved identifier 'bdt'`, so B is `In` for NO event and
   the claim is false. This is the SAME false-claim SHAPE as K4/K6 (a fact
   discharging a cut over a quantity the interpreter cannot value) but a
   DIFFERENT seam: out-of-fragment construct, not absent data. The fix has
   the same shape too — `absence(Size(c))` must return `Hard` when `c`'s
   filter chain contains an `Unsupported` predicate — but `QuantityTable`
   does not currently carry fragment tags, so it needs the unsupported
   `ElemPredId` set plumbed into the table, and its own corpus A/B and
   adversarial pass. Deliberately NOT folded into Phase B: mixing it in
   would have made the presence-model diff unverifiable.

2. **Certify the other UNSAT shapes** (empty, subset, bins, and the XSUB
   derivation frames): mechanical extensions of `certify_disjoint`'s
   pattern (audit F3), removing bare-solver trust everywhere.
3. **Bins have no post-hoc net** (audit F4): extend the sampling gate to
   bin pairs and coverage.
4. **XSUB/XEQ facts**: assert into a scoped frame (or certify the
   derivation UNSATs) to close §5.7's residuals (audit F2).
5. `certified: null` conflates "interval proof, nothing to certify" with
   "certification skipped" (audit; also the review's coverage finding) —
   split the states so consumers can see which claims carry certificates.
6. Merger and resolver compute the interning render with separate code;
   divergence fails safe (missed sharing), but a parity test would pin it.

---

*Bottom line.* Given P1–P5 (+A1 where flagged), every PROVEN-tier verdict
the engine can emit is sound, the enumeration of emission paths is
machine-checked complete, and demotion is one-way. The premises that are
dischargeable by construction or mathematics are so discharged (P1, P1m,
P4-pairwise, P5, L1–L5); the empirical premises (P2, P3) are exactly the
ones the always-on batteries audit; and as of today the one layer that had
no randomized audit — cross-file identity — has two.*

---

*Addendum, 2026-08-01 (Phase B).* The document's own §8 item 1 is closed.
Definedness is no longer a premise about the event but a conjunct of the
formula: P2 loses its definedness side-condition, P3 loses its pad-with-0
premise, and L1 becomes L1′ with an exact leaf case in both directions.
Five live false claims died with it (K1, K2, K4, K6, K7 — each verified
through `smash2 run` on the counterexample event), and the 18 disjointness
proofs Phase A had to withdraw came back, all certified.

Two things are honest to say alongside that. **First**, closing a seam is
not the same as proving the encoder: what E-i checks is that the SHAPES
L1′ assumes are the shapes the encoder emits, over every corpus region —
which converts a code-review property into a machine-checked one, and
converts P2 from "audited" to "audited on a strictly smaller surface".
**Second**, validating Phase B turned up a NEW open false-claim class of
the same shape through a different seam (§8 item 1b: out-of-fragment filter
predicates make `size(C)` a hard error, while the analyzer treats it as
total). "No known false-claim class" was true of the absent-property seam
for about as long as it took to look one seam over. The honest claim is the
narrower one: the absent-property seam is closed, by construction, with the
machinery to close its analogue already built.*
