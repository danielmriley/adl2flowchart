# The axioms, explained intuitively

*A plain-language companion to the formal catalog in
`crates/adl-axioms/src/lib.rs`. Every axiom below is a fact the prover is
allowed to assume about **any physical event** without checking it. The
rule for admission is strict: each one must be true of every event that
could ever come out of a detector (or event generator), carry a written
justification, and pass a test that fires it at generated events. When a
proof uses one, the report names it — that's the `== axioms used ==` line.*

**Why axioms at all?** The solver only sees the cuts you wrote. It doesn't
know that a jet count can't be negative, or that `jets[0]` is the hardest
jet. Without that background knowledge, almost nothing is provable — the
solver would happily imagine an event with −3 jets to defeat your proof.
Axioms are the physics common sense we hand the solver, one audited fact
at a time.

---

## Group 1 — Collections are finite, ordered lists

These are facts about *lists*, not physics. A collection in ADL is a
finite list of objects, and that alone guarantees a lot.

### SZ0 — "you can't have a negative number of jets"
**Says:** `size(C) >= 0` for every collection.
**Why obvious:** a collection is a list; a list has zero or more entries.
**What it unlocks:** kills phantom counterexamples. If region A needs
`size(jets) >= 1` and region B needs `size(jets) + size(muons) == 0`,
the solver needs SZ0 to rule out "−1 jets and 1 muon."
This is why you see `SZ0×24` in reports — it's the cheap fact that gets
used everywhere.

### SUB — "filtering never adds jets"
**Says:** if collection F is built by filtering P (an object block with
`take P` plus `select` cuts), then `size(F) <= size(P)`.
**Why obvious:** a filter keeps some entries and discards the rest; it
never invents new ones.
**Example:** `bjets` is `take jets` + `select btag == 1` → there can
never be more b-jets than jets.
**Assumption tag:** *take = filter* — this is only sound because ADL's
`take` really is a filter, never a transformer.

### UNI — "a merged collection is at least as big as each part, at most the sum"
**Says:** for a union U of parts, `size(U) >= size(each part)` and
`size(U) <= sum of the parts`.
**Why obvious:** pooling two lists can't lose entries, and can't create
more than both lists combined. The bounds are deliberately loose enough
to be true whether the union concatenates or deduplicates — we don't
assume which.
**Example:** `leptons` = electrons ∪ muons → `size(leptons)` is at least
`size(electrons)` and at most `size(electrons) + size(muons)`.

### SZSLICE — "a slice of a list fits inside the list"
**Says:** `0 <= size(coll[a:b]) <= size(coll)`, and if the bounds are
concrete numbers, also `size(coll[a:b]) <= b − a`.
**Why obvious:** taking entries 2 through 5 of a list gives you at most
4 entries, and never more than the list had.
**Example:** `jets[0:2]` (the two leading jets) has size at most 2 —
even if the event has ten jets.

### SZPERM — "sorting doesn't change how many there are"
**Says:** `size(sort(C, …)) = size(C)`.
**Why obvious:** sorting reshuffles a list; it neither adds nor removes.
**Example:** a region that sorts jets by mass still has the same jet
count as one that doesn't — so count-based cuts can be compared across
them.

### COMBSIZE — "counting pairs is arithmetic, not physics"
**Says:** facts about combinatoric (`comb`) collections — e.g. a
projection of pairs has as many entries as there are pairs, and for
same-source *distinct* pairs the count follows tuple combinatorics
(n objects → n·(n−1) ordered pairs, and so on).
**Why obvious:** if you form all pairs of jets, how many pairs exist is
pure counting.
**Example:** with `size(jets) == 3` there are exactly 6 ordered distinct
jet pairs — a cut demanding 8 such pairs is impossible, and the prover
can say so.
**Assumption tag:** *distinctness is by kinematic value* — two "different"
jets with byte-identical kinematics would fool this, which can't happen
for real detector objects.

---

## Group 2 — Detector quantities have known shapes

These are facts about the *numbers* physics hands us: magnitudes,
angles, flags.

