# Lasting Soundness Testing System for smash2 Validation Engine

**Goal:** A testing architecture that would have failed the *pre-fix* tree on C1–C6 (especially C4 `MET+0.5` and C5 `MET*0.3`) and that continuously guards the UNSAT side (PROVEN DISJOINT / EMPTY / SUBSET) against encoder↔interpreter divergence.

**Constraints respected:**
- Interpreter is f64; analyzer is exact `Rat`. Exact interpreter was tried and reverted (breaks witness consistency). Do not re-propose Phase 6 exact-interp without a rational event model.
- UNSAT side has no witness oracle by default — false PROVEN is silent unless an external net refutes it.
- Sampling gate + cut-constant pools are a net, not a proof.
- Corpus invariant: PROVEN DISJOINT must not rise unintentionally.

---

## 1. Gap analysis — what existing nets miss and why C1–C6 slipped

### Failure class (one systemic bug)

Encoder folds a comparison operand to an exact `LinAtom` whose boundary is *not* the interpreter’s stepwise-f64 boundary. Complementary exact atoms → solver/interval UNSAT → **PROVEN DISJOINT** about values the interpreter never computes. Certifier correctly certifies the *encoded* formula. Interpreter accepts a shared event. Release-blocking by the README’s own criterion — and silent on the UNSAT side.

### Net-by-net miss reasons

| Net | What it checks | Why C1–C6 slipped |
|---|---|---|
| **Sampling gate** (`battery_with_cuts`, cut ±1 ulp) | PROVEN ⇒ no sampled event in both regions | **Pre-fix:** pools were fixed (`PT_POOL`/`MET_POOL`); off-pool cuts (0.4, 10/3-ish, 1.0 with dual-rounding) got only random draws that never land on half-ulp flat spots. **Post-fix cut pools help C2/C4/C5/C6** (literals appear in source) but **still miss C1** (witness is a *paired* MET+HT solving a dual-rounding equation, not “literal ±1 ulp”) and **C3** (needs catastrophic HT≈2⁶⁰, never in cut pools). Net ≠ proof. |
| **`adl-difftest` property oracle** (`prop_encoder_vs_interp`) | PROVEN ⇒ no *sampled* event contradicts | Generator **cannot emit the shapes**: `SCALE_POOL = [2.0, 0.5, -1.0]` (powers of two — C5’s `*0.3` never appears); no `Scale(Scale(q))` / regrouped mul chains (C1); `QConst` is `q+c` with CONST_POOL including 0.1/0.3 but paired with complementary bare atoms only by chance; `Sum3` cancellation exists but events never hit 2⁶⁰. Oracle is **passive sampling**, not adversarial witness search — even a generated C4-shaped pair would usually stay green because the shared event is one ulp off the grid. |
| **Metamorphic battery** | Verdict *class* invariant under rewrite | Does not invent new arithmetic shapes; preserves whatever casegen emits. Consistency ≠ soundness. |
| **Golden corpus** (`examples/golden/`, `golden_regions`/`golden_cross`) | Pins known ground truth | Grep (audit §10): no active shapes for mul-chains, `Neg(Num)` adds, `abs(multi-op)`, single dyadic add vs bare complement. Goldens pin what authors already understood. |
| **`f64_fold_regressions` / `exact_f64_fold` / CE-8…13** | Lock *known* C1–C6 | **Exist only after the audit.** Perfect regression locks; zero discovery power for the next shape (ratio-clearing residual, new pattern path that bypasses the gate). |
| **Certifier (`adl-certify`)** | Encoded formula is real-unsat | Explicitly out of scope for encoder meaning (README trust table). Certifying harder made C1–C6 *more* confident false PROVENs. |
| **Corpus sweep / PROVEN DISJOINT count** | Count must not *rise* | C1–C6 shapes absent from examples → count stayed stable while the bug shipped. Corpus is a regression detector for *existing* analyses, not a shape fuzzer. |
| **Interval prefilter** | Fast UNSAT on LinAtoms | Amplified C2/C4/C5/C6: wrong atoms ⇒ false PROVEN *without even calling z3*. |

### Skeptical takeaway

