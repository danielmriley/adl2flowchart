# COUNTEREXAMPLES.md — real bugs found by the TESTING §2 heavyweight layers

Every entry below was found mechanically by the property-based
encoder-vs-interpreter battery or the metamorphic battery
(`crates/adl-difftest/tests/{prop_encoder_vs_interp,metamorphic}.rs`),
minimized, fixed in engine code, and locked as a regression test in
`crates/adl-difftest/tests/regressions.rs` (CE-1…CE-7) or
`crates/adl-analysis/tests/f64_fold_regressions.rs` (CE-8…CE-13).
Dated 2026-06-11 (CE-1…CE-7) / 2026-07-28 (CE-8…CE-13).

## CE-1 — false PROVEN DISJOINT: unguarded negation over missing elements

```adl
object jets
  take Jet

region RA
  reject pT(jets[0]) < 50

region RB
  reject pT(jets[0]) >= 50
```

The interpreter (SPEC_LANGUAGE §4.4 extended): a comparison over a
non-existent element is **false**, so `reject` of it is **true** — the
zero-jet event is a member of BOTH regions. The encoder negated the bare
atom (`pt0 ≥ 50` vs `pt0 < 50`), got UNSAT, and claimed PROVEN DISJOINT.
A clean instance of the legacy lesson "guarded references do not imply
existence", this time on the negative polarity.

**Fix (adl-formula `encode.rs`)**: every exact comparison leaf is
conjoined with element-existence guards `size(C) > i` for each
element-indexed quantity it references (directly or through angular-pair
anchors). NNF negation then distributes over the guard
(`¬(size>0 ∧ atom) = size≤0 ∨ ¬atom`), which is exactly the
interpreter's reading. Verdict after fix: PROVEN OVERLAPPING with the
validated empty-jets witness.

## CE-2 — false REGION EMPTY (same root cause)

```adl
region RA
  reject pT(jets[0]) > 30
  reject pT(jets[0]) < 60
```

`pt0 ≤ 30 ∧ pt0 ≥ 60` is UNSAT ⇒ "provably selects no events" — but the
zero-jet event passes both rejects. Found independently through both the
solver path and the interval fast path (the un-guarded negated atoms sat
on the And-spine). Same fix; the guards turn each reject into a
disjunction, which the interval spine soundly ignores.

## CE-3 — false PROVEN SUBSET (same root cause, UNSAT direction)

```adl
region RA
  reject pT(jets[0]) < 50

region RB
  select pT(jets[0]) >= 50
```

Claimed RA ⊆ RB (and RB ⊆ RA — "equal"). The zero-jet event is in RA but
not in RB. Same fix; RB ⊆ RA remains correctly proven.

## CE-4 — event loader perturbs values: serde_json lossy float parsing

