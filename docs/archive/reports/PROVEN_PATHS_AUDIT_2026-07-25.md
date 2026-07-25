# PROVEN-tier emission inventory — ADL2 / smash2

Audit date: 2026-07-25. Tree state: `main` @ `759ccd9` (working tree clean except two
untracked `.png`/`.svg`). Verified against a live build: `z3 4.12.2`,
`reimplementation/adl2/target/release/smash2`, `cargo test -p adl-analysis` = **117
passed, 0 failed**, `cargo test --doc -p adl-formula` = 4 passed (3 compile-fail + 1
positive control).

All paths below are relative to
`/home/daniel/Projects/adl2flowchart/reimplementation/adl2/` unless absolute.

---

## 0. FINDINGS FIRST

### 0.1 No site violates the (a)/(b)/(c) contract — but the nets are wildly uneven

Task item 5 asks for any proven-tier outcome not fitting (a) solver-UNSAT +
certifier-agreed/off, (b) interval reasoning over Over atoms, or (c) SAT +
interpreter-validated witness. **Every one of the 17 emission sites below fits one of
the three.** There is no rogue path, no heuristic shortcut, no "if it looks disjoint"
branch. That is the headline and it is a genuinely good result.

The finding is not a violated contract; it is that clause (a)'s escape hatch **"or
certify off"** is silently doing most of the work, and that the three post-hoc nets
(certifier, sampling gate, interpreter) cover the five UNSAT-side claim families very
unevenly:

| UNSAT-side claim | Certifier | Sampling gate | Corpus count |
|---|---|---|---|
| PROVEN DISJOINT (solver path) | **yes** | yes | 48 |
| PROVEN DISJOINT (interval path) | *unreachable* | yes | **114** |
| REGION EMPTY (solver path) | *unreachable* | yes | 5 |
| REGION EMPTY (interval path) | *unreachable* | yes | 8 |
| PROVEN SUBSET | *unreachable* | yes | — |
| bin `disjoint_pairs_proven` | *unreachable* | **none** | 14 |
| bin `CoverageStatus::Proven` | *unreachable* | **none** | 2 |
| XSUB / XEQ derived facts | *unreachable* | **none** | (cross runs only) |

Counts are measured, not estimated: `smash2 verify --json` over `examples/*.adl` +
`examples/golden/*.adl`, classifying `proven_disjoint` by whether `certified` is
`None` (interval path — no solver proof exists to certify) or `true`.

**F1 — 70% of PROVEN DISJOINT verdicts never touch the certifier.** 114 of 162 come
from the interval fast path (`engine.rs:535`), which returns *before* the solver is
ever consulted (`engine.rs:545`, `return report;`). These ship with `certified: null`
and `core: []`. This is by design — there is no solver proof object to replay — but
the practical consequence is that the flagship "trusted base collapses to the
certifier kernel" property applies to a minority of proven verdicts. This is also
precisely the path that shipped the two false PROVEN DISJOINTs fixed yesterday by
`541b6f6` (its commit body: "both via the interval fast path").

**F2 — the reconciliation-derived facts (XSUB/XEQ) are the least-defended premise in
the system, and they are premises for everything else.** `engine.rs:1135-1137` asserts
each derived `size(A) <= size(B)` at the **base frame**, outside any push/pop, so it
is live for every subsequent pairwise, empty, subset and bin query in the run. It
rests on solver UNSAT alone (`frame_sat` precheck + two `subset` calls), with **no
certifier and no sampling gate**. Worse, when a later PROVEN DISJOINT *is* certified,
`certify_disjoint` inserts these facts into the checked set as **givens**
(`engine.rs:1204-1206`), so the certificate proves "UNSAT given XR3", not XR3 itself.
A wrong XSUB yields a *certified* false PROVEN. Mitigating: `--cross` only (default
`reconcile: false`, `lib.rs:94`), and the derivation has four fail-closed guards
(§4.13).

**F3 — certification covers exactly one query shape.** `certify_disjoint` is called
from exactly one place, `engine.rs:590`, inside the pairwise disjointness branch. The
region-empty query (`engine.rs:487`), the subset queries (`engine.rs:1042`), the bin
disjointness query (`engine.rs:1410`) and the bin coverage query (`engine.rs:1440`)
are all `matches!(result, Some(SatResult::Unsat))` with nothing behind them. For these
four, z3 is fully in the trusted base.

**F4 — bin checks have zero post-hoc net.** `disjoint_pairs_proven` (`engine.rs:1392`)
and `CoverageStatus::Proven` (`engine.rs:1440`) are the only proven-tier claims with
neither a certifier nor a sampling gate. `run()` calls `bin_check` at
`engine.rs:328-331` and never gates the result. 14 + 2 live instances in the corpus.

**F5 — the structural-identity premise sits upstream of all three nets and is
uncatchable by them.** Every proof reasons over `QuantityId`s; if two physically
different collections intern to one id, every downstream layer agrees on a wrong
world. The `541b6f6` commit body documents this explicitly: forcing the collision onto
the solver path yields `certified: true` and a `--combine` bundle that **replays OK**,
because `q1>=0, q0-q1>=3, q0<=1` genuinely *is* UNSAT — the error is that `q0` denotes
two collections. The certifier structurally cannot catch this class. Only the sampling
gate can, and only if a battery event happens to separate them. **Status: the fix is
in and correct** (§6.1).

### 0.2 Type-level guarantees hold (verified by execution, not by reading)