The project’s own README claims the differential oracle “guarantees no false PROVEN.” That is false for this class: the oracle guarantees no false PROVEN *among (generated shape × sampled event) pairs that hit the divergence*. C4/C5 are one-op folds the old guard *explicitly allowed*; they are the smoking gun that syntactic faithfulness + random sampling cannot substitute for a semantic fold↔f64 oracle.

---

## 2. Layered test architecture

### L0 — Unit / structural (fast, no z3)

| Suite | Location | Asserts | Cost |
|---|---|---|---|
| **Exact-foldability criterion** | keep + expand `adl-formula/tests/exact_f64_fold.rs` | Quantity-bearing trees flatten iff IEEE-exact steps; non-exact → structure-keyed opaque; const-only → f64-emulate then `from_decimal_f64`; `abs_cmp` never unguarded `lin`; same-shape complementary opaques still unify | ~1s |
| **Fold semantics oracle (pure)** | **NEW** `adl-formula/tests/fold_vs_f64_semantics.rs` | For generated expr trees: `Rat::fold(E)` vs stepwise `f64_eval(E,x)` over adversarial `x`; if they disagree at any finite `x`, encoder must **not** emit a bare `LinAtom` for `E` (must opaque or f64-emulate). Directly encodes §12 revised rule without going through verify | ~2–5s CI / 30s deep |
| **No f64 in rational fragment** | **NEW** `adl-sema`/`adl-formula` lint-style unit tests + optional `rg` CI check | `LinAtom` coeffs, interval bounds, axiom constants are `Rat`; no resurrected f64 fold path in atom construction | <1s |
| **Solver classify fail-closed** | keep `adl-solver` classify tests | `unknown`/`timeout`/`error`/`unsupported` ≠ UNSAT | <1s |
| **Opaque-masking / Kleene** | keep `adl-interp` Tri/`region3` tests | False beats Unknown; soft non-value absorbing | ~1s |

### L1 — Property / metamorphic (existing + targeted extensions)

| Suite | Location | Asserts | Cost |
|---|---|---|---|
| Encoder vs interp (sampling) | `adl-difftest/tests/prop_encoder_vs_interp.rs` | Existing soundness contract on sampled events | 2k cases ~minutes; deep 100k nightly |
| Metamorphic | `adl-difftest/tests/metamorphic.rs` | Rewrite class invariance | similar |
| **Casegen enrichment (necessary but insufficient alone)** | `adl-difftest/src/casegen.rs` | Extend `SCALE_POOL` with non-dyadics `{0.1,0.2,0.3,0.6,1.1}`; add `Scale2(a,b,q)` / `ConstMul(c1,c2)`; `NegConst` form `q + -c`; `AbsSum` over multi-op; **and** attach per-case **derived boundary events** (see L2) into `run_case`’s event set | CI: still 2k; deep: 100k |
| Axiom battery | `adl-axioms` | Every axiom holds on generated physical events | ~seconds |

### L2 — Adversarial boundary oracle (**the load-bearing new net**)

| Suite | Location | Asserts | Cost |
|---|---|---|---|
| **Boundary oracle core** | **NEW crate or module** `adl-difftest/src/boundary_oracle.rs` (+ `tests/prop_boundary_oracle.rs`) | See §3. If `verify` returns ProvenDisjoint/Empty/Subset and oracle finds an interpreter-accepted counterexample event → **hard fail** | CI: ~5–20s (fixed seed, ~2–5k shaped pairs); nightly: 50k–200k + wider search |
| **Pinned adversarial ADL** | expand `adl-analysis/tests/f64_fold_regressions.rs` + `examples/golden/adversarial_f64_*.adl` | C1–C6 + ratio-clearing hunts + any new CE; `# GOLDEN` / explicit `must_not: ProvenDisjoint` + witness JSONL | ~10–30s with solver |
| **Sampling gate stress** | `adl-analysis` unit tests | Gate refutes synthetic false-PROVEN when cut constants / derived boundaries are injected; certified flag cleared on demotion (G2) | <5s |

### L3 — Corpus / golden / CI gates

