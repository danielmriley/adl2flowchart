# Dual-encoding disjointness engine — implementation report

June 2026. Follow-up to `docs/REVIEW_NOTES.md`, which identified seven
verified soundness bugs in the region disjointness analysis plus compiler-core
defects. This report describes the rewrite, what it provably fixes, the test
evidence, and recommended next steps.

## 1. The core idea

The old pipeline extracted whatever cuts it could recognize into per-key
intervals and fed one formula to both kinds of proof. Every extraction gap
was resolved silently and arbitrarily — sometimes weakening the region
(dropped conjuncts), sometimes strengthening it (dropped OR branches, ITE
then-hoisting, define injection, broken reject complement). A strengthened
formula yields false **PROVEN DISJOINT** verdicts; a weakened one yields
false **PROVEN OVERLAPPING** verdicts. Both happened.

The new engine (`adl/constraint_encoder.cpp`, `adl/region_formula.h`,
`adl/region_analysis.cpp`) makes the approximation direction explicit:

1. **Exact compilation.** Each region's selection logic is recursively
   compiled to a boolean formula over per-event scalar variables:
   `And / Or / Not(via NNF) / comparison atoms`. `reject X` is exactly
   `¬encode(X)`. Ternaries become `(g ∧ then) ∨ (¬g ∧ else)`. Boolean
   defines are inlined *at their reference site* (so a define under `||`
   stays disjunctive). Region inheritance is inlined recursively.
2. **Explicit ignorance.** Any subformula that cannot be translated
   faithfully becomes an `Unknown` leaf carrying a human-readable note.
   Nothing is silently dropped or hoisted, ever.
3. **Two projections, two proof directions.**
   - R⁺ replaces Unknown with `true` (superset of the real region).
     `UNSAT(R1⁺ ∧ R2⁺)` ⇒ **PROVEN DISJOINT** — sound because supersets
     that cannot intersect imply the real regions cannot.
   - R⁻ replaces Unknown with `false` (subset of the real region).
     `SAT(R1⁻ ∧ R2⁻)` ⇒ **PROVEN OVERLAPPING** with a concrete witness —
     sound (within the scalar model) because the witness satisfies real,
     fully-encoded cuts of both regions.
   - `UNSAT(R1⁺ ∧ ¬R2⁻)` ⇒ **PROVEN SUBSET** A ⊆ B — new verdict, nearly
     free once negation is exact. This is the CR/SR bookkeeping check
     physicists actually want.

   When a region has no Unknown leaves, R⁺ = R⁻ = R and the verdict is
   labeled *exact encoding*.

The structural consequence: the bug class "a dropped cut flipped the
verdict" is impossible by construction, not fixed case-by-case. An
unencodable cut can only ever widen R⁺ (making disjointness *harder* to
prove) or shrink R⁻ (making overlap *harder* to prove). Lost coverage
degrades to honest "POSSIBLY", never to a wrong "PROVEN".

## 2. Supporting changes that the proofs needed

- **Key identity.** A variable key is now `COLLECTION[index].property`
  with the index attached to the collection (a parser bug dropped `[0]`
  from `jets[0].BTag`; fixed in the `id_qualifiers` rule). Spelling aliases
  and in-file *pure renames* (`object MHT take MissingET` with no cuts)
  merge into one variable; **filtered collections no longer merge with
  their parents** — `bjets[0].pt` and `jets[0].pt` are different event
  quantities, and the old union-find that merged them (plus the alias file
  folding BJet/FatJet/scalarHT into base collections) could fabricate
  disjointness proofs. `object_aliases.txt` is trimmed accordingly.
- **Quantifier guard.** Cuts on un-indexed collection properties
  (`pt(jets) > 30` at region level) have ambiguous any/all semantics and
  become Unknown instead of being scalarized. Defines, MET-family/HT-family
  singletons, indexed elements, `size(...)`, angular pairs, and functions of
  scalar arguments are modeled as scalars.
- **Background axioms**, asserted with every check because they are true of
  every event: pT-ordering of indexed elements (`pt(C[0]) ≥ pt(C[1])`),
  `size ≥ 0`, referencing `C[i]` implies `size(C) ≥ i+1`, and
  `size(derived) ≤ size(parent)`. Sound for both proof directions; they
  show up visibly in witnesses (e.g. jets at 101/76/31/31 GeV).
- **One z3 process per file** with `(push)/(pop)` per check, instead of one
  process spawn (plus a `command -v z3` shell-out) per pair. CMS-SUS-16-032
  (10 regions, 45 pairs, dual checks + subset checks): **0.14 s**.
- **Honest coverage.** Coverage = encoded fraction of *condition leaves in
  the AST*; every Unknown is listed per region as `dropped: <why>`. The old
  metric counted only what was already extracted, reporting 100% while
  silently deleting branches.