`cargo test --doc -p adl-formula` — all three `compile_fail` doctests still compile-fail
with the asserted error codes, and the positive control still compiles:

```
test crates/adl-formula/src/formula.rs - formula::Over (line 222) - compile fail ... ok
test crates/adl-formula/src/formula.rs - formula::Under (line 269) - compile fail ... ok
test crates/adl-formula/src/formula.rs - formula::Over (line 234) - compile fail ... ok
test crates/adl-formula/src/formula.rs - formula::Over (line 240) ... ok
```

`Over`/`Under` are newtypes with a **private positional field** (`formula.rs:248`,
`formula.rs:278`), constructible only via `Formula::over()` / `Formula::under()`
(`formula.rs:126`, `formula.rs:132`). See §3 for the one place the type is not carried
end-to-end.

---

## 1. Master table — every proven-tier emission site

E = emission. Verified by reading the file; line numbers are from the current tree.

| # | file:line | Emits | Trigger | Approx side | Nets |
|---|---|---|---|---|---|
| S1 | `engine.rs:536` | `ProvenDisjoint` | `ca.intervals.disjoint_with(&cb.intervals)` = `Some` | Over spine ∩ Over spine | gate only |
| S2 | `engine.rs:549` | `ProvenDisjoint` | either region's `intervals.self_empty()` = `Some` | Over spine | gate only |
| S3 | `engine.rs:606` | `CandidateDisjoint` | solver UNSAT **and** `certified == Some(false)` | Over ∧ Over | (demotion) |
| S4 | `engine.rs:614` | `ProvenDisjoint` | solver UNSAT, `certified ∈ {Some(true), None}` | Over ∧ Over | certifier + gate |
| S5 | `engine.rs:745` | `ProvenOverlapping` | `Validation::Validated(json)` | Under ∧ Under | interpreter |
| S6 | `engine.rs:753` | `CandidateOverlapping` | `Validation::Candidate(why)` | Under ∧ Under | (demoted tier) |
| S7 | `engine.rs:626-630` | `subset_a_in_b` / `_b_in_a` | `subset()` UNSAT | Over sub / Under sup | gate only |
| S8 | `engine.rs:478` | `EmptyStatus::Proven` | `ctx.intervals.self_empty()` = `Some` | Over spine | gate only |
| S9 | `engine.rs:489` | `EmptyStatus::Proven` | solver UNSAT on `Ax ∧ R⁺` | Over | gate only |
| S10 | `engine.rs:1382` | `disjoint_pairs_proven += 1` | `bins_disjoint()` true | Over | **none** |
| S11 | `engine.rs:1410` | (feeds S10) | solver UNSAT on `Ax ∧ R⁺ ∧ Bᵢ⁺ ∧ Bⱼ⁺` | Over ×3 | **none** |
| S12 | `engine.rs:1417` | (feeds S10) | no-solver interval fallback | Over spine | **none** |
| S13 | `engine.rs:1440` | `CoverageStatus::Proven` | solver UNSAT on `Ax ∧ R⁺ ∧ ⋀¬Bᵢ⁻` | Over outer / Under inner | **none** |
| S14 | `engine.rs:1123` | XSUB/XEQ fact asserted | `prove_pred_implies` → 1 or 2 UNSATs | Over sub / Under sup | **none** |
| S15 | `engine.rs:594` | `CombineBundle{verdict:"PROVEN DISJOINT"}` | S4 certified **and** `--combine` | (inherits S4) | retained-filter |
| G1 | `engine.rs:1266` | demote → `PossiblyOverlapping` | gate event in both regions | — | (net itself) |
| G2 | `engine.rs:1283` | demote `subset_* = false` | gate event in sub not sup | — | (net itself) |
| G3 | `engine.rs:278` | demote `Proven`→`NotProven` | gate event is a member | — | (net itself) |

**Completeness argument.** `grep -n "\.kind = \|kind:"` over `engine.rs` returns exactly
17 lines (522, 536, 549, 559, 606, 614, 637, 657, 673, 745, 753, 762, 813, 822, 826,
832, 1266); of those only 536/549/606/614/745/753 are proven-or-candidate tier, 522 is
the `PossiblyOverlapping` initializer, and the rest are demotions. `EmptyStatus::Proven`
is constructed at exactly two sites (478, 489) workspace-wide; `CoverageStatus::Proven`
at exactly one (1440); `disjoint_pairs_proven` assigned at exactly one (1392);
`subset_*` written at exactly 526/527 (init false), 626 (assign), 1297/1298 (gate
clear). An independent exhaustive sweep of all 14 crates found **zero** verdict-setting
assignments outside `engine.rs` — everything else is a consumer (render, report tally,
CI findings, difftest oracle) or an upstream fact producer.

---

## 2. Per-site notes

### S1 — ProvenDisjoint, interval fast path (`engine.rs:535-546`)

```rust
535        if let Some((q, ia, ib)) = ca.intervals.disjoint_with(&cb.intervals) {
536            report.kind = VerdictKind::ProvenDisjoint;
```

**Trigger.** `IntervalMap::disjoint_with` (`interval.rs:184-193`) finds the first
`QuantityId` present in both maps whose `Iv`s fail to intersect.

**Inputs.** Both maps are built at `engine.rs:258-261` from the region's own
over-projections **only**:

```rust
258                let mut intervals = IntervalMap::default();
259                for (_, o) in &overs {
260                    intervals.add_over(o.qformula());
261                }
```

