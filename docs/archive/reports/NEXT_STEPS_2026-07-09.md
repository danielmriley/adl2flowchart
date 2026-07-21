# Next steps — post abs-unlock / learn-adl conformance

**Date:** 2026-07-09 · **Baseline:** main @ `001d449` — 71 suites green,
136-file corpus, certification default-on (100% corpus rate), XSUB firing
on 28/78 CMS file-pairs, learn-adl syntax fully supported.

The tool's features are in place; what remains is **proof reach** (how many
true facts it can prove rather than fail-closed), **trust deliverables**
(artifacts a collaborator can re-check without running smash2 or z3 —
they still trust that the exported formulas/axioms are the right claim), and
**adoption polish**. Ordered by value-per-effort within each track.
Effort: S = hours, M = a day-ish, L = multi-day.

---

## Track A — Proof reach (highest leverage first)

### A1. Witness-realizer completeness (M)
The only two CMS pairs that LOST proofs after the abs unlock are realizer
gaps, not soundness (docs/ABS_UNLOCK_2026-07-03.md §honest downgrades):
- **Size-0 preference for interpreter-opaque collections.** When a
  collection's membership predicate is interpreter-undecidable (e.g. a
  `D0` cut with no reference interpretation), the realizer should ask the
  solver for a model with that collection EMPTY when the regions permit
  it — an empty collection needs no per-element evaluation, so validation
  passes vacuously. Fixes 032 `compressed` vs `compressednc2`.
- **Derived-scalar consistency (HT linkage).** The solver treats `HT` as a
  free scalar; the interpreter derives it (Σ jet pT). Either assert the
  linkage `HT = Σ pt(jets[i])` on the witness frame when the collection is
  smaller than the pad bound, or patch the realized event's HT from the
  materialized jets before validation. Fixes 033 `SR7` vs `SR11`.
- Exit: both CMS pairs re-prove; full gate; no golden pin regressions.

### A2. Certifier unit-propagation (S/M)
CE-7's follow-up (COUNTEREXAMPLES.md): propagate top-level conjuncts and
detect complementary-literal clashes BEFORE case-splitting. The inherit-
form core (`d0 ∧ ¬d0` buried in a monolithic region-reference conjunction)
then certifies instead of exhausting the 100k-branch budget.
- Exit: the CE-7 base file yields PROVEN (not CANDIDATE) DISJOINT; the
  adl-certify property/tamper suites green; corpus certification rate
  stays 100%.

### A3. `size()` cardinality reasoning (L) — roadmap rank 12
Beyond subset facts: bounds like `size(A∩B) ≥ size(A) + size(B) − size(P)`
for siblings filtered from one parent, unlocking disjointness/overlap
verdicts XSUB alone cannot reach. Needs a new axiom family with the usual
justification + interpreter conformance + prohibited-pattern review.
- Exit: new golden pins for a sibling-overlap shape; axioms_hold covers
  the family; oracle green.

### A4. Composite verification — lift from interpret-only (L)
Composites (`Zcands` etc.) are the largest class of real-analysis content
the prover cannot see (P1). A first sound slice: encode `size(composite)`
bounds from the binder collections (COMBSIZE exists) and per-tuple cuts
over EXACT binder predicates, fail-closed elsewhere.
- Exit: a golden pair proving disjointness through a composite size cut;
  no change to interpret-only behavior otherwise.

## Track B — Trust deliverables

### B1. `verify --combine` certificate artifact (M) — roadmap rank 10 — **SHIPPED 2026-07-21**
Emit a machine-checkable artifact per PROVEN cross-file relation: the
unsat core, axiom instances with justifications, and the replayable
Farkas certificate. A collaborator re-verifies with the standalone
checker — no trust in solver search or smash2's replay run required
(the exported formula/axiom set being the *right claim* is still
smash2's encoder speaking). This is the "combination claims
you can hand to a working group" deliverable; certification (Phase 4)
did the hard part already.
- Exit: `verify --cross --combine out/` writes the artifact; a fresh
  `adl-certify`-only binary replays it; JSON schema documented.

### B2. Owner decisions on pinned semantics (S, mostly Daniel's call)
`docs`/PHASE0: the dPhi/dEta sign convention and `~=` are convention-
neutral defaults; deciding them upgrades a set of POSSIBLY verdicts to
exact. Needs a decision + a sweep flipping the Dual encodings + golden
pin updates. Low code risk, real verdict gains.

### B3. Residual closure or formal acceptance (M)
Document-or-fix, one at a time: same-base-name cross-file assumption
(structural enforcement was Phase-3-designed), the property-alias class,
and the single-subtraction catastrophic-cancellation f64 edge (oracle-
monitored, out of corpus). Each gets either a fix or a signed-off
"accepted residual" paragraph in SPEC_ANALYSIS.

## Track C — Adoption & polish

### C1. Real-corpus expansion sweep (S)
Run `verify --cross` over the ADL_NPS corpus (9 real CMS analyses beyond
our 13) and the full CMS dir; classify every non-POSSIBLY verdict; file
encoder gaps the new files surface. Cheap, high-signal after the abs
unlock + learn-adl fixes.

### C2. Interpreter sort-semantics test depth (S)
Region-local `Sorted` views landed (v2 Phase 5) but interpreter-side
coverage is thin: add spec4_semantics cases (sort then index; sort desc
alias-gate; sort inside inherited region) + one golden file.

### C3. Small fixes (S each)
- Spurious "unresolved identifier `descend`/`ascend`" warning on
  recognized `sort(coll, key, dir)` take-sources.
- Legacy port: object-pair disjointness printout (`smash -r`'s last
  unported report section).
- Mid-selection `histoList` fill points (currently diagnosed honestly;
  implement per-cut fill or keep the diagnostic and document).
- Provenance `tool` field: embed git hash behind a build-time env seam.

### C4. Release engineering (M)
Tag v0.2.0 from main; CI release workflow building the three-platform
binaries; README install section; CHANGELOG distilled from the commit
log. Makes the tool shareable beyond this repo.

---

## Suggested sequence

1. **A1 realizer** (recovers known losses, improves every overlap verdict)
2. **A2 certifier unit-prop** (closes CE-7 properly)
3. **C1 corpus sweep** (cheap validation of 1–2 on real files)
4. **B1 --combine artifact** (the headline deliverable)
5. **B2 owner decisions** → then **A3 size() cardinality**
6. **C4 release** once 1–4 are in
7. **A4 composites** as the next big capability block, with B3/C2/C3
   interleaved as breathers.
