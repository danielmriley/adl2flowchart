# Validation-Engine Audit — `smash2` (2026-07-28)

**Scope:** the verification/validation pipeline of the ADL2 Rust workspace
(`reimplementation/adl2/`): `adl-formula` (encoder + polarity projection),
`adl-axioms` (axiom catalog), `adl-certify` (Farkas certificate kernel),
`adl-analysis` (verdict orchestration, witness validation, sampling gate,
certification gating), and the `smash2 verify` CLI plumbing.

**Method:** four parallel adversarial audit lanes (one per crate), followed by
independent verification of every critical claim against the source and, where
possible, end-to-end reproduction with the built `smash2` binary (release,
subprocess z3 backend) and the reference interpreter (`smash2 run`).

**Confidence labels used throughout:**

- **DEMONSTRATED** — reproduced end-to-end on this tree; the input files and
  full transcripts are in Appendix A.
- **CODE-CONFIRMED** — the defect is visible in the source and the logic was
  traced by hand; not separately runtime-reproduced.
- **UNVERIFIED** — suspected; reachability from real inputs not established.

---

## 1. Executive summary

Three **critical soundness bugs** were found in the encoder's
f64-faithfulness guard (`adl-formula/src/encode.rs`), and **all three were
demonstrated end-to-end**: `smash2 verify` prints `PROVEN DISJOINT` for region
pairs that the reference interpreter accepts simultaneously with a concrete
witness event. This is the exact failure class the reimplementation's
architecture exists to prevent — the README's own release-blocking criterion
("if the verifier and the interpreter ever disagree on a satisfying event,
that is a release-blocking bug").

All three bugs share one systemic root cause: the f64-faithfulness guard is a
*syntactic* pattern-match over the expression tree, while the hazard it guards
against is *semantic* (two expression trees whose exact-rational folds agree
but whose stepwise f64 evaluations disagree). It currently has holes in three
directions at once:

| # | Hole | One-line trigger |
|---|------|------------------|
| C1 | Multiplicative constant regrouping is not modeled at all | `MET*0.2*0.3` vs `0.06*MET` |
| C2 | The dyadic-constant probe misses `Neg(Num)` literal operands | `MET + -0.1 > 0.3` |
| C3 | `abs_cmp` re-flattens its inner expression with unguarded `lin` | `abs(MET + HT - HT) < 50` |

Beyond the criticals: **no unsound axiom** was found in `adl-axioms`, the
**Farkas replay kernel in `adl-certify` is mathematically sound** (attacked
empirically; no way to certify a real-satisfiable system was found), and the
polarity discipline in `adl-analysis` (`Over`/`Under` newtypes, fail-closed
solver arms, witness re-validation) checks out. The remaining findings are
contract/reporting gaps (emptiness/subset/bin UNSAT verdicts are never
certified despite the README promising it; `certified: true` survives a
sampling-gate demotion), proof-bundle hygiene issues, and two one-line-input
denial-of-service paths.

**Corpus impact (good news):** none of the three bug *shapes* appears in an
active (non-commented) line of the 138-file example corpus, so existing
golden/corpus verdicts are unlikely to be affected. The golden corpus also
does not cover these shapes, which is precisely why the bugs survived.

---

## 2. Critical findings (DEMONSTRATED — false PROVEN DISJOINT)

### C1. The f64-faithfulness guard is blind to multiplicative regrouping

**Location:** `reimplementation/adl2/crates/adl-formula/src/encode.rs:284-298`
(and the guard entry point `is_f64_faithful`, :337-339).

```rust
/// Number of `Add`/`Sub` nodes anywhere in an arithmetic source tree
/// (`Mul`/`Div`/`Pow` are not counted — only additive associativity and
/// cancellation are the f64-faithfulness hazard).
fn count_add_sub(node: &HNode) -> u32 {
    match &node.kind {
        HKind::Binary {
            op: ArithOp::Add | ArithOp::Sub,
            lhs,
            rhs,
        } => 1 + count_add_sub(lhs) + count_add_sub(rhs),
        HKind::Binary { lhs, rhs, .. } => count_add_sub(lhs) + count_add_sub(rhs),
        HKind::Neg(a) | HKind::Abs(a) => count_add_sub(a),
        _ => 0,
    }
}
```

**Root cause.** The comment's premise is false. Exact-rational folding
regroups `(MET*0.2)*0.3` to `MET·(3/50)`, identical to the fold of `0.06*MET`.
But in f64, `fl(fl(x·0.2)·0.3) ≠ fl(0.06·x)` in general — the left side rounds
twice, the right side once. Both source forms pass `is_f64_faithful` (zero
additive ops), so two regions whose expressions round differently unify into
complementary exact atoms, and the solver correctly proves those atoms
disjoint — a proof about values the interpreter never computes.

More generally: any expression tree in which **two or more non-exact f64
roundings merge into one exact atom** is unfaithful, regardless of whether
the operations are additive or multiplicative. The guard only counts one of
the four operation kinds. ~~(A *single* multiply by a non-dyadic constant is
not exploitable this way: both regions fold identically, and the analyzer
sees the resulting boundary sliver. It takes two different evaluation orders
folding to the same exact form.)~~ **ERRATUM (follow-up eval, §12):** this
parenthetical is wrong — a single multiply IS exploitable (finding C5), and
so is a single *dyadic add*, the guard's explicitly allowed case (C4). See
§12 for the corrected analysis and revised fix requirements.

**Reproduction (Appendix A.1).**

```adl
region a
  select MET*0.2*0.3 + HT > 1

region b
  select 0.06*MET + HT <= 1
```

`smash2 verify` → `a vs b: PROVEN DISJOINT — core: a line 2 ∧ b line 5`.
`smash2 run` with witness `{"MET": {"pt": 947.8280087844788, "phi": 0.0},
"HT": -55.869680527068724}` → `a -> PASS`, `b -> PASS`
(`fl(fl(947.8280087844788·0.2)·0.3) + HT = 1.000000000000007 > 1`;
`fl(0.06·MET) + HT = 1.0 ≤ 1`).

**Severity:** critical soundness. **Status: DEMONSTRATED.**

---

### C2. `additive_consts_dyadic` misses `Neg(Num)` literal operands

**Location:** `reimplementation/adl2/crates/adl-formula/src/encode.rs:305-328`.

```rust
        } => {
            for side in [lhs.as_ref(), rhs.as_ref()] {
                if let HKind::Num(s) = &side.kind {
                    match parse_rat(s) {
                        Some(c) if c.is_dyadic() => {}
                        _ => return false,
                    }
                }
            }
            additive_consts_dyadic(lhs) && additive_consts_dyadic(rhs)
        }
        // ...
        HKind::Neg(a) | HKind::Abs(a) => additive_consts_dyadic(a),
        _ => true,
```

**Root cause.** The dyadic probe only matches a *bare* `HKind::Num` operand of
an `Add`/`Sub`. A negative literal in source arrives as `Neg(Num("-0.1"))`:
the `if let HKind::Num` fails, and the `Neg` arm recurses into the inner
node, which trivially returns `true`. So `MET + 0.1` is correctly judged
non-faithful (0.1 is non-dyadic) and routed to an opaque scalar — but
`MET + -0.1`, the identical hazard, folds exactly to the atom `MET > 0.4`.
One `Neg` node defeats the guard.

**Reproduction (Appendix A.2).**

```adl
region a
  select MET + -0.1 > 0.3

region b
  select MET <= 0.4
```

`smash2 verify` → `a vs b: PROVEN DISJOINT — MET.pt: (0.4, inf] vs [-inf, 0.4]`
— note this fired on the **interval prefilter**, no solver query needed.
`smash2 run` with witness `{"MET": {"pt": 0.4, "phi": 0.0}}` → both regions
PASS: `fl(0.4 + -0.1) = 0.30000000000000004 > 0.3` and `0.4 ≤ 0.4`.

The interval layer in `adl-analysis/interval.rs` does **not** re-implement
flattening — it consumes the `LinAtom`s the encoder produced — so the single
root fix in the encoder covers this path too.

**Severity:** critical soundness. **Status: DEMONSTRATED.**

---

### C3. `abs_cmp` bypasses the guard with unguarded `lin` on its inner expression

**Location:** `reimplementation/adl2/crates/adl-formula/src/encode.rs:1667-1670`,
contrasted with `band` at :1726-1729.

```rust
    /// Exact absolute-value expansion against a constant:
    /// `|E| < c ⇔ E < c ∧ E > −c`, `|E| > c ⇔ E > c ∨ E < −c`, etc.
    fn abs_cmp(&mut self, inner: &HNode, rel: Rel, c: Rat, span: Span) -> Formula {
        let e = match self.lin(inner) {   // <-- raw `lin`, no faithfulness guard
```

```rust
        // `lin_guarded` (not `lin`): a non-f64-faithful band expression
        // (`MET+HT-HT [] lo hi`) must route to the opaque/per-bound path, not
        // flatten — else the cancellation false-PROVEN resurfaces in bands.
        let e = match self.lin_guarded(expr) {
```

**Root cause.** `band` guards against precisely the cancellation hazard;
`abs_cmp` does the forbidden thing. The live path, confirmed by reading the
dispatch:

1. `cmp_node_const` (:1614) calls `lin_guarded(abs_node)`, which correctly
   fails for a non-faithful inner (`count_add_sub(abs(MET+HT-HT)) = 2`).
2. The `NonLinear` failure falls through to `pattern` (:1522).
3. `pattern` dispatches `HKind::Abs(inner) => self.abs_cmp(inner, …)` (:1529).
4. `abs_cmp` calls `self.lin(inner)`, which flattens `MET + HT - HT` to
   `MET` exactly.

So the guard fires, is correctly consulted, and its verdict is then routed
around by the fallback path.

**Reproduction (Appendix A.3).**

```adl
region a
  select abs(MET + HT - HT) < 50

region b
  select abs(MET) >= 50
```

`smash2 verify` → `PROVEN DISJOINT`. `smash2 run` with witness
`{"MET": {"pt": 50.0, "phi": 0.0}, "HT": 1152921504606846976.0}` (2⁶⁰) →
both regions PASS: `fl(50 + 2⁶⁰) = 2⁶⁰`, `fl(2⁶⁰ − 2⁶⁰) = 0`, `abs(0) < 50`
and `abs(50) ≥ 50`.

**Severity:** critical soundness. **Status: DEMONSTRATED.**

---

## 3. Why the defense nets missed all three

The project runs several independent nets that should catch encoder
unsoundness. Each missed all three bugs, for structural reasons:

1. **Sampling gate** (`adl-analysis`, on by default). Pushes deterministic
   boundary events through the interpreter and refutes false PROVENs. Its
   boundary-event pools are **fixed constants** (`PT_POOL`/`ETA_POOL`/
   `MET_POOL`); a cut at an off-pool constant (0.4, 50, 1.0 here) gets only
   uniform random draws, which essentially never land on the boundary. All
   three counterexamples need the witness to sit exactly on a cut boundary,
   so the gate's samples never exercised the divergence. The net designed to
   catch this class has a blind spot for exactly this class.
2. **Differential property battery** (`adl-difftest`). Random regions ×
   sampled events. Its generated expression shapes (bounded depth, pool
   constants) do not produce non-dyadic multiplicative chains, negative
   additive literals, or `abs` over multi-op arithmetic at boundary
   precision — and hitting an f64 boundary requires adversarial constant
   search, not random sampling.
3. **Golden corpus** (`examples/golden/`). Pins known ground truth, but no
   golden file exercises these expression shapes.
4. **The certifier** (`adl-certify`). Working as designed — it certifies the
   *encoded formula*, and the encoded formulas here are genuinely
   real-unsatisfiable. C1–C3 are encoder bugs (the formula doesn't mean what
   the region means), which is the trust surface certification explicitly
   does not cover. This is why the README's "Trust surface after
   certification" table lists the encoder as audited by testing nets only —
   and those nets (1)–(3) all missed it.

**Actionable consequence:** feeding each file's own cut constants (±1 ulp)
into the sampling-gate boundary pools would convert all three demonstrated
bugs from silent false PROVENs into gate refutations, independent of any
encoder fix. That is cheap insurance precisely because the guard is
syntactic and will keep losing whack-a-mole against expression shapes.

---

## 4. Systemic root cause and fix directions

The three criticals are one bug, architecturally: **`is_f64_faithful` is a
syntactic approximation of a semantic property** ("the exact-rational fold of
this tree coincides with its stepwise-f64 evaluation for every input"), and
every pattern encoder that re-enters flattening must remember to consult it.

Fix directions, in increasing invasiveness:

- **Minimal patches** (close the three demonstrated holes):
  - C2: in `additive_consts_dyadic`, unwrap `Neg(Num)` operands (treat
    `Neg(Num(s))` as `Num(s)` for the dyadic probe).
  - C3: route `abs_cmp`'s inner flattening through the same faithfulness
    check as `band` — either call `lin_guarded(inner)` and fall back to the
    per-bound/opaque path on `NonLinear`, or have `pattern`'s `Abs` arm
    decline non-faithful inners (the `band` NonLinear arm at :1733-1746 is
    the template).
  - C1: extend the guard to multiplicative structure. Sufficient syntactic
    condition: an expression is faithful only if **at most one non-exact
    rounding** can occur in its f64 evaluation — i.e. count *all* operations
    with non-dyadic constants (including `Mul`/`Div` chains that fold
    constants together), not just `Add`/`Sub`. This deserves a design
    decision on how conservative to be (see next bullet).
- **Structural fix** (stop playing whack-a-mole): make faithfulness a
  property of the *fold*, not the *syntax* — e.g. compute the exact fold
  *and* bound the f64 evaluation of the tree; if the analyzer cannot prove
  they coincide for all inputs, intern the whole expression as a
  structure-keyed opaque scalar (the mechanism that already exists for
  non-faithful sources). This moves the rule from "enumerate the dangerous
  shapes" to "prove the safe shapes".
- **Defense-in-depth regardless of which fix:** cut-constant-aware sampling
  pools (§3), and regression-lock all three counterexamples in
  `docs/archive/adl2/COUNTEREXAMPLES.md` + `regressions.rs` per project
  practice.

---

## 5. Contract & reporting gaps (CODE-CONFIRMED unless noted)

### G1. Emptiness, subset, and bin UNSAT verdicts are never certified — README overclaims

**Location:** `reimplementation/adl2/crates/adl-analysis/src/engine.rs:472-496`
(`region_empty`), :1028-1043 (`subset`), :1397-1411 (`bins_disjoint`),
`bin_coverage`.

`region_empty` reports `EmptyStatus::Proven` straight from the solver's UNSAT
with no `certify_unsat` call; certification is only wired into the pairwise
disjointness path (`certify_disjoint`, :1188). The README (line 219-221)
states: "When the solver returns UNSAT for a **disjointness/emptiness**
query, the unsat core is handed to a self-contained exact-rational checker."
A user reading the contract believes `PROVEN EMPTY` carries the
independent-check guarantee; it does not — only the sampling gate audits it.
Either the code or the README overclaims. Note the certifier's
`CombineBundle` schema already supports arbitrary formula sets, so extending
`certify_disjoint` to the emptiness query is mechanically straightforward.

**Severity:** correctness (contract gap). Fix the code or the doc; the code
is the better side to move.

### G2. `certified: true` survives a sampling-gate demotion

**Location:** `reimplementation/adl2/crates/adl-analysis/src/engine.rs:1255-1274`
(`gate_pair`).

When the gate refutes a *certified* PROVEN DISJOINT, the demotion rewrites
`kind`/`reason` and clears `core`, but never resets `report.certified`
(set at :592). The final `--json` row ships as
`"kind": "possibly_overlapping", "certified": true` — while `report.rs`
documents `certified: true` as present only when "a replay-checked Farkas
certificate backs the disjointness". A consumer gating CI on the `certified`
flag while ignoring `kind` trusts a retracted verdict, in exactly the failure
scenario the gate exists for. The bundle side *is* handled correctly (the
`retain` at :356-364 filters by final verdict) — only the flag leaks.

**Severity:** correctness (reporting honesty). One-line fix
(`report.certified = None`/`false` on demotion).

### G3. Multi-file `--combine`: same-basename inputs silently overwrite each other's bundles

**Location:** `reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:49-56`
(`write_bundles`).

Bundle files are named `<unit>__<idx>__<a>__<b>.json` where `unit` is the
input file's **basename**, and `write_bundles` runs once per input file into
the same directory. `smash2 verify run1/analysis.adl run2/analysis.adl
--combine out/` writes file 2's bundle over file 1's; one analysis's
certified proof silently disappears and `smash2-recheck out/` validates an
artifact the user believes covers both. The careful `unit_labels()`
disambiguation exists only on the `--cross` path. Related nit: multi-file
`--json` mode emits identical `"unit"` fields for same-basename reports.

**Severity:** correctness (medium; single-file workflows unaffected).

### G4. `--combine` never cleans stale bundles from prior runs

**Location:** same function. `write_bundles` only `create_dir_all`s and
writes; it never removes pre-existing bundles. In-run demotions are correctly
retracted (`engine.rs:356-364`), but a **re-run** into the same directory
after a verdict demotes leaves the old artifact on disk, and
`smash2-recheck` still accepts it — the "demoted verdicts must not leave
exported proof bundles behind" failure mode, shifted one run later.

**Severity:** robustness (medium-low).

### G5. Bundle metadata is unauthenticated — tampered `verdict`/`note`/names pass replay (DEMONSTRATED by audit lane)

**Location:** `reimplementation/adl2/crates/adl-certify/src/bundle.rs:219-223`.

```rust
    pub fn replay(&self) -> bool {
        self.schema == BUNDLE_SCHEMA && self.certificate.replay(&self.formulas())
    }
```

The audit lane fed `smash2-recheck` a bundle with `verdict: "PROVEN
OVERLAPPING"`, forged region names, and a forged scope `note`: it printed
`OK … (FORGED-NAME-A vs FORGED-NAME-B)` and exited 0; the summary line even
directs the user to the (tampered) `note` for scope. The math guarantee is
unaffected (the listed formulas are still unsat), but the README's "a
tampered bundle fails replay" only holds for formula/certificate/schema
tampering, and the tamper suite never touches metadata fields. Cheap
hardening: check `verdict == "PROVEN DISJOINT" && note == SCOPE_NOTE` in
`replay()`.

**Severity:** robustness (trust cosmetics around a sound core).

### G6. `--fail-on=gap` never fires on unproven bin-pair disjointness

**Location:** `reimplementation/adl2/crates/adl-analysis/src/report.rs:366-375`.

Only coverage holes (`CoverageStatus::NotProven`) fire the `gap` finding. If
`disjoint_pairs_proven < disjoint_pairs_total` (solver timeout, inexact bin
formulas), bins can double-count events — a physics finding CI cannot gate
on — while the human findings renderer flags *both* as bin issues. The CLI
gate and the report disagree about what a "gap" is. Intent unverified (SPEC
§6 doesn't define the token precisely).

**Severity:** correctness (low-medium; possibly intended).

### G7. Solver-degraded warning can be absent despite degraded verdicts

Two related holes (CODE-CONFIRMED):

- `Engine::check()` increments `spawn_failures` (drives the loud
  `solver_degraded` CLI warning), but the witness retry loop
  (`engine.rs:726-730`) and `refined_model`'s `try_with` (:956-958) call
  `s.check()` directly; Unknowns there never count.
- A spawnable-but-broken `z3` (answers `-version`, errors on every script)
  yields `Unknown("solver reported an error: …")`, which does not contain
  `"spawn"`, so `spawn_failures` stays 0 and every verdict silently caps at
  POSSIBLY/UNKNOWN with zero warning — the exact failure mode the warning
  machinery exists for.

**Severity:** robustness (low).

---

## 6. Robustness findings

### R1. Uncapped exponentiation in constant folding — one-line input hangs the CLI (DEMONSTRATED by audit lane)

**Location:** `reimplementation/adl2/crates/adl-formula/src/encode.rs:2049-2062`.

Constant-folded `Pow` accepts exponents up to `i32::MAX`; `Rat::powi`
multiplies in a loop, producing a `BigRational` of ~2³¹·log₂9 bits (~800 MB)
for e.g. `select 9^2000000000 > MET`. The audit lane's probe timed out at
>20 s (release build). No magnitude cap anywhere on the path. Fix: cap
`|n|` (e.g. 10⁴) and route larger exponents to `Unknown`/opaque.

**Severity:** robustness (DoS).

### R2. `encode_static_slice_reduce` iterates a user-controlled bound (DEMONSTRATED by audit lane)

A static slice `jets[0:4294967295]` (end bound clamped to `u32::MAX`
upstream rather than rejected) makes the reducer encoder loop
`for j in 0..n` 4.3×10⁹ times, interning quantities per iteration;
`select any(jets[0:4294967295].pt > 30)` timed out identically. Fix: cap the
reducible slice width (any physical collection is tiny) or reject
absurd widths at resolve with a diagnostic.

**Severity:** robustness (DoS).

### R3. "Never panics" has a stack-overflow hole for deeply nested formulas (UNVERIFIED)

`adl-certify` promises `certify_unsat` "never panics" and `MAX_DEPTH = 1024`
keeps *replay* recursion off a deep stack — but `saturate`/`build_child`
clone `QFormula` subtrees, and `QFormula`'s derived `Clone`/`Drop` are
recursive; a ~100k-deep nested `And`/`Or` aborts the process (SIGSEGV, not a
panic) before the depth cap matters. The crate's own test comment admits it
(`tests/budget.rs:63-66`). Reachability from real solver cores is doubtful
(encoder formulas are shallow). Fix: iterative drop or a pre-pass depth
check that fails closed without cloning.

**Severity:** robustness (latent).

### R4. `MAX_DEPTH = 1024` vs serde_json's 128 recursion limit — deep certificates can't round-trip (CODE-CONFIRMED on the parse side)

Each `Split` level costs two JSON nesting levels, so a certificate deeper
than ~63 split levels exceeds serde_json's default recursion limit (the
workspace does not enable `unbounded_depth`). Consequences: `verify
--combine` *errors the whole run* on a legitimately certified deep pair, and
`smash2-recheck` cannot parse such a bundle (fails closed, confirmed with a
300-deep synthetic JSON). Real cores are shallow, so latent — but the two
caps should agree.

**Severity:** robustness (latent).

### R5. Potential panic: `elem_preds[p].unwrap()` in `try_comb_existence` (UNVERIFIED)

**Location:** `reimplementation/adl2/crates/adl-formula/src/encode.rs` (~:1150+).
The guard-fold path unwraps `elem_preds[p]` whenever the folded formula at
slot `p` is `Some`, but `elem_preds[p]` is `None` for collections needing no
existence guards (e.g. static slices), and a `comb` mixing a slice with an
open collection can place per-element predicates at that slot. The audit
lane's probe routed to `Unknown` before reaching the unwrap, so reachability
is unverified; if reachable it is a crash, not a wrong verdict.

**Severity:** robustness (unverified).

### R6. JSONL event input validates triggers ∈ {0,1} but not tag properties

`adl-interp/src/event.rs:421-436` validates trigger flags; `btag`/`ctag`/
`tautag` element properties pass through unvalidated. A hand-written
`{"btag": 0.5}` event would violate the TAG axiom on the `smash2
run`/difftest-oracle path. **No proof impact** (`verify` consumes no event
file; the sampling battery and casegen only generate 0/1 tags; ingest
profiles are correct). An asymmetric validation that could mask bad input
data on the `run` path.

**Severity:** robustness nit.

---

## 7. Completeness gaps (weaken verdicts to POSSIBLY; never a false PROVEN)

### K1. EPRED does not inherit ancestor filter predicates down a chain

`adl-axioms/src/lib.rs:927-934` uses only the object block's **own** cuts for
the element-predicate axiom. For `object jets take Jet select pT > 30` →
`object bjets take jets select btag == 1`, EPRED on `bjets[0]` asserts only
`btag(bjets[0]) == 1`, never `pT(bjets[0]) > 30`. A region requiring
`size(bjets) >= 1 ∧ pt(bjets[0]) < 30` is physically empty but the solver
sees SAT → POSSIBLY instead of PROVEN EMPTY. (IDOM gives the wrong direction
— `pt(F[i]) ≤ pt(P[i])` — for inheriting lower bounds.)

**Severity:** correctness/completeness (missed PROVEN).

### K2. EPRED never fires for back-indexed element references

The target loop (`lib.rs:913-925`) matches only `ElemIndex::FromFront(i)`. A
cut like `select btag(bjets[-1]) == 1` gets no
`size(bjets) ≥ 1 ⇒ btag(bjets[-1]) == 1` fact, although the last element of a
filtered collection equally passed the filter (guard would be the `FromBack`
floor already used by back-index ORD).

**Severity:** completeness nit.

### K3. NNEG omits `abs(·)`, which the spec's catalog row explicitly lists

`docs/archive/specs/SPEC_ANALYSIS.md:83` lists `NNEG | pt, m, e, ht, dR,
abs(·) ≥ 0`; `adl-axioms/src/lib.rs:806` has
`NNEG_EXTFN_KEYS = ["pt", "m", "mass", "e", "energy", "dr", "sqrt"]` — no
`"abs"` (though `sqrt`, the analogous unconditional-nonnegative, is present).
Reachability of an `ExternalFn("abs")` quantity (rather than `HKind::Abs`,
handled structurally) is UNVERIFIED; if reachable, the `|·| ≥ 0` fact is
silently missing.

**Severity:** completeness nit; spec/impl divergence either way.

### K4. `pair()` never consults the region-level EMPTY verdict

If region A is solver-proven empty but the pair's `UNSAT(A⁺ ∧ B⁺)` (a
strictly harder query, same timeout) times out, the region report says
`EMPTY` while the pair reports POSSIBLY instead of the trivially-disjoint
verdict the already-proven emptiness entails. Fail-closed, internally
inconsistent; timeout-dependent, UNVERIFIED in practice.

**Severity:** robustness (low).

---

## 8. Nits

- **N1. `~=` means `!=`.** `CmpOp::ApproxEq → Rel::Ne` consistently in sema,
  encoder, axioms, interpreter, witness (the documented OPEN-4 decision, with
  a parser warning). Consistent everywhere, so no false verdicts — but any
  physicist reading `~=` as "approximately equal" gets the negation of
  intuition. Worth resolving OPEN-4 sooner rather than later.
- **N2. `MAX_SOURCE_ELEM_INDEX` clamp merges distinct huge indices**
  (`adl-sema/src/quantity.rs`): `jets[3000000000]` and `jets[4000000000]`
  intern to the same quantity — theoretical false unification, unwitnessable
  in practice (existence guards make such regions vacuous), documented bound.
- **N3. SZ0 and COMBSIZE double-assert `size(K) ≥ 0`** for combination sizes
  (`adl-axioms/src/lib.rs:730-738` vs :1074-1078); different dedup ids keep
  both, so the fact is asserted and counted twice.
- **N4. `CertNode::Contradiction` doc comment** (`adl-certify/src/
  certificate.rs:94-97`) claims it covers empty disjunctions; empty `Or`
  actually goes through `Split { branches: [] }` and a `Contradiction` there
  is correctly rejected. Code self-consistent; comment misleads a kernel
  auditor.
- **N5. Tamper suite lacks reordering coverage** (multiplier permutation
  against a non-symmetric system; branch swap in a `Split`).
- **N6. Bin-coverage "gap witness" is a raw over-approx model** with no
  candidate caveat label, unlike the overlap-witness path (`engine.rs:1441-
  1453`); behavior pinned by tests, labeling nit.
- **N7. Duplicate region names (warning only) collide in name-keyed logic**:
  the combine-bundle `retain` matches by `(region_a, region_b)` strings, and
  the verdict matrix's `by_pair` map keeps the last duplicate.
- **N8. `--explain` prints a trailing bare `UNSAT: `** for interval-proven
  empties (empty core joined); `validated_witness_values` rows unsorted while
  candidate rows are label-sorted (still byte-deterministic); dead match arm
  at `engine.rs:761` (`Rejected` never assigned).

---

## 9. Verified-correct inventory (what the audit attacked and confirmed solid)

This matters for the trust surface: the nets below are what PROVEN verdicts
actually rest on, and all were explicitly probed.

**`adl-formula` (beyond the guard):** projection mechanics
(`not` swaps `Dual`, `over()` maps Unknown→true/Dual→plus, `under()`
Unknown→false/Dual→minus; polarity duality property-tested); `Over`/`Under`
newtypes genuinely unswappable (private fields, no leaked constructors);
`Rel::negated`/`flipped` tables exact; ratio encoding clears constant
denominators exactly **with sign flip when d<0** and `d=0 → False` (matching
the interpreter's non-finite→false rule); `abs_cmp`'s six case splits
mathematically exact (the bug is solely the unguarded inner); scalar min/max
monotone identities exact with `==`/`!=` honestly Unknown; existence guards
conjoined on both projections and never negated under `reject`/`not`;
quantity identity (bare `MET` ≡ `MET.pt`, case-insensitive interning,
back-index canonicalization, structure-keyed reducer opacity) sound;
`parse_rat` shortest-round-trip decimal → exact rational, bad literals →
`Unknown`, no silent defaults.

**`adl-axioms`:** all three ORD families correct (the front-to-back
`i == 0 || k == 1` guard combinatorics verified by hand; the omitted cases
genuinely straddle); every element-dependent fact routes through the
`guarded()` definedness chokepoint with floors from the single-source
`existence_floor`; `pt_ordered` posture sound (unions/combinations/projections
and unknown-direction sorts excluded); SUB single-source only; UNI bounds
sound under both union semantics; TAG exact-name with the `btagDeepB`
regression pinned; DPHI `PI_UPPER` compile-time-asserted; TWIN
orientation-exact; IDOM guarded and requires ordered parent; COMBSIZE
degenerate cases correctly excluded; JSONL + ROOT pT-descending validation
intact on both input paths; the full `axioms_hold` battery (including
`every_axiom_holds_on_generated_physical_events` and the prohibited-axiom
regressions) is green on this tree (24/24).

**`adl-certify`:** Farkas math sound — multiplier non-negativity enforced,
zero multipliers skipped before the strictness flag, Motzkin contradiction
thresholds exact (`≤`: S<0; `<`: S≤0), all-coefficients-cancel required,
all-zero multiplier vector fails; equalities split to free multipliers
correctly; case splits require every disjunct refuted; certified set ⊆ solver
frame by construction (persistent named assertions, region-unique assert
names, fail-closed unmapped names); integrality handled honestly (real
relaxation only; integrality-only refutations surface as CANDIDATE);
defensive self-replay before emitting `Certified`; budgets fail closed; no
f64/epsilon anywhere; `#![forbid(unsafe_code)]`; 2000-case proptest +
sat-by-construction-never-certified + forged-certificate fuzzing + z3
agreement all green.

**`adl-analysis`:** polarity discipline exactly per spec (disjoint =
`UNSAT(A⁺ ∧ B⁺)`, overlap = `SAT(A⁻ ∧ B⁻)`, subset = `UNSAT(sub⁺ ∧ ¬sup⁻)`
with exact NNF negation, vacuity on R⁺ not R⁻); every one of the ~10
solver-outcome match arms degrades Unknown/timeout/error to POSSIBLY/UNKNOWN
(no wrong default); PROVEN OVERLAPPING reachable only via interpreter-
validated witnesses (rejection downgrades and files an internal diagnostic);
bounded witness retry with blocking clauses and dyadic-snap second chance;
cross-file namespacing by index, not name; determinism (no HashMap iteration,
seeded battery, canonical orderings) pinned by tests; bin edge strictness
correct; core-only certification sound (UNSAT of subset ⇒ UNSAT of
superset); reconciliation `frame_sat` precheck and polarity correct.

---

## 10. Corpus impact assessment

Grepped the 138-file example corpus for the three bug shapes:

- **`abs(<expr with +/- inside>)`:** only `abs(leptons[0].charge +
  leptons[1].charge) == 2` (CMS-SUS-16-035) — a *single* additive op, which
  passes the guard and never reaches the `abs_cmp` bypass (and charges are
  small integers, f64-exact regardless). The `pTrel` `abs` usages are
  commented out.
- **Non-dyadic multiplicative chains (`*0.x*`) / negative additive literals
  (`+ -0.1`):** no matches.

So no active corpus verdict is expected to be affected — but the golden
corpus also covers none of these shapes, which is why the bugs survived two
prior audits. The demonstrated counterexamples should be regression-locked.

---

## 11. Recommendations, prioritized

1. **Fix the three guard holes (C1–C3)** — §4 has the minimal patches and the
   structural alternative. C1 needs a short design decision (how conservative
   to be on multiplicative folding); C2 and C3 are small mechanical fixes.
2. **Regression-lock the three counterexamples** in
   `docs/archive/adl2/COUNTEREXAMPLES.md` + `regressions.rs`, and add golden
   files for the shapes.
3. **Feed cut constants (±1 ulp) into the sampling-gate boundary pools** —
   converts this entire bug class from silent false PROVEN to loud gate
   refutation, independent of the encoder fix (§3).
4. **Reset `certified` on gate demotion (G2)** — one line.
5. **Decide the certification contract (G1):** either certify
   emptiness/subset/bin UNSAT verdicts (mechanically easy; the bundle schema
   already supports it) or narrow the README's promise.
6. **Bundle hygiene (G3–G5):** disambiguate multi-file unit names, clean or
   refuse stale output directories, and check `verdict`/`note` metadata in
   `replay()`.
7. **DoS caps (R1–R2):** bound `powi` exponents and static-slice widths;
   both are one-check fixes.
8. **Completeness backlog (K1–K4)** at leisure; **OPEN-4 (`~=` → `!=`)**
   deserves a language decision before users meet it.

---

## 12. Follow-up evaluation (2026-07-28 evening) — the guard's premise is broken, not just leaky

Before implementing §11, the fix guidance in §4 was stress-tested. Result:
**the "minimal patch" for C1 as written in §4 is unsound guidance.** The
morning report framed the problem as "the guard misses some multi-rounding
shapes"; re-derivation plus three new demonstrated counterexamples show the
correct statement is stronger: **folding a comparison operand across even a
single f64 rounding operation is unsound** at half-ulp boundaries, because
stepwise f64 evaluation is only *weakly* monotone — it has flat spots one
ulp wide, and an exact-rational atom slices through them. A "count rounding
ops ≤ 1" rule would have shipped C4/C5 unfixed.

Tree state: unchanged since the morning audit (HEAD `ec4111c`); C1–C3
reproduce as before.

### New demonstrated criticals (same class, three new directions)

**C4. The guard's explicitly-allowed case is itself unsound (single dyadic
add).** `select MET + 0.5 <= 1` vs `select MET > 0.5` → PROVEN DISJOINT via
the interval prefilter (`[-inf, 0.5] vs (0.5, inf]`). Witness
`MET = 0.5000000000000001` (= 0.5 + 2⁻⁵³): the interpreter computes
`fl(MET + 0.5) = 1 + 2⁻⁵⁴·2` exactly halfway between 1.0 and the next f64;
round-half-to-even lands on **1.0 ≤ 1**, so region a PASSES, and
`MET > 0.5` PASSES. Both constants dyadic, `count_add_sub = 1` — the guard
judges this fold faithful by design. **DEMONSTRATED**
(`/tmp/adl_audit/bug4_dyadic_add.adl`).

**C5. Single multiply by a non-dyadic constant, zero additive ops.**
`select MET*0.3 <= 1` (folds exactly to `MET ≤ 10/3`) vs
`select MET >= 3.3333333333333335` (bare atom, and 10/3 < 3.3333333333333335
so the atoms are disjoint). Witness `MET = 3.3333333333333335`:
`fl(MET · fl(0.3)) = 1.0 ≤ 1` PASSES, and `MET ≥ 3.3333333333333335`
PASSES. This directly refutes the struck-through parenthetical in §C1.
**DEMONSTRATED** (`/tmp/adl_audit/bug5_single_mul.adl`).

**C6. Constant-only *multiplicative* subtrees fold exact-rationally.**
`select MET <= 0.1 * 3` folds to `MET ≤ 3/10`, but the interpreter computes
`fl(fl(0.1) · 3) = 0.30000000000000004`. Paired with
`select MET >= 0.30000000000000004`: PROVEN DISJOINT
(`[-inf, 0.3] vs [0.30000000000000004, inf]`), witness
`MET = 0.30000000000000004` passes both. The `additive_consts_dyadic` probe
protects constant *adds* (verified: `MET <= 0.1 + 0.2` correctly degrades
to POSSIBLY) but not constant *multiplies*. **DEMONSTRATED**
(`/tmp/adl_audit/bug6_const_mul.adl`).

### Attacks that did NOT land (important negative results)

- **Bare literal comparisons are sound.** Because literals rationalize via
  the shortest-round-trip decimal of the parsed f64 (`Rat::from_decimal_f64`;
  see the `adl2-rational-numeric` skill), the map f64 → Rat is an order
  isomorphism onto its image: two decimal literals denoting the same f64 get
  the same rational, and an interpreter-accepted event always has a rational
  image satisfying the corresponding atom. No fold ⇒ no divergence. This is
  why the entire hazard is concentrated in *folding across arithmetic*.
- **Constant additive subtrees** are already guarded (see C6 above — the
  additive probe works; only multiplicative constant folds leak).

### Suspected residual, not demonstrated: ratio clearing

The exact denominator clearing (`L/D ⋈ c` ⇒ `L ⋈ c·D`) replaces the
interpreter's single rounded division `fl(L/D)` with a solver-side exact
product. By the same weak-monotonicity argument there are half-ulp slivers
where `fl(L/D) ⋈ c` and `L ⋈ c·D` disagree, and the ≤-direction sliver
falls outside the over-approximation. Alignment analysis suggests concrete
f64 witnesses are blocked in the common anchored cases (representable
`c·D` boundaries) but could exist for non-representable anchors; finding
one needs a targeted search, not hand arithmetic. **Left as a documented
residual for a dedicated follow-up** — same status as the known
catastrophic-cancellation residual, which the revised fix below closes
incidentally.

### Revised fix requirement (supersedes §4's "minimal patches")

A comparison operand containing quantities may flatten to an exact linear
atom **only when every arithmetic step the interpreter would perform is
provably exact in f64** — not "at most one rounding". Concretely:

1. **Allowed exact structure:** bare `Quantity`, `Neg`, `Abs` (via a
   *guarded* `abs_cmp` — C3), multiplication/division of a quantity subtree
   by a **power-of-two** constant (exact in IEEE; the subnormal/overflow
   corner is over-approximation-safe on the UNSAT side or physically
   unreachable, and is documented), and node-node comparisons of allowed
   trees (`MET > 2*HT` — the subtraction happens solver-side only, exactly).
2. **Integer-context exemption:** `Add`/`Sub`/`Mul` over subtrees whose
   quantity leaves are all integer-sorted (`size(...)`) with integer
   constants — f64 integer arithmetic is exact below 2⁵³, so size-relation
   proofs keep their precision.
3. **Constant-only subtrees fold by f64 emulation:** evaluate stepwise in
   f64 exactly as the interpreter would, then rationalize the *result*
   (shortest-decimal). Closes C6, and turns the currently-opaque
   `MET <= 0.1 + 0.2` decidable again — strictly better on both axes. This
   also removes the exact-rational `powi` blowup path (R1) by construction.
4. **Everything else quantity-bearing** goes to the structure-keyed opaque
   scalar (`intern_opaque_scalar`) — which remains sound for *identical*
   trees across regions (identical trees compute identical f64s), so
   same-shape complementary cuts (`x+0.5<=1` vs `x+0.5>1`) still prove
   disjoint. Completeness is lost only where soundness was an illusion.

Expected corpus impact: PROVEN counts may *decrease* where they rested on
cross-form folding; per the corpus-sweep skill the invariant is that
PROVEN DISJOINT must not *rise*. Baseline sweep captured pre-fix at
`/tmp/sweep_before_20260728` for the diff.

### Display nit found in passing (D1)

`bug5`'s explanation prints `MET.pt: [-inf, 3.3333333333333335] vs
[3.3333333333333335, inf]` — two *closed* intervals sharing an endpoint,
which visually contradicts "disjoint". The left bound is the exact rational
10/3 rendered lossily as f64. The verdict logic is exact; only the
explanation string is misleading (same lossy-render family as commit
`fee9378`).

---

## Appendix A. Reproduction transcripts

All runs on this tree, `smash2` built per Workaround A
(`cargo build --release -p adl-cli --no-default-features`), subprocess z3
backend (`z3` on PATH). Input files left in `/tmp/adl_audit/`.

### A.1 — C1 (multiplicative regrouping)

`/tmp/adl_audit/bug1_mul.adl`:

```adl
region a
  select MET*0.2*0.3 + HT > 1

region b
  select 0.06*MET + HT <= 1
```

```
$ smash2 verify /tmp/adl_audit/bug1_mul.adl
  a vs b: PROVEN DISJOINT — core: a line 2 ∧ b line 5
summary: 1 pair — 1 proven disjoint, 0 proven overlapping, 0 possibly overlapping, 0 unknown

$ echo '{"MET": {"pt": 947.8280087844788, "phi": 0.0}, "HT": -55.869680527068724}' > w1.jsonl
$ smash2 run /tmp/adl_audit/bug1_mul.adl w1.jsonl
event 0: a -> PASS
event 0: b -> PASS
```

Arithmetic check: `fl(fl(947.8280087844788·0.2)·0.3) + (-55.869680527068724)
= 1.000000000000007 > 1`; `fl(0.06·947.8280087844788) + (-55.869680527068724)
= 1.0 ≤ 1`.

### A.2 — C2 (`Neg(Num)` literal)

`/tmp/adl_audit/bug2_negconst.adl`:

```adl
region a
  select MET + -0.1 > 0.3

region b
  select MET <= 0.4
```

```
$ smash2 verify /tmp/adl_audit/bug2_negconst.adl
  a vs b: PROVEN DISJOINT — MET.pt: (0.4, inf] vs [-inf, 0.4]     # interval prefilter; no solver query

$ echo '{"MET": {"pt": 0.4, "phi": 0.0}}' > w2.jsonl
$ smash2 run /tmp/adl_audit/bug2_negconst.adl w2.jsonl
event 0: a -> PASS     # fl(0.4 + -0.1) = 0.30000000000000004 > 0.3
event 0: b -> PASS     # 0.4 ≤ 0.4
```

### A.3 — C3 (`abs_cmp` guard bypass)

`/tmp/adl_audit/bug3_abs.adl`:

```adl
region a
  select abs(MET + HT - HT) < 50

region b
  select abs(MET) >= 50
```

```
$ smash2 verify /tmp/adl_audit/bug3_abs.adl
  a vs b: PROVEN DISJOINT — core: a line 2 ∧ b line 5

$ echo '{"MET": {"pt": 50.0, "phi": 0.0}, "HT": 1152921504606846976.0}' > w3.jsonl   # HT = 2^60
$ smash2 run /tmp/adl_audit/bug3_abs.adl w3.jsonl
event 0: a -> PASS     # fl(fl(50+2^60)−2^60) = 0, abs(0) < 50
event 0: b -> PASS     # abs(50) ≥ 50
```

---

## Appendix B. Audit coverage map

| Lane | Crate(s) | What it covered |
|------|----------|-----------------|
| Encoder & polarity | `adl-formula`, fragment tagging in `adl-sema` | full read of `encode.rs`/`formula.rs`/`lin.rs`; polarity mechanics; ratio/abs/band/min-max encodings; existence guards; quantity identity; f64-faithfulness guard; panic paths; **found C1–C3, R1, R2, R5, N1, N2** |
| Axiom catalog | `adl-axioms` + consumption in `adl-analysis` | full read of `lib.rs`; ORD/SUB/UNI/TAG/DPHI/TRIG/TWIN/NNEG/EPRED/IDOM/SZ*/COMBSIZE families; guards & floors; ingest sorting invariants; cross-file reconciliation; ran the crate batteries (24/24 green); **found K1–K3, R6, N3** |
| Certifier | `adl-certify` + integration | full read of kernel/search/saturate/bundle/recheck; Farkas math; boolean case-split logic; integrality posture; budgets; tamper suite; empirical forged-bundle probes; ran the crate battery (green); **found G1(integration), G5, R3, R4, N4, N5** |
| Verdict orchestration | `adl-analysis` + `adl-cli` verify path | full read of all 7 src files; verdict assembly; witness realization & re-validation; sampling gate; certification gating; bin checks; `--fail-on` exit codes; determinism; cross-file plumbing; solver-backend conformance; **found G2–G4, G6, G7, K4, N6–N8** |

Follow-up: every critical claim (C1–C3) was independently reproduced by the
reviewing agent with the built binary and the reference interpreter before
inclusion in this report; G1/G2 were confirmed by direct code reading.

---

## 13. Implementation status (evening of 2026-07-28)

Three Grok 4.5 subagents implemented the §12 revised fix and the
contract/hygiene items. Parent re-verification after their work:

### Encoder (`adl-formula`) — C1–C6, R1, R2

Replaced `is_f64_faithful` / `count_add_sub` / `additive_consts_dyadic` with
an **exact-f64 foldability** gate (`is_exact_f64_linear` and helpers in
`encode.rs`). Quantity-bearing trees flatten only when every interpreter
arithmetic step is IEEE-exact (bare quantity, Neg/Abs of allowed trees,
×/÷ by power-of-two, integer size arithmetic); constant-only subtrees are
f64-emulated then rationalized; everything else goes to structure-keyed
opaque scalars. `abs_cmp` no longer re-flattens unguarded. Static-slice
reduce capped at 1024; huge const powers take the f64-inf path (R1/R2).

| Counterexample | Post-fix `verify` | Interpreter both PASS |
|---|---|---|
| bug1–bug5 (C1–C5) | POSSIBLY OVERLAPPING | yes |
| bug6 (C6) | PROVEN OVERLAPPING | yes |
| sameshape (`MET+0.5` ≤/≥ 1) | PROVEN DISJOINT (shared opaque) | n/a |

Regression locks: `docs/archive/adl2/COUNTEREXAMPLES.md` CE-8…CE-13;
`adl-analysis/tests/f64_fold_regressions.rs`;
`adl-formula/tests/exact_f64_fold.rs`.

### Bundle hygiene (`adl-cli` + `adl-certify`) — G3–G5, N4/N5, README G1

- Multi-file `--combine` uses `unit_labels()` (same basename → path-qualified).
- Stale `*__NNN__*__*.json` cleaned before write; non-matching files kept.
- `CombineBundle::replay()` requires schema + verdict + scope note + cert.
- Tamper suite extended (forged verdict/note, multiplier permute, branch swap).
- README: certification claimed for **pairwise disjointness only**;
  emptiness/subset/bin certification noted as open.

### Analysis honesty (`adl-analysis` + `adl-interp`) — G2, G6, G7, sampling

- Gate demotion clears `certified`.
- Cut-constant-aware sampling (`battery_with_cuts`, ±1 ulp around cut
  literals; cap 32 constants, `|v|≤1e6`). Sampling event count in CLI
  snapshot: 64 → 88 (accepted).
- `--fail-on=gap` also fires on unproven bin-pair disjointness.
- Solver-error / spawn accounting wired through witness-retry and
  `refined_model`.

### Corpus sweep (68 examples, subprocess release)

| metric | before | after | Δ |
|---|---:|---:|---:|
| pairs | 1893 | 1893 | 0 |
| PROVEN DISJOINT | 816 | 794 | **−22** |
| PROVEN OVERLAPPING | 70 | 53 | −17 |
| CANDIDATE OVERLAPPING | 33 | 44 | +11 |
| POSSIBLY | 974 | 1002 | +28 |
| UNKNOWN | 0 | 0 | 0 |
| nonzero exits | 0 | 0 | 0 |

Disjointness **decreased** only (soundness direction per the corpus-sweep
skill). Largest mover: CMS-SUS-16-032 variants (41→30 proven disjoint) —
cross-form arithmetic folds that were never sound in f64. No new INTERNAL
DIAGNOSTICS files observed. Golden `# GOLDEN` headers unchanged (batteries
green); one report-rendering snapshot updated for the weaker CMS-SUS-16-032
matrix.

### Test status (re-verified after agents)

- `cargo test --release -p adl-formula -p adl-certify -p adl-analysis -p adl-interp` (native Workaround B): **all green**, 0 failures.
- `cargo test --release -p adl-cli --no-default-features --test cli`: green after accepting the sampling-event-count snapshot.
- Full workspace battery was interrupted mid-run earlier; the four load-bearing crates above are the ones that own this change. Re-run `--workspace` before commit if desired.

### Deliberately still open (from original audit)

| ID | Status |
|---|---|
| G1 (certify emptiness/subset/bin) | **doc-fixed**; code still open |
| K1–K4 (EPRED completeness, empty-pair short-circuit) | backlog |
| R3–R4 (deep-formula stack / serde depth) | backlog |
| N1 (`~=` → `!=`, OPEN-4) | language decision |
| Ratio-clearing residual (§12) | documented residual |

**Working tree:** changes are uncommitted (agents instructed not to commit).
Architecture PNG/SVG under `reimplementation/` are unrelated untracked
artifacts from earlier work.
