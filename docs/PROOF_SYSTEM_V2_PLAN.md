# Proof System v2 — implementation plan

**Date:** 2026-07-01 · **Companion:** `SOUNDNESS_REVIEW_2026-07-01_VALIDATION_SYSTEM.md`
(the evidence), the v2 design discussion (identity-by-construction,
partiality-by-construction, certify-don't-trust, regions-as-folds,
grammar-derived generation).

**Goal.** Convert the five confirmed false-PROVEN bug *classes* from
"fixed case-by-case" into "impossible by construction", and remove the
encoder+solver from the disjointness trusted base — without a rewrite. The
polarity core, interval layer, solver protocol, witness pipeline, and
interpreter are keepers; the work re-founds identity, partiality, and
certification around them.

**Effort legend:** S < 1 day · M = 1–3 days · L ≈ 1 week · XL = multi-week.
Every phase ends green: full workspace suite + clippy + corpus sweep diff
(hand-checked) + the deep oracle where the phase touches UNSAT-side facts.

---

## Phase 0 — Stopgap soundness fixes (live criticals) — **M total**

The review's §5 fix order, unchanged. These are fail-closed patches the later
phases replace with structural guarantees; they land first because S1/S2 are
exposed on corpus-standard idioms today.

| # | Fix | Files | Effort |
|---|---|---|---|
| 0.1 | Taint rule: fresh `ElemPredId` when `has_unsupported()`; `opaque_arg`/`quantity_arg` refuse context-dependent keys (`ElemSelfProp`/`ThisElem`/`ReduceElem`/`Binder`/elem-alias); Unsupported propagates onto external calls with unresolved args; `lin_pred`+`encode_pred_exact` reject element-dependent opaque leaves (second net) | `adl-sema/resolve.rs`, `dump.rs`, `adl-axioms/lib.rs` | M |
| 0.2 | Guard F2B ORD (`k==1,i>=1`) and back-back ORD in EPRED shape (`Or(size<=i, fact)`) | `adl-axioms/lib.rs:640-683` | S |
| 0.3 | ScalarMinMax: unconditional existence guards + non-exact args conjoin their Unknown | `adl-formula/encode.rs:629,1401` | S |
| 0.4 | Region-`sort` taint cascade onto subsequent ordered-quantity statements | `adl-sema/resolve.rs` or `adl-analysis/encode.rs` | S |
| 0.5 | `subst`/`subst_reduce`/`subst_binders` ScalarMinMax arms (crash) | `adl-formula/encode.rs` | S |
| 0.6 | Strict sort-direction tokens; duplicate object/define/region diagnostics | `adl-sema/resolve.rs` | S |
| 0.7 | Regression tests for every reproduced shape (the 8 repro files from the review become golden/behavior tests) | tests | S |

**Exit:** all review repros return POSSIBLY/diagnostic; corpus sweep shows
PROVEN counts only *decrease* (each decrease hand-classified as a
formerly-false proof or an acceptable precision loss); deep oracle green.

---

## Phase 1 — Sampling gate on UNSAT-side verdicts — **M**

The 50-line-idea, productionized. Independent of every other phase; lands
early because it monitors all remaining and future UNSAT-side bugs.

1. **Extract event synthesis** from `adl-difftest` into `adl-interp` (new
   `adl_interp::sample` module: the boundary grid + a deterministic inline
   LCG for the random battery — no `rand` dependency; `toy_events` moves or
   re-exports). Difftest re-imports from there (dependency direction:
   adl-analysis → adl-interp already exists; difftest → both unchanged).
2. **Gate in `Engine`**: after a would-be PROVEN DISJOINT / REGION EMPTY /
   PROVEN SUBSET, evaluate the sampled battery (default ~64 events,
   `AnalysisOptions.sample_gate: usize`, 0 = off) through the interpreter:
   - disjoint: any event with `Ok(true)` in both regions → **internal
     contradiction**: verdict downgraded to POSSIBLY, `internal_diagnostics`
     entry "sampling refuted PROVEN DISJOINT — encoder/axiom bug", exit code
     unaffected (bug channel, not user error).
   - empty: any sampled member refutes; subset: any sampled counterexample.
   - Events with interpreter errors (opaque) count as no-information.
3. Sampled events must pass the loader invariant (pT-descending) — the
   generator already guarantees this; assert it.
4. **Report plumbing:** a `sampling: {events, refuted}` line under
   `--explain`/JSON so certified-vs-monitored status is visible.

