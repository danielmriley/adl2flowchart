# Plan: grammar refinement and cross-file disjointness

June 2026. Planning document — nothing here is implemented yet. Builds on
`docs/DUAL_ENCODING_REPORT.md` (current state) and `docs/REVIEW_NOTES.md`
(original audit). Three tracks: **G** (grammar/front-end), **D** (single-file
disjointness refinement), **X** (cross-file disjointness). The X track is the
goal; G and D items are sequenced by whether X depends on them.

---

## Track G: grammar and front-end refinement

### G1. Fractional boundary lists (small, X-relevant)
`bin MET 250.5 300 ...` and range lists truncate to int because the parser
stores `range`/`index` values in `VarNode`'s `std::vector<int>` accessor.
Indices and boundary lists are different concepts sharing one field.
**Plan:** keep integer accessors for indexing; add a separate
`std::vector<double> boundaries` to VarNode (or a dedicated BinNode AST
type — preferred), fed by a `bins`-style grammar rule that accepts signed
reals. Removes the documented bin-edge limitation and unblocks negative
boundary values cleanly.

### G2. Resolve the remaining 5 reduce/reduce conflicts (medium)
All five trace to `take_id`/`comb_args`/`param_list` overlap inside
`object ... : comb(...)`. The space-separated `comb_args` and the
comma-separated `param_list` both match a single argument, and bison
silently picks one. **Plan:** make COMB syntactically distinct — either
require the literal keyword `comb`/`union` before the paren (lexer keyword,
like `union` already is) or require commas in comb args. Decide against the
corpus (the corpus parses today, so any change must keep all 68 files
green). Then turn on `-Werror=conflicts-rr` in the Makefile so the count
can never silently grow.

### G3. Shift/reduce conflict triage (medium, ongoing)
57 S/R conflicts remain, mostly the dangling-ITE chain (`chain QUES
chained_cond COLON chained_cond`) and `LPAR chain RPAR` vs `LPAR expr
RPAR`. Most resolve in the intended direction, but they are unverified.
**Plan:** add `%expect`/precedence declarations only after writing the AST
snapshot tests in G5, so every conflict resolution is locked by a test, not
by hope. Not worth doing before G5 exists.

### G4. Signed-literal cleanup (small-medium)
`5-3` still lexes as `INT(5) INT(-3)` because list contexts (bins, ranges,
tables, `[] -0.1 -0.05`) rely on signed literals after operands. **Plan:**
once G1 gives lists their own grammar rules with explicit `SUBTRACT num`
alternatives, remove `[-]?` from the number rules and let the existing
unary-minus production handle expression contexts. Corpus-gated.

### G5. AST snapshot tests (small, high leverage — do first)
`validate_corpus.sh` only checks exit codes; a grammar change that produces
a *different tree* passes silently. **Plan:** add `--dump-ast` (stable
canonical text form: node kind, op, children — not the DOT output) and
golden snapshots for ~10 representative corpus files plus the tests/golden
set. Every G2–G4 change then diffs against these. This is the enabling
investment for all other grammar work.

### G6. AST hygiene: enum kinds + arena ownership (medium)
String-token dispatch (`getToken() == "DEFINE"` + unchecked static_cast)
and the clone-on-construct ownership mess remain. **Plan:** (a) add an
`enum class NodeKind` alongside the token string, populated in
constructors, and convert the `getXNode` helpers into checked casts that
assert on mismatch; (b) arena ownership — Driver owns
`vector<unique_ptr<Expr>>` of every node ever created, node destructors
never delete children, `clone()` registers into the arena. Mechanical but
wide; schedule after G5 so snapshots catch regressions. Not an X
prerequisite.

---

## Track D: single-file disjointness refinement

### D1. Object-block cut encoding (medium — the bridge to X)
Today object blocks are only scanned for take-lineage and pure-alias
detection; their per-object cuts (`object bjets take jets / select BTag ==
1`) are not modeled. **Plan:** for each derived object, encode its
per-object selection as a predicate over element properties. Two sound
consequences become available *within* one file:
- **Subset axioms with content**: if `A = filter(B, φ)` then beyond
  `size(A) ≤ size(B)` we can assert, for pt-ordered collections,
  `pt(A[i]) ≤ pt(B[i])` for each i (the i-th best of a subset cannot beat
  the i-th best of the superset) — sound and currently missing.
