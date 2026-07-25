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
encoding and every `e` where `I` decides the source cut, the quantities the
atom mentions are defined under `v(e)` and the atom's truth value under
`v(e)` equals the interpreter's evaluation of that cut. (Includes the
exact-rational literal semantics: `0.3` is `3/10` on both sides; the
f64-faithfulness guard interns non-faithful additive shapes as opaque
scalars rather than shared atoms.)

**P3 — Axiom truth.** Every catalog instance asserted into a solver frame
holds of `v(e)` for every `e ∈ E` (each family carries a written physical
justification and an assumption tag; the pT-ordering family is sound
because the loader *rejects* non-pT-descending events).

> **Discovered seam (2026-07-25, by the cross-oracle work; live, gate-held).**
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
| P2 | `adl-difftest` encoder-vs-interpreter property oracle (100k deep gate); exact-rational core by construction; f64-faithfulness guard | empirical, audited |
| P3 | per-axiom justifications + `axioms_hold` event tests + prohibited list + sampling gate | empirical, audited |
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
   golden battery).
4. `rustc`, `BigRational`, and the kernel's ~1.1k lines (fuzzed).
5. Verdicts *not* backed by a certificate trust z3's UNSAT (70% of corpus
   PROVEN DISJOINT arrive via the certificate-free interval path — though
   that path's premise set is only P1+P2 — and empty/subset/bin claims are
   solver-trusted; see §8).

## 8. Known gaps this proof does NOT close (hardening list, in order)

1. **The absent-property seam** (P3 box above): a wrong SUBSET derivation
   exists today, held only by the default-on sampling gate. Decide the
   semantics (Unknown-for-absent vs input-completeness + guarded
   instantiation) and re-run the full opaque-masking battery either way.
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