**Tests:** unit test with a deliberately-poisoned axiom set (scripted
injection) proving the gate demotes; perf check on the CMS corpus (gate cost
target: < 10% of verify wall time — membership eval is microseconds/event;
only PROVEN verdicts pay).
**Exit:** S3's repro (pre-fix build) demotes at the gate; corpus sweep
verdicts unchanged on the fixed build.

---

## Phase 2 — Partiality by construction — **L**

Definedness becomes part of atom formation; the pad-0 comment-contract dies.

1. **`adl-formula`: guarded constructors.** New chokepoint API:
   - `mk_pred_atom(table, terms, rel, k) -> Formula` — for membership
     predicates: `And(def_guards) ∧ atom`, where `def_guards` are computed
     from the quantity table for every element-dependent term
     (`size(C) > i` for `FromFront(i)`, `size(C) >= k` for `FromBack(k)`,
     recursively through AngularSep anchors).
   - `mk_axiom_fact(table, terms, rel, k) -> QFormula` — for axioms:
     implication form `Or(¬def_any, fact)`.
   - Both intern the needed `Size` quantities themselves (no caller
     bookkeeping).
2. **Quantity definedness metadata.** `QuantityTable` gains
   `def_domain(q) -> DefDomain { Total, Element(coll, index), … }` computed
   at intern time — the single source the constructors and the Phase-0
   `lin_pred` guard both read (Phase-0's ad-hoc check migrates here).
3. **Migrate emitters:** `ord` (all three variants), `idom`, `epred` (its
   hand-rolled guard collapses into `mk_axiom_fact`), `f2b`; `tag`/`nneg`
   element instances. Delete `guard_existence` (encode.rs) — leaf guards now
   come from `mk_pred_atom`; the ScalarMinMax special case disappears with it
   (Phase-0.3 becomes structural).
4. **Enforcement:** `Emit::atom` and raw `LinAtom` construction over
   element-dependent quantities become module-private; a debug assertion in
   the solver-emission path (`declare`/assert walk) rejects any unguarded
   element atom reaching the base frame — the invariant is checked where it
   matters even if a new call site bypasses the constructors.
5. **Meta-test (T2):** `axioms_hold` gains (a) back-index vocabulary and
   (b) a **joint-satisfiability** check — the conjunction of ALL emitted
   instances evaluated under the canonical pad-0 extension on every generated
   event (catches S3-class extension conflicts categorically).

**Exit:** deleting any single guard in a mutation test fails the meta-test;
corpus sweep: expect small PROVEN-count changes only where Phase-0.2/0.3
already moved them; deep oracle green.

---

## Phase 3 — Identity by construction — **L–XL**

Structural keys everywhere; text-rendered identity dies.

1. **`StructKey`: a canonical structural hash for `HNode`.** Fold over the
   closed constructor set: discriminant + children keys + literal *raw text*
   (never f64) + interned ids; spans excluded. Two node classes poison the
   key: `Unsupported` (carries a per-instance nonce → never equal) and
   context-relative leaves (key includes the owning context: the enclosing
   collection/pred id supplied by the resolver — or, where no context exists,
   a nonce). Implemented in `adl-sema` next to `dump.rs`; the render remains
   for *display only*.
2. **Re-key the three interning sites:** `intern_elem_pred`
   (`resolve.rs:897`), `QuantityArg::Opaque` (becomes
   `Opaque(StructKey)` — type change ripples through `quantity.rs`,
   `merge.rs` remap, `dump.rs` display), and reducer-body keys
   (`reduce_body_key` in `adl-formula/encode.rs` — same discipline).
3. **Cross-file merge:** `merge.rs` re-interns by the same structural keys;
   the unit-ord namespacing for opaques stays (nonce keys are per-unit by
   construction). Verify the `cross_opaque_external_same_unit_name_attack`
   family still holds.
4. **The whitelist, documented and pinned:** deliberate merges enumerated in
   `SPEC_ANALYSIS` §identity — dR symmetry, Sum commutativity (Add-only),
   literal raw-text equality, same-structure cross-region/cross-file terms —
   each with an identity-battery test. Everything else: distinct.
5. **Delete Phase-0.1's string checks** (subsumed); fix the `dump.rs:3-7`
   invariant comment to state the real rule ("identical *keys*", with the
   taint/nonce semantics).
6. **Binder contexts:** `ParticleRef::Binder` gains the owning composite's
   identity (review S18) — cheap here since keys are being rebuilt anyway.

**Risk & measurement:** fresh-by-default loses legitimate merges → precision.
Before/after corpus sweep with every PROVEN-count decrease classified; the
golden corpus pins the floor. Expected: near-zero legitimate loss (the
whitelist covers all sound merges the corpus actually exercises — verified in
the review's identity lens).
**Exit:** all five S1/S2 repro shapes structurally cannot collide (asserted
by tests that intern both sides and compare ids); corpus + golden + oracle
green.

---

## Phase 4 — Certification: exact-rational core checking + CANDIDATE DISJOINT — **XL**

Removes encoder/solver/session from the disjointness TCB.

1. **New crate `adl-certify`** (deps: `adl-formula`, `adl-sema::Rat` only —
   no solver, no analysis):
   - Input: the *checked set* — the named assertions of an UNSAT frame
     (region-over formulas + axiom instances, as `QFormula`s), ideally
     restricted to the solver's unsat core (small: 2–10 members).
   - Engine: an exact-rational **DPLL(Farkas)** over the set: case-split on
     `Or` nodes (guarded axioms are two-branch; region overs are shallow
     NNF), and at each conjunctive leaf find nonnegative Farkas multipliers
     yielding `0 < 0` via a tiny exact simplex/Fourier–Motzkin (the LP is
     over the handful of core atoms). Output: a certificate tree
     (branch decisions + multiplier vectors) — replayable and serializable.
   - **Integrality policy:** relax Int sorts to Real. Real-infeasible ⇒
     int-infeasible (sound). Proofs that need integrality (rare; size cuts
     are almost always real-infeasible already) fail certification →
     CANDIDATE tier, counted.
   - Budget caps (branch count, time) — over-budget → uncertified, never
     wrong.
2. **Engine integration:** on UNSAT for disjoint/empty/subset, fetch the
   core (already plumbed), hand the corresponding formulas to `adl-certify`:
   - certified → PROVEN (+ certificate serialized under `--json`/`--explain`:
     this is the seed of the `verify --combine` machine-checkable artifact —
     roadmap rank 10 alignment).
   - uncertified → **`VerdictKind::CandidateDisjoint`** (matrix letter `d`,
     JSON `candidate_disjoint`, schema v3) — mirrors the overlap side's
     candidate tier. `--fail-on` semantics unchanged (disjointness is not a
     gated finding); render legend + summary counts updated.
   - Cores unavailable (solver quirk) → certify against the full frame set
     (slower) or CANDIDATE.
3. **Rollout:** `--certify` opt-in → measure certification rate on the
   corpus (target: >95% of current PROVEN DISJOINT certify) → default-on with
   CANDIDATE demotion once the rate holds; the sampling gate (Phase 1) stays
   as the independent monitor for the certified path too (belt + braces).
4. **Trusted-base statement:** after this phase the disjointness TCB is:
   `adl-certify` (~1–2k lines, exact arithmetic, property-tested against
   random LP instances), the axiom catalog's physical truth (Phase 2's
   meta-test), the quantity-identity layer (Phase 3), and the interpreter.

**Exit:** certification rate report on the full corpus; a mutation test
(corrupt one emitted numeral post-encoding) shows the certificate check
catching what the solver run would have believed; docs: SPEC_ANALYSIS §2
gains the certified/candidate-disjoint rows; TESTING.md updated.

---

## Phase 5 — Regions as folds — **M–L**

The region lowering models statement *sequence*, not a bag of conjuncts.

1. `resolve.rs` region resolution threads an `Env { sort_state:
   Option<(key, dir)>, taint: Option<reason> }` through statements in order.
2. **`sort` becomes semantics, not taint:** a region-level sort interns a
   `Collection::Sorted { source, key, dir }` with *region-local* identity;
   subsequent element-indexed references in that region resolve against it.
   Existing machinery then does the right thing for free: `pt_ordered` is
   false for non-pt/ascending sorts (no ORD), the descend-pt alias gate
   applies where sound, SZPERM links the sizes. Phase-0.4's taint cascade
   remains only for constructs with genuinely unknown semantics.
   *(Note: this narrows SPEC_LANGUAGE §5's "sort semantics unknown" to a
   defined lowering — needs a spec paragraph + sign-off that region sort =
   re-index for subsequent statements, matching CutLang's operational
   behavior.)*
3. `RegionPred`/`Inherit` explicitly modeled as fold steps (they already
   behave correctly; this is documentation + an invariant test that a
   region's encoding is invariant under statement-reorder ONLY when no
   env-mutating statement intervenes).

**Exit:** the S4 repro yields the *correct* verdict (not just POSSIBLY):
ascending-sorted region proves what its real semantics imply; golden pins
added for sorted-region shapes.

---

## Phase 6 — Grammar-derived generation + coverage gate — **L**

The oracle's reach becomes a CI-enforced invariant instead of a hand-curated
vocabulary.

1. **Constructor census:** a build-script/test in `adl-difftest` that
   enumerates `HKind`, `Quantity`, `Collection`, and statement variants via a
   checked exhaustive `match` (compile error when the IR grows) and maps each
   to at least one generator production.
2. **Vocabulary extensions (T1):** binder-arg externals (`dR(j, leptons)`),
   reducer selects/rejects, wrapped element props (`sqrt(pt)`), back-index
   selects, min/max with ternary args, region-level sort, duplicate names,
   slices-of-slices, unions in cuts.
3. **Coverage gate:** each generated batch records which constructors
   appeared; CI fails if any in-fragment constructor (or any
   Unsupported-producing source shape on a curated list) has zero coverage
   across the batch. Weekly deep run (100k) publishes the coverage report.
4. **Oracle strengthening:** `check_sound` already refutes PROVEN DISJOINT
   against sampled events; add the same for REGION EMPTY and subset flags
   (aligning with the Phase-1 production gate); candidate-disjoint treated as
   a non-claim.

**Exit:** re-running the generator against the *pre-Phase-0* binary
reproduces at least S1/S2/S3/S5-class failures automatically (proving the
net now catches the classes we found by hand).

---

## Sequencing & dependencies

```
Phase 0 (stopgaps)  ──────────►  ship immediately, unblocks trust
Phase 1 (sampling gate) ──────►  independent; land right after 0
Phase 2 (partiality)  ─┐
Phase 3 (identity)     ├──────►  independent of each other; either order;
                       │         both replace parts of Phase 0
Phase 5 (region folds) ┘         (5 after 2 is convenient, not required)
Phase 4 (certification) ──────►  independent, but cleanest after 2
                                 (guarded shapes → smaller cores)
Phase 6 (generator)   ────────►  start alongside Phase 0 (T1 subset);
                                 full census gate any time
abs(x)<c precision unlock ────►  after Phase 0 + the T1 generator subset
                                 (its prior prerequisites — reconcile oracle,
                                 golden-cross pins — already shipped)
```

Suggested order of landing: **0 → 1 → 6(T1 subset) → 2 → 3 → 4 → 5 → 6(gate)**,
with the abs() unlock slotting in after 2 (it extends `encode_pred_exact`,
which Phase 2 touches — do it on the guarded API, not before).

## Risk register

| Risk | Phase | Mitigation |
|---|---|---|
| Precision loss from fresh-by-default identity | 3 | whitelist + classified corpus diff + golden floor |
| Certification rate too low (integrality, big cores) | 4 | measure under `--certify` before demoting; Real-relaxation policy; CANDIDATE tier is honest, not wrong |
| Sampling gate wall-time on huge merges | 1 | only PROVEN verdicts pay; cap N; skip when regions share no interpreter-decidable statement |
| Region-sort semantics decision (spec change) | 5 | owner sign-off required before implementing the non-taint lowering |
| Type-change ripple from `Opaque(StructKey)` | 3 | mechanical; merge.rs + dump.rs are the only nontrivial consumers |
| Schema v3 consumer impact | 4 | additive tier + version bump, same playbook as v2 |

## What gets deleted (net simplification)

`guard_existence` and its `is_exact` bypass; the hand-rolled guards in
`epred`; the pad-0 comment-contract (becomes a checked meta-test); the
render-keyed `elem_pred_ids`/`Opaque(String)` identity; Phase-0's string
taint checks (subsumed by structural keys); the "identical text = identical
resolution" doc claim.

## Metrics to publish per phase

- Corpus sweep verdict counts (with every PROVEN delta classified).
- Certification rate (% PROVEN DISJOINT certified) once Phase 4 exists.
- Generator constructor coverage (% of in-fragment IR reachable).
- Sampling-gate refutation count (should be 0 in steady state; any hit is a
  filed bug).
