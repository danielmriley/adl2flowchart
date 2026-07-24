# Work notes — 2026-07-24

Two items to take on, each with the current-state findings (grounded by
running the tool) and idea options with a recommended first step.

---

## Item 1 — Cross-file: same object, different name

**The belief:** "we already reconcile objects that are the same but
named differently." **The reality:** we do — but only inside a narrow
band, and the interesting cases fall outside it.

### What actually works today (verified)

1. **Different *object* names, same base, identical structure.**
   `object signalJets: take Jet, pt>40` in file A and
   `object hardJets: take Jet, pt>40` in file B are unified by
   **structural interning** into one `CollectionId` — they become
   literally the same quantity, no axiom needed. Names are discarded.
2. **Different *object* names, same base, provably-equal structure.**
   `pt>30` vs `pt>20 then pt>30` over `Jet` → linked by **XEQ** (both
   refinement directions proven). This is `examples/golden/cross/xeq-equivalent/`.
3. **Predefined base aliases.** `take Jet` and `take AK4jet` both
   canonicalize to the base symbol `JET` (the `ext_objs.txt` alias
   table), so they reconcile; the `Jet` vs `Electron` control correctly
   stays POSSIBLY. Same for `Ele`/`Electron`/`ELE`.

The common thread: reconciliation candidates are bucketed by the
**base symbol** (`quantity.rs::reconciliation_candidates` → `filter_chain`).
Two filtered collections can only be related when they flatten to the
*same base symbol*. That is the load-bearing constraint.

### Where it breaks — the real gap

1. **Different base names the alias table doesn't know are equal.** The
   `ext_objs.txt` equivalence list is fixed and curated. Two collaborations
   using a spelling the table doesn't pair (a custom `SignalTrack` base, a
   `Delphes_Jet` vs `Jet` split, an experiment-specific name) never bucket
   together — no XSUB/XEQ can ever fire, regardless of identical cuts.
2. **Non-filter shapes are excluded entirely.** `filter_chain` returns
   `None` for `Union`, `Combination`, `Sorted`, `Slice`. So two analyses
   whose `leptons = Union(ele, muo)` are "the same" composite object are
   *not* reconciled at all — even when their parts individually reconcile.
3. **The base-identity assumption is unprovable from ADL.** Even when base
   names match, "same base name = same base input" is an *assumption*
   (surfaced in every XSUB/XEQ report). Two files can both write `take Jet`
   and mean differently-calibrated jets. ADL text alone can't settle this.

So gap (1) and (2) are things we can *fix*; gap (3) is a residual we can
only ever *manage* (surface, let the user assert or deny).

### Ideas (ranked by value-per-effort)

- **A. User-declared cross-collection identity (the escape hatch).**
  A way for the physicist to assert "A's `signalTrack` IS B's
  `dispTrack`" when the tool can't infer it. Two sub-options:
  - a CLI/sidecar mapping: `--identify a::signalTrack=b::dispTrack`;
  - an ADL annotation on the object block.
  Sound because it's declared, not inferred — treat it exactly like the
  existing base-name residual: emit the link as an *assumption-tagged*
  fact, surface it in the report, and (cheap safety) refuse the
  identification if the two collections expose contradictory element
  properties. This directly answers "I know they're the same, the tool
  doesn't." **Recommended first step** — small, general, honest.

- **B. Extend the base-alias table + make it user-extensible.** Let an
  analysis (or a shared config) declare base-symbol equivalence classes
  beyond the built-in `ext_objs.txt` list. Table-driven, low-risk, and
  it captures the common "two spellings of a standard object" case
  without touching the engine. Complements A.

- **C. Structural reconciliation of composite shapes.** Teach
  `filter_chain`/`reconciliation_candidates` about `Union` and
  `Combination`: two unions reconcile when their parts pairwise reconcile
  (and sizes relate by the UNI bounds already in the axiom set). This
  generalizes "same object, different name" to derived collections — the
  deeper engine work, worth doing after A/B prove the demand.

