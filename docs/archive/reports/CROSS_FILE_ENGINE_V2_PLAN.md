# Cross-file validity engine v2 — synthesized plan

**Date:** 2026-07-16 · **Method:** three independent designs (identity-first,
proof-reach-first, artifact-first), each grounded in the current code
(reconcile.rs, engine.rs, merge.rs, the XSUB/XEQ catalog rows), then judged
and synthesized by a fourth agent that verified every load-bearing claim
against real line numbers. The three full designs are preserved in the
session workflow transcript; this document is the combined roadmap.

**One-line rationale:** the proof-reach design wins execution order (its
phases are quantified against the ABS_UNLOCK corpus baseline), the
identity design wins the epistemics (identity as recorded, per-verdict
evidence rather than prose residuals), and the artifact design wins the
endgame (the exportable combination certificate IS the field's goal) —
with its certify-at-emission rule promoted to a standing invariant.

**Standing constraints (global):**
- Every new fact kind must be Farkas-certifiable and flow through
  `recon_facts` (no side-channel assertions).
- Every phase ends with a corpus A/B sweep and hand-classification of
  every PROVEN increase.
- An adversarial review round gates every fact-emitting phase.

Verified the load-bearing claims of all three designs against the codebase (merge.rs:409 `unit_ord` opaque namespacing; engine.rs:1002 `reconcile`/1097 `certify_disjoint`/`recon_facts`; reconcile.rs:105 ext gate; encode.rs:1722,1742 `Opaque(body_key)` reducer interning; quantity.rs:491 `filter_chain` returning None for Union; adl-certify `Certificate`+`QRat` serde ready). All three are accurately grounded — no design disqualifies on factual errors.

# SCORES (1–10 per dimension)

## Design A — identity-first
- **Soundness rigor: 8.** The tier lattice (Proven/Declared/Convention), fail-closed conflict namespacing, and demotion-only threshold are exemplary. Provenance and ledger only remove or annotate — trivially sound. Two real hazards, both self-identified: (1) XCOLL widens the documented float-vs-real gap from sizes to *element properties* — the memory on this gap says the general additive-boundary case is unresolved, so full-generality XCOLL is premature; (2) digest injectivity is load-bearing and recreates the exact failure class of the earlier CRITICAL false-unify.
- **Real-corpus impact: 5.** The current corpus has zero provenance declarations, so M1/M2 change no verdict by default (the design admits this). XCOLL is the only verdict-adding piece and its corpus yield is unquantified. The unique impact is *distinctness proof* (killing false PROVENs) — valuable but unmeasured.
- **Implementability: 6.** Ledger threading through the memoized merge and digest rewrites in resolve.rs/dump.rs touch the highest-churn, most adversarially-scarred code (the 6 merge bugs). Feasible, verified against real line numbers, but heaviest integration risk of the three.
- **Cost: ~3–4 wks** (L+M+M+L–XL). Honest.

## Design B — proof-reach-first
- **Soundness rigor: 7.5.** Per-fact written justifications are the best of the three (XSUM's sublist+nonneg argument, XIX-incl's pointwise indicator identity, XUNI's dual-reading argument are all correct given "take=filter, same base"). Structural gates (single `CollProp`, no slice, distinct-base unions) are the right fail-closed shape. Two deductions: XSUM is a false-PROVEN factory if the monotonicity gate leaks (design knows; NNEG exact-name sharing mitigates), and — the key gap — **B stacks five new fact kinds on the same-base-name convention while remaining silent about it**, exactly the residual A attacks. XIX-mono/incl being *unconditional* (not proof-gated) demands extra adversarial scrutiny, though the arguments hold.
- **Real-corpus impact: 9.** The only design measured against the ABS_UNLOCK baseline: 28/78 pairs XSUB-reached, 45 HT mentions, 99 size cuts on unions, and the two documented honest downgrades (032/033) explicitly recovered by Phase D. HT and nlep are genuinely *the* SR axes in this corpus.
- **Implementability: 9.** Everything flows through the reconcile→engine→catalog choke point that survived three adversarial rounds; `filter_chain`, `derived_size_le`, `recon_facts` are all exactly where B says. Phase A's interning-key churn is the one risk and is correctly isolated as its own guarded commit. Phase D is pure SAT-side — zero soundness exposure, verified claim.
- **Cost: ~3–4 wks**, best-decomposed (each phase independently shippable and measurable).

## Design C — artifact-first
- **Soundness rigor: 9.** Adds essentially no new fact-emission risk: §3.1 is retention, §3.2 re-packages already-trusted formulas, §3.3 *narrows* emission. The four-layer trust decomposition (L1/L2 zero-trust, L3/L4 auditable) is the most honest soundness statement in any of the three, and the mandatory `excluded_pairs` coverage check directly closes the field's double-counting hazard. Risk #1 (overclaiming "zero trust") is correctly ranked as the worst failure mode.
- **Real-corpus impact: 6 as verdicts, 10 as mission.** Zero new proven relationships. But per the strategic-direction memory, the exportable combination certificate *is* the 5-year-unfilled field goal and MULTIFILE_PLAN Phase 5. Certification-coverage shortfall (Farkas is real-relaxation; integrality-only refutations demote) will visibly shrink the combinable set — correct but a real UX cost, and the baseline rate is unmeasured today.
- **Implementability: 8.** `Certificate::replay` + `QRat` serde already exist (verified) — the load-bearing enabler is real. New crates have a small dependency cone and barely touch the risky interning paths. The forever-contract risk on the canonical format is real and its mitigation (spec-defined grammar, semver, pinned fixture) is adequate.
- **Cost: ~3–4 wks (XL)**, M1–M4 independently shippable.

# CONTRADICTIONS AND RESOLUTIONS

1. **B builds on the assumption A demolishes.** B's XSUM/XUNI/XIX all carry "same base name = same base input" as a prose assumption tag; A makes that assumption a recorded, demotable ledger tier. *Resolution: not a conflict but a sequencing constraint — land A's ledger (metadata-only M1) before B's fact expansion, so every new fact kind carries an identity basis from day one instead of retrofitting.* A is right that prose residuals nobody can act on per-verdict are a defect; B is right that fact-space completeness is where the corpus wins are.

2. **A's XCOLL vs B's XSUMEQ — same insight, different blast radius.** Both derive "XEQ both directions ⇒ same sequence." XCOLL (equality of *all* in-use quantities) subsumes XSUMEQ but maximally widens float-vs-real exposure to element properties. *Resolution: ship B's XSUMEQ first (narrow, structural, sum-shaped, already covered by the sampling gate), then XCOLL restricted to A's own proposed syntactic-safe subset (predicates equal modulo conjunct order / comparison flip / abs form), widening only after the exact-real question from the soundness memory is settled or documented as a limitation.* A's full XCOLL v1 is rejected on its own risk #1.

3. **A's portable digests vs B's Phase-A structural reducer keys — two attacks on the same `Opaque(String)` root cause.** B's is the minimal safe subset (single-prop reducers → structural `CollProp` args, merge.rs needs no change); A's generalizes to arbitrary collection args with a hash. *Resolution: B Phase A first — it is a prerequisite for XSUM and carries no injectivity burden. A's digests are deferred to the measured phase, keeping the `unit_ord` fallback, because digest injectivity re-opens the old CRITICAL class for a completeness gain nobody has quantified.*

4. **C's certify-at-emission vs B's fact proliferation.** C refuses to emit uncertified XSUB/XEQ in combine mode; B adds five fact kinds C doesn't know about. *Resolution: adopt C's rule as a standing design constraint on B — no new fact kind ships without a `certify_unsat` path and a `derived/` sub-proof shape. B already asserts all its facts are Farkas-linear, so this costs little and prevents the bundle from immediately lagging the fact space.*

5. **C's fixed `residuals` prose vs A's per-claim ledger.** C hardcodes "same-base-name" and "property-alias" as static text in `axioms.json`; A computes the actual basis per verdict. *Resolution: A's ledger feeds C's `axioms.json` — the bundle reports the computed identity basis of each claimed pair, not boilerplate.* This is strictly better for C's own L4 goal.

6. **Bundle format timing.** C wants M1 (canonical serialization freeze) first; but freezing `derived/` sub-proof shapes before XSUM/XUNI exist guarantees a format rev. *Resolution: freeze `canon.rs` (QFormula/Rat grammar) early — it's fact-kind-agnostic — but cut bundle-format 1.0 only after the Phase-2/3 fact kinds land.*

# SYNTHESIS — combined roadmap

**Standing constraints (from C, applied globally):** every new fact kind must be Farkas-certifiable and flow through `recon_facts`; every phase ends with corpus A/B sweep + hand-classification of PROVEN increases; adversarial round before any fact-emitting phase is "done."

- **Phase 0 — audit substrate (S–M, no behavior change).** A-M1: `identity.rs` ledger + tier recording in `merge_hirs` + basis in report/JSON (includes R2 property-alias recording — auditability with zero interpreter re-plumbing, A's own correct call). C-M1: `adl-formula/canon.rs` canonical serialization + `QRat` string form. Gate: corpus byte-identical except new report fields.
- **Phase 1 — structural reducer identity (S–M).** B-Phase-A: `intern_reduce` structural keys for single-prop, no-slice reducers. Isolated commit; full byte-diff sweep + golden floor + metamorphic battery (B's risk #3 discipline).
- **Phase 2 — XSUM/XSUMEQ (M).** B-Phase-B with C's emission-time certification built in (fact emitted only if solver-UNSAT ∧ kernel-certified when combining; sub-proofs recorded). Facts carry ledger basis. Negative-prop refusal test, `GExtra::SumCut`, XSUM_UNLOCK doc mirroring ABS format.
- **Phase 3 — XUNI + realizer completeness (M–L).** B-Phases C+D. Distinct-base fail-closed union matching; size-0 and derived-scalar-linkage wishes (zero soundness exposure). Gate: 032/033 downgrades recover as golden pins; no PROVEN OVERLAPPING without `witness_validated`.
- **Phase 4 — provenance + distinctness (M).** A-M2: `info`-block + `--inputs` sidecar provenance, conflict ⇒ per-unit namespacing (removes unifications only), `--identity` threshold flag defaulting to `convention`. First mechanism proving *distinctness* — the anti-false-PROVEN counterweight to Phases 2–3's recall gains.
- **Phase 5 — the combination bundle (L–XL).** C-M2…M6: certificate retention (`CertOutcome`), interval/self-empty certification routing, `bundle.rs` + quantity dictionary + `axioms.json` (consuming the Phase-0 ledger per-claim), `adl-replay check` with mutation battery + Python reference replayer, then `tier2`/`tier3`. Bundle format 1.0 frozen here, covering XSUM/XUNI `derived/` shapes. Spec + trust-tier statement + adversarial bundle round. This is the deliverable the field is waiting for, now over a fact space roughly twice as reachable as today's.
- **Phase 6 — measured extensions (each gated on corpus data + adversarial round, owner decision).** B-XIX/XDISJ behind `--recon-ix`; A-M3 portable digests (fallback retained, injectivity property tests); XCOLL syntactic-safe subset; A-M5 threshold default flip to `declared` once sidecars exist for the corpus. Composite/`Combination` reconciliation stays deferred (B rank 5 — agreed by all evidence).

**Rationale in one line per design:** B wins execution order (quantified impact through the battle-tested choke point), A wins the epistemics (identity as recorded evidence, cheap when taken as metadata-first and dangerous only in its XCOLL/digest maximalism, which Phase 6 contains), C wins the endgame (the artifact is the mission) and its certify-at-emission rule is promoted from feature to global invariant. Estimated total: ~8–10 weeks, every phase independently shippable and corpus-guarded.