- **Element-property propagation**: every `A[i]` inherits φ as a guarded
  fact (`size(A) > i ⇒ φ(A[i])`), e.g. every bjets element has btag == 1,
  pt > 30, |eta| < 2.4. This often decides pairs that are POSSIBLY today.
This is also the foundation of cross-file object identity (X1), which is
why it leads the D track.

### D2. Adaptive quantifier bound (small)
k=3 is hard-coded. When the region constrains `size(C) ≤ s`, expand to
k = s and drop the Dual node entirely (the expansion becomes exact). Add
`--quant-bound k` for the unbounded case.

### D3. Explanations via unsat cores (medium, high collaborator value)
A verdict line says *that* two regions are disjoint, not *why* (the
heuristic names one interval; SMT proofs name nothing). **Plan:** name
each assertion (`:named r1_cut3`) and request unsat cores; report
"disjoint because: HT bins [200,500) vs [500,1000)". Cores also make the
bin-coverage gap reports actionable. Modest z3 plumbing; big readability
win for the talking-to-collaborators use case.

### D4. Property-based encoder testing + CI (medium)
The encoder is now the trusted core; goldens cover known bugs but not the
unknown ones. **Plan:** a generator producing random small regions from a
fixed key vocabulary (comparisons, AND/OR/NOT/ITE/reject/defines), an
oracle that grid-samples the variable space and evaluates the ADL
semantics directly, and an assertion that PROVEN verdicts never contradict
the oracle. Run a fixed-seed batch in CI (GitHub Actions: flex/bison/clang
+ z3, `make test` + sampler). This is the guardrail the X track will lean
on, because cross-file work multiplies the encoder's exposure.

### D5. Remaining encoding gaps (small, opportunistic)
`min/max(a,b) op c` (exact Or/And split), `abs(linear)` two-branch,
`sqrt(x) op c` for nonneg x (square both sides), `|eta|`-style braced
forms if the corpus uses them. Each is an isolated `leafFromCompare` case
with a golden.

### D6. Witness/report polish (small)
Round witness values sensibly, order them by key, mark axiom-derived
values; add `--quiet` for machine consumption (stdout is still noisy from
parser debug prints — fold the remaining ungated couts behind ADL_DEBUG
while there).

---

## Track X: cross-file disjointness

**Physics framing.** The question "can an event land in region R_A of
analysis A and region R_B of analysis B?" is the statistical-independence
question behind analysis combination and reinterpretation (overlap
matrices à la TACO/SModelS). The verdict is only meaningful under an
explicit assumption: **both analyses run over the same underlying event
stream with comparable object reconstruction.** Two ATLAS and CMS analyses
are trivially "disjoint" (different events) and comparing them is
meaningless. The tool must state this assumption loudly rather than bury
it.

### X0. Prerequisites (mostly already planned above)
- D1 object-block encoding (object identity needs object *definitions*).
- Reentrancy: the alias/pure-alias caches are per-process
  (`resetTakeAliasCache` exists but nothing calls it). Make them per-Driver
  state so two files can load in one process without contamination. Audit
  the remaining parser globals (`paramlist`, counter) — sequential parsing
  of two files is fine today, but verify with a test that parses A then B
  and re-checks A's AST snapshot.
- G5 snapshots (any front-end change during X lands safely).

### X1. Cross-file key identity (the heart of it — design carefully)
Same name across files does **not** mean same quantity; different names
may be identical. Repeating the single-file lesson: merging two keys that
denote different event quantities fabricates disjointness proofs, so
identity must be earned, never assumed. Proposed model:

1. **Namespace everything by default**: every key from file A becomes
   `A::key`, from B `B::key`. With zero unification the analysis is sound
   but useless (no shared dimensions) — unification only ever *adds*
   sound equalities.
2. **Unify the detector-level common ground** (the same-events assumption
   makes these identical): base collections from ext_objs (Jet, Muon,
   Electron, MissingET...), MET-family scalars, event-level functions of
   base collections, and trigger flags with identical names.
