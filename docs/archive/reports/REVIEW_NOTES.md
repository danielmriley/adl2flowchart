# Project review notes — June 2026

> **Status update:** the disjointness soundness bugs in §1 and the
> top-priority core fixes are resolved by the dual-encoding rewrite — see
> `docs/DUAL_ENCODING_REPORT.md` for what changed and the remaining roadmap.
> §4's architecture/test debt items remain open except where noted there.

Focus: overall code health + deep dive on disjointness/overlap detection
(`adl/region_analysis.cpp`, extraction in `adl/semantic_checks.cpp`).

All disjointness findings below were reproduced empirically against the current
build (z3 4.8.12 on PATH); repro files are inlined.

## 1. Disjointness: soundness bugs (verified, highest priority)

The central problem: **the verdicts say "PROVEN" but several extraction paths
strengthen a region's formula** (make it accept fewer events than the real
ADL). Strengthening is only safe for *overlap* proofs; for *disjointness*
proofs it produces false positives. The pipeline currently has no notion of
approximation direction, so unsound transforms leak into UNSAT-based
"PROVEN DISJOINT" verdicts — the verdict a physicist is most likely to act on.

### 1a. `reject` with OR / ITE is never complemented → inverted constraint

`gatherRegionConstraints` (semantic_checks.cpp:2388-2413) applies
`complementConstraint` only to plain conjunct atoms. OR clauses and
implications extracted from a `reject` statement are asserted **positively**.

```adl
region SR_band
  reject MET.pT < 100 || MET.pT > 200   # keeps the band [100,200]
region SR_mid
  select MET.pT > 120
  select MET.pT < 180                    # inside the band
```
True answer: overlapping (SR_mid ⊂ SR_band). Output: **PROVEN DISJOINT [SMT]**.

### 1b. `reject (A && B)` complemented per-atom → De Morgan violation

`reject` of a conjunction complements each atom independently, encoding
¬A ∧ ¬B instead of ¬A ∨ ¬B. For `reject MET.pT > 100 && MET.pT < 200` the two
complements intersect to an empty interval which is then **silently dropped**
(semantic_checks.cpp:2453-2454) — the region loses its only cut and reports
"0/0 atoms" while still counting the select as "encoded 1/1".

Also `complementConstraint` (semantic_checks.cpp:1254) is a **silent no-op**
for bounded two-sided intervals and for equalities on non-size keys — the
constraint is kept *un-negated*.

### 1c. OR with a non-encodable branch becomes a hard conjunct

`extractConstraintStructure` (semantic_checks.cpp:1848-1861): when only one OR
alternative yields atoms, that alternative is pushed as an unconditional
conjunct. Dropping an OR branch strengthens the region.

```adl
region SR_orcut
  select MET.pT > 500 || aplanarity(jets) > 0.9
region SR_lowmet
  select MET.pT < 100
```
True answer: can overlap (high-aplanarity low-MET events pass both).
Output: **PROVEN DISJOINT** (heuristic), and coverage says "1/1 atoms,
selects encoded 1/1" — the dropped branch is invisible.

### 1d. `not` is mis-parsed in extraction → garbage key, negation lost

`not X` parses as a FunctionNode wrapping X (parser.y:283-286), but
`tryFindDiscreteTagConstraint` recurses through function params with no
negation tracking. `select not jets[0].BTag == 1` extracts as key
`NOT.jets == 1` (the "not" VarNode is treated as an object named NOT).
Complementary regions `BTag == 1` vs `not BTag == 1` report
"POSSIBLY OVERLAPPING — no shared constraint dimension" instead of
PROVEN DISJOINT. Conservative here, but the same negation-blind recursion
will extract `tag == 1` positively from under a `not` in other shapes
(inversion).

### 1e. Tag keys lose bracket indices

`jets[0].BTag == 1` canonicalizes to `JET.BTag` (no `[0]`), so
`jets[0].BTag` and `jets[1].BTag` alias into one SMT variable — contradicts
the WP1 index-safety fix, which only landed for the `pT(jets[0])` path
(`buildKeyFromVar` appends the accessor, but the tag/`smartKeyFromSide`
paths drop it).

### 1f. Define constraints injected as unconditional conjuncts