**Premises.**
1. `R⁺ ⊇ R` (projection soundness, `formula.rs:147-163`).
2. The spine walk keeps only *necessary* conditions of `R⁺` — `Or` contributes nothing
   (`interval.rs:131`), `Rel::Ne` contributes nothing (`interval.rs:159`), multi-term
   and zero-coefficient atoms are dropped (`interval.rs:133-136`).
3. `Iv` bounds are the **exact rational** `k/c` (`interval.rs:141`,
   `a.constant().checked_div(c)`), so no rounding can shift a boundary.
4. `disjoint_from` correctly intersects (max of lows, min of highs, strictness ORed on
   ties — `interval.rs:60-92`).
5. **Structural identity of `QuantityId`s** — the whole comparison is keyed on `q`.
6. Loader-valid event semantics (only via the gate).

**Not a premise:** the axiom catalog. The interval map is built from region statements
alone; `self.axioms` never feeds it. This is a real reduction in trusted base for 70%
of proven disjoints — and simultaneously means the axiom test suite provides this path
no coverage at all.

**Downgrade path if a premise fails.** Premises 1-4: **silent false PROVEN**, no net
except the gate. Premise 5: silent, and this is the class that actually fired
(`541b6f6`).

### S2 / S8 — self-empty (`engine.rs:547-556`, `engine.rs:477-479`)

```rust
548            if let Some(why) = ctx.intervals.self_empty() {
549                report.kind = VerdictKind::ProvenDisjoint;
```
```rust
477        if ctx.intervals.self_empty().is_some() {
478            return (EmptyStatus::Proven, Vec::new());
```

**Trigger.** `interval.rs:167-180`: either `self.falsified` (a `QFormula::False` sat
directly on the And-spine, set at `interval.rs:123`) or some `Iv::is_empty()`
(`interval.rs:49-54`: `lo > hi`, or `lo == hi` with either bound strict).

**Premises.** Same 1-5 as S1. Note S2 is logically implied by S8 but is computed
independently, so a bug in `self_empty` produces both a spurious empty region *and*
spurious disjointness with every other region — the blast radius is n-1 pairs, not one.

**Downgrade.** S8 is gated at `engine.rs:275-280`; S2 is gated via `gate_pair`.

### S3 / S4 — solver disjointness and the certifier fork (`engine.rs:573-618`)

```rust
577        let disjoint_result = self.check(self.timeout);
578        if matches!(disjoint_result, Some(SatResult::Unsat)) {
...
590            let (certified, cert_payload) = self.certify_disjoint(core_names.as_deref(), c1, c2);
...
601            if certified == Some(false) {
606                report.kind = VerdictKind::CandidateDisjoint;
613            } else {
614                report.kind = VerdictKind::ProvenDisjoint;
```

The frame is the base frame (axioms, `engine.rs:237-239`; recon facts,
`engine.rs:1135-1137`) plus `assert_overs(&c1.overs, true)` and
`assert_overs(&c2.overs, true)` (`engine.rs:575-576`) — i.e. `UNSAT(Ax ∧ A⁺ ∧ B⁺)`.
`assert_overs` takes `&[(AssertName, Over)]` (`engine.rs:414`), so the polarity is
type-carried here.

**Demotion direction is safe by construction.** `certify_disjoint` is reachable only
from inside the `Some(SatResult::Unsat)` branch. Its three outcomes map to:
`None` (certify off, `engine.rs:1194-1196`) → `ProvenDisjoint`; `Some(true)` →
`ProvenDisjoint`; `Some(false)` → `CandidateDisjoint`. **`Some(false)` can only
demote; there is no input on which `certify_disjoint` promotes anything.** A pair that
was not solver-UNSAT never reaches line 590.

**Premises for S4.**
- With `--certify` (default `true`, `lib.rs:96`): encoder faithfulness + axiom-catalog
  truth + structural identity + **certifier replay-kernel correctness**. z3 drops out
  of the trusted base — `certify_unsat` re-runs `Certificate::replay` on its own output
  before returning `Certified` (`adl-certify/src/lib.rs:191-200`), and a core name that
  maps to no known formula fails closed (`engine.rs:1213`, `None => return (Some(false), None)`).
- With `--no-certify`: the same minus the kernel, **plus z3's UNSAT correctness**.

**Certifying the core only is sound** — UNSAT of a subset implies UNSAT of the asserted
superset (`engine.rs:1181-1183`). When no core is available it falls back to the full
frame (`engine.rs:1218`).

**AssertName collision check (I looked, because `fmap` is a `BTreeMap` keyed by name).**
The three namespaces are prefix-disjoint: statements `R{ridx}S{sidx}`
(`encode.rs:271`), axioms `AX{i}` (`engine.rs:204/238/1202`), recon facts `XR{k}`
(`engine.rs:1124`). No collision is constructible. Even if one were, the overwritten
entry would still be a formula that *was* asserted, so the certified set stays a subset
of the frame — sound, just weaker.

### S5 / S6 — overlap (`engine.rs:740-760`)

```rust
741                    Some((model, Validation::Validated(json))) => {
745                        report.kind = VerdictKind::ProvenOverlapping;
749                        report.witness_validated = Some(true);
751                    Some((model, Validation::Candidate(why))) => {
753                        report.kind = VerdictKind::CandidateOverlapping;
```

**Frame.** `assert_unders(&c1.unders)` + `assert_unders(&c2.unders)`
(`engine.rs:650-651`) — `SAT(Ax ∧ A⁻ ∧ B⁻)`. `assert_unders` takes `&[Under]`
(`engine.rs:422`).

