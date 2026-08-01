# The verify engine, end to end — state of 2026-07-31

A complete technical description of what happens when you run
`smash2 verify [--cross] file.adl…`, as the system stands after the
production campaign. The canonical user document is
`reimplementation/README.md`; this is the engineer's tour, one stage per
section, with the trust story threaded through.

## 0. The contract the whole pipeline serves

| Verdict | Claim | Proof side | Nets |
|---|---|---|---|
| PROVEN DISJOINT | no loader-valid event is in both regions | UNSAT over over-approximations (`Ax ∧ A⁺ ∧ B⁺`) | certificate + sampling gate + refute gate |
| PROVEN OVERLAPPING | a concrete event is in both | SAT over under-approximations + the event itself | interpreter re-validation (definitional) |
| PROVEN SUBSET | every A-event is a B-event | UNSAT (`Ax ∧ A⁺ ∧ ¬B⁻`) | certificate + gates |
| EMPTY | the region accepts nothing | UNSAT (`Ax ∧ R⁺`) | certificate + gates |
| CANDIDATE / POSSIBLY / UNKNOWN | no claim | — | vacuously sound; reachable only by demotion |

**The reference interpreter is the meaning.** Every claim above is a
statement about what `adl-interp` decides; the prover is judged against
it, and where they could disagree, the interpreter wins.

## 1. Front end: parse → HIR

`adl-syntax` is a hand-written recursive-descent parser (statement-level
recovery, spans, suggestions; expression depth capped at 64 with a located
diagnostic — corpus maximum is 9). `adl-sema` resolves to the HIR: names
are resolved, defines inlined at reference sites, every node carries a
fragment tag (`InFragment` or `Unsupported(reason)`).

**Identity is structural, never nominal.** Collections and quantities
intern by structure into shared tables; a pure rename IS its source; a
filtered collection is never its parent. The one interning discipline
that history proved load-bearing lives in `ElemPredInterner`: an element
predicate containing an `Unsupported` node always receives a fresh,
never-shared id, because lossy `<unsupported: …>` render strings must not
become identity (two real false-PROVEN classes came from exactly that,
one through element predicates, one through opaque sort keys — both now
unit-namespaced or fail-closed). Cross-file `--cross` merges units into
one identity space with the same interner and per-unit namespacing of
every opaque string; inheritance is flattened to per-statement granularity
(`flatten_inherits`) so unsat cores can name — and certificates can
drop — individual cuts regardless of how the region was written.

## 2. The event model (M3)

Events load as **exact rationals** end to end: every JSONL value becomes
the shortest-decimal `Rat` of its f64 (`0.3` is `3/10`). The loader
defines E, and E equals the axioms' domain: collections must be
pT-descending, magnitudes NNEG covers (pt, e, MET.pt, HT-family) must be
≥ 0, exact-name tags must be in {0,1}, and a missing MET vector is a hard
evaluation error. An event violating any of these is rejected at load
with a physics-facing message — so no axiom can be false on an admitted
event.

The interpreter evaluates in `NumVal { Exact(Rat), Approx(f64) }`:
literals, event values, sizes, `+ − × ÷`, neg/abs/min/max stay `Exact`;
irrationals (`sqrt`, `dR/dPhi/dEta`, LV kinematics, `^`) are `Approx`;
a mixed comparison converts Exact→f64 at the comparison edge. Comparisons
over an ABSENT property are a decidable soft `false` (the ROOT/C++ NaN
convention); a missing event-level datum (MET vector, event scalar, trigger
flag) is a HARD error, so the event is `In` no region that reads it. The
prover models both — see §3.

## 3. Encoding: HIR → three-valued formulas → Over/Under

Each region statement encodes into a three-valued `Formula` whose leaves
are exact-rational linear atoms; anything unencodable is an explicit
`Unknown(diag)`, convention ambiguities are `Dual{plus, minus}`. Two
projections give the polarity pair: `R⁺` (Unknown→true, Dual→plus — a
guaranteed superset) and `R⁻` (Unknown→false, Dual→minus — a guaranteed
subset). **The type system enforces consumption**: disjoint/empty/subset
proofs can only be handed `Over` values, overlap only `Under` — the wrong
polarity does not compile.