`appendDefineConstraints` (semantic_checks.cpp:1181) walks the whole condition
tree — through OR, ITE guards, function params — and appends the define's body
atoms as **hard conjuncts**. `select lowmet || MET.pT > 500` gets both
`MET < 100` and `MET > 500` as conjuncts → empty → the region's MET cut
vanishes entirely (verified: pair reported as "no shared dimension").

### 1g. Lineage-based key merging conflates different collections

`constraintKeysRelated` (semantic_checks.cpp:2303) unifies keys whose objects
are lineage-related (e.g. `bjets` taken from `jets`). But `bjets[0].pt` and
`jets[0].pt` are *different event quantities* (leading b-jet ≠ leading jet);
merging them into one SMT variable can prove false disjointness. Same family:
the legacy analyzer's "tag mutex via object lineage" (semantic_checks.cpp:2590)
treats `jetsA.BTag==1` vs `jetsB.BTag==0` on related collections as a
disjointness proof — unsound for per-object tags quantified over different
subsets.

Related config risk: `adl/object_aliases.txt:7` maps `MHT`,
`Delphes_scalarHT`, `scalarHT` → `MET`. scalarHT is a scalar jet sum — not
interchangeable with MET. Any file cutting on both gets them unified into one
variable.

### 1h. Collection-level cuts have no quantifier semantics

Every key is one scalar SMT variable. `select pT(jets) > 100` (per-object /
any/all semantics) is encoded identically to a cut on a single scalar. This is
the documented "fragment" limitation, but nothing in the verdict
distinguishes "proved over scalars that really are event-level scalars"
(MET, HT, size, jets[i].x) from "proved by pretending a collection is a
scalar".

## 2. Disjointness: correctness/quality issues (smaller)

- `intervalsOverlap` (region_analysis.cpp:101) at a touching boundary
  `a.hi == b.lo` returns true without checking inclusivity (the `atLo`/`atHi`
  checks only cover identical-endpoint cases). Masked today because
  `intervalsDisjoint` runs first, but wrong as a standalone predicate.
- SAT witness is mangled: `runZ3` (region_analysis.cpp:401) captures
  `(define-fun ...)` header lines but z3 prints values on the *next* line, so
  witnesses show variable names with no values. Use `(get-value (...))`
  or parse the model as an s-expression.
- `isSmtEncodableKey` = `key.find("BDT") == npos` — substring blocklist; other
  MVA/discriminant cuts are happily encoded as free reals, BDT-named-anything
  is dropped. Should be a whitelist of understood key shapes instead.
- Coverage metrics are misleading: the denominator is *extracted atoms*, not
  cuts present in the ADL. A select whose OR branch was dropped, or that
  extracted a garbage key, counts as fully encoded (verified in 1c/1d).
  "selects encoded N/M" counts "at least one atom extracted", not "fully
  encoded" as the warning text claims.
- `abs(eta(x)) < 2.4` — the single most common cut shape in HEP — is not
  extracted (no `abs()` handling; only a literal `abseta` function name).
- `PossiblySubset` enum value is dead (WP5 deferred) but serialized in JSON
  switch — fine, just noise.
- Performance: `z3Installed()` shells out (`std::system`) once per pair
  (region_analysis.cpp:466) on top of one temp-file + `popen` z3 process per
  pair → O(N²) process spawns. For 50 regions that's ~2,450 subprocesses.
- Global mutable state: `g_disjointDrv`, `g_takeAliasToCanon`,
  `g_takeAliasesReady` (never reset between parses) make the analysis
  single-shot and untestable as a library.