**The net.** `Validated` is constructed at exactly one place workspace-wide,
`witness.rs:124`, and only when **both** regions returned `Ok(true)` from
`interp.eval_region_membership_idx` (`witness.rs:89-90`) and no opaque was hit. A
decidable `Ok(false)` returns `Rejected` immediately (`witness.rs:91-97`) *even if the
other region was opaque* — the Kleene "decidable False beats Unknown" invariant, at
pair level. Region lookup is **by index, not name** (`witness.rs:80-84`), which is what
stops a merged unit's duplicate region names from masking the second region's cuts.

**Four caps before this point**, all sound-direction (they only weaken):
`shared_dimensions.is_empty()` → POSSIBLY (`engine.rs:655-661`); twin pairs → POSSIBLY
(`engine.rs:636-646`); back-index `coll[-k]` → POSSIBLY (`engine.rs:671-679`);
`MAX_WITNESS_ATTEMPTS = 6` exhausted → POSSIBLY (`engine.rs:694`, `engine.rs:761`).

**Premises for S5.** Interpreter correctness (it *is* the meaning), loader-valid
synthetic event (`parse_event`, `witness.rs:72-77`), witness realizer fidelity. Note
S5's premise set is **disjoint from the UNSAT side's** — it does not depend on axiom
truth or on `R⁺ ⊇ R`. If the encoder is wrong, S5 degrades to a downgrade, not a false
proof. This is the safe side and it behaves like it.