- **D. Element-signature matching (research).** Match collections by the
  *set of element properties/cuts* they use rather than the base name,
  then prove equivalence. Most powerful, most likely to over-unify —
  gate behind the certifier and the sampling net if pursued at all.

**Suggested path:** A first (unblocks real cross-experiment work
immediately, stays sound by construction), B alongside it (cheap), C when
composite objects show up in a real combination request.

---

## Item 2 — A reusable named boolean predicate ("variable" for a cut chain)

**The ask:** set a named variable holding a chain of boolean expressions,
reuse it later in the file.

### What already works today (verified — more than expected)

- A **top-level boolean `define`** reused as a region cut works, bare or
  inside a compound `select`:
  `define isTight = size(jets) >= 3 and pT(jets[0]) > 200` → `region SR: isTight`
  encodes all leaves.
- **Chains work:** a define referencing an earlier define,
  `define fullSel = baseSel and pT(jets[0]) > 200`, expands correctly.
- Used inside a larger expression, `select isTight and size(jets) >= 3`,
  also works.

So for **event-context** predicates the feature substantially exists via
`define` + macro inlining. That's the good news.

### The gap — element-context predicates

A predicate over an object's *element* fails at top level:

```
define isCentral = abs(eta) < 2.4 and pt > 30   # eta/pt have no element to bind to
object jets
  take Jet
  select isCentral                               # -> "unresolved identifier `eta`"
```

`eta`/`pt` need a `this`-element binding. Object-*scoped* defines (indented
inside one object block) resolve, but a **shared element predicate reusable
across multiple object blocks** has nowhere to bind its element. That is
the missing piece — and it's exactly the reuse a physicist wants ("define
`isCentral` once, apply it to jets, electrons, and photons").

### Ideas (ranked)

- **A. Parameterized predicates (element binding made explicit).**
  `define isCentral(j) = abs(j.eta) < 2.4 and j.pt > 30`
  applied as `select isCentral(this)` in any object block. The parameter
  supplies the binding the current top-level form lacks, it composes
  (predicates calling predicates), and it stays a **pure macro inlining**
  — expand at reference sites before encoding, so the HIR, prover, and
  every axiom are untouched. Fragment behavior is inherited (an
  out-of-fragment leaf makes the expansion partial, fail-closed, as today).
  **Recommended** — smallest change that closes the actual gap, reuses the
  existing object-scoped-define inlining machinery, matches ADL's `define`
  idiom.

- **B. Implicit-element predicate with context inference.** Keep the
  bare `define isCentral = abs(eta) < 2.4` form and infer element context
  from the free variables (`eta`/`pt` → element predicate; `size(...)` →
  event predicate). Nicer to write, but the inference is a new source of
  ambiguity (what about a predicate mixing both?) and worse error
  messages. Prefer A's explicitness unless authors push for it.

- **C. A dedicated keyword** (`cut`/`condition`/`predicate`) instead of
  overloading `define`. Clearer intent and diagnostics, at the cost of new
  grammar surface and a second concept authors must learn. Only worth it if
  overloading `define` (value vs boolean) proves confusing in practice.

**Suggested path:** A. It's a contained parser/resolve change (inline at
reference sites), needs no engine work, and turns the half-feature we
already have into the full one. Add golden coverage: an element predicate
applied to two different object collections, and a predicate-references-
predicate chain, both proven to expand to the same HIR as the written-out
form (the identity test we used for object-scoped defines).

### Standing invariant for item 2

Whatever the surface syntax, keep it **pure inlining that expands before
the HIR**. No new node kinds reach the prover; the census stays satisfied;
soundness is inherited from the expanded cuts. A named predicate must be
observationally identical to writing its body out by hand — that identity
is the test.

---

## Cross-cutting note

Both items are really the same theme from two directions: **naming should
never change meaning.** Item 1 says two different names can denote one
object; item 2 says one name can stand in for a chain of conditions. In
both, the safe implementation is to resolve the name to its meaning early
(intern / inline) and keep the prover working on meanings, not labels —
exactly the discipline the identity layer already enforces.
