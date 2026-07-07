# BUILD_NOTES.md — ADL2 workspace build log

Append-only. Each phase records toolchain facts and any deviation from the
specs, dated.

## 2026-06-11 — Phase 1 workspace bootstrap

### Toolchain (probed on this machine)

- `cargo 1.93.1` / `rustc 1.93.1` (stable). Edition 2024 supported
  (SPEC_ARCHITECTURE / ADR-001 mandate edition 2024; MSRV pinned to 1.93 in
  `[workspace.package]`).
- `nproc` = 12.
- libz3: `libz3-dev 4.8.12` and `libz3.so` present; `z3 4.8.12` CLI on PATH;
  `cvc5` also on PATH (`/home/daniel/bin/cvc5`). Both available for the
  Phase-5 subprocess backend.

### Online build — crates.io reachable

crates.io **is** reachable from this environment, so the build is ONLINE.
Probed each spec-named dependency in a throwaway crate and confirmed it
builds and (for z3) links + runs:

| Dep | Version resolved | Notes |
|---|---|---|
| serde / serde_json | 1.0.228 / 1 | derive feature on |
| clap | 4.6 | derive feature on |
| z3 (native) | 0.20.0 (z3-sys 0.11.0) | links against system libz3 via pkg-config; **no `Z3_SYS_Z3_HEADER` needed** — z3-sys found the system header automatically. A `z3::Context` was created and `SatResult` round-tripped at runtime. |
| insta | 1.48 | snapshot testing |
| proptest | 1.11 | property/metamorphic testing |

All six are declared in `[workspace.dependencies]` in the root `Cargo.toml`
but are **not yet pulled into any member crate** — member `Cargo.toml`s have
empty `[dependencies]`/`[dev-dependencies]` so the placeholder build stays
minimal. Later phases add `serde.workspace = true` etc. to their own crate
without touching the root (the version/feature decisions are already frozen
here).

Because the build is online, the offline fallbacks named in the task brief
(hand-rolled snapshot/property helpers, subprocess-only solver, manual arg
parsing) are **NOT** in effect. If a future run finds crates.io unreachable,
strip the unused `[workspace.dependencies]` entries and switch to those
fallbacks, recording it here.

### Workspace layout

All ten members from SPEC_ARCHITECTURE §1 are declared now so later phases
never edit the root `Cargo.toml`:

- libs: adl-syntax, adl-sema, adl-interp, adl-formula, adl-axioms,
  adl-solver, adl-analysis, adl-viz, adl-difftest
- bin: adl-cli (binary name `smash2`, via `[[bin]]`)

Each crate has a placeholder `lib.rs` (a `CRATE_NAME` const) / `main.rs` and
exactly one trivial test. Internal crates are wired as path deps in
`[workspace.dependencies]` (also unused so far) so dependency arrows can be
turned on later without root edits.

### Corpus gate

`scripts/corpus_gate.sh` finds all 68 `examples/**/*.adl` files and currently
exits 0 (TODO stub — `smash2 check` is not implemented). Flip `RUN_CHECK=1`
once the Phase-1 parser lands to enforce zero parse errors. The CLI `check`
subcommand is a no-op success stub for the same reason.

### Verification (run before returning)

- `cargo build --workspace` — green.
- `cargo test --workspace` — green, 11 tests pass (1 per crate, incl. the CLI
  binary's `version_string_is_set`).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean (exit 0).
- `scripts/corpus_gate.sh` — exit 0, reports 68 files.

### Deviations from spec

1. **Edition 2024, MSRV 1.93** — SPEC_ARCHITECTURE states "Rust (2024
   edition)"; ADR-001 says the same. No deviation in edition; documenting the
   MSRV choice (1.93, the installed stable) so later phases know the floor.
2. **`[profile.dev.package."*"] opt-level = 1`** — added so the heavy
   transitive deps (notably z3-sys, once a crate uses it) don't dominate
   incremental dev builds, while our own crates stay at `opt-level = 0` for
   fast rebuilds. Pure build-time ergonomics; no semantic effect. Remove if it
   ever surprises a CI profile.

No semantic/spec deviations. Nothing weakened or skipped.

## 2026-06-11 — Phase 1: adl-syntax (lexer, parser, AST, dump, diagnostics)

Implemented per SPEC_LANGUAGE §2–3 and SPEC_ARCHITECTURE §3: hand-written
lexer (underscore-split identifier rule with visible notes, NEWLINE tokens
consulted only by greedy productions, no signed literals, sci-notation
lexical error with rewrite help, adjacent-pair operators `[]` `][` `+-`
`==` `!=` `>=` `<=` `~=` `&&` `||`), recursive-descent parser (one function
per EBNF nonterminal; expression precedence per §3 via the explicit
nonterminal chain), spanned owned-enum AST, multi-error diagnostics with
statement resynchronization (span + label + help, rustc-like rendering),
canonical deterministic AST dump.

Deliverables:
- `crates/adl-syntax/src/{span,diag,token,lexer,ast,parser,dump}.rs`
- `crates/adl-syntax/examples/parse_adl.rs` (`--dump-ast`, `--quiet`;
  exit 1 on errors) — used by `scripts/corpus_gate.sh`, which now REALLY
  gates: builds the example binary and requires all 68 corpus files to
  parse with zero errors (green at this commit).
- Tests: 123 passing workspace-wide. Lexer unit battery (every §2 rule),
  parser battery (every §3.1 divergence: not-recursion, dotted access,
  unary minus everywhere, real bin edges, multi-arg union, particle-list
  args, plus underscore indexing on `goodJets_1` and `{JET_}` trailing
  underscore), 6-case error-quality battery (`selct` → "did you mean
  `select`?", stray `;`, unterminated string, `not not x` PARSES, `1e6`
  with `1000000.0` help, mid-file garbage with recovery), insta snapshots
  for 12 corpus files + all 25 legacy goldens (dump + diagnostics),
  corpus-gate test (68/68, zero errors), dump-determinism test.
  `cargo clippy --workspace --all-targets -- -D warnings` clean; rustfmt
  applied.

### Grammar gaps found in the corpus — minimal extensions over the spec EBNF

The spec EBNF §3 plus divergences was treated as the contract; the corpus
needed the following minimal additions (each parsed into a real AST node,
nothing silently dropped). SPEC_LANGUAGE §3 should pick these up at the
next spec revision:

1. **`reject` inside object blocks** (ATLAS-EXOT-1704-0384, CMS-SUS-21-006
   etc.): `object-block` statements now include `reject-stmt`.
2. **Take binders**: `take jets j` (ATLAS Delphes), `take leptons l1, l2`
   (composite blocks, CMS-SUS-16-041_Delphes). Optional same-line
   `ident {"," ident}` binder list after `take-source`.
3. **Take alias suffix**: `object OSdileptons : COMB(...) alias adilepton`
   (Examples/CMS-SUS-16-041). Optional same-line `alias ident`.
4. **Counts tails contain commas**: `counts results 997 +- 32 +- 40 , 933`
   (ex12). `","` added to the counts token set.
5. **Bare histo variable-bin edge lists**: `histo h, "t", 0.0 10.0 20.0, MET`
   (ex02) — `histo-arg` accepts an unbracketed `signed-num { signed-num }`
   list in addition to the spec's bracketed form.
6. **Open-ended slices**: `jets[:2]`, `jets[3:]` (ex08, CMS-SUS-21-009).
   Spec had `"[" index [":" index] "]"`; both endpoints are now optional.
7. **Trailing underscore (`JET_`)**: legacy per-element loop notation,
   live in ATLAS files (`{ JET_ }Pt`, `dR(muons_, cleanjets_)`). Parsed as
   an `UnderscoreAll` postfix node (implicit whole-collection reference).
8. **Particle-list as define body**: `define Zreco : leptons[-1] leptons[-2]`
   (cl_examples/CMS-SUS-16-041) — same-line juxtaposition of object refs is
   accepted as the body of a define (divergence-7 node reused).
9. **`type search|control` region metadata** (cl_examples/CMS-SUS-21-002):
   new `TypeTag` region statement, recognized only as `type <ident>` on one
   line (`type` is otherwise a plain identifier).
10. **Path-token rule made concrete**: in argument position, a contiguous
    `[A-Za-z0-9_.\-/]` run starting at an identifier is merged into a path
    token iff it extends beyond the identifier AND contains `.` AND
    (`-` or `/`) — so `MET.phi` stays dotted access and `x-1` stays
    subtraction. Deprecation warning suggests quoting (spec §2 note).

### Phase-0 resolutions honored

- Negative `[-n]` indices parse with a *warning* (OPEN-3: reserved,
  `Unsupported` tagging is adl-sema's job in Phase 2); required by ex07 /
  CMS-SUS-21-009 / CMS-SUS-16-041, which the corpus gate covers.
- `~=` parses as a distinct `ApproxEq` comparison with the once-per-file
  OPEN-4 warning ("treated as `!=` downstream"); mapping to `!=` happens
  in sema so the surface form is preserved for diagnostics.
- Keywords and resolution case-insensitive at the lexer level; identifier
  case preserved in AST and diagnostics.

### Notes / known gaps

- `defines` appearing inside object blocks (CMS-SUS-21-006 lines 100–103)
  are parsed as top-level sections: indentation is insignificant and
  `define` ends any block, which matches their event-scope semantics; the
  corpus has no object statements *after* such defines, so nothing is
  misattributed.
- Top-level `trigger <name>` object blocks (spec `object-block` keyword
  alternative) parse at file level; inside a region `trigger` is always
  the region statement, matching corpus usage (no top-level trigger blocks
  exist in the corpus).
- `multiplicative` keeps the spec's flat `* / ^` precedence (left-assoc):
  `ptErr / pt^2` parses as `(ptErr/pt)^2`. Flagged here because it is
  surprising but it IS what SPEC_LANGUAGE §3 specifies; revisit at spec
  freeze if `^` should bind tighter.
- cargo-fuzz target (SPEC_ARCHITECTURE §3 "in-tree") not added in this
  pass — not in the Phase-1 task brief's deliverable list; the corpus gate,
  goldens and the error-recovery battery cover panic-freedom on the
  realistic input space. Add `fuzz/` before the Phase-1 exit review.
- `smash2 check` remains the Phase-6 stub; the corpus gate intentionally
  drives the `parse_adl` example instead (adl-cli is outside this task's
  crate boundary).

## 2026-06-11 — Phase 2: adl-sema (Quantity model, resolution, HIR)

Implemented per SPEC_ARCHITECTURE §4 and PHASE0_RESOLUTIONS: case-insensitive
symbol resolution (case preserved for display), structural interning of
`Collection`/`Quantity`/`PropId`/`ElemPredId`, pure-alias unification as a
resolution fact (transitive; `object X take Y` with no cuts binds Y's
`CollectionId`), define resolution with cycle errors (numeric → body HIR,
boolean → predicate HIR, inlined at every reference), fragment tagging
(`InFragment`/`Unsupported(reason)` on every `HNode`), HIR for
objects/regions/defines/bins/triggers, and deterministic
`quantity_table_dump`/`hir_dump`.

Deliverables:
- `crates/adl-sema/src/{intern,ext,quantity,hir,resolve,dump}.rs`, plus
  `examples/diag_probe.rs` (dev tool: print sema diags for one file).
- `ExtDecls` ingests `ext_objs.txt`/`ext_lib.txt`/`property_vars.txt` (and
  the `object_aliases.txt` spelling map) — embedded via `include_str!` from
  `legacy_parser/adl/` with a `load_dir` runtime alternative.
- Tests: identity battery (15 tests: pure rename ≡ source incl. MET family
  and transitivity; filtered ≢ parent; `jets[0].x` ≢ `jets[1].x`;
  pT/pt/Pt one quantity; dPhi order-sensitive, dR order-insensitive;
  define resolves to body; define and object cycles → error; bare MET ≡
  MET.pt; size/Size/count/.size one quantity; union order matters and
  multi-take ≡ union; `[-n]` Unsupported; unknown fn interned-but-
  unsupported; region-as-predicate + inheritance), HIR snapshots for all
  25 legacy goldens, quantity-table snapshots for CMS-SUS-16-032 and
  CMS-SUS-16-033_Delphes (hand-reviewed), corpus smoke over all 68 files
  (zero error-severity diagnostics + byte-identical double-run dumps).
  Workspace: 146 tests green; clippy `-D warnings` clean; rustfmt applied.

### Decisions / deviations (dated 2026-06-11)

1. **Tag properties keep exact-name identity.** The legacy
   `property_vars.txt` maps `ctag -> isBTag`, which would merge `ctag`
   into `btag` — an unsound over-merge (false disjointness risk, audit
   Bug 6 family). `btag`/`ctag`/`tautag` canonicalize to their own exact
   lowercase names, matching the SPEC_ANALYSIS TAG axiom's exact-name
   rule. All other property synonyms (m/mass→Mof, q/charge→Qof,
   pt/pT/Pt→Ptof, ...) merge per the file.
2. **Bare event-scalar names.** `ht/st/fht/scalarht/delphes_scalarht`
   used as bare values resolve to `EventScalar(EventVar(...))`; the MET
   family resolves to `EventScalar(MetProp(pt))` per the spelling-map
   semantics. Other base collections used as bare values are
   `Unsupported` (honest refusal).
3. **Self-referencing object blocks** (corpus-required): a take source
   spelled like the block's own name (`object met` / `take MET`,
   CMS-SUS-16-017) resolves to the external base, not a cycle; inside a
   block's cuts the block's own name means the implicit element
   (`select pdgID(OSdileptons) == 0`, cl_examples/CMS-SUS-16-041).
   Genuine cycles (a takes b takes a) remain errors.
4. **Composite/COMB blocks** get real identities
   (`Collection::Combination { parts }`, binder slots as
   `ParticleRef::Binder`) but are tagged `Unsupported` — combinatorial
   semantics are outside the Phase-2 checked fragment. Single-binder
   takes (`take jets j`) alias the implicit element.
5. **Unknown functions/properties** (e.g. `D0`, `aplanarity`) are
   interned as `ExternalFn` (identity preserved, no over-merge) with the
   node tagged `Unsupported` and a once-per-name warning — SPEC §5
   "declared external functions" are in-fragment; undeclared ones are not.
6. **`ElemIndex::FromBack` is constructed** for `[-n]` so identity stays
   exact, but every node involving it is tagged
   `Unsupported("negative index `[-n]` is reserved (OPEN-3)")` per
   PHASE0 (no FromBack semantics anywhere downstream yet).
7. **Region references**: a bare statement name resolves to a prior
   region (inheritance) or a boolean define, else error (spec §3); an
   identifier *inside* a select (`select presel`, live in
   CMS-SUS-16-033_Delphes) resolves to a prior region as a
   `RegionPred` node. Prior-only references make region cycles
   unrepresentable.
8. **`~=` → `!=`** happens here (OPEN-4), preserving the surface form in
   the AST; `ALL`→true, `NONE`→false at resolution.

## 2026-06-11 — Phase 4 (part): adl-formula (formula IR, polarity, encoder)

Implemented per SPEC_ARCHITECTURE §5 and SPEC_ANALYSIS §1: `Formula`
(True/False/Atom(LinAtom)/And/Or/Unknown(DiagId)/Dual), NNF `not()` with
Dual branch swap (¬plus/¬minus swapped, involutive), `Over`/`Under`
projection types wrapping a `QFormula` that is Unknown/Dual-free by type
(private fields; only constructors are `Formula::over`/`under` —
compile_fail doctests demonstrate both misuse directions and the forged
constructor), `LinAtom::new` returning `Result` and rejecting non-finite
coefficients/constants including merge overflow, and the HIR→Formula
encoder (`encode_region`/`encode_regions`) covering every §1 table row:
reject = exact negation, inheritance inlining with a cycle→Unknown guard,
trigger atoms `trig(t)=1`, linear arithmetic with sema-inlined defines and
Int-size coercion, exact two-branch ratio encoding with D=0-fails,
ternary expansion (missing else ⇒ true), `[]`/`][` bands, the OPEN-1
Dual bounded expansion k=3 with the empty-collection case in the plus
branch (PHASE0; legacy audit Bug 1), Unsupported→Unknown(diag).