### NNEG — "magnitudes aren't negative"
**Says:** `pt`, `m`, `E`, the HT family, `MET.pt`, `dR` (and declared
non-negative externals) are all `>= 0`.
**Why obvious:** these are magnitudes by definition — a transverse
momentum is the *length* of a vector; ΔR is a *distance*.
**Example:** region A wants `HT > 500`, region B wants
`HT + MET < 300`. Without NNEG the solver imagines `MET = −400` and
"finds" an overlap. With it, the regions are proven disjoint.

### ORD — "jets come sorted, hardest first"
**Says:** within one collection, `pt(C[i]) >= pt(C[j])` whenever
`i < j` (with a carefully restricted form when negative indices like
`C[-1]` are involved: front-to-back comparisons are only asserted where
provably sound — `C[0]` beats `C[-k]` always; `C[i]` beats `C[-1]`
always).
**Why obvious:** detector reconstruction delivers collections
pT-descending; that's the reconstruction contract, and the interpreter
enforces the same order.
**Example:** if a region cuts `jets[1].pt > 200`, the prover knows
`jets[0].pt > 200` comes for free — the leading jet is at least as hard
as the subleading one. That's how two regions cutting on different
indices can still be compared.
**Note:** the restriction on negative indices exists because the naive
"any front index beats any back index" is FALSE for short lists — we
found that out by testing, and the prohibited version lives on the
banned list.

### DPHI — "an azimuthal difference is an angle"
**Says:** `−π <= dphi <= π` (widened by one floating-point ulp so the
exact-rational prover never cuts it too fine).
**Why obvious:** Δφ is wrapped into the principal range by construction.
**Example:** a cut `dphi(jet, MET) > 4` is unsatisfiable — no event can
pass it — and the prover can prove regions built on it empty or
disjoint.

### TWIN — "swapping the arguments flips (or keeps) the sign"
**Says:** for reversed-argument `dphi`/`deta` pairs: the two values are
equal, or negatives of each other.
**Why obvious:** `deta(a,b)` and `deta(b,a)` measure the same separation
from opposite ends; whichever sign convention holds, `|x| = |y|`.
**Example:** analysis A cuts `abs(dphi(j1, MET)) > 0.5`, analysis B
writes it `abs(dphi(MET, j1)) > 0.5` — TWIN lets the prover treat these
as the same constraint instead of two unrelated unknowns.
**Assumption tag:** *either convention* — the axiom is deliberately weak
enough to hold under both sign conventions, because the community
hasn't standardized one (that's open question OPEN-2).

### TAG — "a tag is a yes/no answer"
**Says:** exact-name `btag`/`ctag`/`tautag` element properties, and
trigger flags, take values in {0, 1}.
**Why obvious:** these are boolean decisions rendered as numbers.
**Example:** `select btag(jets[0]) > 0.5` and `select btag(jets[0]) < 0.5`
are disjoint only if btag can't be, say, exactly 0.5 or 0.3 — TAG pins
it to 0 or 1 and the disjointness goes through.
**Careful edge:** this applies by *exact name only*. A property named
`btagDeepB` is a continuous discriminant, NOT a boolean — the exact-name
rule exists precisely so discriminants never inherit this axiom.

### TRIG — "cosine and sine live in [−1, 1]"
**Says:** for otherwise-opaque `cos(x)`/`sin(x)` calls, the result is in
[−1, 1].
**Why obvious:** that's the range of the circular functions, no matter
what the (unknown) argument is.
**Example:** even though the prover treats `cos(dphi)` as an unknown
number, it knows `MT2 = sqrt(2·pt1·pt2·(1+cos(dphi)))`-style expressions
can't see `1 + cos(...)` exceed 2. Bounds flow through formulas the
prover otherwise can't open.

---

## Group 3 — Definitions mean what they say