**Downgrade if a premise fails.** Loudly: `internal.push("INTERNAL: witness validation
failed …")` at `engine.rs:796-799`, but only when the pair is `exact` and the reason is
not an opaque/unresolved class (`engine.rs:780-785`) — otherwise a quiet downgrade,
which is correct (a non-exact region's witness need not realize).

### S7 — PROVEN SUBSET (`engine.rs:624-630`, prover at `engine.rs:1024-1043`)

```rust
1024    fn subset<'o>(
1025        &mut self,
1026        sub_overs: impl IntoIterator<Item = &'o Over>,
1027        sup_unders: &[Under],
1028    ) -> bool {
1029        if self.solver.is_none() {
1030            return false;
...
1042        matches!(result, Some(SatResult::Unsat))
```

Asserts `sub_overs` and `negated_under(sup_unders)` — the latter being
`QFormula::Or(unders.map(|u| u.not()))` (`engine.rs:433-435`), the exact NNF negation
of the conjunction. So `UNSAT(Ax ∧ A⁺ ∧ ¬B⁻)` ⇒ `A ⊆ A⁺ ⊆ B⁻ ⊆ B`. **Polarity is
type-carried in the signature** — the outer side literally cannot be handed an `Under`.
No-solver returns `false` (fail closed, `engine.rs:1030`).

Results are mapped back through the canonical-order swap at `engine.rs:626-630`, which
I checked term by term: `a_first` is `ra.name <= rb.name` (`engine.rs:570`), `(c1, c2)`
is `(ca, cb)` when `a_first` (`engine.rs:571`), `one_in_two` is `subset(c1.overs,
c2.unders)` (`engine.rs:624`) — so when `a_first`, `one_in_two` is `a ⊆ b`, and the
tuple assignment `(one_in_two, two_in_one)` is correct; the `else` branch swaps.
Correct.

**No certifier.** Gate-only (G2).

### S9 — region empty, solver path (`engine.rs:472-496`)

```rust
486        let out = match result {
487            Some(SatResult::Unsat) => {
488                let items = self.core_items(origins);
489                (EmptyStatus::Proven, items)
490            }
491            Some(SatResult::Sat) => (EmptyStatus::NotProven, Vec::new()),
492            _ => (EmptyStatus::Unknown, Vec::new()),
```

`UNSAT(Ax ∧ R⁺)`. The `_` arm catches `Unknown`, timeout and no-solver → `Unknown`,
never `Proven`. Fail-closed. Gated at `engine.rs:275-280`. **No certifier** (F3).

### S10-S12 — bin pairwise disjointness (`engine.rs:1371-1418`)

```rust
1381            for i in 0..n {
1382                for j in i + 1..n {
1383                    if self.bins_disjoint(region_ctx, &overs[i], &overs[j]) {
1384                        proven += 1;
```
```rust
1400    fn bins_disjoint(&mut self, region_ctx: &RegionCtx, bi: &Over, bj: &Over) -> bool {
1410            return matches!(r, Some(SatResult::Unsat));
...
1417        a.self_empty().is_some() || b.self_empty().is_some() || a.disjoint_with(&b).is_some()
```

`overs` is built by `set.bins.iter().map(adl_formula::Formula::over)`
(`engine.rs:1374`) — all three inputs (`region_ctx.overs`, `bi`, `bj`) are Over.
Correct polarity: `UNSAT(Ax ∧ R⁺ ∧ Bᵢ⁺ ∧ Bⱼ⁺)` ⇒ bins i,j cannot co-occur within R.

The no-solver fallback (S12, `engine.rs:1412-1417`) clones the region's Over spine and
adds each bin's Over — same soundness argument as S1. Sound.

**Nets: none.** `run()` calls `bin_check` at `engine.rs:328-331` and never gates it.

### S13 — bin coverage (`engine.rs:1422-1457`)

```rust
1440            Some(SatResult::Unsat) => (CoverageStatus::Proven, Vec::new()),
```

Frame: `assert_overs(&region_ctx.overs, false)` (`engine.rs:1432`) plus
`s.assert(&u.qformula().clone().not(), None)` for each `u` in `unders`
(`engine.rs:1434-1436`). So `UNSAT(Ax ∧ R⁺ ∧ ⋀ᵢ ¬Bᵢ⁻)` ⇒ `R⁺ → ⋁Bᵢ⁻` ⇒ `R → ⋁Bᵢ`.
**Polarity is correct and non-obvious**: Over on the region, Under on the bins being
negated — negating an under-approximation gives an over-approximation of the
complement, which is the sound direction for a coverage claim. Verified: `unders` comes
from `set.bins.iter().map(adl_formula::Formula::under)` at `engine.rs:1375`.

`Sat` → `NotProven` + gap witness; `_` → `Unknown`. Fail-closed. **Nets: none.**

### S14 — XSUB / XEQ derived facts (`engine.rs:1052-1153`)

```rust
1087            let (a_in_b, b_in_a) = self.prove_pred_implies(&cand.phi_a, &cand.phi_b);
...
1105            let facts: &[(QuantityId, QuantityId, AxiomId)] = if a_in_b && b_in_a {
1106                &[
1107                    (cand.size_a, cand.size_b, AxiomId::Xeq),
1108                    (cand.size_b, cand.size_a, AxiomId::Xeq),
1109                ]
1110            } else if a_in_b {
1111                &[(cand.size_a, cand.size_b, AxiomId::Xsub)]
...
1123                let fact = derived_size_le(sub, sup);
...
1135                if let Some(s) = self.solver.as_deref_mut() {
1136                    s.assert(&fact, Some(name.clone()));
```

**These are FACT emissions, not verdicts** — asserted at the base frame with no
push/pop, so they are premises of every later query in the run. Treat them as axioms
minted at runtime.

**The derivation chain** (`engine.rs:1163-1174`):
```rust
1168        if !self.frame_sat(phi_a, phi_b) {
1169            return (false, false);
1170        }
1171        let a_in_b = self.subset([&phi_a.over()], &[phi_b.under()]);
1172        let b_in_a = self.subset([&phi_b.over()], &[phi_a.under()]);
```

**Premises.**
1. **Universality of the generic element.** Both predicates are lowered onto one shared
   `base[GENERIC_INDEX]` (`reconcile.rs:248`). The proof `UNSAT(φ_A(g) ∧ ¬φ_B(g))` with
   `g`'s properties as *free* variables is a valid ∀-proof. This depends on those
   helper quantities carrying **no axioms of their own** — enforced by ordering:
   `reconcile::build` must run after `emit_axioms` (`reconcile.rs:117-118`; the call
   order is `lib.rs:200` then `lib.rs:225`). Verified.
2. **Same base collection.** Guaranteed by `reconciliation_candidates()` enumeration
   plus the re-read at `reconcile.rs:126-131`.
3. **Base is a real detector object.** `ext.base_collection(...).is_none()` → skip
   (`reconcile.rs:138-149`). This is the guard that stops two analyses' private
   same-spelled bases from colliding.
4. **Groundability.** `lower()` returns `None` for composite/reduce/peer-element
   predicates → whole pair dropped (`reconcile.rs:151-161`).
5. **Non-degenerate frame.** `frame_sat` (`engine.rs:1329-1341`) requires
   `SatResult::Sat` with both unders asserted; `Unknown` and `Unsat` both return
   `false`, so a vacuous frame yields no fact.
6. Dedup against intra-source SUB by **formula equality** (`engine.rs:1349-1369`),
   which cannot silently invert.

**Order dependence (noted, not a defect).** Facts are asserted inside the candidate
loop, so candidate *k+1*'s `frame_sat`/`subset` frames see candidates *0..k*'s facts.
Sound by induction if each is sound, but it means one bad fact can cascade. In practice
the derived facts constrain only `Size` quantities while the subset frames constrain
only generic-element properties, so they do not interact — but nothing enforces that.

**Downgrade if a premise fails: NONE.** Silent, and it poisons the base frame.

### S15 — the portable bundle (`engine.rs:593-600`, `adl-certify/src/bundle.rs:187-208`)

```rust
197            verdict: "PROVEN DISJOINT".to_owned(),
```

A literal proven-tier label written into an on-disk artifact, independent of
`VerdictKind`. `cert_payload` is `Some` only when `self.combine && Certified`
(`engine.rs:1223-1226`), so a bundle exists only for a certified S4. Crucially it is
then **re-filtered against the final verdicts** (`engine.rs:359-364`):

```rust
360        combine_bundles.retain(|b| {
361            pairwise.iter().any(|p: &PairReport| {
362                p.kind == VerdictKind::ProvenDisjoint && p.a == b.region_a && p.b == b.region_b
```

so a gate demotion retracts the artifact too. Correct.

The bundle carries an honest scope sentence (`bundle.rs:27-30`) that states the residual
trusted base exactly right: *"Replaying this bundle proves the listed formulas are
(real-)unsatisfiable together. That the formulas faithfully encode the named regions
(encoder, polarity projection, axiom catalog) is smash2's claim, audited by its testing
nets - not established by this replay."*

### G1-G3 — the sampling gate is demotion-only (verified exhaustively)

```rust
1256        if report.kind == VerdictKind::ProvenDisjoint {
1266                    report.kind = VerdictKind::PossiblyOverlapping;
```
```rust
1277            if !*flag {
1278                return;
1279            }
1283                    *flag = false;
```
```rust
275            if empty == EmptyStatus::Proven
276                && self.gate_empty(r.idx, &interp, &mut internal, &mut gate_refutations)
277            {
278                empty = EmptyStatus::NotProven;
```

Three writes total, all strictly weakening: `ProvenDisjoint → PossiblyOverlapping`;
`true → false` on a subset flag, guarded by an early return when the flag is already
false; `Proven → NotProven`. **The gate has no code path that sets any flag to `true`
or any `kind` to a proven tier.** Interpreter errors are discarded via `.ok()`
(`engine.rs:1254`, `engine.rs:1311`) so an error carries no information — it can
neither refute nor confirm.

---

## 3. Type-level enforcement — where it holds and where it lapses

**Holds.** `Over(QFormula)` / `Under(QFormula)` with private fields
(`formula.rs:248`, `formula.rs:278`); only constructors are `Formula::over` /
`Formula::under` (`formula.rs:126`, `formula.rs:132`). Signatures that carry it:
`assert_overs(&[(AssertName, Over)])` (`engine.rs:414`), `assert_unders(&[Under])`
(`engine.rs:422`), `negated_under(&[Under])` (`engine.rs:433`),
`subset(impl IntoIterator<Item = &Over>, &[Under])` (`engine.rs:1024`),
`bins_disjoint(_, &Over, &Over)` (`engine.rs:1400`),
`bin_coverage(_, _, &[Under])` (`engine.rs:1422`). Projection soundness itself is at
`formula.rs:147-163`: `Unknown → True` under Over, `→ False` under Under; `Dual → plus`
/ `→ minus`.

**Lapses — one, and it is on the highest-volume path.** `IntervalMap::add_over` takes a
**raw `&QFormula`**, not an `&Over`:

```rust
interval.rs:116    pub fn add_over(&mut self, f: &QFormula) {
```

The name carries the contract; the type does not. There are exactly three call sites
and all three are currently correct — `engine.rs:260` (`o.qformula()` from
`&(AssertName, Over)`), `engine.rs:1414` and `engine.rs:1416` (`bi.qformula()` /
`bj.qformula()` from `&Over`). But `Under` also exposes a `qformula()`
(`formula.rs:283`) with an identical signature, so `intervals.add_over(u.qformula())`
would compile and silently invert the polarity of the fast path that produces 70% of
PROVEN DISJOINT verdicts. Changing the parameter to `&Over` is a one-line, zero-cost
fix. **Recommend it.** (Not filed as a live bug — all three call sites are correct
today.)

---

## 4. interval.rs — detailed soundness review (task item 4)

**What feeds `IntervalMap`.** Only `add_over`, only from the three sites above. The
region map (`engine.rs:258-261`) is built from `overs` — the region's own statement
over-projections — and **nothing else**. Confirmed axiom-free: `self.axioms` is never
passed to an `IntervalMap`. `grep -n "add_over\|IntervalMap\|intervals"` over
`crates/adl-analysis/src/` (excluding interval.rs) returns exactly: `engine.rs:16`
(import), `:180` (the `RegionCtx` field), `:258`/`:265` (construction), `:260`/`:1414`/
`:1416` (the three `add_over` calls), `:477`/`:535`/`:538`/`:548` (reads), `:1413`/
`:1415` (clones), and `render.rs:124` (a reason-string match for display). So the
interval fast path's premise set excludes the entire axiom catalog.

**Spine extraction is drop-only (`interval.rs:120-162`).** `True` → nothing;
`False` → `falsified = true`; `And` → recurse; `Or` → **ignored** (`interval.rs:131`,
"Disjunctive structure leaves the spine; ignoring it is sound (we only ever DROP
necessary conditions)"); `Atom` → contributes only if it is single-term
(`let [(c, q)] = a.terms() else { return }`, `interval.rs:133`) and `c != 0`
(`interval.rs:134-136`); `Rel::Ne` contributes nothing (`interval.rs:159`). Every branch
either records a genuine necessary condition of `R⁺` or records nothing. Sound.

**Exact rationals.** `interval.rs:141`:
```rust
                let Some(bound) = a.constant().checked_div(c) else {
```
The bound is the exact rational `k/c`. Relation flips on negative `c`
(`interval.rs:144-148`) — verified by the `negative_coefficient_flips` test
(`interval.rs:241-251`, `-2q ≤ -400 ⇔ q ≥ 200`). The old f64 subnormal-guard and
fma-residual ulp-nudge are gone, as the skill requires; nothing in this file touches
f64 except `human()` for display (`interval.rs:99-100`).

**`disjoint_from` (`interval.rs:58-93`).** Computes the intersection's lower bound as
the greater of the two (with `None` = −∞ weakest) and upper as the lesser, ORing
strictness on ties (`interval.rs:70`, `interval.rs:85`), then reports empty iff
`lo > hi || (lo == hi && (lo_strict || hi_strict))` (`interval.rs:90`). I checked all
nine `(Option, Option)` cases: correct. Over exact rationals with correct strictness
handling, this is precise, not merely sound.

**`self_empty` (`interval.rs:167-180`).** `falsified` or any `Iv::is_empty()`. `is_empty`
(`interval.rs:49-54`) returns `false` whenever either bound is `None` — a half-bounded
interval is never empty. Correct.

**`bins_disjoint` fast path (`engine.rs:1412-1417`).** Clones the region's Over spine,
adds each bin's Over, and reports disjoint if either side is self-empty or the two are
interval-disjoint. Since `R⁺ ∧ Bᵢ⁺` over-approximates `R ∩ Bᵢ`, an empty intersection of
the spines proves the real intersection empty. Sound. Only reachable with no solver.

**Coverage.** Four inline tests (`interval.rs:209-261`) pin: disjointness + strict/closed
touching bounds, `Or` contributing nothing, negative-coefficient flip, self-empty
detection. That is decent unit coverage of the *arithmetic* and zero coverage of the
*identity* premise (S1 premise 5) — which is exactly where it broke.

---

## 5. The `certify_disjoint` demotion direction (task item 1, last bullet)

Re-stated because it is the single most important "can this strengthen?" question:

```rust
1194        if !self.certify {
1195            return (None, None);
1196        }
...
1213                        None => return (Some(false), None),
...
1221        match adl_certify::certify_unsat(&formulas, &adl_certify::Budget::default()) {
1222            adl_certify::CertifyResult::Certified(cert) => { ... (Some(true), payload) }
1229            adl_certify::CertifyResult::Uncertified(_) => (Some(false), None),
```

Three return values, one call site (`engine.rs:590`), and that call site is inside
`if matches!(disjoint_result, Some(SatResult::Unsat))`. The verdict written is
`CandidateDisjoint` on `Some(false)` and `ProvenDisjoint` otherwise. Since the branch
is only entered on solver UNSAT — which without certification would have produced
`ProvenDisjoint` unconditionally — **the certifier's only possible effect is
`ProvenDisjoint → CandidateDisjoint`.** It cannot promote, cannot reach a non-UNSAT
pair, and fails closed on an unmappable core name (`engine.rs:1213`) and on a
self-replay failure inside the crate (`adl-certify/src/lib.rs:196-199`).

`adl-certify`'s own guarantee is genuinely strong: `Certified` implies
`certificate.replay(formulas) == true` by construction, and replay is exact-rational
Farkas checking with `λ ≥ 0` enforced — no search, no solver, no encoder
(`adl-certify/src/lib.rs:46-88`). Real-infeasibility implies integer-infeasibility, and
the converse direction is never claimed (integer-only-UNSAT surfaces as
`Uncertified("branch satisfiable …")` → CANDIDATE). Conservative, never wrong.

---

## 6. Premise audit map — what is covered and what is not

### 6.1 Structural identity of QuantityIds — **FIXED, verified in tree**

`ElemPredInterner` is defined at `crates/adl-sema/src/hir.rs:246`, both fields private:

```rust
245    #[derive(Debug, Default)]
246    pub struct ElemPredInterner {
247        preds: Vec<ElemPred>,
248        by_render: std::collections::HashMap<String, ElemPredId>,
249    }
```

The fail-closed rule (`hir.rs:254-266`):

```rust
254        pub fn intern(&mut self, node: HNode, render: String) -> ElemPredId {
255            let id = ElemPredId(u32::try_from(self.preds.len()).expect("pred id overflow"));
256            if node.has_unsupported() {
257                self.preds.push(ElemPred { node, render });
258                return id;
259            }
260            if let Some(&prev) = self.by_render.get(&render) {
261                return prev;
262            }
```

**Both callers route through it — verified by direct grep, not by report.** Exactly two
construction sites (`resolve.rs:130` field / `resolve.rs:243` init; `merge.rs:47` field
/ `merge.rs:82` init) and exactly two `.intern(node, render)` call sites:
`resolve.rs:1099` (inside `Resolver::intern_elem_pred`) and `merge.rs:293` (inside
`Merger::remap_pred`, which is memoized on the source id at `merge.rs:275-277/294` and
reached from both `Filtered` at `merge.rs:180` and `Combination{cuts}` at
`merge.rs:214`). No collection variant carrying a pred bypasses it. `ElemPredId(...)` is
constructed literally only at `hir.rs:255` and in three `#[cfg(test)]` fixtures in
`quantity.rs`. `ElemPred { .. }` only at `hir.rs:257` and `hir.rs:264`.

Commit `541b6f62fa2ea352b04d487d65e1ca97928a45c1`, 2026-07-24 17:24, *"fix(soundness):
one shared element-predicate interner; kills a false PROVEN DISJOINT"*, 10 files
+578/−45. The deleted merge-path code was an unconditional render-key dedup with **no**
`has_unsupported()` branch. The resolver's logic was *moved*, not weakened. Fully
committed; working tree clean.

**Audited by:** `merge.rs:651` `merge_never_unifies_unsupported_cuts` (with a
precondition assert at `merge.rs:664-668` that the renders genuinely collide, so it
cannot silently stop testing what it claims); `merge.rs:699`
`merge_still_unifies_identical_supported_cuts_across_units` (the counterweight —
cross-file identity must survive); `crates/adl-sema/tests/identity.rs:567`
`unsupported_cuts_never_share_identity`;
`crates/adl-analysis/tests/soundness_review_regressions.rs:56`
`s1_unsupported_render_must_not_unify` (the only *verdict*-level pin); golden fixture
`examples/golden/cross/opaque-cut-collision/{a,b}.adl` driven by
`crates/adl-analysis/tests/golden_cross.rs:75`.

**Not audited:** no test asserts that the merger's render function agrees with the
resolver's. Divergence there causes *missed* sharing (over-approximation, safe
direction), so it is not a soundness hole — but the key function is duplicated while
only the fail-closed rule is shared.