- `buildKeyFromVar` uses magic sentinel `6213` for empty accessor slots.
- `extractSimpleConstraint` is a ~200-line cascade of six overlapping
  fallback strategies ("aggressive pass", "broad fallback", "symmetric
  version") with subtly different key synthesis — this is where most of the
  soundness bugs live.

## 3. How to do disjointness better (recommended architecture)

1. **Track approximation direction.** Build *two* encodings per region:
   - R⁺ (over-approximation): non-encodable subformulas become `true`.
     Only transform allowed: weakening. UNSAT(R1⁺ ∧ R2⁺) ⇒ sound
     **PROVEN DISJOINT**.
   - R⁻ (under-approximation): non-encodable subformulas become `false`.
     SAT(R1⁻ ∧ R2⁻) ⇒ sound **PROVEN OVERLAPPING** (within scalar-modeling
     assumptions), and the witness is a real event candidate.
   Anything that doesn't hold in both directions is reported as POSSIBLE,
   never PROVEN. This single change fixes 1a–1c and 1f *by construction*,
   because branch-dropping/conjunct-injection becomes a type error in the
   encoder rather than a silent default.

2. **Replace the extraction pattern-zoo with one recursive encoder.**
   `encode(Expr*, polarity) -> SMT formula + coverage info` handling
   AND/OR/NOT/ITE/comparison/define-inline structurally, with negation done
   by polarity flip instead of interval complementation. Defines get inlined
   *at their reference site in the tree* (not appended globally), which fixes
   1f and makes `reject X` simply `encode(X, negated)` — fixing 1a/1b/1d.
   Most of `extractSimpleConstraint`'s fallbacks then delete.

3. **Fix key identity.** A key should be a structured value
   `(collection, index?, property)` rendered to a string at the end —
   not a string assembled differently by five helpers. Merge two keys only
   when they denote the same event quantity: same canonical collection AND
   same index/property. Model subset lineage (`bjets ⊆ jets`) as
   *implications over cardinalities* (`size(bjets) <= size(jets)`) and shared
   per-index variables only when provably the same ordering — not by aliasing
   variables.

4. **Talk to z3 once.** Use one persistent z3 process (stdin, `(push)`/`(pop)`
   per pair) or link libz3. Declare each region's formula once as a named
   assertion; pairwise checks become push/assert/check/pop. Kills the O(N²)
   process spawning and enables `(get-value ...)` for clean witnesses.

5. **Subset detection is nearly free once encoding is sound:**
   UNSAT(R1⁻ ∧ ¬R2⁺) ⇒ R1 ⊆ R2. This is the verdict physicists actually want
   for CR/SR bookkeeping, and WP5 deferred it mainly because negation was
   unsafe in the current encoder.

6. **Honest coverage.** Coverage = encoded fraction of the *condition AST*
   (count comparison leaves, not extracted atoms). Print per-region "dropped:
   <expr>" lines for anything that became `true`/`false` in R⁺/R⁻. A verdict
   line should carry its assumptions: `PROVEN DISJOINT (full encoding)` vs
   `DISJOINT IN FRAGMENT (62% encoded)`.

7. **Adversarial golden tests.** Each bug above should become a golden test
   asserting the *physics* truth (files in this review: reject-OR band,
   reject-AND band, OR-with-MVA-branch, not-BTag, define-under-OR, indexed
   tags). Longer term: property-based testing — generate random small regions,
   compare SMT verdicts against brute-force sampling of the cut space.

## 4. General project notes (compiler core)

### Critical — wrong behavior today

- **Grammar conflicts**: `bison -Wall` reports 57 shift/reduce and 30
  reduce/reduce conflicts; rule `chain QUES chain` (parser.y:272) is dead due
  to conflicts. R/R conflicts mean some inputs silently parse to the wrong AST.
- **Lexer mis-tokenizes subtraction**: `alpha-beta` lexes as one ID (malformed
  regex `*?+` in scanner.l:90); `5-3` lexes as `INT(5) INT(-3)` → syntax error.
- **Invalid characters silently echoed** (no catch-all lexer rule) instead of
  erroring.
- **Error positions are garbage**: `Parser::error` prints the AST uid counter
  as the line number (parser.y:392); locations are constructed but never
  tracked (scanner.l, driver.h:108).
- **`typeCheck` is a no-op outside `ADL_DEBUG`** (semantic_checks.cpp:368-390)
  and files every binop define under `"UNKNOWN"` in `dependencyChart`
  (semantic_checks.cpp:409-411) — this pollutes lookups the region analysis
  consumes.
- **Swapped arithmetic stubs**: `mult` returns `left - right`, `sub` returns
  `left * right` (cutlang_declares.cpp:111-112, verified); `fMR`/`fMTR` return
  uninitialized values.
- **dR/dPhi/dEta built with the same particle twice** — `params[1]` used for
  both arguments (driver.cpp:552-567).
- **End-iterator dereferences** after failed map lookups (driver.cpp:589-635).
- **Binary only runs from repo root**: hardcoded relative
  `"adl/property_vars.txt"` with `exit(1)` (main.cpp:17-21) — and the map it
  loads is never read; uncaught `runtime_error` in the Driver ctor if no
  `adl/` ancestor.
- **AST ownership incoherent**: `BinNode` deep-clones + deletes children while
  six other node types shallow-copy raw pointers with no destructor;
  `BinNode::clone` mutates `this`; nothing is ever freed (ast.hpp:59-86,
  258-577). Recommended fix: arena ownership (Driver owns all nodes, no node
  deletes children) and remove clone-on-construct.
- **Parser builds semantic values in shared global vectors**
  (parser.y:33-39); right-recursive `param_list` delivers params **reversed**,
  which downstream code compensates for by indexing `params[1]` — fragile.
- Unchecked `fopen` for ast.dot/fc.dot (semantic_checks.cpp:348, 2659).

### Important

- Cascading "Failed X()" messages for stages that never ran (main.cpp:89-105);
  a missing input file is never diagnosed (empty-stream parse).
- `Driver::parse(fileName)` ignores its argument (driver.cpp:69-73); Driver
  heap-allocates 11 containers as raw pointers, never freed (driver.cpp:26-36).
- Duplicate, contradictory parent-object factory tables — `fillParentObjectsMap`
  maps MUO/TAU/PHO/JET→`createNewEle` (driver.cpp:121-128) vs. the correct
  mapping in `createParentObject` (driver.cpp:751-781).
- `fillFuncMaps`: `"ctag"→isBTag`, `"anyof"/"allof"→abs`, `"mass"` mapped twice
  (driver.cpp:994-1067).
- Debug spam: 100+ ungated `std::cout` in driver.cpp/main.cpp/parser.y; golden
  tests grep this soup.
- Makefile: no object files / incremental build; `cutlang_declares.cpp` missing
  from prerequisites (edits don't trigger rebuild); no `-Wall`; `clean` uses
  `rm` without `-f`.
- ast.hpp not self-contained (`ExprVector` typedef precedes `Expr`); works only
  via Parser.h include order.

### Cleanliness / architecture

- `semantic_checks.cpp` is a god-file (2752 lines, 7 concerns). Natural split:
  `dot_output.cpp`, `decl_checks.cpp`, `constraint_extract.cpp` (IR shared
  with region_analysis), `object_lineage.cpp`. The constraint IR structs in
  semantic_checks.h:69-99 belong in their own header.
- The entire CutLang lowering path (~450 lines in driver.cpp + ~600-line node
  hierarchy in cutlang_declares.h that stores none of its constructor args) is
  unreachable from main and stubbed at every leaf — candidate for wholesale
  deletion in a visualizer-focused repo.
- Type discrimination by string token + unchecked `static_cast` everywhere; an
  enum kind or visitor removes that bug class.
- `findTagProperty` ~140 lines re-implementing the same substring scan five
  ways; discrete-tag extraction block repeated three times.
- Dead code: empty `external_functions.cpp`, `histoBinsLists`, `fsfunction_map`,
  37 commented lines in main.cpp, magic `6213` sentinel in 5 places.

### Testing

- Only 7 golden ADL files, all stdout-grep of the region analysis; z3-gated
  assertions silently skip without z3. Nothing tests DOT output, checkDecl
  failures, lexer/parser errors, or negative inputs (`examples/bad/` is never
  referenced). `validate_corpus.sh` checks exit codes only, so wrong-AST
  parses from grammar conflicts are invisible. No CI, no warnings, no linter.

### Suggested priority order

1. Soundness-direction-aware encoder rewrite for disjointness (§3.1-3.2) —
   this is the headline feature and currently emits false proofs.
2. Lexer/grammar correctness (conflicts, subtraction, error locations) — it
   silently mis-parses the inputs everything else depends on.
3. typeCheck no-op + dependencyChart pollution.
4. Run-from-anywhere + error-reporting hygiene.
5. AST ownership cleanup; split semantic_checks.cpp; delete dead CutLang path.
6. Test harness: negative tests, AST snapshot tests, adversarial disjointness
   goldens, CI with `-Wall -Wextra` and bison conflict gate.