| Suite | Location | Asserts | Cost |
|---|---|---|---|
| Golden verdicts | `examples/golden/` + `golden_regions`/`golden_cross` | Exact expected kinds | ~1–3 min |
| **Verify corpus gate** | **NEW** `scripts/verify_corpus_gate.sh` (extend corpus-sweep skill) | 68/68 exit 0; PROVEN DISJOINT ≤ baseline (or == pinned snapshot); UNKNOWN=0; no *new* INTERNAL DIAGNOSTICS | ~5–15 min release subprocess |
| Analysis behaviors | `analysis_behaviors.rs`, `soundness_review_regressions.rs` | Contract/reporting honesty | seconds–minutes |

### L4 — Deep / nightly only

- `cargo test -p adl-difftest --features deep`
- Boundary oracle with expanded budgets + ratio-clearing search (§12 residual)
- Full 68-file verify sweep with JSON baseline diff
- Optional: cross-file adversarial (opaque-cut-collision style) with fold shapes

---

## 3. Key idea — Adversarial boundary oracle

### Contract (UNSAT-side witness oracle)

```
∀ generated pair (RA, RB) and ∀ event e found by the oracle:
  if smash2 verify says ProvenDisjoint(RA,RB)
     and smash2-run/interpreter accepts e ∈ RA ∩ RB
  → RELEASE BLOCKER
```

Same for ProvenEmpty (member found) and ProvenSubset (member of A\B found).  
SAT-side stays as today (interpreter re-validation). Do **not** require exact-Rat interpretation of `e`.

### Why this catches C4/C5 on the pre-fix tree

C4/C5 are not “exotic syntax”; they are **one arithmetic op + complementary bare atom**. Generation that *starts from algebraic equivalence / fold identity* and then *searches for f64 disagreement* will emit them by construction — unlike casegen, which samples shapes hoping to land on a bug.

### Generation strategy (concrete)

**Phase A — expression algebra (no solver).**

Work over a tiny vocabulary: `MET`, `HT`, optionally `jets[0].pt`. Build expr ASTs:

```
E ::= Q | Neg(E) | Abs(E)
    | E + E | E - E | E * E | E / E
    | E + c | E * c | c          (c from CONST/SCALE pools + random decimals)
```

Maintain two evaluations for each closed term:

1. **`rat_fold(E)`** — exact `Rat` fold (same rules as `LinExpr` / const fold), producing either `a·Q + b` (linear) or `⊥` (nonlinear / opaque).
2. **`f64_step(E, σ)`** — interpreter-faithful stepwise f64 given assignment `σ: Q → f64`.

**Phase B — pair synthesis (the C4/C5 machine).**

For each linear `rat_fold(E) = a·q + b` with `a ≠ 0`:

1. Pick relation `⋈ ∈ {≤,<,≥,>}` and cut form:
   - **Complementary bare atom** (C4/C5 shape):  
     `RA: E ⋈ k` vs `RB: q ⋈' k'` where `k'` is the *exact* cleared bound (`(k−b)/a` as `Rat`, rendered via shortest decimal / next f64).
   - **Cross-form equivalent** (C1 shape): find `E₂` with `rat_fold(E₂) = rat_fold(E)` but different AST (assoc, `0.06*MET` vs `MET*0.2*0.3`, const mul `0.1*3` vs `0.3`). Emit complementary cuts on `E` vs `E₂`.
   - **Const-only RHS** (C6): `q ≤ (c₁*c₂)` vs `q ≥ fl(c₁*c₂)`.

2. **Classify foldability risk** (optional filter to keep CI cheap): keep pairs where `∃σ. f64_step(E,σ) ≠ rat_eval(a·q+b,σ)` *or* where E contains any non-power-of-two multiply / any add / const mul — i.e. prefer the §12-forbidden fragment. Do **not** only test the forbidden fragment forever; also fuzz “allowed” shapes so a future over-permissive gate fails.

**Phase C — witness search (the part sampling never does).**

Given `(RA, RB)` whose *encoded* atoms look complementary in `Rat` space, search for `σ` such that interpreter membership holds for both:

1. **Bound-anchored candidates.** Let `b★` be every numeric literal appearing in either region, plus `rat`-cleared endpoints rendered to f64, plus `nextafter(b★, ±∞)`, plus midpoints of known flat spots (`0.5 + 2⁻⁵³`, values where `fl(x+0.5)=1.0`, etc.).
2. **Univariate scan for single-q cuts.** For `E(q)` vs bare `q`, binary-search / grid-search `q` in `[0, 1e4]` (and log-spaced to `1e20` for cancellation class) maximizing  
   `indicator(f64_step passes RA) ∧ indicator(f64_step passes RB)`.  
   Practical recipe that finds C4/C5 in milliseconds:
   - Start at the Rat boundary `q₀ = clear(E, k)`.
   - Probe `{q₀, nextafter(q₀,±∞), nextafter², …}` up to ~64 ulps.
   - For adds `q+c ⋈ k`, also probe the known half-ulp witness family: `q = (k−c) + ε` with `ε ∈ {0, ±ulp, ±2ulp}` and check `fl(q+c)`.
   - For muls `q*c ⋈ k`, probe `q = k/c` in both Rat-decimal and `k/fl(c)` f64 senses.
3. **Bivariate for C1/C3.** Fix one variable on a cut, solve the other; for cancellation, set `HT ∈ {2^52…2^60}`, `MET` near the cut. Cap attempts (e.g. 2k probes/pair).
4. **Validate.** Build a minimal JSONL event (`MET.pt`, `HT`, empty collections, valid triggers). Run through `Interp::eval_region_membership` (same path as `smash2 run`). Reject non-pT-descending / invalid events — same discipline as the soundness skill.

**Phase D — verdict check.**

```rust
let report = analyze_source(adl, …);
if matches!(pair.kind, ProvenDisjoint | …) {
    if let Some(e) = search_shared_event(...) {
        panic!("false PROVEN: {adl}\n witness {e}");
    }
}
```

Also assert the **positive control**: on the *pre-fix semantics* (or a feature-flagged “legacy fold” harness used only in a historical replay job), C4/C5 fixtures must fail. On current tree they must *not* return ProvenDisjoint (already in `f64_fold_regressions`). The *generator* must still be able to synthesize C4/C5-shaped ADL so the oracle remains armed after the fix (looking for regressions and residuals).

### Minimal seed corpus the generator must emit every CI run (deterministic)

Hard-include these templates (not only random), so CI fails if the oracle regresses to blindness:

1. `MET+0.5 ≤ 1` vs `MET > 0.5`  (**C4**)
2. `MET*0.3 ≤ 1` vs `MET ≥ 3.3333333333333335`  (**C5**)
3. `MET*0.2*0.3+HT > 1` vs `0.06*MET+HT ≤ 1`  (**C1**)
4. `MET + -0.1 > 0.3` vs `MET ≤ 0.4`  (**C2**)
5. `abs(MET+HT-HT)<50` vs `abs(MET)≥50`  (**C3**)
6. `MET ≤ 0.1*3` vs `MET ≥ 0.30000000000000004`  (**C6**)
7. Same-shape control: `MET+0.5 ≤ 1` vs `MET+0.5 > 1` → **may** be ProvenDisjoint (shared opaque) — assert still disjoint or POSSIBLY, never “both PASS” under run.

### What this is *not*

- Not a proof of foldability (that remains `is_exact_f64_linear`).
- Not an exact interpreter.
- Not “more random events on the same SCALE_POOL.” Enrichment without Phase C would still miss C4 for weeks.

---

## 4. CI vs nightly / deep

### PR / CI (must stay under ~10–15 min wall on the z3-workaround builders)

- `adl-formula`: `exact_f64_fold` + **fold_vs_f64_semantics** (small budget) + encoder unit tests  
- `adl-analysis`: `f64_fold_regressions`, golden_regions (or a fast subset), sampling/certified honesty tests, soundness_review_regressions  
- `adl-difftest`: default 2k prop + metamorphic + **boundary oracle with forced C1–C6 templates + ~1–2k random pairs**  
- `adl-axioms` / `adl-certify` / `adl-solver` classify batteries  
- `smash2 check` corpus gate (parse/resolve) — already cheap  
- Optional: verify-sweep on a **pinned 5–10 file soundness canary** (including new adversarial goldens), not full 68