### 6.2 Coverage summary by premise

| Premise | Audited by | Gap |
|---|---|---|
| `R⁺ ⊇ R`, `R⁻ ⊆ R` | `formula.rs:329` `projections_resolve_unknown_and_dual`; 3 compile-fail doctests | `add_over` takes raw `QFormula` (§3) |
| Interval arithmetic | `interval.rs:209-261` (4 tests) | no identity-premise coverage |
| Axiom-catalog truth | `adl-axioms/tests/axioms_hold.rs`; sampling gate | — |
| Solver UNSAT correctness | certifier (pairwise only) | empty / subset / bins / XSUB (F3) |
| Certifier kernel | `adl-certify/tests/{tamper,property,hand_cases,budget}.rs` | — |
| Witness → interpreter | `adl-difftest` oracle; `witness.rs` tests | — |
| Encoder ↔ interpreter | `adl-difftest` property battery | — |
| Structural identity | §6.1, 5 tests + golden fixture | merger/resolver render parity |
| XSUB/XEQ derivation | `engine.rs:1642-1709` (5 tests: Unknown-frame, Unsat-frame, Unknown-subset, XSUB/XEQ direction, SUB dedup); `tests/cross_file.rs`; `tests/reconciliation_ledger.rs` | no gate, no certifier (F2) |
| Sampling gate itself | `engine.rs:1790` `gate_demotes_a_fabricated_disjoint`, `:1811` `disabled_gate_ships_the_fabrication`, `:1820` `gate_leaves_true_verdicts_alone` | — |
| Bin checks | `tests/golden_battery.rs:265-289` (values only) | no gate, no certifier (F4) |