3. **Unify derived objects by proven equivalence**: object signature =
   (canonical base root, per-object cut formula from D1). If
   `valid(φ_A ⇔ φ_B)` (one z3 query over element-property variables), the
   collections are the same filter of the same base → unify all their
   keys. Name-insensitive: A's `goodJets` can unify with B's `cleanjets`.
4. **Relate by proven implication**: if `valid(φ_A ⇒ φ_B)` then A ⊆ B →
   subset axioms (size monotonicity, per-index pt domination from D1),
   not key merging.
5. **Defines**: inline as today (each file's defines inline into its own
   regions), so they need no cross-file treatment; value-defines used as
   opaque scalars get namespaced unless their bodies linearize into
   already-unified keys (the linear-atom machinery makes most of these
   comparable for free).
6. Everything else stays namespaced and shows up in the report as
   "private dimensions" of each file.

Output should include an **identity report**: which objects unified, which
got subset relations, what stayed private — so a physicist can audit the
assumptions before trusting the matrix.

### X2. Cross-product analysis
Load both files (two Drivers), build region encodings per file with
namespaced/unified keys, then run the existing dual-encoding pair analysis
over A×B pairs (the engine is already key-agnostic; only the key-identity
layer is new). Batched z3 as today; for M×N pairs the single-process
design matters even more. Within-file pairs are also re-reported so the
output is one coherent matrix.

### X3. CLI and outputs
- `./smash -r --cross fileA.adl fileB.adl` (N≥2 files later; start with 2).
- `--assume-same-events` explicit (default on for `--cross`, with a
  banner stating the assumption).
- JSON: overlap matrix `{a_region, b_region, kind, shared_dimensions,
  subset flags}` plus the identity report; optionally a plain CSV matrix
  for combination tooling.
- Headline summary: "K of M×N cross pairs proven disjoint; J proven
  overlapping; coverage caveats: ...".

### X4. Validation
- Goldens: (a) the same file duplicated under two names → every region
  proven overlapping with itself and subset both ways (identity sanity);
  (b) two crafted files with complementary HT windows on equivalent jet
  definitions → proven disjoint; (c) same cuts on *differently filtered*
  jets → must NOT prove disjoint (the cross-file analogue of the lineage
  bug); (d) object equivalence despite different names/order of cuts.
- Corpus pair: CMS-SUS-16-032 × CMS-SUS-16-033 (both Delphes-based, share
  HT/MET/njet vocabulary) — eyeball the matrix with your collaborators as
  the acceptance test.

### X5. Later extensions (out of scope for the first cut)
- N-file matrices and transitive identity.
- "Effective overlap" ranking: among non-disjoint pairs, use witness
  geometry / shared-dimension counts to rank likely correlation (input to
  combination tools; not a proof, label accordingly).
- Import/include semantics if ADL ever grows them.

---

## Suggested sequencing

1. **G5** AST snapshots (enables everything else safely).
2. **D1** object-block encoding + subset/element axioms (single-file value
   now, X1 foundation).
3. **X0** reentrancy + two-driver test.
4. **G1** fractional boundaries; **D2/D5/D6** opportunistically alongside.
5. **X1–X3** cross-file identity, cross product, CLI/JSON.
6. **X4** goldens + 032×033 acceptance.
7. **D3** unsat-core explanations (lands best once cross matrices exist —
   that's where "why" matters most).
8. **D4** property-based testing + CI before declaring the X track stable.
9. **G2–G4, G6** grammar/AST debt as the slower background track.

## Decisions needed from you

1. **Scope of "cross-file"**: two analyses from the same experiment/dataset
   (the combination use case), or also same analysis split across files?
   The plan assumes the former; the latter would need include semantics
   instead of the identity layer.
2. **Identity strictness default**: unify base collections automatically
   (assume comparable reconstruction), or require an explicit
   `--unify-base` opt-in? Plan assumes automatic-with-banner.
3. **Output consumer**: is the JSON/CSV matrix feeding an existing
   combination workflow (TACO-style), and if so what schema do your
   collaborators want?
4. **Trigger semantics**: treat identical trigger names across files as the
   same event flag (plan assumes yes)?
5. The CMS-SUS-16-032 vacuous-region finding: fix the example file in this
   repo, report upstream, or both?