Key encoding disciplines, each earned by a counterexample:
- **Flattening licence (M4)**: a comparison operand folds to an exact
  atom only when the interpreter provably evaluates it `Exact`
  (`is_exact_valued`); constant trees fold through the same shared
  `adl_sema::num` code the interpreter runs, so their boundaries agree
  bit-for-bit. Approximate trees keep the conservative old gate
  (pow2 scaling only, `fl(k)` thresholds) or intern as structure-keyed
  opaque scalars.
- **Presence (definedness) modelling (Phase B, 2026-08-01)**: the prover
  reasons over TOTAL rational valuations while the interpreter's is
  PARTIAL, so the encoding gained a way to say "this quantity has a value".
  Every cut over a possibly-absent quantity encodes `p_q ≥ 1 ∧ atom`, where
  `p_q` is a first-class interned quantity (`Quantity::Present`) whose
  meaning is fixed against the interpreter: 1 iff `eval_quantity` yields a
  value. `QuantityTable::absence` classifies each quantity
  `Never | Soft | Hard` and is the single source for the encoder chokepoint
  AND the axiom guards.
  Negation follows: a SOFT-absent leaf negates by plain De Morgan to
  `p < 1 ∨ ¬atom` — "absent, or present and failing", which is exactly the
  interpreter's rule, so `reject` is EXACT and no longer poisons the
  `Under` projection. A HARD-absent leaf keeps its presence literal
  CONJOINED through the negation, because `¬Unknown` is Unknown. The
  Phase-A `Dual` hedge is deleted; the placement canonicalizations
  (`not not c → c`, `reject not X → select X`) stay and the
  rewrite-invariance pin is now over presence shapes.
  The definedness footprint survives algebraic cancellation
  (`LinExpr.mentioned`): `MET + HT − HT > 50` still requires HT, because
  the interpreter evaluates it.
  Invariant **E-i** — no atom over a possibly-absent quantity outside a
  presence-guarded shape — is machine-checked over every corpus region.

## 4. The engine: three proof paths, one demotion direction

Per pair (canonical query order, so swap(A,B) cannot change results):

1. **Interval fast path.** Each region's `R⁺` And-spine folds into an
   `IntervalMap` (single-quantity bounds with **provenance** — which named
   assert produced each bound). Two disjoint intervals on the SAME
   quantity, or an empty self-interval, decide DISJOINT/EMPTY without the
   solver — and since Phase B a refutation over a possibly-absent quantity
   is accepted only if BOTH regions also pin it PRESENT, which makes E-i a
   runtime check on the one path with no certificate behind it — and since W2a emit a **closed-form Farkas certificate**
   (`certify_bounds`: the two bounds, multipliers `1/|a|`, Motzkin
   strictness), accepted only if the trusted replay kernel agrees.
2. **Solver path.** The persistent z3 child (one per solver instance;
   `(reset)` + full script per query; sentinel-framed replies; `sat`/
   `unsat` believed, everything else — `unknown`, `timeout`,
   `unsupported`, `(error …)`, process death — is `Unknown`, fail-closed).
   An UNSAT core is handed to `adl-certify`: a DPLL(Farkas) replay kernel
   (~1.1k lines, exact arithmetic, no search at check time) that must
   reproduce the refutation; failure demotes to CANDIDATE. Every proven
   tier — pairwise, EMPTY, SUBSET, bins — is certification-gated.
3. **Overlap path.** SAT on `A⁻ ∧ B⁻` yields a model; the witness
   realizer builds a concrete `Rat` event from it (layered refinement:
   interior-ε wishes, dyadic snapping, bounded retries) and the
   **interpreter must accept it in both regions** — otherwise CANDIDATE
   or POSSIBLY. Overlap is therefore sound by construction; no encoder or
   axiom fact is load-bearing.

**Demotion is one-way.** The certifier, the sampling gate (battery of
loader-valid boundary events fired at every UNSAT-side claim), and the
adversarial refute gate (cut-anchored + flat-spot probes, priority-ordered,
domain-clamped) can only weaken verdicts. No code path promotes.

## 5. Axioms and cross-file reconciliation