- **Witnesses** are parsed from the model properly (`v_MET_pt=121.0`)
  instead of the old truncated `define-fun` headers with no values.

## 3. Review findings → outcomes

All "before" rows were reproduced live against the old build
(`docs/REVIEW_NOTES.md` §1); all "after" rows are locked in by golden tests
(`tests/golden/`, `make test`).

| Finding | Case | Before | After |
|---|---|---|---|
| 1a | `reject (A \|\| B)` vs region inside the kept band | **false PROVEN DISJOINT** (reject-OR asserted positively) | PROVEN OVERLAPPING + PROVEN SUBSET + witness (`reject_or_band.adl`) |
| 1b | `reject (A && B)` | De Morgan violation deleted the region's only cut; missed disjointness | PROVEN DISJOINT, exact (`reject_and_band.adl`) |
| 1c | `select MET>500 \|\| mva>0.9` vs `MET<100` | **false PROVEN DISJOINT** (dropped branch became hard conjunct) | PROVEN OVERLAPPING with witness through the MVA branch (`or_unencodable_branch.adl`) |
| 1d | `select not jets[0].BTag == 1` | garbage key `NOT.jets`, negation lost (lexer never emitted NOT) | exact NE atom; complementary regions PROVEN DISJOINT (`not_tag.adl`) |
| 1e | `jets[0].BTag` vs `jets[1].BTag` | indices aliased into one variable | distinct keys; never proven disjoint across indices (`tag_index.adl`) |
| 1f | `select lowmet \|\| MET>500` | define body injected as hard conjunct; region's cut vanished | define inlined in place; true disjointness proven (`define_under_or.adl`) |
| 1g | filtered-collection lineage merging, scalarHT→MET alias | could fabricate UNSAT across distinct quantities | merging only for identity (aliases/pure renames); subset handled via size axioms |
| 1h | collection cuts scalarized | silent | quantifier guard → Unknown + dropped-note |

Compiler-core fixes from the same review, landed alongside: NOT token,
real line numbers in errors (`ERROR at line 5` instead of the AST counter),
invalid characters warn instead of vanishing, `alpha-beta` is subtraction
(hyphens kept only in the filename-style underscore rule that real BDT
arguments use), reduce/reduce conflicts 30→5, `typeCheck` functional in
release builds and no longer polluting `dependencyChart["UNKNOWN"]`,
run-from-anywhere (dead cwd-relative loader removed), honest stage-failure
messages, swapped `mult`/`sub` stubs, uninitialized `fMR/fMTR` returns,
`dR(x,x)` copy-paste, incremental-build prerequisites + `-Wall`.

Validation: 23 golden checks pass (including five soundness-regression
goldens and a negative assertion), all 68 corpus files parse and analyze,
and the Delphes-033 spike passes with every region encoding exactly. On
CMS-SUS-16-033 the analysis now proves the size(BJETS)/HT/MHT signal-region
binning disjoint and identifies SR ⊆ presel subsets — with all 13 regions at
100% leaf coverage.

## 4. What should come next

In rough priority order:

1. **Physical range axioms** for bounded quantities (dR ≥ 0, b-tag ∈ {0,1},
   plausibly |Δφ| ≤ π once the convention is pinned down). Today a witness
   may set `aplanarity = 1.9`; the verdict is still sound relative to the
   encoded cuts, but range axioms would make witnesses physical and catch
   vacuous regions (cuts that no physical event can satisfy).
2. **Arithmetic atoms.** The one cut dropped in CMS-SUS-16-032 is a ratio,
   `(pT(...) + MET)/MET < 0.5`. Encoding linear arithmetic over named
   scalars (sums, differences, constant multiples — and ratios via
   multiplication when the sign is known) is squarely within QF_LRA and
   would push most real files to exact encodings.
3. **Per-object quantification.** The remaining honest gap: model
   `pt(jets) > 30`-style cuts with a small bounded-index expansion
   (quantify over the first k elements + size guard) instead of dropping
   them. This would also let filtered-collection relations (`bjets ⊆ jets`)
   be expressed per element rather than only via size monotonicity.
4. **Partition checking for bins**: `bin` lists partition a region by
   construction; verifying the bins actually cover and don't overlap
   (UNSAT pairwise ∧ region ⊆ union of bins) is a one-evening feature on
   this engine and a real analysis-review aid.
5. **Compiler core debt** (tracked in REVIEW_NOTES §4): the remaining 5
   reduce/reduce conflicts (take_id grammar), AST ownership (arena model),
   splitting semantic_checks.cpp, deleting the unreachable CutLang lowering
   path (~1000 lines), and a unit-test harness beyond stdout greps —
   property-based testing (random small regions vs. brute-force sampling)
   would guard the encoder itself.
6. **Retire the legacy printer** (`--legacy-region-report` and its
   extraction stack in semantic_checks.cpp) once collaborators have
   migrated; it still contains the old unsound lineage merging.
