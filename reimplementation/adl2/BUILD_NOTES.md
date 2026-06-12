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