`parse_event` returned `50.99999904632568` for the JSON literal
`50.999999046325684` (std `str::parse::<f64>` is correctly rounded;
serde_json's default float path is not). Every loaded event value could
be off by ulps — breaking bit-exact witness re-validation and, more
importantly, the loader's basic fidelity for `smash2 run` users.

**Fix (adl-interp `Cargo.toml`)**: enable serde_json's `float_roundtrip`
feature.

## CE-5 — verdict instability under swap(A,B): model-dependent witness search

Found by the metamorphic battery: semantically identical files (regions
swapped / double-negated / define-inlined / …) flipped between PROVEN
OVERLAPPING and POSSIBLY OVERLAPPING because witness realization
depended on whichever model z3 happened to return. Distinct sub-causes,
each reproduced and fixed:

1. **Unbounded model sizes** (`size = 401` > realizer cap 64) —
   refinement now wishes `size ≤ 64` for every mentioned size.
2. **Forced size bumps**: the realizer forced every mentioned element to
   exist even when the model said `size = 0` (a region can be IN via a
   rejected missing-element comparison — only correct after CE-1's
   guards made model sizes authoritative). Realizer now honors explicit
   model sizes.
3. **dPhi at the wrap discontinuity**: z3 picked `dphi = next_down(π)`
   (the DPHI axiom/wish bound); `wrap(v) = ((v+π) mod 2π) − π` flips it
   to −π (a 2π error). Wish bounds are now dyadic and strictly inside
   the wrap range (±3.140625), and the realizer fix-point-corrects phi
   through the actual `wrap_dphi`.
4. **Boundary vertices vs f64 re-evaluation**: exact-rational sums at
   `Σ = k` vertices round one ulp off in f64. The refinement now prefers
   ε-interior models of the under-formulas (ε = 2⁻²⁰, dyadic; pure-Int
   size atoms exempt — fractional tightening would change their
   meaning), and prefers `dPhi = 0` outright (dyadic, trivially
   realizable) when tolerated.
5. **z3 `approx_f64` is lossy** (truncated decimal string): an exactly
   representable dyadic model value came back 2 ulps off. The native
   backend now extracts numerator/denominator and divides (exact for
   dyadics, correctly rounded below 2⁵³).
6. **Non-dyadic equality vertices**: two quantities sharing a
   non-representable fractional part (difference exactly 50 in rationals)
   round independently in f64 and miss the equality. A rejected model now
   gets a second chance snapped to the 2⁻²² dyadic grid (equal fractional
   parts move identically, so exact differences survive), then bounded
   blocking-retry (≤ 6 models) before downgrading.

Additionally, pairwise solver queries now run in canonical (name-sorted)
order so declaration order cannot influence model selection.

The TESTING §3 downgrade path is untouched: an unrealizable witness
still downgrades to POSSIBLY with an internal diagnostic — these fixes
make the *search* complete enough that semantically equal inputs reach
the same verdict.

## CE-6 — witness realizer: folded atoms reference unpinned properties

```adl
region RA
  select size(jets) >= 1
  select size(eles) >= 1
  select dPhi(jets[0], eles[0]) - dPhi(jets[0], eles[0]) < 25
```

The encoder folds the third cut to `True` (correct over the SPEC §4.1
event model, where objects carry their properties), so no formula
mentions `phi` and the model never pins it; the realizer built objects
WITHOUT `phi`, the interpreter soft-failed the comparison, and the
witness was rejected. Synthetic witness objects now always carry the
standard property set (pt fill always runs; eta/phi/m and the exact-name
tags default to free values after angular realization), and a hard
"missing event-level datum" during validation patches the free scalar /
trigger / MET component to 0 and re-evaluates instead of rejecting.

## CE-7 — certification tier is not invariant under statement inlining

```adl
define d0 = not ((Eta(jets[1]) >= 2 or (pT(eles[1]) <= 50 and pT(eles[-1]) >= 100)))

region RA
  select not (d0)
  select ((... or ...) and (d0 or d0))     # RA is empty: ¬d0 ∧ d0

region RB
  RA                                        # inherit; paste inlines RA's selects
  select (...)
```

Found by the metamorphic battery (2026-07-05, post abs-unlock RNG re-roll).
The pair is UNSAT-disjoint in both renderings — RA is empty — but the
verdict tier differed: paste PROVEN DISJOINT, inherit CANDIDATE DISJOINT.
z3's minimized unsat core for the inherit rendering was {one RA select,
the monolithic `RB: RA` reference conjunction}, and `adl-certify`'s
case-split search exceeded its 100 000-branch budget inside the reference
conjunction before finding the shallow `d0 ∧ ¬d0` clash; the paste core
was two small facts that certify instantly. Certification is budgeted
best-effort and runs on the solver's core CHOICE, which inlining
legitimately changes — so `Summary::consistent` now treats {PROVEN,
CANDIDATE} DISJOINT as one metamorphic class, the same way it already
treated overlap proof strength. Disjointness itself, empties, subsets and
interpreter membership remain strict.

Follow-up (certifier completeness, not soundness): unit-propagating
top-level conjuncts before case-splitting would find `d0 ∧ ¬d0` without
entering the branch explosion, certifying the inherit core too.

## CE-8 — false PROVEN DISJOINT: multiplicative regrouping (C1, 2026-07-28)

```adl
region a
  select MET*0.2*0.3 + HT > 1

region b
  select 0.06*MET + HT <= 1
```

Exact-rational folding unifies `(MET·0.2)·0.3` with `0.06·MET`, but
stepwise f64 rounds twice on the left and once on the right. Witness
`MET=947.8280087844788, HT=-55.869680527068724` passes both regions
under the interpreter. Old verdict: PROVEN DISJOINT.

**Fix (adl-formula encode.rs):** strict f64-exactness foldability —
quantity-bearing arithmetic that is not provably exact in f64 interns as
a structure-keyed opaque scalar. Verdict after fix: POSSIBLY OVERLAPPING.
Regression: `f64_fold_regressions::c1_multiplicative_regrouping_not_proven_disjoint`.

## CE-9 — false PROVEN DISJOINT: `Neg(Num)` additive literal (C2, 2026-07-28)

```adl
region a
  select MET + -0.1 > 0.3

region b
  select MET <= 0.4
```