### Nightly / deep

- `adl-difftest --features deep` (100k)  
- Boundary oracle 50k–200k + bivariate cancellation + **ratio-clearing residual hunt**  
- Full 68-file `smash2 verify` sweep → fail if PROVEN DISJOINT **rises** vs committed baseline JSON  
- Cross-file golden + reconcile oracle deep  
- Historical “would-have-caught” job: if a `legacy_fold` test harness is kept behind `#[cfg(feature = "unsound_fold_replay")]`, nightly proves templates still detect the old bug class (prevents silently deleting the oracle)

### Explicitly out of CI

- Exhaustive ulp sweeps over all f64  
- Full Delphes 20k physics samples as soundness oracles (useful for completeness, not for fold divergence)

---

## 5. Success criteria / invariants to encode as tests

### Hard invariants (fail CI)

1. **I-UNSAT-REFUTE:** No `ProvenDisjoint` / `ProvenEmpty` / `ProvenSubset` when an interpreter-accepted counterexample event exists (boundary oracle + regressions + sampling gate).  
2. **I-C1C6-LOCK:** CE-8…CE-13 fixtures never return `ProvenDisjoint`; witnesses remain members of both regions (`f64_fold_regressions`).  
3. **I-SAME-SHAPE:** Identical arithmetic trees across complementary cuts may still prove disjoint (opaque identity); oracle must not demand POSSIBLY for `x+0.5≤1` vs `x+0.5>1`.  
4. **I-NO-F64-ATOM:** Atom coefficients / interval bounds / axiom constants constructed only via `Rat` / `parse_rat` / f64-emulation-then-rationalize for const-only.  
5. **I-SOLVER-FAIL-CLOSED:** Non-UNSAT solver outcomes never become PROVEN.  
6. **I-CERT-HONEST:** Gate demotion clears `certified`; demoted pairs leave no combine bundle.  
7. **I-CORPUS-DISJOINT:** On the 68-file corpus, `proven_disjoint` ≤ committed baseline (allowlisted decreases OK; **any increase fails**).  
8. **I-GOLDEN:** `# GOLDEN` / `# GOLDEN-CROSS` headers match analyzer output.  
9. **I-WITNESS-SAT:** Every `ProvenOverlapping` has `witness_validated == true`.  
10. **I-ORACLE-ARMED:** Deterministic C4/C5 templates are always executed; the *search* finds the known witnesses (meta-test: oracle infrastructure can rediscover CE-11/CE-12 witnesses even if verdict is already POSSIBLY — assert `search_shared_event` returns `Some` on those ADL files). **This is the meta-invariant that keeps the net from rotting.**

### Soft / nightly

- PROVEN OVERLAPPING / CANDIDATE / POSSIBLY counts tracked; unexplained spikes investigated.  
- UNKNOWN remains 0 on corpus.  
- No new INTERNAL DIAGNOSTICS files.  
- Ratio-clearing: either find a concrete false PROVEN (promote to CE) or keep a “residual budget” test that documents search depth with no hit.

### Anti-criteria (do not encode)

- Exact numerical parity between analyzer Rat and interpreter f64 on witnesses (Phase 6 reverted).  
- “PROVEN DISJOINT must never decrease” (sound fixes decrease it; −22 on the C1–C6 fix is success).  
- Certifier rejects encoder bugs (wrong layer).

---

## 6. Prioritized implementation plan

### P0 — Would have failed pre-fix C4/C5; prevents silent reintroduction (1–3 days)

1. **`boundary_oracle` module** in `adl-difftest`:
   - Forced templates C1–C6 + witness search (univariate ulp walk is enough for C2/C4/C5/C6; special-case bivariate for C1/C3).
   - Assert: `ProvenDisjoint ⇒ search returns None`; **and** meta-assert: search returns `Some` on the six ADL strings (oracle armed).
