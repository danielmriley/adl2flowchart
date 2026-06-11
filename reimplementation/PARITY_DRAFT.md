# PARITY_DRAFT — smash2 `verify` vs legacy `smash -r` (golden suite)

Date: 2026-06-11. Status: **DRAFT** for the Phase-7 sign-off (this is not
the signed PARITY.md; every classification below needs Daniel's sign-off
per PLAN Phase 7).

Method: both tools run on all 25 files in
`legacy_parser/tests/golden/*.adl`; legacy invoked as `./smash -r <file>`
from `legacy_parser/` (it resolves its data dir from cwd), ADL2 as
`smash2 verify <file>` (z3-native backend). Verdicts compared pair-by-pair
(disjoint / overlapping / possibly / subset flags / region-empty / bin
checks / parse failure). Raw outputs preserved during the run under
`/tmp/parity/{legacy,adl2}/`. Legacy untouched.

## Result summary

- 25/25 files run to completion on both tools (bad_syntax fails to parse
  on both, exit 1 on both — as designed).
- **21 files verdict-identical** (including subset flags, region-empty
  detection on vacuous_dphi/define_arith, and the bins_partition bin
  checks).
- **4 verdict-level differences**, classified below: 3 × adl2-better,
  1 × spec-change. 0 × legacy-better.

## Verdict-by-verdict table

| Golden | Legacy verdict | ADL2 verdict | Match / class |
|---|---|---|---|
| angular_order | POSSIBLY (convention-dependent) | POSSIBLY (OPEN-2 twin cap) | match |
| bad_syntax | parse error, exit 1 | parse error, exit 1 (span + "did you mean `select`?") | match |
| bins_partition | PROVEN OVERLAPPING + 2 subsets; SR_binned bins disjoint+covered; SR_gap coverage not proven | identical + concrete gap witness (MET.pt ≈ 251) | match |
| btag_discriminant | PROVEN OVERLAPPING, SR_b ⊆ SR_a, witness 0.5 | identical (witness ≈ 0.5, interpreter-validated) | match |
| btag_threshold | PROVEN DISJOINT | PROVEN DISJOINT + unsat core citing axiom TAG | match |
| collection_quant (allhard/unbounded) | PROVEN OVERLAPPING (under-approx SAT) | POSSIBLY (witness re-validation refused: OPEN-1) | **DIFF — spec-change** |
| collection_quant (allhard/softlead) | PROVEN DISJOINT | PROVEN DISJOINT + core citing ORD | match |
| collection_quant (unbounded/softlead) | POSSIBLY (encoding gap) | POSSIBLY (encoding gap) | match |
| define_arith | PROVEN DISJOINT (SR_a empty) | PROVEN DISJOINT (interval (100,20) empty) + region-empty warning | match |
| define_under_or | PROVEN DISJOINT | PROVEN DISJOINT + core | match |
| disjoint_jet_index | PROVEN DISJOINT (interval) | PROVEN DISJOINT (interval) | match |
| disjoint_pt | PROVEN DISJOINT (interval) | PROVEN DISJOINT (interval) | match |
| independent_jet_index | POSSIBLY ("no shared constraint dimension") | PROVEN OVERLAPPING, interpreter-validated witness | **DIFF — adl2-better** |
| inf_constant | POSSIBLY (cut dropped, 0% coverage) | PROVEN DISJOINT (SR_a provably empty: constant-false cut) | **DIFF — adl2-better** |
| ite_conditional_dphi | single region, no pairs | single region, no pairs | match |
| not_tag | PROVEN DISJOINT | PROVEN DISJOINT + core | match |
| or_met | PROVEN OVERLAPPING | PROVEN OVERLAPPING, validated witness | match |
| or_unencodable_branch | PROVEN OVERLAPPING | PROVEN OVERLAPPING, candidate witness (opaque `aplanarity`) | match |
| overlap_met | PROVEN OVERLAPPING (witness 201) | identical, validated | match |
| quant_empty_forall | POSSIBLY (encoding gap) | POSSIBLY (encoding gap) | match |
| ratio_met | PROVEN DISJOINT | PROVEN DISJOINT + core citing NNEG | match |
| reject_and_band | PROVEN DISJOINT | PROVEN DISJOINT + core | match |
| reject_or_band | PROVEN OVERLAPPING, SR_mid ⊆ SR_band | identical, validated | match |
| size_bjets | PROVEN OVERLAPPING, SR_ge4 ⊆ SR_ge2 | identical, validated (size witness 4) | match |
| tag_index | POSSIBLY ("no shared constraint dimension") | PROVEN OVERLAPPING, interpreter-validated witness | **DIFF — adl2-better** |
| union_size | PROVEN OVERLAPPING, mutual subset | identical, validated | match |
| vacuous_dphi | PROVEN DISJOINT (SR_dead empty) | identical + DPHI axiom named in core | match |

## Classification of the four differences

### 1. independent_jet_index — **adl2-better**