`engine.rs:1811` `disabled_gate_ships_the_fabrication` deserves special mention: it
asserts that with `sample_gate = 0` the same scripted bug **does** ship as
`ProvenDisjoint`. That is an unusually honest test — it pins the size of the hole the
gate is covering.

---

## SUSPICIOUS

Things I could not fully justify, smallest to largest. None is a demonstrated false
PROVEN; all are places where I could not close the argument from the code alone.

1. **`IntervalMap::add_over(&QFormula)` is not `&Over`** (`interval.rs:116`). All three
   call sites are correct today, and `Under::qformula()` has an identical signature.
   The highest-volume proven path is protected by naming convention, not by the type
   system that the module header claims protects it. One-line fix.

2. **XSUB/XEQ facts accumulate mid-loop** (`engine.rs:1117-1150`). Candidate *k+1*'s
   `frame_sat` and `subset` frames see candidates *0..k*'s asserted facts, so the
   derivation is order-dependent and one bad fact cascades. I argued they cannot
   interact (facts constrain `Size`, subset frames constrain generic-element
   properties) but nothing in the code enforces the separation, and I did not test it.

3. **Bin checks have no net at all** (F4). I could not find any reason bins were
   excluded from the sampling gate; it looks like an omission rather than a decision.
   `bins_disjoint` and `bin_coverage` are ordinary UNSAT queries over the same encoder
   and axioms as the pairwise path, so the same fabrication modes apply. 14 + 2 live
   claims in the corpus.