2. **Keep/extend** `f64_fold_regressions.rs` + CE-8…13 (already done post-audit) — wire into CI job list explicitly.
3. **Casegen:** add non-dyadic scales + `NegConst` + double-scale; inject **derived boundary events** per rendered case into `run_case` (cut literals ±1 ulp + cleared Rat endpoints). Cheap synergy with sampling gate.
4. **Commit verify-corpus baseline** JSON (pairs, proven_disjoint, …) + `scripts/verify_corpus_gate.sh` enforcing I-CORPUS-DISJOINT.
5. **Golden adversarial files** under `examples/golden/` for C4/C5 at minimum (`# GOLDEN` expecting not-disjoint / possibly / overlapping as appropriate).

*Acceptance:* On a temporary revert of `is_exact_f64_linear` to the old `is_f64_faithful`, P0 tests fail on C4 and C5 within seconds. (Do this once as a validation of the net, then restore the fix.)

### P1 — Generalize discovery beyond known CE shapes (3–7 days)

1. Random pair synthesis from Phase A/B (algebraic equivalences under `rat_fold`).  
2. `fold_vs_f64_semantics.rs` unit oracle (encoder must opaque when pure fold diverges).  
3. Expand boundary search: cancellation HT ladder; const-mul trees; `Abs` wrappers.  
4. Ratio-clearing residual hunt (nightly feature).  
5. Sampling-gate unit test: plant a false complementary pair through a test-only hook or historical encoder flag and assert refutation when cut pools include the boundary.  
6. README trust table: replace overclaim “guarantees no false PROVEN” with “sampling + boundary oracle audit the encoder; neither is proof.”

### P2 — Depth, corpus productization, residuals (ongoing)

1. Deep boundary oracle in nightly CI.  
2. Cross-file fold/opaque collision adversarial cases.  
3. Emptiness/subset certification (G1) — reduces solver trust; does **not** replace encoder nets.  
4. Rational event model (true Phase 6) only if residual false-PROVEN class reappears and cannot be opaque’d away.  
5. Optional `cargo fuzz` / AFL on `rat_fold` vs `f64_step` disagreement → feed encoder.

---

## Skeptical notes for the follow-up agent

- **Cut-constant sampling alone is not P0.** It likely catches C2/C4/C5/C6 *after* the audit’s gate fix, but would still have missed C1/C3 and any bug whose witness is not near a source literal (cleared `10/3` rendered as a different decimal than the user’s `1` and `0.3`). The boundary oracle’s Phase C is the difference.
- **Do not “fix” C4 by nudging intervals in f64.** That reintroduces the four-audit defect class. Opaque or refuse the fold.
- **Meta-test I-ORACLE-ARMED is mandatory.** Without it, a future change can “green” the suite by making `search_shared_event` always return `None`.
- **Same-shape opaque disjointness is a feature.** Over-aggressive tests that forbid all PROVEN DISJOINT involving `+` will destroy precision and get disabled — then you have no net.
- **Measuring success on the pre-fix tree** (git stash / feature flag) is the only honest proof the new system works; green-on-post-fix is necessary but insufficient.

---

## File / crate checklist (for execution)

| Action | Path |
|---|---|
| NEW | `reimplementation/adl2/crates/adl-difftest/src/boundary_oracle.rs` |
| NEW | `reimplementation/adl2/crates/adl-difftest/tests/prop_boundary_oracle.rs` |
| NEW | `reimplementation/adl2/crates/adl-formula/tests/fold_vs_f64_semantics.rs` |
| EXTEND | `reimplementation/adl2/crates/adl-difftest/src/casegen.rs` (pools + shapes) |
| EXTEND | `reimplementation/adl2/crates/adl-difftest/src/oracle.rs` (`run_case` event merge) |
| EXTEND | `reimplementation/adl2/crates/adl-analysis/tests/f64_fold_regressions.rs` |
| NEW goldens | `examples/golden/adversarial_f64_c4.adl`, `…_c5.adl`, … |
| NEW | `reimplementation/adl2/scripts/verify_corpus_gate.sh` + `baselines/corpus_verify.json` |
| UPDATE | `docs/archive/adl2/COUNTEREXAMPLES.md` (process note: new finds → CE-N + P0 lock) |
| UPDATE | `reimplementation/README.md` trust surface wording |