### EPRED — "if a filtered element exists, it passed the filter"
**Says:** if `size(F) > i`, then element `F[i]` satisfies F's selection
predicate (for cuts we can encode exactly).
**Why obvious:** every element *of* a filtered collection is an element
that *survived* the filter. If your third clean-jet exists, it passed
the clean-jet cuts.
**Example:** `goodjets` = `take jets, select pt > 40`. A region requiring
`size(goodjets) >= 1` and also `goodjets[0].pt < 30` is impossible —
EPRED supplies `goodjets[0].pt > 40` the moment the element exists.
**Assumption tag:** *take = filter*, same as SUB.

### IDOM — "a filtered element is dominated by its parent at the same rank"
**Says:** `pt(F[i]) <= pt(P[i])` when F filters P.
**Why obvious:** this is ORD and SUB combined: F's i-th hardest element
is some P-element of rank ≥ i, so it can't beat P's i-th hardest.
Intuition: the 3rd-fastest sprinter *from one country* can't be faster
than the 3rd-fastest sprinter *overall*.
**Example:** if a region proves `jets[1].pt < 60`, then any filtered
sub-collection's `[1]` element is also below 60 — cuts on the parent
constrain cuts on the child at equal index.

---

## Group 4 — The cross-analysis links (these are *earned*, not assumed)

XSUB and XEQ look like axioms in the report, but they're different in
kind: they are **only emitted after the prover has already proven the
underlying implication** between the two analyses' object definitions.
The "axiom" is just the bridge that carries an already-won proof into
the size arithmetic.

### XSUB — "a strictly tighter filter of the same input catches no more"
**Says:** `size(A) <= size(B)` when A (from analysis 1) and B (from
analysis 2) filter the **same base collection**, and we have *proven*
that every element passing A's cuts passes B's.
**Why sound:** the fact is only emitted when the element-level
implication `A's cuts ⟹ B's cuts` came back UNSAT-certified. Given the
same input list, the tighter sieve keeps a subset — so at most as many.
**Example (the mini demo):** analysis A's jets need `pt > 30, |eta| < 2.4`;
analysis B's need `pt > 25, |eta| < 2.4`. Passing A's cuts implies
passing B's (30 > 25), so `size(A-jets) <= size(B-jets)`. Now A's
"at least 3 jets" and B's "at most 2 jets" can never hold on one event
→ PROVEN DISJOINT.
**The one real assumption:** *same base name = same base input* — both
files' `take Jet` is assumed to mean the same underlying jet list. This
is the documented cross-file residual; everything else in the chain is
proven, not assumed.

### XEQ — "two spellings of the same filter are the same collection size"
**Says:** `size(A) = size(B)` when both implication directions were
proven — A's cuts imply B's *and* B's imply A's.
**Why sound:** it's XSUB applied both ways. Same input, provably
identical acceptance → identical count.
**Example:** analysis 1 writes `select pt > 30 and abs(eta) < 2.4`,
analysis 2 writes `select abs(eta) < 2.4, select pt > 30` — different
files, different names, same filter. XEQ lets every count cut in one
analysis talk directly to the other's.

---

## Reading an `== axioms used ==` line

```
ORD×2, SZ0×24, SUB×15, UNI×15, NNEG×10, DPHI×5, EPRED×2, IDOM×2, XSUB×6
```

Each entry is "this family of background facts was load-bearing in at
least one proof, this many times." The multipliers are instances, not
importance — SZ0 firing 24 times just means 24 size variables needed
their `>= 0` floor. The interesting entries are usually the rare ones:
an `XSUB×6` means six cross-file collection links were *proven* and then
used; an `EPRED×2` means two proofs turned on "the element exists, so it
passed its own cuts."

Three global guarantees to keep in mind:

1. **Every axiom is physically justified** — the catalog test literally
   requires each justification to argue "true of every physical event."
2. **Every assumption is surfaced** — when a proof leans on
   *take = filter* or *same base name = same base input*, the report's
   assumption line says so. You never get a silent hypothesis.
3. **There is a prohibited list** — plausible-looking axioms we tried
   and found unsound (like the naive negative-index ordering) are kept
   in the code as documentation, with the counterexample that killed
   them. The catalog earns trust by what it refuses to include.