The dyadic-constant probe missed `Neg(Num)` operands, so `MET + -0.1`
folded across a non-dyadic add. Witness `MET=0.4` passes both. Old
verdict: PROVEN DISJOINT.

**Fix:** same strict foldability rule (and cut-constant-aware sampling
gate as defense-in-depth). Verdict after fix: POSSIBLY OVERLAPPING.
Regression: `f64_fold_regressions::c2_neg_num_literal_not_proven_disjoint`.

## CE-10 — false PROVEN DISJOINT: `abs_cmp` re-flatten (C3, 2026-07-28)

```adl
region a
  select abs(MET + HT - HT) < 50

region b
  select abs(MET) >= 50
```

`abs_cmp` re-flattened its inner with unguarded `lin`, cancelling
`+ HT - HT` exactly while f64 keeps a residual at huge HT. Witness
`MET=50, HT=1152921504606846976`. Old verdict: PROVEN DISJOINT.

**Fix:** strict foldability + guarded `abs_cmp`. Verdict after fix:
POSSIBLY OVERLAPPING.
Regression: `f64_fold_regressions::c3_abs_cancellation_not_proven_disjoint`.

## CE-11 — false PROVEN DISJOINT: single dyadic add (C4, 2026-07-28)

```adl
region a
  select MET + 0.5 <= 1

region b
  select MET > 0.5
```

The guard's previously-allowed case (one dyadic add) is itself unsound
at a half-ulp flat spot: witness `MET=0.5000000000000001` has
`fl(MET+0.5)=1.0≤1` and `MET>0.5`. Old verdict: PROVEN DISJOINT
(interval prefilter).

**Fix (AUDIT §12):** folding across *any* rounding op is refused.
Verdict after fix: POSSIBLY OVERLAPPING.
Regression: `f64_fold_regressions::c4_dyadic_add_not_proven_disjoint`.

## CE-12 — false PROVEN DISJOINT: single non-dyadic multiply (C5, 2026-07-28)

```adl
region a
  select MET*0.3 <= 1

region b
  select MET >= 3.3333333333333335
```

Folds to `MET ≤ 10/3` vs bare `MET ≥ 3.333…5` (disjoint as rationals);
witness `MET=3.3333333333333335` has `fl(MET·fl(0.3))=1.0≤1`. Old
verdict: PROVEN DISJOINT.

**Fix:** same. Verdict after fix: POSSIBLY OVERLAPPING.
Regression: `f64_fold_regressions::c5_single_mul_not_proven_disjoint`.

## CE-13 — false PROVEN DISJOINT: constant-only multiplicative fold (C6, 2026-07-28)

```adl
region a
  select MET <= 0.1 * 3

region b
  select MET >= 0.30000000000000004
```

Exact fold gives `MET ≤ 3/10`; interpreter computes
`fl(fl(0.1)·3)=0.30000000000000004`. Witness at that boundary passes
both — the regions genuinely share the f64-emulated cut. Old verdict:
PROVEN DISJOINT.

**Fix:** constant-only subtrees fold by f64 emulation, then rationalize
the result. Verdict after fix: PROVEN OVERLAPPING (or weaker-but-not-
disjoint). Regression: `f64_fold_regressions::c6_const_mul_not_proven_disjoint`.

## CE-14 — false PROVEN DISJOINT: non-pow2 constant-denominator ratio clearing (2026-07-29)

Found in the post-28859fb double-check. Exact-f64 fold closed C1–C6 but
left `ratio()` clearing `L/d ⋈ c` → exact `L ⋈ c·d` for *any* constant
`d`. Against stepwise `fl(L)/fl(d)` that is unsound for non-power-of-two
`d`. Sampling caught the MET twin; object `pT` slipped through the
interval path with `refutations: 0`.

```adl
object jets
  take Jet

region a
  select pT(jets[0]) / 0.3 <= 0.1

region b
  select pT(jets[0]) > 0.03
```

Witness `jets[0].pt = 0.030000000000000002`: `fl(pt/0.3)=0.1≤0.1` and
`pt>0.03`. Old verdict: PROVEN DISJOINT (even under `--no-solver`).
Same class: electron pT, `min(pT/0.3,…)`, band `pT/0.3 [] …`, and a
false mutual-SUBSET twin (`pT/0.3≤0.1` vs `pT≤0.03`).

**Fix:** clear constant denominators only when `d` is a ±power of two;
otherwise intern the whole ratio as a structure-keyed opaque. EPRED
`clear_ratio` matches. Gate also emits dedicated Jet/Electron/Muon
boundary events. Regression:
`f64_fold_regressions::c14_ratio_const_den_not_proven_disjoint`.