4. **`region_empty` and `subset` are not certifiable** (F3). Both are plain
   `matches!(…, Some(SatResult::Unsat))`. Extending `certify_disjoint`'s pattern to them
   looks mechanical (same frame shape, same `origins` map), so I could not tell whether
   the omission is scope or a deliberate cost decision.

5. **`certified: None` is overloaded.** It means both "certification did not run" and
   "this is an interval verdict with nothing to certify". `report.rs:168-171` documents
   the conflation, but a downstream consumer counting `certified == true` as "the good
   ones" will silently score 114 interval proofs the same as a `--no-certify` run. The
   difftest oracle does not distinguish them either.

6. **`snap_model` / `WITNESS_EPS` dyadic reasoning** (`engine.rs:88`, `engine.rs:119-140`).
   SAT-side only, so it cannot fabricate a proof, and I confirmed the retry loop
   downgrades on exhaustion (`engine.rs:761`). But I did not verify that snapping to the
   2⁻²² grid preserves membership for *equality* atoms over sums — the comment asserts
   it, the code re-validates afterward (`engine.rs:706-708`), so a wrong snap yields a
   rejection, not a false pass. Listed only because I read the argument rather than
   checking it.

7. **`existing_size_le` matches on `AxiomId::Sub` only** (`engine.rs:1352`). If a future
   axiom emitted a size-ordering fact under a different id, the dedup would miss it and
   reconcile would assert a duplicate. Duplicates are harmless for soundness (same
   formula) but would double-count in the axioms-used ledger. Cosmetic; noting for
   completeness.

8. **The merger and the resolver compute the interning key with different code**
   (`resolve.rs:1098` `self.render_node(&node)` vs `merge.rs:282-288` a locally-built
   `RenderCtx`). Divergence fails in the safe direction (missed sharing), and I
   confirmed unification requires byte-equal renders — but the *fail-closed rule* is
   now shared while the *key function* is not, and no test pins their agreement. A
   related deliberately-unfixed analogue is documented at `merge.rs:343-361`
   (`SortKey::Opaque` is not unit-namespaced, safe only because the fragment gate blocks
   it upstream) — I did not independently verify that upstream gate.

9. **`s1_unsupported_render_must_not_unify` skips when no solver is available**
   (`soundness_review_regressions.rs:75-78`). The single verdict-level pin for the F5
   identity class is silently vacuous on a no-solver CI leg. Since the bug it guards
   fired through the *interval* path — which needs no solver — the test could pass
   vacuously in exactly the configuration where the bug is reachable. Worth converting
   to a no-solver-capable assertion.