Nineteen axiom families (ORD, SZ0, SUB, UNI, NNEG, EPRED, IDOM, TAG, PRES,
PDEF, EPRES, …),
each with a written physical justification, an assumption tag surfaced in
reports, and an events test (now including an ABSENCE battery — property-less
elements and events with no MET/HT/trigger block); a prohibited list
documents the plausible axioms that testing killed. Every element-touching
family is emitted through one guard chokepoint that makes the instance
VACUOUS wherever the interpreter obtains no value, which is what deletes the
old "canonical pad-with-0 extension satisfies the fact" premise. Three of
the families are presence bookkeeping: PRES (`0 ≤ defined(q) ≤ 1`), PDEF
(presence ⇒ element existence) and EPRES (membership in a filtered
collection PROVES the filtered-on property present, via a fail-closed
syntactic recogniser). Under `--cross`, reconciliation lowers
same-base filtered collections onto one generic element and proves
refinement (XSUB) or equivalence (XEQ) **before** any size fact is
asserted — and since W2a each derived fact carries its own certified
derivation, embedded in bundles. The one extra-logical assumption in the
system, A1 ("same detector-base name = same physical input across
files"), is consumed only here and printed on every claim that uses it.

## 6. The trust surface a user sees

Every proven line carries its evidence inline:

```
SR_low vs SR_high: PROVEN DISJOINT [certified · gate 124/124 · probes 64] — …
```

plus a `== trust ==` block (solver identity, nets and their scale,
per-tier counts with certification percentage, refutation totals, the
assumption list). `--explain` adds proof paths, certificate sizes, axiom
statements, and full witnesses. `--json` is additive-versioned.
`--fail-on=unknown` gates CI on solver silence; a failing solver is loud
in the header, the trust block, and the findings.

`--combine DIR/` exports one `smash2-combine/2` bundle per certified
claim: the named formula set in replay order, the certificate, the
derivation chain of every reconciliation fact used, a quantity
dictionary, per-assert source text, producer version and input SHA-256s —
deterministic, no timestamp. `smash2-recheck` replays all of it with the
kernel alone (no z3, no smash2 analysis) and states exactly what replay
does not vouch for.

## 7. What still stands between here and "no known false-claim class"

The **absent-property seam is closed** (Phase B, 2026-08-01): presence
indicators make definedness a conjunct of the formula instead of a premise
about the event, the guarded-negation hedge is deleted, and the withdrawn
complement-of-one-predicate proofs returned. Five live false claims died
with it, each verified through `smash2 run` on its counterexample event
before and after — the TAG-subset and `abs ⋈ negative` folds the previous
edition of this section named, plus three found while validating the fix
(`select HT >= 0`, `select MET >= 0`, and a cancelled `MET + HT − HT`).

Validating it turned up a NEW seam of the SAME false-claim shape, still
OPEN, one layer over: `size(C)` is treated as total, but materializing `C`
raises a HARD evaluation error when `C`'s filter predicate is out of
fragment, so `select size(weird) >= 0` is `In` for no event while SZ0
discharges the cut and a `subset` ships. It is not the absent-property
class — it is out-of-fragment data-independent refusal — and it needs the
unsupported-predicate set plumbed into `QuantityTable` before `absence` can
classify it. Recorded as SOUNDNESS_PROOF §8 item 1b with the repro.

The honest statement is therefore the narrow one: **the absent-property
seam is closed by construction**, and the machinery to close its analogue
is built and waiting.

Cost, measured: presence roughly halves again the headroom, not the
verdicts. Corpus sweep 70 s → 105 s (1.5×); the 3-file `--cross` 3.2 s →
8.4 s (2.6×), with identical verdicts; declared quantities on the largest
corpus file +60%. Every proven tier stayed 100% certified.

## 8. The premise ledger (proof §3, post-campaign status)

| Premise | Status |
|---|---|
| P1/P1m — quantity identity | by construction (interner + namespacing), oracle-audited |
| P2 — leaf faithfulness | arithmetic **by construction since M3/M4** (shared `num`); definedness **by construction since Phase B** (presence literals in the leaf; E-i machine-checked over 447 corpus regions) |
| P3 — axiom truth | domain enforced by the loader; families tested (absence battery included); instantiation **guarded by construction since Phase B** — the pad-with-0 premise is deleted |
| P4 — refutation validity | certificate-replayed on every proven tier |
| P5 — witness validity | definitional (interpreter accepts) |
| A1 — base-name identity | declared, per-claim surfaced, only in XSUB/XEQ |