Deliverables: `crates/adl-formula/src/{lin,formula,encode}.rs`; tests:
proptest law battery (`tests/laws.rs`: not∘not=id,
over(¬f)=¬under(f) and its mirror, exact-formula projection agreement),
encoder per-row battery (`tests/encoder.rs`, 30 tests incl. Dual branch
swap through reject and a hand-built cyclic HIR), LinAtom unit tests,
3 compile_fail doctests + 1 positive doctest. 50 tests in-crate;
workspace green; clippy `-D warnings` clean (adl-formula); rustfmt applied
(adl-formula only — adl-interp is another task's in-flight crate).

### Decisions / deviations (dated 2026-06-11)

1. **`encode_region` takes `&mut Hir`** — the OPEN-1 expansion interns
   `ElemProp`/`Size` quantities, so the encoder mutates the quantity
   table (adl-sema code untouched; only its table instance grows).
   DiagTables are region-local; `DiagId`s index into the returned
   `EncodedRegion.diags`.
2. **§4.4 non-finite split.** *Computed* constant arithmetic that goes
   non-finite (constant division by zero, constant overflow) encodes as
   `False` — exact per SPEC_LANGUAGE §4.4 ("enclosing comparison is
   false") because it is event-independent. A numeric *literal* that
   parses non-finite cannot construct an atom (audit Bug 5) and encodes
   as `Unknown` instead (honest refusal, not a semantic claim).
3. **Exact `|E| ⋈ const` expansion** (e.g. `|Δφ−x| < 0.5` shapes):
   top-level absolute value against a constant expands exactly
   (`<`→∧, `>`→∨, `=`→∨, `≠`→∧ of the two signed bounds). Extension
   beyond the §1 table; exact in both directions, tested.
4. **Encoder And/Or constant folding** (drop True in ∧, collapse on
   False, flatten same-connective nesting) — exact simplifications only;
   `Formula::not` itself never folds, preserving the structural NNF laws.
5. **Bare quantities in boolean position**: only trigger flags become
   atoms (`=1`, the §1 trigger row). Any other bare numeric quantity used
   as a boolean is `Unknown` — CutLang truthiness is unprobed and the TAG
   axiom's exact-name rule lives in adl-axioms, not here.
6. **A comparison referencing two distinct unindexed collections**
   (`Jet.pt > Muon.pt`) is `Unknown` — the OPEN-1 expansion is defined
   for one implicit quantifier; guessing a joint reading would be a
   silent strengthening of exactly the legacy kind.
7. **`Formula::not`/`QFormula::not` keep the spec's method name** with
   `#[allow(clippy::should_implement_trait)]`; `std::ops::Not` is also
   implemented and delegates, so both spellings work.

## 2026-06-11 — Phase 3: adl-interp (reference interpreter) + difftest generator

Implemented per SPEC_ARCHITECTURE §8 and SPEC_LANGUAGE §4 (this crate is
the executable spec): JSONL event model, evaluator over adl-sema HIR
(object filtering order-preserving, union concat, region conjunction
incl. inheritance/reject/trigger, ternary ≡ `(g∧a)∨(¬g∧b)` with
missing/`ALL` branch true, `[]`/`][` bands, defines inlined by sema,
div-by-zero/non-finite ⇒ enclosing comparison false, boundary bins
`[b_i,b_{i+1})` with open last bin), pT-descending re-sort OFF (the
loader validates ordering and refuses unordered input). adl-difftest
gained the deterministic seeded toy-event generator (SplitMix64, no new
deps): per-collection descending pt ≥ 0, |eta| bounded per collection,
phi ∈ [−π,π), m ≥ 0, tags/triggers ∈ {0,1}, HT = Σ jet pT, MET ≥ 0.

Deliverables:
- `crates/adl-interp/src/{event,eval}.rs`, re-exports in `lib.rs`;
  `examples/run_events.rs` (Phase-3 preview of `smash2 run`; the real
  subcommand is Phase 6 — adl-cli is outside this task's crate boundary).
- `crates/adl-interp/tests/spec4_semantics.rs`: one unit test per SPEC §4
  clause (49 tests: event model incl. ordering validation and 0-based
  indices; objects incl. non-contiguous filter order, reject-in-object,
  filtered-of-filtered, union order, pure-rename identity; regions incl.
  inherit-vs-paste equivalence, non-membership statements, bin edge
  battery; expressions incl. ternary truth table vs its expansion, band
  edges, div-by-zero in all comparison directions, overflow via `^`,
  angular orientation/wrapping, guarded references; fragment honesty).
- `crates/adl-difftest/src/lib.rs` generator + `tests/generator.rs`
  (byte-determinism, loader round-trip, physical ranges, HT consistency,
  deterministic interpreter smoke run over 250 toy events).
- Workspace at this commit: cargo build/test green; `cargo clippy
  --workspace --all-targets -- -D warnings` clean; rustfmt applied.

### Decisions / deviations (dated 2026-06-11)

1. **JSONL schema** (spec names JSONL but no schema): one JSON object per
   line; array values = collections (keys canonicalized via the base
   spelling map, case-insensitive), MET-family key = the MET vector
   (object `{pt, phi}` or bare number = pt), other numbers = event
   scalars, `triggers` object = flags (validated ∈ {0,1}). Property keys
   canonicalize through the same `property_vars` map the resolver uses,
   so `pt`/`pT`/`Pt` in data and code always meet. Collisions after
   canonicalization are load errors, not silent overwrites.
2. **Soft vs hard missing data.** Out-of-range element references
   (`jets[9].pt` with 3 jets) and missing object properties are *soft*
   non-values: the enclosing comparison is false, same rule as
   div-by-zero (guarded references do not imply existence — the audit's
   prohibited-axiom family). Missing *event-level* data (MET, a
   referenced scalar, a referenced trigger flag) is a hard diagnosed
   `EvalError`: structural absence is a data mismatch, not physics. An
   absent collection key is an empty collection (events legitimately
   have zero objects of a kind).
3. **OPEN-1 at the interpreter**: an unindexed collection cut at region
   level (`select pt(jets) > 30`, HIR `CollProp`) is a diagnosed
   evaluation error naming OPEN-1. PHASE0's Dual bounded expansion is a
   *verifier* strategy; the interpreter must not invent a quantifier
   reading the spec leaves open.
4. **External functions**: only `sqrt` has a reference interpretation
   (unambiguous; corpus-used; negative argument → NaN → comparison-false
   rule). All other `ExternalFn` quantities — declared or not — raise a
   diagnosed error (`no reference interpretation`), matching the
   verifier's opaque/Unknown treatment (SPEC §5: one diagnosis, two
   consumers). `sort` in a region likewise errors per SPEC §5.
5. **Region conjunction short-circuits in statement order** (cut-flow
   semantics): an event failing an early cut returns false even if a
   later statement would raise an evaluation error. Identical membership
   on error-free regions; documents the error-visibility choice.
6. **Union = pure concatenation** (no dedup), matching SPEC_ARCHITECTURE
   `Collection::Union` ("order is part of the identity"); the UNI
   axiom's `≤ Σ parts` stays sound. Derived collections are NOT
   re-validated for pt order (a union can interleave); only *input*
   collections are validated, which is exactly the PHASE0 ordering
   assumption (ORD axiom applies to base/filtered collections).
7. **dPhi range [−π, π)** (half-open wrap via `rem_euclid`), oriented and
   signed; dEta oriented signed; dR = hypot(dEta, wrapped dPhi),
   unoriented by interning. MET participates in dPhi via its φ; dEta/dR
   against MET are soft non-values (MET has no pseudorapidity).
8. **Boundary-bin underflow**: `v < b0` (or a soft non-value) assigns no
   bin (`None`) — boundary bins do not cover `(−∞, b0)`; the verifier's
   bin-coverage check (SPEC_ANALYSIS §5) is the tool that reports such
   gaps. Non-finite bin values are never binned.

No tests weakened or skipped; no crates outside adl-interp/adl-difftest
touched (adl-difftest added `adl-syntax` as a dev-dep for diagnostics in
its own tests).

## 2026-06-11 — Integration: adl-interp × adl-formula smoke test

No cross-crate compile breaks existed after the parallel Phase-3/Phase-4
builds (workspace built and tested green at 252 before this step). Added
the one cross-crate integration smoke test required by the integration
brief: `crates/adl-difftest/tests/formula_interp_smoke.rs`.

What it locks: the 3-region golden `collection_quant.adl`
(SR_allhard / SR_unbounded / SR_softlead — the only 3-region golden) is
encoded via `encode_regions`, projected both ways (`Over`/`Under`), and
checked against the interpreter on 5 hand-written events (empty
collection, single passing jet, mixed pair, single failing jet, size
beyond the OPEN-1 bound k=3). Per region × event, with hand-computed
expectations asserted exactly:

- sandwich `under ⇒ over` always;
- interpreter `Ok(v)` ⇒ `under ⇒ v ⇒ over`; exact region (SR_softlead)
  ⇒ `over == under == v`;
- interpreter OPEN-1 refusal (unindexed `pT(jets) > 100`) ⇒ error names
  OPEN-1, encoding is non-exact, and BOTH candidate readings (∀ with
  vacuous truth, ∃) sit inside `[under, over]` — the Dual contract,
  including the audit-Bug-1 empty-collection case in plus.

QFormula evaluation in the test pulls quantity values from
`Interp::eval_num` on `HKind::Quantity` nodes (no private semantics);
soft non-values make the enclosing atom false (§4.4 rule).

Notes:
1. Only crate touched: adl-difftest (test + `adl-formula` dev-dep).
2. Recorded subtlety for Phase 5: evaluating an NNF-*negated* atom on a
   soft non-value (atom ⇒ false) is NOT the interpreter's reading of the
   un-negated source comparison under `reject` (¬false = true). Harmless
   for solver use (solver variables are total) and unexercised here
   (`collection_quant.adl` has no reject over missing data), but any
   future concrete-valuation use of QFormula must keep polarity in mind.

Verification: `cargo build --workspace` green; `cargo test --workspace`
253 passed / 0 failed; `cargo clippy --workspace --all-targets -- -D
warnings` clean; rustfmt applied. No tests weakened or skipped.

## 2026-06-11 — Phase 5: adl-axioms, adl-solver, adl-analysis (+ ported golden battery)

Implemented per SPEC_ARCHITECTURE §6–7 and SPEC_ANALYSIS §2–6.

### adl-axioms

One audited catalog (`catalog()`, ten rows: ORD, SZ0, SUB single-source-
only, UNI, NNEG, DPHI, TAG exact-name, TWIN, EPRED, IDOM), each row =
statement + justification ("true of every physical event because …") +
assumption tag. `emit_axioms(hir, ext, quantities)` instantiates ground
`QFormula` facts over a quantity set to a fixpoint (helper quantities —
guard sizes, parent pt's — get their own SZ0/ORD/… facts in the next
round). `twin_pairs` feeds the OPEN-2 SAT-direction cap. EPRED encodes
element predicates exactly-or-not-at-all (top-level conjunction may keep
only the encodable conjuncts — a sound weakening), via a small private
exact encoder (adl-formula's encoder is region-shaped and not reusable
here without modifying that crate, which this task may not touch).

Tests: every axiom instance holds on 300 adl-difftest toy events under
the canonical pad-with-0 extension for out-of-range elements (the same
extension that justifies asserting axioms in UNSAT proofs); prohibited-
axiom regressions (all instances hold on the all-empty event — no
"C[i] ⇒ size>i" can be emitted; btagDeepB gets NO {0,1} TAG instance and
a 0.5 discriminant violates nothing); SUB-not-on-unions; twin detection.

Decisions:
1. **DPHI bound = PI_UPPER (π + 1 ulp)**: an axiom bound must be ≥ true
   π; the f64 π is below it. Wider is sound; vacuous_dphi (3.5) unaffected.
2. **Axiom-test equality tolerance**: TWIN is exact over the reals, but
   the interpreter's f64 `wrap(-d)` is not bit-exactly `-wrap(d)`; the
   test evaluator gives Eq atoms (only) a 1e-9 relative epsilon and keeps
   inequalities exact. Documented in the test.
3. **IDOM emitted unguarded** (spec's form `pt(F[i]) <= pt(P[i])`),
   justified by the pad-with-0 canonical extension; EPRED carries the
   size guard exactly because element facts must stay vacuous for absent
   elements (the prohibited-axiom lesson).

### adl-solver

`Solver` trait per SPEC_ARCHITECTURE §7 (push/pop, assert-with-
AssertName, check-with-timeout, model, unsat_core) plus one extension:
`declare(q, QSort)` so collection sizes are Int-sorted (QF_LIRA);
undeclared quantities default to Real. Constants flow through ONE
conversion for both backends: shortest round-trip decimal → exact
rational (recovers source literals like `0.3` exactly; conformance locks
`0.1·x ≥ 1 ∧ x < 10` UNSAT).

- PRIMARY `NativeSolver` (z3 crate 0.20, thread-local context): typed
  terms, `assert_and_track` trackers for cores, model completion,
  per-check `timeout` param.
- SECONDARY `SubprocessSolver` (z3 binary, `-in -t:ms -T:s`): stateless
  re-run script per check; model/core retrieval re-runs with the getter
  appended; tiny s-expr parser for `(get-value …)`. **Any `(error …)`
  output ⇒ `Unknown` for that check** (audit Bug 5), locked by an
  error-injection test through a `inject_raw` test hook, plus a
  missing-binary-degrades-to-Unknown test.
- Conformance battery runs identically over both backends (sat/unsat,
  model values, core membership AND core-only re-UNSAT, push/pop,
  Int sorts, exact rationals, tiny-timeout no-hang, completion) plus a
  fixed-query agreement test. Feature `native` (default) gates the z3
  dep; `--no-default-features` builds/tests subprocess-only.

### adl-analysis

Pipeline per SPEC_ANALYSIS §2: statement-granularity encoding (each
membership statement encoded through a synthetic single-statement region
so unsat cores map to individual cuts/source lines; region formula =
conjunction by construction) → interval fast path on the unconditional
And-spine of the Over projections (sound; the no-solver fallback) → one
incremental solver session (base frame: sorts + named axiom instances;
push/pop per check): region-empty (UNSAT Ax ∧ R⁺), pairwise disjoint
(UNSAT Ax ∧ A⁺ ∧ B⁺, core → spans), subset both ways (UNSAT Ax ∧ A⁺ ∧
¬B⁻), overlap (SAT Ax ∧ A⁻ ∧ B⁻) with the OPEN-2 twin-pair cap and the
shared-dimension requirement, then **witness re-validation through the
interpreter** (TESTING §3, production behavior): model → all-pass
synthetic event (sizes/element props pinned from the model, filter-chain
repair for free props, pT-descending fill) → both regions must accept,
else downgrade to POSSIBLY + internal diagnostic; an opaque external
function keeps the witness a candidate with the §2 caveat printed.
Witness search first re-checks with sound "mentioned elements exist"
size hints so models prefer realizable events. Bin partition checks per
§5 (pairwise UNSAT within R⁺; coverage UNSAT of R⁺ ∧ ⋀¬Bᵢ⁻ with gap
witness; boundary bins real-valued, open last bin). Outputs per §6:
deterministic human report + versioned JSON (`schema_version: 1`, stable
ordering), `FailOn` plumbing (`findings`/`exit_code`,
`--fail-on=overlap|gap|empty|non-exact`) in the lib API for Phase-6 CLI.

Decisions / deviations:
1. **Opaque-external re-tag pass** (`encode::retag_opaque_externals`,
   verifier-side only): a `Quantity::ExternalFn` node whose ONLY problem
   is "function … is not declared in the external library" is treated as
   an exact atom over an opaque interned free quantity, per the
   SPEC_ANALYSIS §2 model caveat ("opaque external-function values …
   are free") — sound in both directions in the per-event scalar model,
   and required by the legacy golden `or_unencodable_branch` (PROVEN
   OVERLAPPING through the encodable OR branch). The interpreter still
   refuses these quantities, so such witnesses stay candidates. The
   Phase-2 sema tagging is untouched (one diagnosis, two consumers).
2. **adl-solver is a direct path dep** in adl-analysis's Cargo.toml
   (not workspace-inherited): workspace dep inheritance cannot override
   `default-features`, and the crate forwards the `native` feature.
3. Boolean-bin statements of one region form one bin set; each
   boundary-list `bin` statement is its own set (§5 says "per bin set"
   without fixing the grouping).
4. Verdict exit code for fired `--fail-on` findings is 4 (1/2 stay
   reserved for parse/sema errors per §6).

### Ported legacy golden battery (tests/golden_battery.rs)

All 30 checks of `legacy_parser/scripts/run_golden_tests.sh` are encoded
as 37 Rust assertions over the API/report (same regions, same expected
verdicts, including the human-report phrases the script grepped):
encoding structure (ITE exact guarded-OR, OR clause), interval + SMT
disjointness (disjoint_pt with and without solver, jet index), overlap
proofs (overlap_met, size_bjets validated witnesses), the dual-encoding
regression suite (or_unencodable both directions, reject_and_band,
not_tag, define_under_or, tag_index), the z3 suite (btag_threshold with
TAG cited in the core, ratio_met exact, collection_quant both pairs,
bins_partition both bin sets), the June audit suite (quant_empty_forall,
define_arith, angular_order, union_size, inf_constant,
btag_discriminant, vacuous_dphi ×2, reject_or_band overlap + subset,
independent_jet_index), error reporting (bad_syntax line 5 + failure),
JSON export ("proven_disjoint", schema_version), determinism
(byte-identical reruns), no-solver POSSIBLY cap, fail-on gating, and a
subprocess-backend spot check. `analysis_behaviors.rs` adds the
TESTING-§3 downgrade path (unrealizable witness ⇒ POSSIBLY + internal
diagnostic, no witness displayed) and a 68-file corpus no-solver
determinism smoke. The full golden battery also passes with
`--no-default-features` (subprocess backend end to end).

### Verification (run before returning)

- `cargo build --workspace` — green.
- `cargo test --workspace` — green: 315 passed / 0 failed (36 suites).
- `cargo test -p adl-solver --no-default-features` and
  `cargo test -p adl-analysis --no-default-features --test golden_battery`
  — green (subprocess-only configuration).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.

No tests weakened or skipped; crates touched: adl-axioms, adl-solver,
adl-analysis only (plus their own Cargo.toml dependencies).

### Known gaps (honest list for Phase 6+)

- Witness realizer is all-pass per base collection: models needing a
  partially-failing parent collection downgrade to POSSIBLY (sound,
  tested) rather than synthesizing mixed events.
- Bin sets currently report per boundary-list statement; cross-set
  interactions (multiple `bin` statements over different variables in
  one region) are independent checks.
- Subset explanations carry no unsat core (flag + human line only).
- The interval fast path tracks single-quantity atoms only (no 2-term
  difference propagation); everything it misses falls through to z3.

## 2026-06-11 — Phase 6: adl-viz, adl-cli (smash2 subcommands), corpus gate

Implemented per SPEC_ARCHITECTURE §1/§9 and PLAN Phase 6.

### adl-viz

DOT emitters built from the resolved HIR (never the raw AST), so the
flowchart/AST graphs cannot disagree with what the verifier consumes.
Output is deterministic — byte-identical across reruns: all iteration is in
declaration order, node ids are stable HIR indices, no hashing/pointer
order.

- `flowchart_dot(&Hir)` — collections with `take`/union/comb lineage (a
  base `Jet` node feeds its filtered child so `take` edges stay visible
  even when the base is never re-bound), regions with their ordered
  membership statements as the node body, inheritance as dashed
  region→region edges, and dotted object→region usage edges (which named
  collection feeds which selection). Unsupported objects are tinted.
- `ast_dot(&Hir)` — resolved expression trees: every define body, object
  element predicate and region cut becomes a node-per-subexpression
  subtree; leaves render full quantity/literal text, operators render the
  symbol. Unsupported nodes tinted.
- `label.rs` — a `Labeler` over adl-sema's PUBLIC API (the dump module's
  `RenderCtx` is crate-private), sharing the typed quantity model as the
  single source of truth. `strip_coll_ids` removes the `C<n>#name`
  collection-id prefixes that sema bakes into `QuantityArg::Opaque`
  interning keys — display-only, identity untouched.

Tests: 7 lib unit tests (valid-digraph shape, determinism, take/inherit
edge presence, label escaping, `strip_coll_ids`) + DOT snapshots for FIVE
corpus files × {flowchart, AST} = 10 insta snapshots
(`disjoint_jet_index`, `collection_quant`, `bins_partition`,
`ex01_selection`, `ex06_bins` — chosen to cover base→filtered→filtered
lineage, unions, pure renames, indexed/unindexed cuts, defines with opaque
external fns, boundary + boolean bins, region inheritance). Each snapshot
test re-renders and asserts byte-equality (an embedded determinism check).
DOT verified to render through system graphviz (`dot -Tsvg`).

### adl-cli (binary `smash2`)

clap-derive CLI with four subcommands; machine-clean stdout by default
(report/DOT/results only), diagnostics + progress to stderr, `--verbose`
for detail. `cmd/` has one module per subcommand plus a shared `CliError`
(IO/usage → exit 2) and file helpers.

- `check FILES…` — parse+resolve (adl_sema::analyze_str merges parse+sema
  diagnostics); stderr diagnostics, EMPTY stdout on success; exit 1 if any
  file has error-severity diagnostics.
- `verify FILE [--json] [--no-solver] [--fail-on=…]` — the `smash -r`
  equivalent via `adl_analysis::analyze_source`. Human report (default) or
  versioned JSON to stdout; `--no-solver` → SolverChoice::NoSolver
  (interval fast path, verdicts capped at POSSIBLY); `--fail-on`
  comma-list (overlap|gap|empty|non-exact) → exit 4 when a finding fires
  (findings echoed to stderr). Parse/sema errors → exit 1, empty stdout.
- `run FILE EVENTS.jsonl [--json]` — `adl_interp` over JSONL: per-event
  per-region PASS/fail/ERROR + bin assignment. Text table (default) or
  one JSON object per event (JSONL out) to stdout.
- `dot FILE [--ast]` — flowchart (default) or AST DOT to stdout; resolve
  errors → exit 1.

Feature forwarding: `default = ["native"]`, `native =
["adl-analysis/native"]`, with `adl-analysis` a direct path dep
(`default-features = false`) so `cargo build -p adl-cli
--no-default-features` builds the subprocess-only configuration (verified).

Tests (`tests/cli.rs`, 17): per-subcommand exit codes and the
stdout/stderr split (check silent-on-success, diagnostics never on
stdout); `verify` human + JSON snapshots run with `--no-solver` so the
report body (and `solver: none` line) is independent of the installed
backend; `run` text + JSON snapshots; `--fail-on` gating (fires exit 4 on
overlap, never on disjoint, bogus value → usage exit 2); a corpus-file dot
render; and explicit byte-identical-rerun determinism checks for both
`verify` (human + JSON) and `dot`. Plus the clap `debug_assert` validity
test.

### corpus gate

`scripts/corpus_gate.sh` now drives `smash2 check` over all 68
`examples/**/*.adl` in one invocation (parse + resolve, a STRONGER gate
than the old parse-only `parse_adl` path), exits 1 if any file has
error-severity diagnostics. Verified: all 68 pass (only notes/warnings on
stderr, which are non-error severity).

### Verification (run before returning)

- `cargo build --workspace` — green.
- `cargo test --workspace` — green, 343 passed / 0 failed.
- `cargo build -p adl-cli --no-default-features` — green (subprocess-only).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- `scripts/corpus_gate.sh` — exit 0, all 68 files parse+resolve clean.

### Deviations from spec

None semantic. Notes:
1. Flowchart base-collection nodes: SPEC §9 says "regions, objects,
   take/inheritance edges" without fixing whether an unnamed base parent
   gets a node. ADL2 draws base collections as their own node so `take Jet`
   lineage is visible; filtered/union/comb fan-in from every part.
2. `dot` adds dotted object→region usage edges beyond the literal
   take/inheritance set — a readability superset, derived from the same
   HIR quantities, never contradicting the analyzed structure.

No tests weakened or skipped; crates touched: adl-viz, adl-cli, scripts
only (plus their own Cargo.toml dependencies and feature forwarding).

## 2026-06-11 — TESTING §2 heavyweight layers: encoder-vs-interpreter property battery + metamorphic battery (adl-difftest), with engine fixes

Implemented the two heavyweight layers of TESTING.md §2 in adl-difftest,
ran them, and fixed every real engine bug they exposed (the task brief's
"coordinate-free" mandate). Counterexamples are minimized and documented
in `COUNTEREXAMPLES.md` (CE-1…CE-6) and locked in
`crates/adl-difftest/tests/regressions.rs`.

### adl-difftest deliverables

- `src/casegen.rs` — random small-region generator over the fixed
  vocabulary (event scalars `MET`/`HT`; collections `jets`/`eles` with
  `pT`/`Eta`/`BTag` at indices 0/1; sizes; the angular pair
  `dPhi(jets[0], eles[0])`) composing comparisons, `[]`/`][` bands,
  AND/OR/NOT, ternary, `reject`, boolean defines. Constants come from
  small per-domain pools (≤ 1 decimal place) so solver rationals and
  interpreter f64 constants denote identical cut points. proptest
  strategies (`arb_case`, `arb_case_with_define`) give shrinking;
  `render(case, RenderCtx)` derives every metamorphic variant from the
  same case value (swap, flip-polarity, double-neg, inline-defines,
  inherit/paste, pure-rename alias objects).
- `src/oracle.rs` — sampling oracle: shared deterministic event sample
  (72 boundary-grid events; 64 seeded random events clustered around the
  constant pools; 40 toy-generator events with `btag` injected for
  electrons and forced 0-element collection variants — all inside the
  axiom catalog's physical-event class), `run_case` (frontend +
  interpreter passes + `analyze_source` report), `check_sound` (the four
  TESTING §2 assertions), `summary` (order-normalized verdicts for the
  metamorphic battery).
- `tests/prop_encoder_vs_interp.rs` — 2000 cases under plain
  `cargo test`; `--features deep` raises to 100k (PROPTEST_CASES
  overrides both). Asserts: PROVEN DISJOINT ⇒ no sampled event passes
  both; PROVEN OVERLAPPING ⇒ `witness_validated == Some(true)` (the
  engine's interpreter re-validation; the vocabulary has no opaque
  quantities, so candidate-only is also a failure); PROVEN SUBSET ⇒ no
  sampled counterexample; REGION EMPTY ⇒ no sampled member.
- `tests/metamorphic.rs` — six transforms × 250 cases (deep: 10k):
  swap(A,B); reject c ≡ select not c (both polarities); double negation;
  inline-vs-named define; inherit-vs-paste; pure-rename invariance.
  Each requires identical verdict summaries AND identical interpreter
  membership on every sampled event.
- `tests/regressions.rs` — CE-1…CE-6 locked.
- Cargo.toml: `deep` feature; adl-analysis + proptest as regular deps
  (difftest IS the harness crate); adl-syntax moved dev→regular (diag
  rendering in the oracle). `src/gen.rs` was renamed `casegen.rs`
  (`gen` is a reserved keyword in edition 2024).

### Engine fixes (each found by the batteries; all sound-direction)

1. **adl-formula `encode.rs` — element-existence guards** (CE-1/2/3,
   the big one): every exact comparison leaf now carries
   `size(C) > i` guards for each element-indexed quantity it references
   (incl. angular anchors; opaque ExternalFn args excluded — their
   missing-element behaviour is unknown and the §2 model caveat already
   declares them free). Without guards, NNF negation (`reject`, `not`,
   subset/empty UNSAT checks) silently claimed the complement of a
   comparison on events where the element does not exist — false PROVEN
   DISJOINT / REGION EMPTY / PROVEN SUBSET, confirmed live. The OPEN-1
   Dual expansion instances inherit the guards (still a superset of both
   readings; the audit-Bug-1 `size=0` disjunct unchanged). SPEC_ANALYSIS
   §1's table row "comparison over linear arithmetic → LinAtom" should
   gain this guard conjunction at the next spec revision. One
   adl-formula structural test updated to the guarded expansion
   (more-exact encoding, not a weakening).
2. **adl-interp `Cargo.toml` — serde_json `float_roundtrip`** (CE-4):
   default serde_json float parsing is lossy; event values were
   perturbed by ulps on load.
3. **adl-solver `native.rs` — exact model extraction** (CE-5.5):
   `approx_f64` (truncated decimal) replaced by rational
   numerator/denominator division (exact for dyadics, correctly rounded
   below 2^53; `approx_f64` retained as the big-rational fallback).
4. **adl-analysis `engine.rs` — deterministic, realizable witness
   search** (CE-5): canonical (name-sorted) pairwise query order;
   layered model refinement (wish ladder, each layer dropped on UNSAT:
   dPhi = 0 preference → ε-interior tightening of the under-formulas
   (ε = 2⁻²⁰ dyadic; pure-Int size atoms exempt) + element-existence
   hints + size ≤ realizer-cap + dyadic dPhi bounds ±3.140625 → …
   → raw model); bounded witness retry (≤ 6 models, point-blocking
   clauses) with a dyadic-grid (2⁻²²) snapped second chance per model.
   All refinement constraints are strengthenings of the exact under-
   formulas — pure model SELECTION; SAT/UNSAT verdicts still come from
   the exact assertions, and the TESTING §3 downgrade path is unchanged.
5. **adl-analysis `witness.rs` — realizer completeness** (CE-5/6):
   honors explicit model sizes (incl. size = 0; element mentions only
   bump sizes when the model has no size pin — required once guards made
   sizes authoritative); realizes `dPhi`/`dEta` model values into
   phi/eta with a fix-point correction through the interpreter's own
   `wrap_dphi`; always fills pT (ordering-safe) and defaults the
   standard properties (eta/phi/m + exact-name tags) on synthetic
   objects; patches hard "missing event-level datum" errors during
   validation by defaulting the free scalar/trigger/MET component;
   rejected-witness internal diagnostics now name the failing statements
   and carry the synthetic event JSON (bug-report quality).

### Soak verification

- `cargo test --workspace --exclude adl-cli`: 338 passed / 0 failed
  (adl-cli is another task's in-flight crate; see final report).
- Property battery: 3 × 2000-case runs + one 10k-case run green.
- Metamorphic battery: 15 consecutive 400-case runs (36k comparisons)
  green after the fixes; plus the default 250-case run in CI mode.
- `cargo test -p adl-analysis --no-default-features --test golden_battery`
  and `cargo test -p adl-solver --no-default-features` green
  (subprocess-only config unaffected).
- `cargo clippy --workspace --exclude adl-cli --all-targets -- -D
  warnings` clean; rustfmt applied.
- Full ported golden battery (37 assertions) green throughout — none of
  the engine fixes moved a single legacy-anchored verdict.

### Known residuals (documented, not hidden)

- Witness search is complete for the checked vocabulary but still
  heuristic in general: an overlap realizable only at non-dyadic
  equality vertices that all 6 retry models miss would downgrade to
  POSSIBLY (sound; internal diagnostic filed). The metamorphic battery
  bounds the residual rate empirically at < 1 per ~36k comparisons.
- MissingProperty soft-failures (objects lacking a referenced property
  in *user* data) remain outside the guard scheme — SPEC_LANGUAGE §4.1's
  event model declares objects carry their properties; the samplers
  comply, and the witness realizer now always emits the standard set.

Addendum (same day): the parallel Phase-6 adl-cli task landed while this
work was in flight; two of its insta snapshots
(`cli__verify_{human,json}_disjoint_pt`) were generated against the
pre-guard engine. Reviewed and accepted the regenerated snapshots — the
diff is exactly the engine-fix consequence: leaf counts include the new
element-existence guard atoms (SR_low 2→4, SR_high 1→2) and
`size(jets)` now appears in shared_dimensions (the guards genuinely
introduce that shared quantity). Worth revisiting at the Phase-6/7
report review whether the coverage counter should count source-level
comparisons instead of formula leaves.

## 2026-06-12 — Corpus-sweep gap fixes 1–4 (NNEG opaques, inherit edges, AST layout, underscore-note collapse)

### Fix 1 — NNEG extended to pt/m/mass/e/energy/dr-named opaque externals (adl-axioms)

- `nneg` emitter now covers `Quantity::ExternalFn` whose symbol key
  (lowercase) is exactly one of `pt|m|mass|e|energy|dr`: pT, mass and
  energy of ANY particle combination are magnitudes (>= 0; m/E of a
  summed four-vector by the timelike/lightlike physical-state
  condition), and dR is a metric distance. Exact-name rule, same
  discipline as TAG — `bdt`, `aplanarity`, `sum`, ... stay free opaques.
  Catalog row updated (statement + justification + assumption "none").
- Finding power restored: `verify` on CMS-SUS-16-032 now proves the
  whole `compressed` family EMPTY (the known transcription bug legacy
  caught): `(pT(jets[0] jets[1]) + MET)/MET < 0.5` with `MET > 250` is
  UNSAT once the opaque pT is >= 0. Summary moved from
  30 disjoint / 11 overlapping / 4 possibly to 41 / 0 / 4 — the 11
  "overlapping" verdicts were candidate-witness artifacts of the solver
  assigning `pT(...) = −126.5`.
- Locked by: axiom unit test (`opaque_pt_named_external_gets_nneg_but_bdt_stays_free`),
  new fixture `crates/adl-analysis/tests/fixtures/opaque_pt_ratio_empty.adl`
  + integration test (`opaque_pt_in_impossible_ratio_proves_region_empty`,
  asserts EMPTY rests on the NNEG pT(...) core item and a control region
  stays live). Golden battery (37) unchanged — `or_unencodable_branch`
  still PROVEN OVERLAPPING with candidate witness (aplanarity free).
  CMS-SUS-16-033_Delphes verify output byte-identical before/after.
  Witness downgrade policy in `adl-analysis/src/witness.rs` untouched
  (ratified). Snapshot churn: `cli__verify_json_disjoint_pt` (catalog
  statement string only).

### Fix 2 — flowchart inherit edges for the `select <region>` form (adl-viz)

- `HKind::RegionPred` references (region-as-predicate, `select presel`)
  now draw the same dashed region->region `inherit` edge as the
  bare-name `HirRegionStmt::Inherit` form; per-region parents are
  deduped so a region referenced via both forms gets one edge.
- CMS-SUS-21-006 flowchart: 0 -> 24 inherit edges. Bare-name dot
  snapshots unchanged; new unit test
  `select_region_form_draws_the_same_inherit_edge`.

### Fix 3 — AST diagrams stack vertically (adl-viz)

- The AST DOT is a forest; dot laid components side by side
  (SUS-21-006: ~110k pt wide). Component roots are now chained with
  invisible edges (`style=invis, weight=100, minlen=<prev tree depth>`)
  so each tree starts below the deepest rank of the previous one.
- Verified through `dot -Tsvg`: SUS-21-006 AST 110k+ pt -> 10190 pt wide
  (height 14828 pt); ex06_bins 6862×373 -> 4552×1423 (readable).
  Snapshot churn: the 5 `*_ast` dot snapshots + `cli__dot_ast_disjoint_pt`
  (added invis chain lines only).

### Fix 4 — underscore-indexing note once per file (adl-syntax)

- The `identifier X ends before `_`` lexer note now fires once per
  file (first occurrence keeps note + help); subsequent splits are
  counted and emitted as a single trailing summary note
  `(N more underscore-index splits in this file)` only when N > 0.
- CMS-SUS-16-017 `check`: 25 notes -> 1 note + 1 summary. Token-level
  split RULE untouched (lexer token tests unchanged). New tests:
  `single_underscore_split_gets_one_note_no_summary`,
  `repeated_underscore_splits_collapse_to_one_note_plus_summary`.
  Snapshot churn: 3 syntax corpus snapshots (atlas_susy_jetmet,
  ex04_syntaxes, ex10_tableweight).

### Also

- PARITY_DRAFT.md: added the ratified (2026-06-12) corpus-level note on
  CMS-SUS-16-042-class opaque-witness overlap verdicts — smash2's
  POSSIBLY is correct; legacy's PROVEN OVERLAPPING rests on free opaque
  assignments (the negative-pT failure mode); not 'legacy-better'.

## 2026-06-12 — `verify` default report redesign: findings-first, verdict matrix, grouped pairwise (`--explain` for detail)

The default human report on a 10-region file was ~90 near-identical
lines (CMS-SUS-16-032: 41 PROVEN DISJOINT lines, 24 of them the same
b-tag interval reason). Redesigned `Report::human_default` (new
`adl-analysis/src/render.rs`); the old full rendering is unchanged as
`Report::human` and now sits behind `smash2 verify --explain`.

- **findings first**: provably-empty regions (one line + 'run --explain
  for the proof chains'), bin sets with unproven coverage/disjointness
  with a derived cause (region's dropped-leaf reason / no solver),
  regions below full encoding grouped by identical (line, reason).
- **regions** as an aligned table: name | leaves | exact | note
  (EMPTY / drops line N / dual-encoded leaves).
- **verdict matrix** for 3..=20 regions: lower triangle, one letter per
  pair (D/O/s/?/U, E when a side is provably empty), declaration order,
  column-index footer; skipped outside that range (summary counts cover
  it).
- **pairwise grouped**: pairs merge on identical (verdict, subset
  pattern, reason signature) where the signature replaces the pair's own
  region names with placeholders (longest-name-first so prefix names
  can't mangle). Trivially-disjoint pairs touching a provably-empty
  region collapse into one bullet. Group membership renders as a clique
  ('all pairs among …'), a cross product ('X{,a,b} vs Y{…}'), or the
  full wrapped pair list — counts partition the pair total exactly
  (debug_assert), nothing is dropped. Singletons print one line with the
  short reason. First-occurrence group order ⇒ deterministic.
- **axioms** one line (`ID×count`) + one deduped 'assuming:' line;
  summary one line.
- **[-0, -0] fix**: `fix_negative_zero` rewrites standalone `-0` tokens
  to `0` in *both* human renderings (token-aware: `-0.5`, `10-0`,
  `1e-05` untouched). JSON is byte-unchanged (verified: `--json` output
  diffed against a HEAD build on 032 + collection_quant — identical),
  so the engine's reason strings keep their `-0` there.
- **color**: ANSI bold heads + colored verdict letters/words only when
  stdout is a tty AND `NO_COLOR` is unset (`IsTerminal`); piped output
  and all tests take the plain path (verified under `script(1)` both
  ways).
- CLI: `verify --explain` (conflicts with `--json`); `--verbose` stays
  timing/backend stderr only.

Result: 032 92 → 62 lines (and the 24-line b-tag wall is now inside the
one empty-region bullet), 033 180 → 65, tiny files ±0. Tests: new
solver-on snapshots `report_rendering__default_cms_sus_16_032/033/
reject_or_band` (solver label normalized to `<backend>`; plain path
asserted ANSI-free + deterministic in-test); corpus determinism test
extended to `human_default`; CLI `verify_human_disjoint_pt` re-recorded
to the new layout and new `verify_explain_disjoint_pt` snapshot is
byte-identical to the previous default snapshot; render unit tests for
`-0` tokenization and name compression. Golden battery untouched and
green (it pins `Report::human`, which did not change shape).

## 2026-06-12 — `smash2 objects`: object-attribute summary from the HIR

The modern successor of the legacy `printObjectAttributes`
(`legacy_parser/adl/semantic_checks.cpp` `collectObjectAttributes`: object
name → attributes referenced in its cuts, walked through textual take
chains). Rebuilt from the resolved HIR's Collection identity model instead
of string-keyed chain walking.

- **New library fn** `adl_sema::object_table(&Hir, color: bool) -> String`
  (`crates/adl-sema/src/objects.rs`). Pure function of the HIR; one aligned
  row per declared collection in `CollectionId` (= declaration) order:
  - **name**: the collection's bound name(s), pure renames collapsed with
    `=` (a no-cut `object X take Y` adds `X` to `Y`'s name list — same
    `CollectionId`, so renames are a single row, not a chain link).
  - **base chain**: `Filtered.parent` walked up to the detector-level
    `Base`, joined with `<-` (`bjets <- jets <- JET`); union/combination
    render as a single node (parts in the derived-facts line).
  - **element cuts**: the `ElemPred` node tree flattened to a flat human
    predicate (`pt > 25, |eta| < 2.4`): `this.` dropped, `abs(x)` → `|x|`,
    one conjunct per comma; capped at 64 chars with `…` (full text in the
    HIR / quantity-table dumps). Out-of-fragment identifiers render `x?`;
    the verbose `<unsupported: …>` opaque-arg text is collapsed to the same
    `x?` form so an in-fragment opaque external (e.g. `dR(…, PFcand?)`)
    does not leak a diagnostic string.
  - **fragment**: `exact` when the cut tree is fully in-fragment and the
    backing object block is in-fragment, else `partial: <reason>`.
  - **derived facts** (identity model, by construction): `size(C) ≤
    size(parent)` for filtered; `size(U) = Σ size(part)` (disjoint ⇒
    exact, else ≤) for union; part list for combination.
- **New subcommand** `smash2 objects <FILE>` (`crates/adl-cli/src/cmd/
  objects.rs`): table to stdout (machine-clean), diagnostics to stderr,
  exit 1 on resolve errors. Color via `IsTerminal && NO_COLOR` unset — same
  rule as `verify`; piped/redirected output is plain.
- **`verify --explain`** re-resolves the HIR (deterministic, cheap) and
  appends the table as an `== objects ==` section. **DEFAULT verify output
  and `--json` are byte-unchanged** (verified: the `verify_human_*` and
  `verify_json_*` CLI snapshots did not move; only `verify_explain_*` gained
  the appended section).
- Tests: `objects__CMS-SUS-16-032` (jets/bjets/cjets filtered chains +
  leptons union) and `objects__CMS-SUS-21-006` (lepton/DT unions, deep
  chains, `=` renames) snapshots in adl-sema; CLI `objects_cms_sus_16_032`
  snapshot + bad-file exit-1 assertion; determinism + ANSI-free assertions;
  the corpus sweep now also pins object-table determinism. `cargo test
  --workspace` 373 passed / 0 failed; `cargo clippy --workspace
  --all-targets -D warnings` clean.

## 2026-06-12 — Decision: CutLang dropped as dependency/authority

Project decision (Daniel + collaborators): CutLang is no longer used by
this project in any form. The ADL2 reference interpreter (adl-interp) is
the authoritative semantics of the ADL fragment we support. Open semantic
questions are settled by project decision and recorded in the spec — there
is no external probing oracle. Documentation updated accordingly: the spec
`[VERIFY]` markers became `[DECIDE]` (project decision; conservative
convention-neutral defaults stand until decided); TESTING.md dropped the
CutLang oracle tier (interpreter is sole reference, legacy `smash` remains
the transitional oracle); DECISIONS ADR-005 mitigation now rests on the
spec + property tests + collaborator review; PLAN Phase 0 / risk rows and
PHASE0_RESOLUTIONS reframed as collaborator-decision / standing-default
items. The conservative Phase-0 defaults are unchanged.

## 2026-06-12 — Phase 9: histogram accumulation (`smash2 run --histos`)

Ratified decisions implemented as specified: (1) weighted moments
fTsumw/fTsumw2/fTsumwx/fTsumwx2 accumulate AT FILL TIME, in-range fills
only (ROOT `GetStats` convention); (3) `entries` is the raw fill count;
naming/file decisions (2)(4)(5)(6) land with the `rootfile` crate —
`histos.json` carries separate `name` + `region` fields so the writer can
emit the flat `REGION_histo` names.

- **adl-sema** (DEVIATION: task named adl-interp/adl-difftest/adl-cli,
  but histo/weight payloads must resolve where the HIR is built; minimal
  additive extension): `HirHisto`/`HistoSpec` (`Uniform1D` |
  `Unsupported(reason)` for 2-D, variable-bin, malformed shapes, bin
  counts outside 1..=1e6), `HirWeight`/`HirWeightValue` (`Num` canonical
  text | `Other(description)`), and `Hir.histos`/`Hir.weights`/
  `Hir.histolist_regions`. `histo`/`weight` region statements keep their
  `NonMembership` markers — every existing consumer (formula, analysis,
  viz, dumps) is untouched. Two output-protecting choices, both verified
  byte-identical against a HEAD build (`verify` default + `--json` on
  ex02_histograms, ex10_tableweight, CMS-SUS-21-006_TreeMaker2result,
  CMS-SUS-16-032): histo fill expressions resolve AFTER all regions, so
  histogram-only quantities intern at the end of the table (a histoList
  before a region had shifted `shared_dimensions` order otherwise); and
  histo-expression resolution is diagnostic-quiet (`resolve_expr_quiet`
  drops new diags, restores the warn-once set — `Unsupported` node tags
  carry the reasons; the run-time accumulator reports them honestly, and
  the corpus no-error gate stays meaningful). adl-formula's hand-built
  `Hir` test literal gained the three fields (test-only touch).
- **adl-interp** `src/histo.rs`: `Hist1D` (per-bin sumw/sumw2,
  under/overflow with own w2, raw entries, fill-time moments; `x < lo`
  underflows, `x >= hi` overflows, x == lo lands in bin 0, fp guard at
  the top edge) + `HistoSet` (instantiation, region-gated fills via
  `Interp::eval_num`, weight products, deterministic diagnostics,
  canonical JSON). **ndhistogram 0.13 NOT used** (decision left to
  implementer): it tracks neither fill-time moments nor raw entry counts
  nor flow-bin sumw2 in accessible form, so we would have wrapped every
  part of it; the hand-rolled accumulator is ~60 lines and is itself the
  spec. `histos.json` field order is fixed (name, title, region, nbins,
  lo, hi, sumw, sumw2, underflow{w,w2}, overflow{w,w2}, entries, tsumw,
  tsumw2, tsumwx, tsumwx2) via a small ordered-field writer (serde_json's
  map reorders keys); floats print as serde_json/ryu shortest-roundtrip.
  Top level is `{"histograms": [...]}` for v2 extensibility.
- **Semantics** (documented choices): fills happen on FULL region
  acceptance; weight = product of the region's own numeric `weight`
  statements (non-numeric arg → diagnostic, 1.0; inherited regions'
  weights do NOT apply); histoList blocks are templates instantiated
  into each referencing selection region — a repeated reference fills
  once with a diagnostic (mid-selection fill points deferred, matches
  the ratified per-statement scope); plain region inheritance does not
  import histograms; out-of-fragment/2-D/variable-bin → one diagnostic,
  histogram absent from output; per-event soft non-values and eval
  errors are counted and summarized (no entry recorded — no value seen).
- **adl-cli**: `run --histos DIR` writes `DIR/histos.json` (pretty form,
  trailing newline); `run --json` appends one compact
  `{"histograms":[...]}` line ONLY when the file declares histograms —
  no-histo files keep byte-exact pre-Phase-9 output (existing
  `run_json_bins_partition` snapshot unchanged). Histogram diagnostics
  go to stderr; stdout stays machine-clean. New `CliError::Write`.
- **Tests** (392 passed / 0 failed; clippy -D warnings clean):
  adl-interp `histo_semantics` — hand-computed fill/flow/moments incl.
  weighted-mean check, weight product/zero-weight/non-numeric-weight,
  rejected events, honesty skips, histoList instantiation + dedup,
  inheritance non-import, canonical JSON exact-string + zero-event edge,
  pretty/compact agreement + determinism; adl-difftest `histo_golden` —
  ex02_histograms over the committed seeded fixture
  (`tests/fixtures/ex02_events.jsonl`, 200 events, regenerate
  byte-identically with `scripts/gen_ex02_events.py`, hand-rolled LCG
  seed 20260612): histos.json + diagnostics snapshots, rerun
  byte-determinism, entries==pass-count and Σw/tsumw consistency
  cross-checks (baseline 32/200, singlelepton 11/200); adl-cli —
  `--histos` file snapshot + cross-run byte-identity, `--json` trailing
  line snapshot, diagnostics-to-stderr split.

## 2026-06-12 — Phase 9: `rootfile` crate (pure-Rust ROOT TH1D writer, v1)

New standalone workspace member `crates/rootfile` (library only, zero
dependencies; smash2 wiring is a later task). Implements SPEC_ROOT_WRITER.md
v1 exactly: small-format TFile header (fVersion 62400, fBEGIN 100,
fUnits 4, fCompress 100 — uproot's `ZLIB(0).code` byte, copied), TKey v4
records with cycle 1, name record + root TDirectory header (class_version
5, 12 B small-format padding), uncompressed records only
(`fObjlen == fNbytes - fKeylen`), TDatime packing (UTC; ROOT uses local
time — cosmetic, injectable via `with_datime`), vendored uproot
TStreamerInfo blob, flat region-prefixed names (decision 2), terminal
free-list segment `[fEND, kStartBigFile)`. Public API per the ratified
sketch: `RootFile::create().add_th1d(name, &H1Spec { title, nbins, lo,
hi, sumw, sumw2, under, over, entries, tsumw, tsumw2, tsumwx, tsumwx2
})?.finish(path)`; plus `to_bytes` (in-memory), `with_datime`/`with_uuids`
(byte-stable output), and a strict verification reader (`rootfile::reader`)
that re-parses our own bytes and checks every framing/key invariant.

- **Root Cargo.toml**: added the member AND the
  `rootfile = { path = ... }` workspace dependency line (the file's own
  comment says path deps are wired at the root so later phases never edit
  it; one line beyond the strict "member list only" instruction).
  `.gitignore` gained `/.venv-uproot`. No other crate touched.
- **uproot venv (decision 4, network probe outcome)**: network was
  available; created `.venv-uproot` (gitignored) at the workspace root
  with pinned **uproot 5.7.4 + hist 2.10.1** on Python 3.12.3. The oracle
  ran for real in this build — not the fallback path.
- **Vendored fixtures** (`crates/rootfile/fixtures/`, see PROVENANCE.md
  there for sha256s + regeneration commands): `streamerinfo_th1d.bin`
  (StreamerInfo record data bytes from an uproot-written reference file,
  `include_bytes!`-ed into the crate — uproot itself hardcodes these
  blobs, no checksum algorithm implemented, per spec §2),
  `reference_th1d_payload.bin` (uproot's TH1D record payload for the
  pinned `h_met` histogram), `reference.root` (whole reference file, for
  `tools/dissect.py` archaeology). Generators checked in under
  `crates/rootfile/tools/` (`make_reference.py`,
  `extract_streamerinfo.py`, `dissect.py`, `check_with_uproot.py`).
- **Byte-diff result**: our TH1D record payload is **byte-identical** to
  uproot's for the identical histogram (offline unit test
  `payload_matches_uproot_reference_bytes` pins serializer == fixture;
  env-gated oracle test regenerates the reference with uproot at test
  time and pins fixture == fresh uproot output — the chain closes).
  Decoded-and-matched quirks: TH1's TNamed TObject fBits `0x03000008`
  (kMustCleanup on direct bases), axes `0x03000000`, fFunctions TList
  fBits `0x03010000` (the `|(1<<16)` quirk) and **no class tag** (plain
  byte-count + version framing, not object-any), 1-byte speed bump after
  fBufferSize, dummy 1-bin y/z axes on [0,1].
- **Intentional whole-file divergences from uproot** (readers use
  pointers, not physical order; all verified readable by uproot):
  (a) no dead 1024 B initial-allocation StreamerInfo record, hence
  **nfree=1** with a single terminal free segment (uproot fresh files
  carry a freed region + nfree=2); (b) keys-list record exactly sized
  (uproot over-allocates 256 B and zero-pads); (c) record order is
  name → TH1Ds → StreamerInfo → keys list → free list (uproot's cascade
  interleaves differently). Plus UUIDs/datime differ by nature
  (injectable for determinism).
- **Tests** (all green; `cargo test --workspace` 0 failed, clippy
  `--workspace --all-targets -D warnings` clean): unit — pascal-string
  short/long/255-boundary, frame byte-count arithmetic, TArrayD layout,
  TDatime against the reference file's `0x7d9902ed` (2026-06-12
  16:11:45) + civil-date epoch/leap vectors, keylen and full TKey bytes
  vs uproot's `h_met` key hex, payload-vs-fixture gold test, spec
  validation rejections, flow-bin placement; integration
  (`tests/structure.rs`, offline) — full container re-parse with header
  pins (62400/100/100/nfree 1/fNbytesName 64), per-key
  uncompressed-record arithmetic, StreamerInfo/free-list header pointer
  agreement, keys-list contents (histos only, no StreamerInfo entry),
  TH1D member round-trip, byte-determinism with pinned datime/UUIDs,
  empty + multi-histo files, `finish` basename-as-TFile-name, >255-char
  names; oracle (`tests/uproot_oracle.rs`, probes `$ROOTFILE_PYTHON` →
  `.venv-uproot` → `python3`, loud SKIP if absent,
  `ROOTFILE_REQUIRE_UPROOT=1` makes absence fatal for CI) — fixture
  drift vs freshly generated uproot reference, full member read-back of
  our file via `check_with_uproot.py` (values/variances/errors/edges/
  fEntries/stats array/fNcells/axis members/`to_hist` round-trip,
  exact comparisons), multi-histogram key listing + per-key read-back.
- **Deferred per spec**: ZLIB compression, per-region TDirectories,
  TH2D, env-gated `root`/`hadd` binary smoke tests (no ROOT binary on
  this machine; the spec lists them as env-gated extras — first run will
  need `command -v root` wiring when a ROOT install is available).

## 2026-06-12 — Phase 9: `histos.json` bridge renderers (`run --histos` → .C/.py/CSV/SVG)

New module `crates/adl-cli/src/cmd/bridges.rs` — pure, byte-deterministic
renderers of the in-memory `HistoSet` (the same accumulator that writes
`histos.json`, so the bridges and the canonical JSON cannot disagree). All
four formats land next to `histos.json` when `smash2 run --histos DIR`
runs. Touched only adl-cli + its tests (the `rootfile` binary writer is the
separate sibling task above; these renderers are the no-binary-dependency
collaborator path).

- **make_histos.C** (always emitted) — self-contained ROOT macro
  (`root -l -b -q make_histos.C` → `histos.root`). Per histogram: `new
  TH1D(flat_name, title, n, lo, hi)`, `Sumw2()` first, then
  `SetBinContent`/`SetBinError` over bins **0 (underflow) … N+1
  (overflow)** with `error = sqrt(sumw2)`, `SetEntries(raw_count)`, and
  `Double_t stats[4] = {tsumw, tsumw2, tsumwx, tsumwx2}; PutStats(stats)`
  (the SPEC_ROOT_WRITER §4 in-range fill-time moments — `GetMean`/`hadd`
  stay exact). Titles C-escaped (quotes/backslashes/controls).
- **to_root.py** (always emitted) — uproot 5 + numpy script
  (`python3 to_root.py`). Builds each TH1D via
  `uproot.writing.identify.to_TH1x(fName, fTitle, data, fEntries, fTsumw,
  fTsumw2, fTsumwx, fTsumwx2, fSumw2, fXaxis=to_TAxis(...))` with
  `data`/`fSumw2` length `nbins+2` in TArrayD flow order
  `[underflow, bin1.., overflow]`, so entries + the four stats moments are
  set exactly (the high-level `hist` path drops raw entry counts /
  fill-time moments). Signatures verified against current uproot5
  `identify.py`. Titles Python-escaped.
- **--csv** — one `<flat-name>.csv` per histogram,
  `bin_lo,bin_hi,content,error` rows for the **in-range** bins (flow lives
  in the JSON; CSV is the visible-axis quick table); header line included.
- **--svg** — one hand-rolled 640×400 step-plot per histogram (no plotting
  dep): filled bin-top polygon, y scaled to the tallest in-range bin, axis
  lines + ymax label, caption `flat_name [lo, hi) xN entries=E` and an
  `underflow=…/overflow=…` note when flow is nonzero. Coordinates trimmed
  to ≤3 decimals, `-0` normalized.
- **Naming**: flat region-prefixed v1 names (`<region '/'→'_'>_<histo>`,
  e.g. `baseline_hmet`, ratified decision 2) — identical to what the
  `rootfile` writer uses, so a future native `.root` and these bridges
  agree on object names and all three are hadd-mergeable. File stems sanitize
  to `[A-Za-z0-9._-]`.

CLI: `run --histos DIR` now writes histos.json **+ make_histos.C +
to_root.py** unconditionally; `--csv`/`--svg` are additive flags that clap
`requires`-gates to `--histos` (bare `--csv` → usage exit 2). All histogram
diagnostics stay on stderr; stdout machine-clean. **`run --json` no-histo
output and the histos.json bytes are unchanged** (existing
`run_json_bins_partition` / `run_histos_json_file` snapshots did not move);
`verify` default + `--json` untouched.

- **Oracles run on this machine** (probed; recorded per the brief):
  - `python3 + uproot`: **RAN.** `root` not on PATH → ROOT oracle SKIPPED.
  - The repo carries a gitignored `.venv-uproot` (uproot **5.7.4**, numpy,
    hist — created by the sibling rootfile task). Pointed the env-gated
    uproot test's PATH at it: the generated `to_root.py` executed,
    produced a valid `histos.root`, and uproot read it back with
    `fEntries=32`, `baseline_hnjets` bin4=10, errors=sqrt(sumw2),
    `fTsumwx=136`/`fTsumwx2=610`, axis [0,20) ×20, streamers present —
    full data-fidelity confirmation of the bridge, not just a syntax check.
- **Env-gated tests** (`crates/adl-cli/tests/cli.rs`): `--csv`/`--svg` are
  opt-in via `SMASH2_RUN_ROOT_ORACLE=1` / `SMASH2_RUN_UPROOT_ORACLE=1`
  (loud SKIP otherwise, so default CI does not need ROOT/uproot installed);
  each builds the bridge under its tool and reads a known
  bin/entries/value back. A tiny dependency-free `which()` does PATH
  lookup.
- **Snapshot tests** on the ex02 golden (`make_histos.C`, `to_root.py`,
  `baseline_hnjets.csv` + negative-lo `baseline_hjet1eta.csv`,
  `baseline_hnjets.svg`), a cross-run byte-identity test over .C/.py/CSV/SVG,
  the `--csv`-without-`--histos` usage-error, and a flow-bin assertion
  battery (overflow bin N+1, weighted `sqrt(4)=2` errors, raw entries,
  PutStats moments, SVG overflow caption) using the tiny weighted fixture.
  The ex02 events come from the sibling `adl-difftest` fixture
  (`tests/fixtures/ex02_events.jsonl`) via a relative-path helper.

### Verification (run before returning)

- `cargo build --workspace` — green.
- `cargo test --workspace` — green, **424 passed / 0 failed** (includes the
  sibling rootfile + adl-sema/adl-interp Phase-9 work that landed
  concurrently; the adl-cli bridge suite is 32 tests).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt -p adl-cli --check` — clean.
- uproot oracle (PATH → `.venv-uproot`) — ran, read-back exact.

### Notes / boundary

- Brief said "adl-cli or adl-viz — your call": chose **adl-cli**, where
  `run --histos` already lives and adl-interp's `HistoSet` is in scope; no
  reason to route bridge bytes through adl-viz.
- Mid-session race: the sibling `rootfile` crate was being written
  (incomplete `Cargo.toml`/missing `lib.rs`) and briefly made the whole
  workspace manifest unloadable; did **not** touch it (boundary held — a
  stub write was correctly blocked). It landed its `lib.rs` shortly after
  and the full-workspace gate then passed. No deviation on my side.

## 2026-06-12 — Phase 9 integration pass (cross-crate check, oracle audit)

- **No cross-crate breaks found.** The two parallel Phase-9 tasks (rootfile
  crate; adl-cli bridge renderers) compose cleanly: `cargo build
  --workspace`, `cargo test --workspace` (424 passed / 0 failed),
  `cargo clippy --workspace --all-targets -- -D warnings`, and
  `cargo fmt -p rootfile -p adl-cli --check` all green from a fresh pass.
  Determinism tests re-run individually and green:
  `verify_report_is_byte_identical_across_runs`,
  `run_histos_writes_canonical_json_deterministically`,
  `run_histos_bridges_are_byte_identical_across_runs`,
  `dot_is_byte_identical_across_runs`,
  rootfile `builds_are_byte_deterministic_when_pinned`.
- **Oracle audit (honest status).** The rootfile uproot oracle did
  genuinely execute, not skip: re-run with `ROOTFILE_REQUIRE_UPROOT=1`
  (which turns the loud SKIP into a panic) — all 3 tests pass
  (`uproot_reads_back_our_file_exactly`,
  `uproot_lists_and_reads_multi_histo_file`,
  `vendored_fixtures_match_freshly_generated_uproot_reference`); the
  probed interpreter `<workspace>/.venv-uproot/bin/python` imports
  uproot 5.7.4. ROOT binary still absent on this machine → root/hadd
  smoke tests remain unexercised (env-gated, as designed).
- **Housekeeping:** deleted a stray untracked
  `crates/adl-sema/crates/adl-sema/tests/snapshots/` tree (5 insta
  `.snap.new` files written by a builder running tests with cwd inside
  the crate). Verified each body byte-identical to the accepted
  snapshot before deleting — no hidden drift.
- **Open integration gap (deliberate, not done here):** ratified decision
  6 says smash2 depends on `rootfile`. The crate is a workspace member
  and in `[workspace.dependencies]`, but adl-cli has no `rootfile` dep
  yet — `run --histos` emits histos.json + bridges, not a native
  `histos.root`. Wiring it is a follow-up task (new output file, new
  snapshots), out of scope for this integration pass.

## 2026-06-12 — Phase 9 gate fix: deliverables committed to git

- Independent gate flagged that all Phase-9 work (the `rootfile` crate
  incl. `fixtures/streamerinfo_th1d.bin` + `fixtures/PROVENANCE.md`,
  adl-cli `bridges.rs` + 8 snapshots, adl-interp `histo.rs` +
  `histo_semantics.rs`, adl-difftest `histo_golden.rs` + fixtures +
  snapshots, `scripts/gen_ex02_events.py`, and the supporting tracked-file
  edits) existed only as uncommitted working-tree files — never staged.
  Content/quality were not in question; "checked in" was.
- Fix: staged and committed the full Phase-9 set in one commit. Verified
  beforehand the streamer fixture sha256 matches PROVENANCE.md
  (`eaa2bb51…`) and that no build artifacts ride along (`.venv-uproot`
  gitignored; `target/`, `Cargo.lock` already ignored).
- Re-verified before committing: `cargo build --workspace` green;
  `cargo test --workspace` 424 passed / 0 failed; `cargo clippy
  --workspace --all-targets -- -D warnings` clean; all five named
  determinism tests pass individually (verify report, histos.json,
  bridges, dot, rootfile pinned-build).

## 2026-06-12 — Phase 9: wire `rootfile` into smash2 (`run --histos` → native out.root)

Closed the integration gap the previous Phase-9 pass left open (ratified
decision 6: smash2 depends on `rootfile`). `smash2 run --histos DIR` now
ALSO writes a native `out.root` next to `histos.json` and the two bridge
scripts; `--no-root` opts out of just the binary file (JSON + bridges still
written). Crates touched: **adl-cli** (wiring + tests) and **rootfile** (a
one-line `Clone` derive — deviation noted below).

- **adl-cli `cmd/run.rs`**: new `write_root_file` builds a
  `rootfile::RootFile` from the in-memory `HistoSet` (the same accumulator
  that writes `histos.json`, so the native file cannot disagree with the
  JSON or the bridges) and `finish`es it to `DIR/out.root`. `h1_spec`
  maps `Hist1D` → `rootfile::H1Spec`: in-range bins stay, flow bins move to
  `under`/`over`, raw `u64` entries become the f64 `fEntries`, the four
  fill-time moments pass through. Object names are the flat region-prefixed
  v1 names via the now-`pub(crate)` `bridges::root_name`, so out.root, the
  `.C`/`.py` outputs, and the CSV/SVG stems all share names and stay
  hadd-mergeable. A histogram the writer rejects (practically unreachable —
  flat names of a valid HistoSet don't collide) is skipped with a stderr
  diagnostic; the others still write. **Datime + UUIDs are pinned**
  (`pack_datime(2026,6,12,0,0,0)` + zeroed UUIDs) so out.root is
  byte-identical across runs — the same determinism the rest of
  `run --histos` already gives, and what hadd/byte-diffs want.
- **adl-cli `main.rs`**: new `--no-root` flag (clap `requires = "histos"`,
  same gating as `--csv`/`--svg`). The four histogram output flags
  (`histos`, `csv`, `svg`, `no_root`) are bundled into a new
  `cmd::run::HistoOpts<'_>` struct passed to `run` — keeps the `run`
  signature under the clippy `too_many_arguments` ceiling (an elegant fix,
  not an `#[allow]`).
- **DEVIATION (rootfile)**: added `#[derive(... Clone)]` to `RootFile`
  (it already held only `Clone` fields; `Th1d` already derived `Clone`).
  `add_th1d` is a consuming builder that returns the `Error` (not the
  builder) on rejection, so recovering the accumulator on the skip path
  needs a pre-add `clone()`. Minimal, additive, no behavior change; the
  task names rootfile as a dependency of smash2 and this is the wiring it
  requires. No other rootfile code touched.
- **Adl-cli Cargo.toml**: `rootfile.workspace = true` as a normal dep, and
  as a dev-dep (the CLI tests re-parse out.root with the writer's own
  strict reader). The workspace `rootfile` path dep already existed.
- **Tests** (adl-cli `tests/cli.rs`, +4 → 36 in-suite):
  `run_histos_writes_native_root_file` (out.root lands next to
  histos.json, re-parses via `rootfile::reader`, TH1D content matches the
  accumulator: flat name, flow bins, weighted Σw², raw entries, fill-time
  moments), `run_histos_native_root_is_byte_identical_across_runs` (the
  **determinism test over out.root bytes** the brief requires — two runs,
  pinned datime/UUIDs ⇒ identical bytes), `run_histos_no_root_suppresses_
  out_root_only` (`--no-root` ⇒ no out.root, JSON + bridges still present),
  `no_root_requires_histos_dir` (clap usage exit 2). Existing CLI
  snapshots (`run_histos_json_file`, the bridge snapshots) and the JSON
  bytes are **unchanged** — out.root is an additive file, not a change to
  any existing output. `run --json` no-histo output unchanged.
- **End-to-end oracle check (ran on this machine)**: `run --histos` on
  `examples/tutorials/ex02_histograms.adl` over the committed 200-event
  fixture wrote a 10-key out.root (2-D/varbin histos skipped with stderr
  diagnostics, as designed); `.venv-uproot/bin/python` (uproot 5.7.4) read
  it back exactly — 10 flat region-prefixed keys, `baseline_hnjets`
  fEntries=32, flow-inclusive values `[0,0,0,0,10,7,…]`, axis [0,20)×20,
  `fTsumw=32`/`fTsumwx=136`, streamers present. Full data-fidelity, not a
  syntax check.

### Verification (run before returning)

- `cargo build --workspace` — green.
- `cargo test --workspace` — green, **428 passed / 0 failed** (was 424;
  +4 native-root CLI tests).
- `cargo clippy --workspace --all-targets -- -D warnings` — clean.
- `cargo fmt -p adl-cli -p rootfile --check` — clean (my crates).
- Determinism tests pass individually: `verify_report_is_byte_identical_
  across_runs`, `run_histos_writes_canonical_json_deterministically`,
  `run_histos_bridges_are_byte_identical_across_runs`,
  `run_histos_native_root_is_byte_identical_across_runs` (new),
  `dot_is_byte_identical_across_runs`, rootfile
  `builds_are_byte_deterministic_when_pinned`.

### Pre-existing gap (not mine, not touched)

`cargo fmt --check` over the WHOLE workspace reports diffs in
adl-difftest / adl-interp / adl-sema / adl-syntax test+src files —
formatting drift from concurrent Phase-9 work that predates this task. My
boundary is adl-cli + the rootfile wiring; both are `cargo fmt --check`
clean. Left the others untouched (a separate fmt sweep should land them).

## 2026-06-12 — Phase 9 integration-gate fix: README Histograms example path

The independent integration gate found the README's headline Histograms
command broken copy-paste: from the documented cwd (`reimplementation/adl2`,
established by the Quick start), `examples/tutorials/ex02_histograms.adl`
does not resolve (the examples tree lives two levels up), and `events.jsonl`
was a placeholder that exists nowhere. Fixed `README.md` (Histograms code
block) to use `../../examples/tutorials/ex02_histograms.adl` and the
committed 200-event fixture
`crates/adl-difftest/tests/fixtures/ex02_events.jsonl`, with a comment
noting paths are relative to this directory — the command is now verbatim
copy-paste runnable (verified: exit 0, all four outputs + CSV/SVG written;
the expected 2-D/varbin skip diagnostics on stderr). Audited every other
concrete path the README references from this cwd (`scripts/corpus_gate.sh`,
`../SPEC_*.md`, `../../legacy_parser/`, …) — all resolve. Docs-only change;
no crate code touched.

## 2026-06-12 — Phase 9 final validation pass (full battery + oracle coverage summary)

Validation-only pass; no crate code touched. New file:
`HISTOGRAM_REPORT.md` (what shipped / validation evidence / gaps / demo
commands). Results, all run from this directory on this machine:

- `cargo build --workspace` green; `cargo clippy --workspace --all-targets
  -- -D warnings` clean; `cargo fmt -p rootfile -p adl-cli --check` clean
  (whole-workspace fmt drift in other crates is the pre-existing item
  noted 2026-06-12, untouched).
- `cargo test --workspace` (default, z3-native): **428 passed / 0
  failed**. `cargo test --workspace --no-default-features` (SMT-LIB
  subprocess backend, z3 4.8.12 binary): **428 passed / 0 failed**.
- `cargo test --workspace --all-features` (deep property battery: 100k
  encoder-vs-interp + 10k metamorphic cases): **428 passed / 0 failed**
  (exit 0; `encoder_vs_interpreter` 100k cases in 1277 s, metamorphic
  6/6 in 409 s).
- `scripts/corpus_gate.sh`: all 68 corpus files parse + resolve clean.
- Golden batteries: adl-analysis `golden_battery` 37/37; adl-difftest
  `histo_golden` 2/2.
- Determinism, each named test run individually with `--exact`, all pass:
  `verify_report_is_byte_identical_across_runs`,
  `run_histos_writes_canonical_json_deterministically`,
  `run_histos_bridges_are_byte_identical_across_runs`,
  `run_histos_native_root_is_byte_identical_across_runs`,
  `dot_is_byte_identical_across_runs`, rootfile
  `builds_are_byte_deterministic_when_pinned`. Plus a manual double-run
  of the real binary on ex02: `verify` default, `verify --json`,
  `histos.json`, `out.root`, `make_histos.C`, `to_root.py` all
  byte-identical (`cmp`).

### Oracle coverage (what actually executed vs skipped)

- **uproot read-back, rootfile suite — RAN (forced)**:
  `ROOTFILE_REQUIRE_UPROOT=1 cargo test -p rootfile --test uproot_oracle`
  → 3/3 (`uproot_reads_back_our_file_exactly`,
  `uproot_lists_and_reads_multi_histo_file`,
  `vendored_fixtures_match_freshly_generated_uproot_reference`).
  Interpreter: `.venv-uproot` uproot 5.7.4 / Python 3.12.3. Fixture
  sha256s re-verified against `crates/rootfile/fixtures/PROVENANCE.md`.
- **End-to-end ex02 oracle — RAN (ad hoc)**: `smash2 run
  ex02_histograms.adl <200-event fixture> --histos` → uproot read
  `out.root` back exactly: 10/10 histograms, flow-inclusive values, raw
  `fSumw2`, axis edges, `fEntries`, all four `fTsumw*` moments, titles,
  `fNcells`; 14 streamer classes present; zero mismatches.
- **CLI uproot bridge oracle — RAN (forced)**:
  `SMASH2_RUN_UPROOT_ORACLE=1` with `.venv-uproot/bin` on PATH →
  `uproot_script_round_trips_when_available` executed (not skipped).
  Ad hoc on top: `to_root.py`'s `histos.root` vs native `out.root` —
  equivalent across all 10 histograms (values, fSumw2, entries, moments,
  titles, fNcells).
- **ROOT binary + hadd — SKIPPED (env-gated, no `root`/`hadd` on this
  machine)**: `SMASH2_RUN_ROOT_ORACLE=1` run anyway → loud
  "skipping: `root` not on PATH"; hadd smoke test likewise unexercised.
  Standing instruction for the first ROOT-equipped machine recorded in
  HISTOGRAM_REPORT.md.

## 2026-06-12 — Phase 10c (SPEC_EVENT_PIPELINE §1): Delphes ingestion — adl-ingest crate, `smash2 ingest`, `run --profile delphes`

Implemented §1 exactly as the spec decided — **(c) both paths**: native
oxyroot reader primary, generated uproot script as the independent
oracle. New crate `crates/adl-ingest` (workspace member; oxyroot pinned
`=0.1.25` in the root manifest with the why-pinned comment).

### What shipped

- **Profile contract** (`profile.rs`): a pure data table — collections
  (branch prefix → emission key, leaves with `F32 | I32 | TagBit(bit)`
  kinds, constant props), MET spec, scalar specs, weight source, LHE
  branch, known-drop branches. Core `Event`/`Interp` never see experiment
  names. `delphes()` encodes the §1.2 table with the spec's recommended
  defaults for every [DECIDE]: I1 bit 0 (`TagBit(0)` on BTag/TauTag; the
  `btag_bit = N` option is the same field with a different bit — covered
  by a unit test using bit 1), I2 PDG masses as profile constants
  (e 0.000511, μ 0.105658), I3 canonical `fatjet`, I4 `Event.Weight`.
  `Profile::decides()` derives the per-run choices *from the table* (no
  second copy) and `--verbose` prints them on both `ingest` and `run
  --profile` (§6 provenance will reuse it).
- **Native reader** (`reader.rs`): columnar oxyroot read → canonical
  JSONL lines (serde_json/ryu shortest floats, fixed profile key order,
  ints for tags/charges). The `run --profile` path feeds the lines *in
  memory* to `adl_interp::read_jsonl` — one loader, one set of
  event-model validations for both input paths, and `ingest -o` JSONL ==
  what `run` evaluates by construction. Diagnostics are a typed enum
  (`IngestDiag`), deterministic order: LHE multiweight count, unmapped
  leaves of mapped collections (count always, full list under
  `--verbose`), ignored tag bits, MET/scalar/weight multiplicity
  anomalies (first taken / value omitted — per the spec's "first (only)
  element"), unknown branch families, absent mapped collections
  (verbose-only). Hard refusals (`IngestError`, exit 1, no output
  written): missing/mistyped mapped branches, counter/leaf length
  disagreement, negative counts, **non-finite values**, and
  **pT-ordering violations** (named entry + index; never a re-sort).
  Tag domain is {0,1} by construction of the bit extraction, with other
  set bits *diagnosed* — never folded in.
- **Counter-authoritative re-chunking** (the one design judgment beyond
  the spec text, documented in the module header): leaves are flattened
  and re-chunked by `<collection>_size` with a hard totals check, because
  oxyroot's per-entry slice boundaries are correct on real Delphes splits
  but wrong on uproot-written trees (uniform-length baskets omit entry
  offsets — found empirically; the mini fixture would silently mis-slice
  one-element branches otherwise). Delphes defines `_size` as the
  collection length, so the counter is the honest authority; any
  disagreement is a refusal, and the script oracle (uproot asserts
  per-event lengths against the same counters) would catch residual
  infidelity.
- **oxyroot 0.1.25 gotcha**: `Branch::item_type_name()` panics (`todo!`)
  on exotic split members (TRefArray/TLorentzVector leaves —
  `Jet.Constituents`, `GenJet.SoftDroppedJet`) on real Delphes files.
  The reader therefore enumerates branch *names* only and queries types
  solely on branches the mapping actually loads (all simple leaf types).
- **Oracle script generator** (`script.rs`): `to_jsonl.py` rendered from
  the same profile table (one mapping source, two independent readers).
  Python `jnum` converts CPython repr (shortest round-trip, same digits)
  to ryu notation — exact rules pinned by experiment: ryu plain-decimal
  region is e10 ∈ [−5, 15] (CPython switches at < −4), CPython zero-pads
  negative exponents, positive exponents already agree (`1e+16`). An
  env-gated cross-language battery asserts agreement on 26 adversarial
  values (5e-324, f32 subnormals, 1e21, −0.0, …).
- **Core-model change (the only one, per §4)**: `Event` gains
  `pub weight: f64` (Default = 1.0; negative allowed — NLO), JSONL gains
  the optional top-level `"weight"` key (case-folded, duplicate-checked,
  no longer a scalar). Composition/cutflow use is 10a scope, untouched.
- **CLI**: new `smash2 ingest [ROOT] --profile NAME [-o FILE]
  [--emit-script DIR]` (usage-errors when there is nothing to do or `-o`
  lacks an input; unknown profile lists the known set) and `run … --profile
  NAME` (ROOT in, same evaluation as JSONL in). Stdout stays
  machine-clean; all diagnostics on stderr.
- **[DECIDE-I3] follow-through**: `legacy_parser/adl/object_aliases.txt`
  gains the `FATJET FatJet FJet AK8jet` spelling family (its header
  comment explains why `AK8jets` plural is excluded — it is a *derived*
  object name in CMS-SUS-21-009). Without this, ADL `FJet` cuts would
  silently see an empty collection on profile-ingested events. Full
  workspace battery green after the change — no golden churn.
- **Fixtures** (`crates/adl-ingest/fixtures/`, ~150 KB committed, all
  Delphes-shaped trees written by `make_fixtures.py`, sha256s +
  freeze evidence in `PROVENANCE.md`): `delphes_mini.root` (13 real
  T2tt events chosen for collection coverage), `delphes_synth.root`
  (multi-bit tags, negative/odd weights, MET multiplicities 1/1/2/0,
  unknown `Track`, empty event), `delphes_badorder.root`,
  `delphes_nan.root`, plus the two frozen `.expected.jsonl` goldens.
- `scripts/fetch_delphes_sample.sh` — sha256-verified cached download of
  the pinned 20k-event sample for the e2e gate. README: new "Event
  ingestion (Delphes)" section; status/crate-table/test-count updates.

### Verification (run before returning)

- `cargo build --workspace` green; `cargo test --workspace` **457
  passed / 0 failed** (was 428; +29: 13 adl-ingest, +12 CLI ingest suite,
  +4 interp weight-key tests). `cargo test -p adl-ingest -p adl-cli
  --no-default-features` green.
- `cargo clippy --workspace --all-targets -- -D warnings` clean;
  `cargo fmt -p adl-ingest -p adl-cli -p adl-interp --check` clean (the
  pre-existing whole-workspace fmt drift in other crates noted 2026-06-12
  remains untouched).
- **Forced oracle runs on this machine** (not skipped):
  `SMASH2_RUN_UPROOT_ORACLE=1` with `.venv-uproot/bin` (uproot 5.7.4) on
  PATH → `native_jsonl_matches_the_uproot_script_byte_for_byte` (mini +
  synth fixtures) and `script_jnum_matches_serde_json_on_edge_values`
  both executed and passed. `SMASH2_RUN_DELPHES_E2E=1
  SMASH2_DELPHES_SAMPLE=/tmp/delphes_T2tt_700_50.root` (sha256 verified
  in-test) → `delphes_sample_ingestion_fidelity_end_to_end` passed in
  136 s: native vs script **byte-identical across all 20,000 events**
  (21,003,410 bytes), 20000 LHE-weight diagnostic, §1.1 probe values
  pinned on entry 0 (Jet.PT 719.5091552734375, MET 653.098876953125,
  weight 1.0).
- Ad hoc end-to-end: `smash2 run ex02_histograms.adl delphes_mini.root
  --profile delphes` stdout is byte-identical to `run` over the
  materialized JSONL (also asserted by a permanent test).

### Gaps / deferred (by design, named)

- §6 provenance object (input sha256, decides, TNamed carrier) not yet
  emitted — 10c exit item still open; `Profile::decides()` is ready
  for it. `ingest -o` does not yet write the sibling
  `events.provenance.json`.
- No CLI syntax for per-run profile options (`btag_bit = N` exists as a
  profile-table field, exercised in tests); needs a [DECIDE] on flag
  shape when a non-default card shows up.
- Native path materializes columns in memory and re-parses canonical
  JSON lines (~20k events: trivial; full-sample ingest ≈ 1.4 s release).
  Constant-memory streaming + direct `Event` construction is 10d scale
  work, per plan.
- NanoAOD profile: spec'd (§1.3), not built — as planned for v2.
- [DECIDE-I4] cannot be distinguished on this sample (both weight
  branches ≡ 1.0); needs a weighted sample or collaborator sign-off, as
  the spec records.

## 2026-06-12 — Phase 10a (SPEC_EVENT_PIPELINE §2 + §4): cutflows + event-weight composition

Scope: per-region ordered cutflows and the input-weight × positional-ADL-weight
composition, in adl-interp/adl-cli/adl-difftest. Builds on the §4 core-model
change already landed (Event.weight, JSONL `weight` key — see the 10c entry).

### Built

- **Traced membership walk** (`eval.rs`): `region_uncached` is replaced by one
  `region_walk(idx, &mut Vec<StepEval>)` — the single source of truth for §4.3
  membership AND the §2 cutflow. `Interp::run_event_traced` returns
  `(Vec<RegionResult>, Vec<Vec<StepEval>>)`; `run_event` delegates to it, so
  the evaluation sequence (short-circuiting, memoized collections, cached
  region verdicts) is byte-identical to Phase 9. A `StepEval` records
  `{stmt index, Ok(true)/Ok(false)/Err}` per membership-affecting statement
  actually reached.
- **`weights.rs` (new, crate-private)**: the positional weight walker shared
  by histograms and cutflows — `(factor, weighted_incomplete)` in effect
  *before* each statement of a region. **[DECIDE-W1] resolved positional**,
  per the spec recommendation: a step/fill uses the product of the numeric
  `weight` statements declared at earlier positions. Non-numeric ([DECIDE-W2]
  deferred) and malformed weights contribute 1.0 and poison later positions
  with `weighted_incomplete` — flagged in JSON, never folded into the sums.
  The corpus lint (`cutflow_golden.rs::corpus_has_no_weight_after_fill_point`,
  67 files) proves no corpus file has a `weight` after a fill point, so the
  switch from Phase 9's whole-region product is **non-breaking** (and any
  future file where it differs raises a `[DECIDE-W1]` runtime diagnostic).
- **`cutflow.rs` (new)**: `CutflowSet` — per selection region, step 0 `all`
  then one step per `select`/`reject`/`trigger`/inheritance (one step carrying
  the parent's whole predicate; the parent's own table holds its breakdown);
  histoList references and `weight`/`histo`/`bin`/`save`/… contribute no step.
  Per step `raw`/`sumw`/`sumw2`/`errors`; w_eff = `Event.weight ×` positional
  factor; a hard `EvalError` at step i counts the event as failing step i and
  increments `errors`. `bin` appendix per spec: boundary bins
  `[b0,b1)…[bn,∞)` + `out` bucket (below-b0 / non-value) + `failed`;
  boolean bins get true/false buckets; filled only from whole-region passes.
  Regions containing out-of-fragment statements (`sort`, unresolved refs) are
  skipped with a diagnostic — the histogram honesty rule.
- **Emissions** (one accumulator, three renderings): canonical `cutflow.json`
  (`version: 1`, `total {raw,sumw,sumw2}`, `regions[].steps[] {kind,label,
  raw,sumw,sumw2,errors[,weighted_incomplete]}`, `bins[]`; shared `JsonWriter`
  moved to `json.rs`, ryu shortest floats, byte-deterministic); fixed-width
  stdout table (`step | raw | abs% | rel% | errors | sumw +- err`) appended
  after the per-event lines in text mode; `{"cutflow": {...}}` as one final
  line under `run --json`; `cutflow.json` written next to `histos.json` under
  `--histos DIR`. Files with no evaluable region emit none of these.
- **Histogram fills now compose the input weight** (§4 "raw vs weighted
  everywhere"): fill weight = `Event.weight × positional factor` (was: static
  whole-region product, implicitly w_input ≡ 1). `entries` stays raw.
  histos.json gains `weighted_incomplete` (emitted only when true — no churn
  for clean files; ex02/HISTO goldens byte-unchanged since all fixtures carry
  unit input weights and weights precede fills).

### Step labels (deviation, flagged for ratification)

Spec §2 says "verbatim source text of the statement". HIR carries expression
spans, not statement spans, so labels are `keyword + verbatim expression
slice` with the keyword canonicalized (`cut` renders as `select`), and an
inheritance step is the verbatim reference text. Two consequences, documented
in the module header: a `select` naming a boolean define shows the *inlined
define body* (resolution replaced the reference), and a statement whose span
cannot be sliced falls back to `select <statement N>`. Exact statement spans
would need an additive HIR change across 5 crates — deferred; flag if labels
must be raw-source-exact before the TH1D `fLabels` work consumes them.

### Verification (run before returning)

- Hand-computed unit battery `adl-interp/tests/cutflow_semantics.rs` (12
  tests): every step kind, inheritance-as-one-step, error counting (missing
  scalar → `errors`, later steps untouched), skipped out-of-fragment region,
  weighted case **incl. a 0-weight event** (raw counts it, sums add zero;
  positional: `select` before `weight lumi 2.0` carries factor 1, after it
  factor 2 — totals 6/14, step sums 5/13 and 6/36 hand-checked),
  `weighted_incomplete` flagging, boundary + boolean bin appendices, an exact
  hand-written canonical JSON string, table/JSON byte-determinism.
- **ex02 golden extended** (`adl-difftest/tests/cutflow_golden.rs`): cutflow
  JSON + stdout table snapshots over the committed 200-event fixture
  (baseline 200 → 200 → 75 → 62 → 60 → 47 → 32; singlelepton inherits
  baseline as one 32-raw step → 11). **Independent oracle**: every step's
  raw count recomputed by a test-local prefix-conjunction walk
  (`eval_bool`/`eval_region_by_name`, not the traced walk) — exact match;
  sumw == raw under unit weights; monotonicity; byte-determinism (two
  accumulations). Toy-event battery (seeds 1/7/42 × 300 events): final step
  raw == untraced `run_event` membership count.
- CLI: `run_json_cutflow_composes_input_weights` (4 weighted events incl.
  w=0, hand-computed step counts asserted through the `--json` line),
  cutflow.json byte-identity + snapshot under `--histos`, `--json` line
  count/snapshots updated, cutflow.json added to the bridges determinism
  list. Snapshots `run_text_disjoint_pt` / `run_json_bins_partition`
  regenerated (cutflow table/line appended — reviewed, counts hand-checked).
- `cargo test -p adl-interp -p adl-difftest` green; `cargo clippy
  -p adl-interp -p adl-difftest --all-targets -- -D warnings` clean; `cargo
  fmt` clean on touched crates. Full-workspace battery: see note below.

### Gaps / deferred (named)

- §2 emission 2 (TH1D pair `__cutflow_raw`/`__cutflow_wt` in out.root) needs
  the rootfile TAxis `fLabels` THashList extension — in flight as Phase 10b
  (concurrent rootfile work observed in-tree during this pass); wire-up is a
  follow-on once that lands.
- `cutflow.json` carries no §6 `provenance` object yet — same 10c exit gap
  as histos.json/out.root.
- Region-local weights only: a parent's `weight` statements do not propagate
  through inheritance (matches Phase-9 histogram semantics; spec is silent —
  flag for ratification if cross-region weighting is wanted).
- `--fail-on errors>0` gating mentioned in §2 not yet added to `run` (only
  `verify` has `--fail-on` today); errors are surfaced in table + JSON.

## 2026-06-12 — Phase 10b wire-up + §6 provenance: 2-D / variable-bin histos fill end-to-end; provenance embedded everywhere

Scope: close the SPEC_EVENT_PIPELINE §3 sema/accumulator gap and the §6
provenance gap. The `rootfile` crate already had TH2D + variable-bin TH1D +
TNamed writers passing the uproot oracle in isolation, and the adl-interp
accumulator already had `Hist2D`/`Hist1DVar` + histos.json v2; the missing
links were (a) sema still resolved the 2-D/var-bin `histo` syntax to
`HistoSpec::Unsupported`, so those fills never instantiated, and (b) no run
emitted a provenance object. Touched adl-sema, adl-interp, adl-cli, plus the
ex02 difftest/CLI goldens. No new dependencies.

### Built

- **Sema (adl-sema)**: `HistoSpec` gains `Var1D { edges, expr }` and
  `Uniform2D { nx, xlo, xhi, ny, ylo, yhi, xexpr, yexpr }`;
  `resolve_histo_spec` recognizes the variable-bin (`e0 e1 … en, expr`) and
  2-D (`nx, xlo, xhi, ny, ylo, yhi, xexpr, yexpr`) shapes. Edge lists are
  validated strictly increasing with ≥ 2 edges at resolve time (else
  `Unsupported` with the honest reason). Only genuinely malformed argument
  lists remain `Unsupported`.
- **Accumulator (adl-interp/histo.rs)**: `HistoSet::instantiate` now maps the
  two new specs into the existing `HistAcc::H1Var`/`HistAcc::H2` arms (which
  already had fills, the seven 2-D moments, and v2 JSON). A defensive
  re-check of edge monotonicity at instantiation guards a future caller.
  Everything downstream (histos.json v2, bridges, out.root) already rendered
  these forms — no change there.
- **Provenance (SPEC_EVENT_PIPELINE §6)**: new `adl_interp::Provenance` +
  `InputIdentity`, rendered with the shared ordered `JsonWriter`
  (`tool, adl{file,sha256}, input{file,sha256,events,profile?}, seed?,
  decides?` — fixed order, no wall-clock). `HistoSet::to_json_with` /
  `CutflowSet::to_json_with` embed it as a top-level `provenance` key
  (the bare `to_json` stays for the content goldens). The CLI builds **one**
  object per run and embeds the identical bytes in histos.json, cutflow.json,
  the `--json` lines, and an `out.root` `TNamed` named `smash2_provenance`
  (title = compact canonical JSON). Input identity hashes the *original*
  bytes — the ROOT file under `--profile`, the JSONL otherwise — and carries
  the profile id + `decides()` choices.
- **SHA-256 (adl-interp/sha256.rs)**: a self-contained FIPS 180-4
  implementation, vendored rather than a new dependency (same discipline as
  rootfile's streamer blobs; zero external surface, byte-deterministic).
  Verified against the empty/`abc`/56-byte and 1,000,000-byte FIPS vectors.

### Verification (run before returning)

- `cargo build --workspace` green; `cargo test --workspace` **503 passed /
  0 failed** (was 457; +46: 2 sha256, 2 provenance, rewritten/added histo +
  CLI tests, the env-gated ex02 uproot e2e). `cargo test -p adl-interp
  -p adl-cli --no-default-features` green.
- `cargo clippy --workspace --all-targets -- -D warnings` clean; `rustfmt
  --check` clean on every file touched this pass (the pre-existing
  whole-workspace fmt drift in other crates, noted 2026-06-12, remains
  untouched).
- **ex02 now fills all three forms**: `hj1ptMET` (h2, 40×40, 32 entries),
  `hlep1ptMET` (h2, 11 entries), `hmetvarbin` (h1var, exact edges
  `[0,10,20,50,100,500]`, 32 entries) appear in histos.json and out.root —
  the histo_golden diagnostics snapshot drops the two "deferred; skipped"
  lines (only the repeated-histoList note remains). Content pinned in the
  committed golden snapshot.
- **Provenance round-trips through every output**: a permanent CLI test
  (`run_histos_writes_native_root_file`) re-reads the `smash2_provenance`
  TNamed with the writer's own reader and asserts its title bytes equal the
  object embedded in histos.json; histos.json == cutflow.json provenance.
- **Independent uproot oracle** (env-gated `SMASH2_RUN_UPROOT_ORACLE=1`,
  forced on this machine via `.venv-uproot` uproot 5.7.4):
  `ex02_out_root_read_back_by_uproot` runs the full `run --histos` path and
  asserts via uproot: `hj1ptMET.values(flow=True)` is (42,42) summing 32,
  `hmetvarbin` edges are the exact ex02 edges with 32 entries, and the
  provenance TNamed title parses as JSON with the matching ADL sha256.

### Decisions / deviations (flagged)

- **Provenance `tool` = `smash2 <CARGO_PKG_VERSION>`** (no git short hash):
  no build-time git capture is wired, and a hash would not survive the
  byte-determinism guarantee across commits without an injection seam. The
  §6 `+<git>` suffix is left for a later build.rs that pins/injects it like
  `out.root`'s fDatime. Deterministic per build today.
- **Snapshot redaction**: the four provenance-bearing CLI snapshots redact
  only the `provenance.file` basenames (they carry the test pid); both
  sha256s, the tool string, and the event count stay pinned. insta's
  `filters` feature is not enabled in the workspace, so redaction is a plain
  string pass in the test, not an insta filter (no Cargo feature churn).

### Gaps / deferred (named)

- `seed` is wired in the `Provenance` struct but always `None` from `run`
  (synthetic-event seeding is 10d scale work); `decides` is populated only
  under `--profile`.
- `ingest -o` still does not write the sibling `events.provenance.json`
  (§6, the 10c ingest path) — `run` is fully covered; the ingest sibling
  remains the named 10c gap.
- [DECIDE-P1] is implemented as always-full-sha256 (recommended); no
  size-threshold fingerprint scheme.

## 2026-06-13 — Phase 10d: streaming input + deterministic parallel event loop (SPEC_EVENT_PIPELINE §5)

Scope: replace the whole-file `read_jsonl` buffering + serial event loop of
`smash2 run` with a streaming reader and a chunked, parallel loop whose
merge is byte-deterministic regardless of `--jobs`. No experiment specifics
touched (the loop is profile-agnostic); the cutflow/histogram accumulators,
out.root, and provenance from 10a–10c are reused unchanged. No new runtime
dependencies (std threads + channels; criterion is bench-only, gated off the
default graph). Touched adl-interp (streaming reader, accumulator merges,
incremental sha256), adl-cli (parallel driver, `--jobs`, streaming hash),
adl-difftest (bench).

### Built

- **Streaming reader (adl-interp/event.rs)**: `RawChunkReader` yields fixed
  `C = CHUNK_EVENTS = 4096` chunks of *unparsed* lines pulled from any
  `BufRead`, never buffering the file; `RawChunk::parse` turns one into an
  `EventChunk` of `StreamedEvent { ordinal, line, event }`. The parse runs
  **off** the shared reader lock — only line I/O is serialized — so JSON
  parsing + pT validation parallelize across workers (this is the single
  biggest throughput lever, below). `ChunkReader` keeps the inline-parse
  convenience path. Blank lines are skipped but counted (line numbers match
  the file); `ordinal` is the 0-based dense event index the per-event output
  numbers by, stable across `--jobs`.
- **Accumulator merges (adl-interp)**: `Counts::merge`, `Hist1D/Hist1DVar/
  Hist2D::merge`, `HistAcc/HistoFill/HistoSet::merge`, `BinFlow/RegionFlow/
  CutflowSet::merge` — field-wise additive merges in fixed structural order
  (both sides come from the same HIR, so indices align; debug-asserted).
  `0.0 + v == v` means merging one partial into a fresh master reproduces it
  bit-for-bit, which is what makes a single-chunk run byte-identical to the
  old serial pass.
- **Parallel driver (adl-cli/cmd/parallel.rs)**: `N = --jobs` scoped std
  threads pull `RawChunk`s under a mutex, parse + evaluate into **private**
  `HistoSet`/`CutflowSet` partials (no atomics, no shared mutation), and
  send them tagged with the ascending chunk index. The main thread runs a
  single fold with a reorder buffer that merges strictly in ascending index
  and flushes the per-event stdout lines in that same order — so text/JSON
  event output stays in input order and every f64 addition sequence is
  fixed. A malformed line records the earliest-by-line error (deterministic)
  and the run exits 1.
- **`--jobs N` (adl-cli)**: `0` (default) = all cores
  (`available_parallelism`). Documented as never changing outputs.
- **Incremental SHA-256 (adl-interp/sha256.rs)**: refactored the one-shot
  hasher into a streaming `Sha256` (8-word state + one 64-byte block buffer,
  O(1) memory); `run` hashes the §6 input identity in a separate 64 KiB-
  buffered pass so a 1M-event file is hashed without buffering it. The old
  `sha256_hex` is now a thin wrapper; FIPS vectors + a new chunked-vs-one-
  shot test (1-byte … 1000-byte chunkings, incl. the 1M vector) pin it.

### Verification (run before returning)

- **`--jobs 1` == `--jobs 8` byte-identical**, the §5 gate: new CLI test
  `parallel_run_is_byte_identical_to_serial` runs a 10,000-event (3-chunk)
  synthetic stream with float weights through 1-D/var-bin/2-D histos + a
  multi-step cutflow and byte-compares **histos.json, cutflow.json,
  out.root, make_histos.C, to_root.py, and the CSVs** across `--jobs 1`,
  `--jobs 8`, and a second `--jobs 8` run (scheduling independence).
  `parallel_stdout_stays_in_input_order` pins the 9000-event `--json` stream
  to ascending event order at `--jobs 8`.
- **Merge correctness, independent of the CLI**: `adl-interp/tests/
  merge_determinism.rs` — single-chunk merge == naive serial byte-for-byte
  (floats and all); fold at C=4096 reproducible; integer-weight cutflow ==
  serial for *any* chunk size (a transposed merge would corrupt these).
- **Existing determinism tests byte-identical with parallelism ON by
  default**: the whole prior CLI snapshot/golden suite passes unchanged —
  every fixture is ≤ 300 events (< C = 4096), so each run is one chunk and
  the merge-into-empty identity holds exactly. `cargo test --workspace`
  **509 passed / 0 failed** (was 503; +6: 2 parallel CLI, 3 merge, 1
  incremental-sha256). `-p adl-interp -p adl-cli --no-default-features`
  green.
- `cargo build --workspace` green; `cargo clippy --all-targets -- -D
  warnings` clean; rustfmt clean on every file touched this pass (the
  pre-existing whole-workspace fmt drift in untouched crates remains).

### Throughput (committed criterion bench, non-gating)

`cargo bench -p adl-difftest --features bench` (`benches/event_loop.rs`,
behind the off-by-default `bench` feature so criterion never enters the
normal build graph). Synthetic seeded events (the adl-difftest toy
generator) through the real loop primitives over ex02_histograms (2
selection regions, 9 histograms incl. a TH2D — the heavy end of the corpus).
Measured on this machine (12 logical cores, release):

- serial (`--jobs 1`):    ~55k events/s
- parallel (`--jobs 12`): ~306k events/s  (~5.6× scaling; **> 100k target**)

End-to-end measured separately via the release binary on a lighter ADL
(1 region, 3 histos), 1M synthetic events: **685k events/s** at default
jobs (1.46 s), 180k events/s at `--jobs 1`. The parse-off-lock change lifted
the ex02 parallel number from ~103k to ~306k (the JSON parse had been
serialized inside the reader mutex).

### Bounded memory (1M-event synthetic stream, documented measurement)

`/usr/bin/time -v ./target/release/smash2 run scale.adl events_1m.jsonl
--histos out --no-root` on a 172 MB / 1,000,000-event JSONL:

- `--jobs 1`:      peak RSS **19 MB**  (proves the reader never buffers the
                   172 MB file — O(one chunk))
- default (12):    peak RSS **147 MB** (O(jobs × chunk + reorder buffer +
                   accumulators), **not** O(file); under the §5 1 GiB bound)
- histos.json / cutflow.json **byte-identical** between `--jobs 1` and
  default jobs on the full 1M-event run.

(`--no-root` isolates the loop; out.root is buffered in memory before
finish by design — a separate, bounded cost.)

### Decisions / deviations (flagged)

- **std threads + mpsc, not rayon**: the loop is a bespoke producer/reorder-
  consumer with a fixed fold order; rayon's work-stealing would not improve
  on the cheap-lock design and adds a dependency. Kept zero-dep.
- **Parse off the reader lock**: required to scale (above). Determinism is
  unaffected — parsing is pure and the ordinal/line/chunk-index are assigned
  by the sequential raw reader before any parallelism.
- **Malformed-input on the parallel path** records the earliest-by-line
  error and exits 1; good chunks already folded/streamed before the error
  are discarded on the error return (the streamed partial stdout may vary by
  scheduling, but the exit-1 diagnostic — the only surviving output — is
  deterministic). Matches the old "first error wins, exit 1" behavior.
- **Per-event output numbering** uses the dense event ordinal (unchanged for
  the blank-line-free canonical fixtures; now also correct when blank lines
  are present, where the old `evs.iter().enumerate()` index already was).

## 2026-06-13 — Phase 10 real-sample end-to-end validation (SPEC_EVENT_PIPELINE §7)

Scope: no code changes — verification + documentation. Confirmed the wired
Phase-10 pipeline (cutflows, TH2D, variable-bin TH1D, per-region
`TDirectory`s, provenance, native Delphes ingestion, parallel loop) is fully
reachable from `smash2 run … --profile delphes --histos`, and ran the SPEC
§7 e2e on the real pinned Delphes sample. Produced `PIPELINE_REPORT.md`;
refreshed `README.md` (histograms section now documents varbin/TH2D +
TDirectories + cutflows + provenance + a real-data quickstart; status line
and test count updated to 510) and `PLAN.md` Phase 10 (Implemented callout).

### Real-sample e2e (genuine Delphes file, not a synthetic fallback)

Sample `/tmp/delphes_T2tt_700_50.root` was present with the exact pinned
sha256 `04fae8b1…` (71,452,474 bytes, tree `Delphes`, 20,000 events) — no
download needed; the run used the real CutLang tutorial file. All five SPEC
§7 assertions pass, each via an **independent oracle** (uproot 5.7.4 from
`.venv-uproot`; throwaway scripts in `/tmp/e2e/`):

1. **Ingestion fidelity**: native oxyroot JSONL == generated `to_jsonl.py`
   (uproot) JSONL, byte-identical, sha256 `e1a5499b…`, all 20,000 events;
   entry-0 probe values pinned (`Jet[0].pt=719.5091552734375`,
   `MET.pt=653.098876953125`). The committed env-gated test
   `delphes_sample_ingestion_fidelity_end_to_end` (SMASH2_RUN_DELPHES_E2E=1,
   SMASH2_DELPHES_SAMPLE) is green (83 s incl. the oracle subprocess).
2. **Cutflow correctness**: an independent uproot+numpy recompute of both
   regions matches `cutflow.json` raw counts exactly, all 10 steps
   (baseline 20000→16879→14194→10835→10309→8930; singlelepton inherit
   8930→1336; b-tag bit-0 mask, union lepton count, inherit-as-one-step all
   reproduced). errors=0.
3. **Distribution sanity**: hmet mean 424.9 GeV ∈[200,800], hnjets mode 4.5
   ∈[2,8], hjet1eta mean 0.0175 (|·|<0.2 symmetry), 0<flow<entries, pt axes
   non-negative, weighted==raw (all weights 1.0).
4. **Round-trip**: uproot read-back of `out.root` — cutflow TH1D bin labels
   are the verbatim statement texts, TH2D `hj1ptMET` flow-values total
   matches histos.json, `hmetvarbin` edges `[0,10,20,50,100,500]` preserved,
   `smash2_provenance` TNamed title parses as JSON with the matching
   ADL+input sha256s; `to_root.py` bridge histos byte-agree with native
   out.root (13/13, 0 mismatches).
5. **Determinism at scale**: `--jobs 1` ≡ `--jobs 8` ≡ rerun byte-identical
   across histos.json/cutflow.json/out.root/make_histos.C/to_root.py on the
   full sample.

`out.root` TDirectory layout confirmed: `baseline/` + `singlelepton/` each
holding their histos by bare name and the `__cutflow_raw`/`__cutflow_wt`
pair, `smash2_provenance` TNamed at the root.

### Throughput / memory (real-sample run, this machine)

`run --profile delphes --histos --json` (native 71 MB read + ingest-to-JSONL
+ eval over 2 regions/13 histos + 5 outputs): ~0.55 s / 187 MB at default
jobs (12), ~0.99 s / 135 MB at `--jobs 1` — read-bound (raw branch read
alone was ~685k ev/s in the §1.1 probe). The §5 ≥100k ev/s loop target is
met on the committed criterion bench (ex02 ~306k parallel; light ADL 685k on
1M synthetic). The native `ingest` of all 20k events ran in 0.13 s / 64 MB.

### Gates (run before returning)

- `cargo build --workspace` green; `cargo clippy --all-targets -- -D
  warnings` clean; `cargo test --workspace` **510 passed / 0 failed**;
  `corpus_gate.sh` 68/68; subprocess solver backend 8/0;
  `-p adl-interp -p adl-cli --no-default-features` 153/0; CLI insta
  snapshots current (no `.snap.new`). Env-gated oracles with `.venv-uproot`
  on PATH: `rootfile` uproot_oracle (ROOTFILE_REQUIRE_UPROOT=1) 9/0; cli
  root oracle (SMASH2_RUN_ROOT_ORACLE=1) incl. `ex02_out_root_read_back_by_
  uproot` green; ingest uproot oracle + delphes e2e green.

### Known gaps (carried, faithful)

- **[DECIDE-I4] unverifiable on this sample**: `Event.Weight` is 1.0 for all
  20,000 events, so weighted==raw and the branch choice cannot be
  distinguished — needs a weighted Delphes sample to ratify. The 20,000 LHE
  multiweights are reported-and-dropped (v1).
- Provenance `tool` carries no git hash (deterministic per build; injection
  seam deferred); `ingest -o` still writes no sibling
  `events.provenance.json`; NanoAOD/PHYSLITE spec'd, not built; mid-selection
  histoList fill points are honestly diagnosed (filled once on full region
  acceptance), not guessed.

---

## Solver-backend default flipped to subprocess (libz3 link no longer required)

The native libz3 backend was `default = ["native"]` on every backend crate
(adl-solver/adl-analysis/adl-cli), and `adl-difftest` pulled `adl-analysis`
with default features, so feature-unification re-enabled `native` across the
whole workspace. Net effect: `cargo build`/`cargo test` link-failed with
`-lz3` on any machine without system libz3, and the product binary needed
`LD_LIBRARY_PATH` at runtime. The recovery (`--no-default-features`) only
worked when scoped to `-p adl-cli`, and native test builds clobbered
`target/release/smash2` with a libz3-linked artifact that wouldn't launch.

Fix: the SMT-LIB **subprocess** backend is now the default everywhere
(`default = []`), so the default `cargo build` links nothing and runs against
a `z3`/`cvc5` binary on PATH. The in-process backend is opt-in:
`--features native` (system libz3) or the new `--features bundled` (libz3
built from vendored z3 source via the z3 crate, no system install). The
`native` cfg-gates are unchanged; `bundled` enables `native`.

- `cargo build --release` (clean env, no system libz3) — green; `ldd
  smash2` shows no libz3; binary runs.
- `cargo test --workspace` (subprocess, z3 on PATH, no RUSTFLAGS/LD_LIBRARY_PATH)
  — **60 suites / 0 failed**.
- `cargo build --release -p adl-cli --features native` and `cargo test
  --workspace --features native` — compile/run green.
- `corpus_gate.sh` builds clean without the hack: 125/125.
- One snapshot updated: `report_rendering__default_cms_sus_16_033` — the
  default backend now runs the local z3 *binary* (4.12.2) instead of the
  borrowed native lib (4.16); the two z3 versions return different SAT
  witnesses, so 7 pairs that were POSSIBLY (rejected witness) under 4.16 are
  validated PROVEN OVERLAPPING under 4.12.2 (all 28 ProvenOverlapping carry
  witness_validated=true; PROVEN DISJOINT unchanged — no soundness change).
  Report-rendering snapshots track the local z3 version's witness choices.