`SR_lead_high: pT(jets[0]) > 300` vs `SR_sub_low: pT(jets[1]) < 200`.
These regions genuinely overlap (any event with jets[0].pt = 301,
jets[1].pt = 31 passes both; jets is `pT > 30`-filtered). Legacy capped
at POSSIBLY via its "no shared constraint dimension" heuristic — a
defensive hack against trivial SAT claims. ADL2 proves PROVEN OVERLAPPING
the sound way (SPEC_ANALYSIS §2 + TESTING §3): SAT(Ax ∧ A⁻ ∧ B⁻), the
element-existence guards make `size(jets)` a genuine shared dimension,
and the witness is **re-validated through the reference interpreter**
(witness: jets[0].pt ≈ 301, jets[1].pt = 31, size(jets) = 2). A
machine-checked true verdict replaces a heuristic non-answer.

### 2. tag_index — **adl2-better**

`jets[0].BTag == 1` vs `jets[1].BTag == 0`. Same shape as #1: genuinely
overlapping (2-jet event, lead tagged, sub untagged), legacy POSSIBLY by
the same no-shared-dimension cap, ADL2 PROVEN OVERLAPPING with an
interpreter-validated witness. Sound strengthening per SPEC_ANALYSIS §2.

### 3. inf_constant — **adl2-better**

`SR_a: select MET.pT + eles[0].pT > 100 / 0`. `100 / 0` is constant,
event-independent non-finite arithmetic; SPEC_LANGUAGE §4.4 and the
PHASE0 resolution fix the semantics: "division by zero / non-finite ⇒
the enclosing comparison is false". So SR_a selects no events — ADL2
reports the region-empty warning and PROVEN DISJOINT for the pair.
Legacy *dropped* the cut (audit-Bug-5 posture: refuse non-finite
constants) and reported POSSIBLY with a 0% coverage warning. Legacy was
honestly weaker; ADL2 is exact per the now-specified semantics (the
constant-fold to `False` happens before atom construction, so the
non-finite-atom prohibition is not violated). Spec citation:
SPEC_LANGUAGE §4.4 [VERIFY accepted at PHASE0], BUILD_NOTES Phase-4
decision 2.

### 4. collection_quant (SR_allhard vs SR_unbounded) — **spec-change**

Legacy: PROVEN OVERLAPPING from SAT(R1⁻ ∧ R2⁻) — mathematically sound
(the under-projection of the OPEN-1 Dual is conservative under either
quantifier reading), but the displayed witness was never executable:
no interpreter existed.

ADL2 finds the same SAT model but then applies the TESTING §3 production
rule: every SAT-direction proof is re-validated through the reference
interpreter, and a failed validation downgrades to POSSIBLY with an
internal diagnostic. Here validation *cannot run*: `SR_allhard` contains
the unindexed collection cut `pT(jets) > 100`, whose quantifier reading
is exactly the unresolved OPEN-1 — the interpreter refuses to invent a
reading. ADL2 therefore reports POSSIBLY OVERLAPPING with the explicit
reason ("witness re-validation failed … OPEN-1 unresolved").

This is the deliberate ADL2 contract change ("the verifier can never
display a witness the interpreter rejects" — TESTING §3, ADR-005), not a
regression: the legacy verdict relied precisely on the convention the
spec declares open. When OPEN-1 is resolved against CutLang, the Dual
hedge collapses to an exact encoding, the interpreter gains the reading,
and this pair returns to PROVEN OVERLAPPING. Flag for sign-off as a
documented spec change.

## Non-verdict (cosmetic) differences, acknowledged

- **Leaf counts** include the new element-existence guard atoms
  (e.g. disjoint_pt SR_low 2→4) — consequence of the CE-1/2/3 soundness
  fix; the guards are real conjuncts of the exact encoding.
- **Explanations**: ADL2 prints unsat cores mapped to source lines and
  names the axioms used (SPEC_ANALYSIS §3 — a new requirement legacy
  never had); legacy prints fixed phrase + summary-counter soup.
- **Witness values** differ by the ε-interior/dyadic-snap model
  refinement (e.g. 121.00000095367432 vs 121.0); both satisfy both
  regions; ADL2's are interpreter-validated.
- **Region-empty reporting**: ADL2 reports it per-region in `== regions ==`
  and in the pairwise reason; legacy folded it into the pairwise line
  (define_arith, vacuous_dphi — same findings on both tools).
- Known blemish (tracked in BUILD_REPORT known gaps): when region-empty
  is proven by the *interval fast path* (define_arith, inf_constant) the
  human line reads "— UNSAT: " with an empty core list; the verdict and
  the warning are correct, the explanation text is incomplete.

## Performance note (PLAN Phase-7 criterion: ≤ 2× legacy on SUS-16-033)

`smash2 verify CMS-SUS-16-033_Delphes.adl`: ~0.36 s wall. Legacy
`smash -r` on the same file: ~0.88 s (measured same machine, same run).
ADL2 ≈ 2.4× faster — the ≤ 2× criterion is met with margin.
