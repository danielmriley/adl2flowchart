# Review — recent commits (Track C reconciliation keystone)

Date: 2026-07-01
Range: `HEAD~8..HEAD` (2e8fa55 newest → 9f05e70 oldest)
Reviewers: six lenses (recon-soundness, witness-realizer, verdict-reporting, cli-build, tests-coverage, code-quality), each finding adversarially verified against the built binary and the source.

---

## 1. Executive summary

The Track C keystone — cross-file reconciliation that emits `size(A) <= size(B)` facts (XSUB/XEQ) to unlock `PROVEN DISJOINT`/subset across analyses — is **substantially sound and the suite is green**, but it shipped with **one confirmed critical soundness hole** and a cluster of medium reporting/test defects. The two soundness fixes the prompt cites (concrete-peer leak, private-base collision) landed correctly and are pinned by regression tests. The new machinery is exercised by exactly five hand-picked micro-tests, and that thinness is the dominant risk theme running through the findings.

The critical issue (F1) is a **fabricated detector-base identity**: when an object block's `take` source fails to resolve (an unsupported call like `take antikT(Jet, 0.4)`, or no `take` at all — both present in the real corpus), `resolve_object` falls back to `Collection::Base(self_sym)` — the block's *own* name. Because symbol interning is case-insensitive and `ext_objs.txt` contains block-style spellings (`JETclean`, `MUOclean`, …), such a block's cuts get silently re-attributed to the *detector* base of that name, and reconciliation's only guard (`ext.base_collection(...).is_none()`) passes the fabricated base. This was reproduced end-to-end producing a **false `PROVEN DISJOINT` via XSUB** with no witness safety net. It is strictly worse than the documented "same ext name = same input" residual, because file A never uses the ext input at all — the tool invents the identity *after knowingly dropping* the take source (and even emits an `Unsupported` diagnostic that the default `--cross` output swallows). This must be fixed before the keystone can be trusted on real cross-analysis inputs.

Beyond F1, the remaining risks are non-soundness but still material to the tool's headline `--cross` mode: the "N of M pairs span two analyses" summary reports the **exact inverse** on colliding file basenames (made plausible by the new directory expansion), `--fail-on=overlap` fires **exit 4 on fabricated self-pairs** when a file is listed alongside its own directory, the flagship `--explain` XSUB line renders a self-referential `size(jets) <= size(jets)`, and the `PROVEN OVERLAPPING` witness display can show a physically-impossible pT-ascending event labeled "validated by interpreter." None of these fabricate a false PROVEN, but they undermine the auditability that is the project's whole selling point. The reconciliation path also has **zero difftest-oracle reach and no corpus regression net** — so the planned `abs(x)<c` precision unlock (which will greatly expand real-corpus firing) would land with no baseline to diff an unsound expansion against.

**Verdict: healthy foundation, one must-fix soundness hole (F1), then close the reporting-honesty and test-coverage gaps before the precision unlock expands the blast radius.**

---

## 2. Scope & method

**Commits reviewed (newest first):**

| SHA | Summary |
|-----|---------|
| 2e8fa55 | reconciliation fact emission (keystone Steps 4-5): new `reconcile.rs`; `reconcile()`/`prove_pred_implies()`/`frame_sat()`/`existing_size_le()`; `AxiomId::Xsub`/`Xeq` + `derived_size_le`; 5 tests |
| f460f82 | `encode_elem_pred_generic` + `GENERIC_INDEX` (shared-element encoder) |
| 139a257 | `verify` accepts directories (`expand_adl_inputs`) |
| 9845994 | `filter_chain` + `reconciliation_candidates` (quantity.rs) |
| c8fb411 | witness realizer inverts dR + pT-descending normalize |
| c1fc598 | cross-file summary separates cross/intra pairs |
| 0c9c69e | Track A UX/soundness: gate diagnostics, warn on no solver, pin pT-descending |
| cd0f20e / cd0f88 | CANDIDATE OVERLAPPING verdict tier |
| 9f05e70 | subprocess solver backend default |

**Method:** six independent review lenses; every finding was re-verified adversarially against the code and, where behavioral, against the release `smash2` binary (z3 4.12.2). Raw confirmed findings: **34**. After merging duplicates that surfaced under multiple lenses: **27 distinct** — **1 critical, 10 medium, 16 low**. **0 refuted.** Every listed finding carries `verdict=confirmed`.

---

## 3. Findings

Ranked by severity, then impact. Merged findings cite all contributing lenses.

### F1 — [CRITICAL, soundness] Fabricated detector-base identity yields a false `PROVEN DISJOINT`
`reimplementation/adl2/crates/adl-analysis/src/reconcile.rs:105` — lens: recon-soundness

**What/where.** `resolve_object` (`adl-sema/src/resolve.rs`) falls back to `Collection::Base(self_sym)` — the object block's own name — when its `take` source fails to resolve (line ~483: a non-sort call such as `take antikT(Jet, 0.4)` sets `unsupported_reason` and drops the source) or when the block has no `take` at all (line ~531; this case does not even set `Fragment::Unsupported`). Symbol interning is case-insensitive (`intern.rs:24`), `ExtDecls::base_collection` matches case-insensitively (`ext.rs:167-171`), and `legacy_parser/adl/ext_objs.txt` declares block-style spellings (`JETclean`/`MUOclean`/`ELEclean`, …). So the fabricated base passes reconcile's sole guard at `reconcile.rs:105` (`ext.base_collection(display(base_sym)).is_none()`), and the `Fragment::Unsupported` tag has **zero consumers** anywhere in `adl-analysis`/`adl-cli`.

**Evidence.** Reproduced on the built binary: file A `object JETclean / take antikT(Jet,0.4) / select pt > 100` + `region sra: size(JETclean) >= 4`; file B `object cleanjets / take JETclean / select pt > 30` + `region srb: size(cleanjets) <= 1` under `verify --cross` →
`PROVEN DISJOINT — … (using axiom XSUB (size(jetclean) <= size(cleanjets)))`.
The commented-out-`take` variant reproduces identically. The fact is **false** — A's re-clustered `pt>100` jets can number ≥4 while B's `pt>30` filter of the JETclean input has ≤1. The triggering shape is corpus-real: `examples/Examples/CMS-SUS-16-037.adl:25` has `#  take antikT(jets, 1.4)`.

**Impact.** Fabricated false `PROVEN DISJOINT` asserted on the UNSAT side at the **persistent frame** (`engine.rs:941-950`, no push/pop) — DISJOINT has no witness re-validation net — and the same fact also feeds region-empty/subset/bin checks. Strictly worse than the documented residual (file A never consumes the ext input; the identity is invented after the take source is dropped, and the diagnostic is swallowed in `--cross`).

**Verification.** Confirmed end-to-end (code trace + live binary), confidence 0.95.

**Fix.** Fail closed *at the source*: in `resolve_object`, when `unsupported_reason.is_some()`, when `sources` is empty, and in the take-cycle `State::InProgress` arm (`resolve.rs:411-421`), intern a **unit-unique private base symbol** (e.g. `<unit>::<name>#unresolved`) instead of `Base(self_sym)`, and set `Fragment::Unsupported` on the no-take case. Narrower alternative: in `reconcile::build`, skip any candidate whose chain contains an `Unsupported`-tagged or no-take/cycle-fallback collection. Add regression tests mirroring `reconcile_skips_private_base_name_collision` for the unsupported-take-call, no-take, and take-cycle shapes with an ext-spelled block name.

---

### F2 — [MEDIUM, bug] `PROVEN OVERLAPPING` witness display can show an unphysical event labeled "validated by interpreter"
`reimplementation/adl2/crates/adl-analysis/src/witness.rs:527` — lens: witness-realizer

**What/where.** Phase 2.75 (`witness.rs:524-533`, commit c8fb411) sorts each base pT-descending **after** phase-1 elem pins wrote model values by index, permuting which element sits at each index. `validate_witness` checks membership on the post-sort JSON, but on success `engine.rs:606` reports `report.witness = witness_values(hir, &model, …)` — the **pre-sort** model values — and `report.rs:381-386` renders them with `[witness validated by interpreter]`.

**Evidence.** Live: `object bigjets = Jet | pt>100`; region A `size(Jet)>=2, pt(Jet[0])>20`; region B `size(Jet)>=2, size(bigjets)>=1` → `witness: JET[0].pt = 21.0…, size(JET)=2, size(bigjets)=1 [witness validated by interpreter]`. That set is physically impossible (a `pt>100` jet forces `pt(JET[0])>100`; the loader's `validate_pt_descending` at `adl-interp/src/event.rs:548` rejects it). The actually-validated event is the post-sort `Jet=[101, 21.0…]`. Deterministic on the first solver model.

**Impact.** Misleading human/JSON audit output on the flagship `PROVEN OVERLAPPING` artifact. The **verdict stays true** (the sorted event is real), and `adl-difftest` consumes only the `witness_validated` flag — so no false PROVEN. Pre-commit the correspondence was exact (ascending builds were loader-rejected wholesale).

**Verification.** Confirmed live, confidence 0.7.

**Fix.** Derive displayed witness rows from the **validated artifact**: have `build_event_json` return the post-sort structure and recompute each mentioned `ElemProp`/`AngularSep`/`Size` from it (or re-evaluate each mentioned quantity on the validated event via the interpreter before filling `report.witness`). At minimum, detect a non-no-op sort and refresh the affected `pt(...)` rows.

---

### F3 — [MEDIUM, bug] Colliding file basenames break unit identity: cross/intra summary inverts and region labels become indistinguishable
`reimplementation/adl2/crates/adl-analysis/src/render.rs:408` and `reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:202` — lenses: verdict-reporting + cli-build *(merged)*

**What/where.** Units are named by `unit_name(path)` = the **file basename only** (`adl-cli/src/cmd/mod.rs:54`). `merge_hirs` labels merged regions `{unit}::{region}` (`merge.rs:128`); `pair_is_cross` (`render.rs:408-413`, new in c1fc598) classifies a pair as cross-analysis by comparing the prefix before the first `::`. The codebase itself documents the name as collidable (`merge.rs:41-42`: "only a file basename and can collide") and uses `unit_ord` internally for exactly this reason — but the summary line and region labels use the raw name.

**Evidence.** `verify --cross vA/atlas.adl vB/atlas.adl` (two different analyses, same basename, 3 pairs, 2 genuinely cross-file) → `cross-file: 0 of 3 pairs span two analyses … the other 3 are intra-analysis` (the exact inverse); regions table lists `atlas.adl::SRA` twice; pairwise line reads `x.adl::SR vs x.adl::SR … subset: x.adl::SR within x.adl::SR` (direction uninterpretable, also in `--json`). Filenames containing `::` (`x::y.adl` vs `x::z.adl`) split to the same prefix and misclassify identically. Directory expansion (139a257) makes same-basename inputs realistic (`--cross runA/ runB/` each holding `analysis.adl`).

**Impact.** Display + machine-readable identity, no soundness impact (region resolution is by index; `unit_ord` namespacing holds in the disjoint and overlapping repros). But the headline line whose stated purpose (c1fc598) was to make cross-ness honest reports the exact inverse on supported, increasingly plausible inputs.

**Verification.** Confirmed live (both symptoms), confidence 0.9–0.95.

**Fix.** Classify by **unit identity, not the collidable name prefix**: thread the unit ordinal into `RegionReport`/`PairReport` during merge and partition on that. Additionally disambiguate colliding basenames before `merge_hirs` (qualify with parent dir or shortest distinguishing path suffix, or append `#<ord>`), which also fixes the ambiguous duplicate `atlas.adl::SRA` rows in the table and matrix.

---

### F4 — [MEDIUM, docs] CANDIDATE tier, XSUB/XEQ axioms, and `--fail-on` semantics never entered the normative spec/README; `SCHEMA_VERSION` not bumped
`reimplementation/SPEC_ANALYSIS.md:38` and `reimplementation/adl2/crates/adl-analysis/src/report.rs:9` — lenses: verdict-reporting + code-quality *(merged)*

**What/where.** cd0f20e added `VerdictKind::CandidateOverlapping` and made `PROVEN OVERLAPPING` additionally require interpreter re-validation (`engine.rs:607` vs `615`); 2e8fa55 added `AxiomId::Xsub`/`Xeq`. None of the docs followed:
- `SPEC_ANALYSIS.md` §2 verdict table (lines 36-44) still defines `PROVEN OVERLAPPING` as bare `SAT(Ax ∧ A⁻ ∧ B⁻)` with no CANDIDATE row; §4 axiom catalog (ORD/SZ0/SUB/UNI/NNEG/DPHI/TAG/TWIN/EPRED/IDOM) has **no XSUB/XEQ rows**, though `adl-axioms/src/lib.rs:130` cites "SPEC_ANALYSIS §4" as its normative source and the XSUB/XEQ rows are the audited soundness argument.
- `adl2/README.md` "Reading verdicts" (lines ~140-146) lacks the tier; README never mentions directory inputs (139a257) nor `--cross` reconciliation/derived size facts (2e8fa55).
- `docs/SOUNDNESS_HARDENING.md:127-135` still lists "opaque-external candidate overlaps are labeled/aggregated as PROVEN OVERLAPPING" as a residual — **the exact thing cd0f20e fixed** (now misinforms auditors in the opposite direction).
- `--fail-on=overlap` now fires on `CandidateOverlapping` (`report.rs:212-224`; confirmed exit 4 with `candidate overlap: SR_orcut vs SR_lowmet` on `or_unencodable_branch.adl`). Failing closed is the correct conservative choice, but no doc (main.rs help, README, QUICKSTART, §6) says the overlap gate includes unvalidated candidates, so a CI user gating on "proven overlaps" cannot learn or opt out.
- `SCHEMA_VERSION` is still `1` (`report.rs:9`, doc: "Bumped on any breaking schema change") while the versioned JSON `kind` field gained a new value `"candidate_overlapping"`; a candidate pair was previously `proven_overlapping`, so a consumer summing proven overlaps silently changes meaning across the same version.

**Impact.** Docs-only, fail direction conservative — but the drift is in the normative soundness spec the code cites as its audit trail, and per cd0f20e most real-corpus overlaps are candidate-kind, so CI users hit undocumented exit-4 with no documented opt-out.

**Verification.** Confirmed (grep + binary), confidence 0.85.

**Fix.** Add a CANDIDATE OVERLAPPING row to `SPEC_ANALYSIS §2` and README's verdict table (definition: SAT but witness not interpreter-validatable due to opaque quantity; not a proof); add XSUB/XEQ rows to §4; note in `--fail-on` docs that `overlap` includes the candidate tier (fail-closed rationale); update the `SOUNDNESS_HARDENING.md` Status paragraph; add a README sentence on directory inputs and `--cross` reconciliation; and either bump `SCHEMA_VERSION` to 2 or document that `kind` is an open set consumers must treat as extensible.

---

### F5 — [MEDIUM, bug] `expand_adl_inputs` does not dedup; a file listed with its own directory fabricates a self-pair that fires `--fail-on=overlap` (exit 4)
`reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:47` — lens: cli-build

**What/where.** `expand_adl_inputs` appends directory contents and explicit files with no dedup/canonicalization (`out.extend(found)` line 63, `out.push(p.clone())` line 65). Nothing downstream dedups.

**Evidence.** `verify --cross dirA/ dirA/x.adl --fail-on=overlap` merges the same unit twice (header `x.adl + x.adl`, every region listed twice) and emits fabricated self-pairs `x.adl::highMET vs x.adl::highMET … PROVEN OVERLAPPING — mutual subset: the regions provably coincide`, firing the gate and **exiting 4**; the same input without the duplicate exits 0. `--cross dirA/ dirA/` (directory twice) also exits 4. Non-cross `verify dirA/ dirA/x.adl` prints the file under two identical `==== x.adl ====` headers.

**Impact.** Not a soundness violation (the self-pair verdict is trivially true and fails toward safety), but it **breaks the documented `--fail-on` gate semantics** under a plausible invocation (`--cross analyses/ analyses/new.adl`) — a false CI failure — and pollutes the report/summary (self-pairs count as intra-analysis, degrading the F3 summary too).

**Verification.** Confirmed live (4-vs-0 exit delta), confidence 0.92.

**Fix.** Dedup in `expand_adl_inputs`: `std::fs::canonicalize` each expanded path (fall back to the given path on error), keep the first occurrence preserving order, optionally warn on stderr when a duplicate is dropped.

---

### F6 — [MEDIUM, test-gap] Directory expansion and the `--cross → reconcile=true` CLI wiring are entirely untested
`reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:47` — lenses: cli-build + tests-coverage + code-quality *(merged)*

**What/where.** `expand_adl_inputs` (lines 47-69, added 139a257) has no `#[cfg(test)]` module and no test in `adl-cli/tests/` passes a directory to `verify`. Untested: sorted deterministic expansion, `.adl` filtering (`.txt`/`.md` ignored), **non-recursive** semantics (nested `.adl` silently ignored — a `verify --cross analyses/` over a tree gets a partial merge with no warning), the empty-dir usage error, mixed file+dir lists, and the dedup/collision cases from F5/F3. Separately, `reconcile: true` is set in production **only** at `verify.rs:220` (`run_cross`); all five reconciliation tests hand-build `opts_reconcile()` and call `analyze_hir` directly, so if the CLI wiring were dropped `verify --cross` would silently revert to all-POSSIBLY while the whole suite stayed green (the sole `--cross` CLI test uses `--no-solver`).

**Impact.** The new user-facing path feeding the flagship `--cross` flow — and the reconcile-flag wiring — sit in a workspace whose convention is snapshot/golden coverage of every CLI behavior. Failure mode is silent feature loss (weaker, still-sound verdicts), never a false PROVEN.

**Verification.** Confirmed (grep + binary; all behaviors reproduced), confidence 0.75–0.9.

**Fix.** Add `adl-cli/tests/cli.rs` tests: (a) `verify <tmpdir>` with two `.adl` + one `.txt` + a nested subdir asserts the two top-level files in **sorted order**, `.txt`/nested excluded; (b) `verify --cross <tmpdir>` on two keystone files asserts the **XSUB row / PROVEN DISJOINT** appears (covers both directory expansion and the reconcile wiring); (c) empty dir asserts exit 2 and the "no .adl files" message; (d) dir + file-inside-dir asserts deduped behavior once F5 lands.

---

### F7 — [MEDIUM, test-gap] The XEQ (both-directions) reconciliation arm has zero test coverage
`reimplementation/adl2/crates/adl-analysis/src/engine.rs:925` — lens: tests-coverage

**What/where.** The `a_in_b && b_in_a` arm of `Engine::reconcile` (`engine.rs:923-927`) emits two `derived_size_le` facts under `AxiomId::Xeq`. The only test-side mention of Xeq (`adl-axioms/tests/axioms_hold.rs:194-204`) explicitly **exempts** Xsub/Xeq from the full-catalog vocabulary invariant and defers to "the cross-file reconciliation tests" — but none of the five reconcile tests (`cross_file.rs:301-414`) constructs two logically-equivalent-but-structurally-distinct filter chains (`reconciliation_candidates` pairs only distinct interned `CollectionId`s, and identical chains share an id), so the XEQ arm, its double emission, its per-id counting into `axioms_used` (`engine.rs:267-269`), and its `--explain` row never execute. No test asserts `report.axioms_used` at all.

**Impact.** Completeness/reporting gap, not a false-PROVEN path — each direction is separately proven UNSAT-side, and the single-direction misuse is pinned by `reconcile_is_directional_no_false_proven`. But XEQ emits UNSAT-side facts feeding PROVEN verdicts and is currently unpinned against regression.

**Verification.** Confirmed; the arm works correctly today (verified live), so this is a pinning gap plus a stale "covered by reconciliation tests" comment. Confidence 0.9.

**Fix.** Add a `cross_file.rs` test with equivalent-but-byte-distinct chains (e.g. file a `select pt > 30` vs file b `select pt > 20; select pt > 30`), assert disjointness proofs fire in **both** orientations, and assert `report.axioms_used` contains an XEQ entry with the expected instance count. (The redundant-second-cut fixture is ready-made.)

---

### F8 — [MEDIUM, test-gap] The difftest oracle has zero reach over the reconcile path, and TESTING.md over-claims coverage
`reimplementation/adl2/crates/adl-difftest/src/oracle.rs:44` — lens: tests-coverage

**What/where.** Every difftest entry point passes `AnalysisOptions::default()` (`reconcile: false` — `regressions.rs:25`, `prop_encoder_vs_interp.rs:59`, `p2_reducer_slice.rs:35`, `metamorphic.rs:54`), and `run_case` analyzes a single generated unit, so the interpreter-vs-analyzer oracle — the project's strongest anti-false-PROVEN net — can **never** observe a derived XSUB/XEQ size fact. `grep -i reconcil` over `TESTING.md` and `SPEC_ANALYSIS.md` returns nothing; worse, `TESTING.md` claims "each catalog axiom holds on every generated physical event," yet Xsub/Xeq are catalog axioms (`AxiomId::ALL`) explicitly exempted in `axioms_hold.rs` because `emit_axioms` never emits them.

**Impact.** The designated anti-false-PROVEN net has no reach over the newest UNSAT-side fact emitter, and the strategy doc both omits and over-claims. The entire reconciliation soundness net is 5 hand-picked scenarios.

**Verification.** Confirmed (code inspection), confidence 0.85.

**Fix.** Document in `TESTING.md` that reconciliation is outside the oracle's reach (single-unit generator, reconcile off) and is covered only by targeted `cross_file` tests. Roadmap: add a generator mode emitting two filtered objects over one base with `reconcile: true`, plus an oracle check that any pair with a derived subset claim satisfies `passes(A) ⇒ passes(B)` on every sampled event (reconciliation candidates arise intra-unit, so this fits the existing single-unit oracle).

---

### F9 — [MEDIUM, test-gap] `realize_dr` branch matrix and the pT-descending normalization have no unit tests; coverage is z3-model-dependent
`reimplementation/adl2/crates/adl-analysis/src/witness.rs:704` — lens: tests-coverage

**What/where.** `witness.rs` has no `#[cfg(test)]` module. `realize_dr`'s branch matrix (one-eta-pinned fills at `:747-748`, both-phi-pinned-unequal early return `:731-732`, pinned-vs-free phi fill `:754-758`, the `v<0`/non-finite guard `:712-714`) plus the phase-2.75 stable pT-descending sort (`:517-533`) execute **only when the solver returns a model of that shape** — coverage varies with z3 version/seed. The only deterministic end-to-end pins (`features-angular_09`/`_10`, and one Met-early-return via a cross-file test) exercise the `(None,None)` happy path; the pinned-eta branches, conflicting-phi return, pinned-phi fill, v-guard, and the sort's actually-reorders case are covered by **zero** tests.

**Impact.** All misses fail open (downgrade to POSSIBLY), so no soundness exposure — but the golden harness pins **exact** verdicts, so an uncovered branch flipping behavior surfaces as a **machine-dependent golden flake**. One uncovered branch (F13) carries a real latent boundary-exactness weakness.

**Verification.** Confirmed; `realize_dr`'s signature (Locs + built map + keys, no `Model`) makes hand-built-map unit tests trivially feasible. Confidence 0.8.

**Fix.** Extract unit tests driving `realize_dr` over a hand-built `built` map: each pinned/free eta+phi combination, mismatched pinned phis (assert no clobber), negative/NaN v; plus a `build_event_json`-level test that an ascending-pt model serializes descending and an already-descending one is byte-identical (stable no-op).

---

### F10 — [MEDIUM, test-gap] No corpus-level regression net for reconciliation verdicts; the golden harness cannot express cross-file or CANDIDATE pins
`reimplementation/adl2/crates/adl-analysis/tests/golden_regions.rs:43` — lens: tests-coverage

**What/where.** `golden_regions.rs` runs each `examples/golden` file single-file with `reconcile: false` (`opts()` :24-31), and `expected_kind` (:43-50) accepts only `DISJOINT|OVERLAPPING|POSSIBLY` — no CANDIDATE token, no cross-file pair syntax. Reconciliation-derived verdicts are pinned nowhere outside the five micro-tests in `cross_file.rs`. `axioms_hold.rs:194-198` even exempts Xsub/Xeq from the axioms-hold-on-generated-events check, so those UNSAT-side facts have neither randomized-event validation nor a corpus pin.

**Impact.** When the `abs()`-opacity fix lands (top roadmap item), real-corpus reconciliation firing will expand with **no pinned baseline and no oracle** — an unsound expansion would be invisible to both the golden suite and the corpus sweep. (Golden dir integrity is otherwise fine: 57 files, all with GOLDEN pins, clean status.)

**Verification.** Confirmed, confidence 0.8.

**Fix.** Add an `examples/golden/cross/` subcorpus of paired files with a `GOLDEN-CROSS` pin syntax (and a CANDIDATE token in `expected_kind`), driven by a harness that merges with `reconcile: true` — pinning at least one reconciliation-proven DISJOINT and one deliberately-blocked (opaque conjunct) POSSIBLY pair.

---

### F11 — [MEDIUM, quality] The flagship XSUB/XEQ `--explain` line renders a self-referential `size(jets) <= size(jets)`
`reimplementation/adl2/crates/adl-analysis/src/engine.rs:943` — lens: code-quality

**What/where.** `Engine::reconcile` builds the `CoreItem::Axiom` statement via `adl_axioms::quantity_label`, whose `collection_label` (`lib.rs:405`) returns the **first** `coll_names` entry — which `merge_hirs` deliberately does **not** namespace (`remap_coll` unions raw symbols). So two files each naming their filtered collection `jets` produce distinct `CollectionId`s with identical labels. The merged-unit cut text uses the *other* renderer (`RenderCtx::coll` → disambiguated `C{id}#name`), so the new XSUB statement is inconsistent with its own surrounding cut labels.

**Evidence.** Live `verify --cross --explain` on the keystone scenario:
`PROVEN DISJOINT — UNSAT core: a.adl::RA line 1: (size(C1#jets) >= 3) cannot hold together with b.adl::RB line 1: (size(C2#jets) <= 2) (using axiom XSUB (size(jets) <= size(jets)))`.
The derived fact reads as a self-referential tautology; its direction (which file's `jets` is the subset) is unrecoverable — in the one report line whose entire purpose is explaining the cross-file proof.

**Impact.** No soundness impact (solver facts are keyed on ids), purely explanation quality — but it hits the marquee cross-file scenario in `--explain`.

**Verification.** Confirmed live, confidence 0.95.

**Fix.** Render the XSUB/XEQ statement (`engine.rs:943-947`) with the same disambiguated labeling the cut text uses (the `C{id}#name` form, or prefix `size(...)` labels with the `CollectionId`). `RenderCtx` is `pub(crate)` in `adl-sema`, so this needs either a public quantity renderer (like `render_node`) or id-prefixed labels in `adl-axioms`.

---

### F12 — [LOW, soundness] `GENERIC_INDEX` (`u32::MAX`) is reachable from source via index clamp, voiding the "free generic element" invariant
`reimplementation/adl2/crates/adl-axioms/src/lib.rs:1137` — lens: recon-soundness

**What/where.** The `GENERIC_INDEX` doc claims `u32::MAX` "is unreachable by any real element access," and `reconcile.rs:83-86` requires `build` to run after `emit_axioms` so the generic element gets no base axioms. But `resolve.rs:330` does `u32::try_from(v.value).unwrap_or(u32::MAX)`: any source index ≥ 2³² (e.g. `Jet[5000000000].pt`) — or exactly `Jet[4294967295]` — clamps silently to `FromFront(u32::MAX)`, interning the **exact** id reconciliation treats as the universally-quantified generic. That quantity then exists before `emit_axioms`, joins the mentioned set, and receives ORD/IDOM/TAG instances, so the subset frames quantify over a **constrained** element.

**Evidence.** Live: a region with `pT(Jet[0])>0` and `pT(Jet[5000000000])>0` alongside the keystone pair reports `ORD×1` (`pT(Jet[0]) >= pT(Jet[4294967295])`) **and** `XSUB×1` in the same run. Two distinct ≥2³² indices also silently merge (`Jet[5e9]` vs `Jet[6e9]` → `PROVEN DISJOINT — JET[4294967295].pt: (100, inf] vs [-inf, 50)`).

**Impact.** Invariant violated, but **no physically-false PROVEN is constructible** (EPRED constants sit behind an always-escapable size guard and target Filtered collections only; base elements get only one-sided bounds by constant-free variables; the merged-index DISJOINT is vacuously true since both regions require `size(Jet) > 4294967295`). Adversarial directional tests stayed POSSIBLY. Hence low, not a demonstrated false PROVEN.

**Verification.** Confirmed mechanism end-to-end; severity downgraded to low on the constructibility analysis. Confidence 0.7.

**Fix.** Make the sentinel unrepresentable: add an `ElemIndex::Generic` variant, or reject/diagnose out-of-range indices at resolve into an `Unsupported` node instead of `u32::MAX`. Minimal defensive fix: in `reconcile::build`, check `hir.table.quantity_id` for each generic `ElemProp` before interning and fail closed if the id already existed pre-`emit_axioms`; add a `debug_assert` + regression test with an index ≥ 2³².

---

### F13 — [LOW, bug] `realize_dr` pinned-eta branches skip the fix-point correction, producing one-ulp dR misses at cut boundaries
`reimplementation/adl2/crates/adl-analysis/src/witness.rs:747` — lens: witness-realizer

**What/where.** `(None, Some(eb)) => set(eta_a, eb + v)` and `(Some(ea), None) => set(eta_b, ea - v)` (`:745-753`) assume the interpreter's `dEta = fl(fl(eb+v) - eb)` reproduces `v`, but the f64 round-trip is inexact (~28% of typical 2-decimal (eta, dR) pairs). The DEta/DPhi path in `realize_angulars` applies a 4-iteration `correct()` fix-point loop (`:663-691`) for exactly this mode; `realize_dr` omits it, and the doc-comment's exactness claim only holds for the `(None,None)` branch.

**Evidence.** Live: `select eta(jets[1]) == 0.3` pinned with touching bands `dR(jets[0],jets[1]) >= 0.4 / <= 0.4` → interpreter dR `0.39999999999999997` → witness rejected → `POSSIBLY OVERLAPPING` (plus a spurious INTERNAL DIAGNOSTICS banner since both regions are exact). A true witness exists (`eta0 = fl(0.3-0.4)`, equal phi → dR `0.4` bit-exact).

**Impact.** Downgrade on the SAT side only — no soundness exposure. Mitigated by `snap_model`'s 2⁻²² grid (exact on the second-chance pass) and `WITNESS_EPS`; residual bites only boundary-forced models (equality atoms / exactly-touching dR bands) with a pinned non-dyadic eta — sparse in real corpora.

**Verification.** Confirmed live, confidence 0.8.

**Fix.** Apply the sibling branch's fix-point correction — **but** the `+`-side correction is insufficient near `x=eb+v` (the x-grid ulp is coarser than the dEta-target ulp), so the corrector must also try the `eb - v` direction (|dEta|=v allows either sign), where the finer binade round-trips bit-exact. Fix the doc-comment to scope the exactness claim to `(None,None)`, and add a behavior test with a pinned non-dyadic eta and a touching dR band.

---

### F14 — [LOW, perf] dR realization never uses the dPhi degree of freedom, so dEta-pinned + upper-bounded-dR overlaps exhaust retries and file spurious INTERNAL diagnostics
`reimplementation/adl2/crates/adl-analysis/src/witness.rs:746` — lens: witness-realizer

**What/where.** `realize_dr` forces `dPhi=0` and returns entirely when both etas are pinned (`:746 (Some(_), Some(_)) => return`); the DEta branch likewise skips doubly-pinned anchors. So for any pair mentioning both `dR(a,b)` and `dEta(a,b)`, every realized event has `dR = |dEta|` regardless of `model.iter()` order. No axiom relates dR to its components, so the solver picks them independently; the blocking clause excludes only single points, so all `MAX_WITNESS_ATTEMPTS` reject.

**Evidence.** Live: `select dEta(j0,j1) [] 0.5 1.0` vs `select dR(j0,j1) [] 1.2 2.0` (genuinely overlapping; real witness needs dPhi≠0) → `POSSIBLY OVERLAPPING`, realized event has `phiof=0.0` on both jets, and `INTERNAL: witness validation failed for RA vs RB` appears on stderr + JSON — the exact spam class c8fb411 claims it "drops at its source" (exit code stays 0).

**Impact.** Downgrade to POSSIBLY is sound; the cost is completeness loss plus dilution of the genuine encoder/interpreter-contradiction signal.

**Verification.** Confirmed live, confidence 0.5.

**Fix.** When one/both etas are pinned and `|dEta_current| <= v`, realize the remainder through phi: `wrap(dPhi) = sqrt(v² - dEta²)` on a free phi (with the fix-point correction), instead of returning. Alternatively classify structurally-unrealizable dR/dEta combinations as **quiet** downgrades rather than INTERNAL diagnostics, and add a behavior test for the dEta-band + dR-band pair.

---

### F15 — [LOW, quality] Matrix/legend letter `c` is the only uncolored verdict letter in color mode
`reimplementation/adl2/crates/adl-analysis/src/render.rs:78` — lens: verdict-reporting

`Style::letter` (`:70-81`) has arms for `D O s ? U E` but not `c`, so the new candidate letter falls through `_ => return c.to_string()` uncolored, while `Style::verdict` maps `CandidateOverlapping` to ANSI 36. Confirmed via pty: legend/matrix show a plain `c` beside green `D` (matrix needs 3-20 regions to appear). Cosmetic, tty-only. **Fix:** add `'c' => "36",` (matches the verdict color; note `E` already uses 36).

### F16 — [LOW, test-gap] CANDIDATE tier wire format, `--fail-on` gating, the cross-file summary line, and the oracle branch are untested
`reimplementation/adl2/crates/adl-analysis/src/report.rs:220` and `tests/golden_battery.rs:508` — lens: verdict-reporting + tests-coverage *(merged)*

No test/snapshot contains the JSON value `"candidate_overlapping"` (the sole `--json` snapshot is a disjoint file), so a serde rename passes the suite. The `--fail-on` `"candidate overlap:"` branch (`report.rs:219-221`) is unexercised (both overlap-gate tests use a `ProvenOverlapping` file). The difftest oracle's `CandidateOverlapping` consistency check (`oracle.rs:120-134`) is unreachable by the opaque-free generator and has no synthetic `CaseRun`. The `pair_is_cross` / "N of M pairs span two analyses" summary (`render.rs:384-413`) has **zero** coverage — which is why the F3 misclassification shipped unnoticed. **Fix:** snapshot `verify --json` over `or_unencodable_branch.adl` (pins the wire value); add a `--fail-on=overlap` CLI test over it asserting exit 4 + the `candidate overlap:` message; add a `check_sound` unit test feeding a synthetic `CandidateOverlapping` with `witness_validated=Some(true)` to assert the mislabelling error fires; assert the cross-file split line for a two-unit merge (and a colliding-basename case once F3 lands).

### F17 — [LOW, hygiene] In-repo corpus-sweep skill silently drops the candidate tier from its totals
`.claude/skills/adl2-corpus-sweep/SKILL.md:72` — lens: verdict-reporting

The tracked skill's aggregation (`:71-79`) greps only `proven disjoint|proven overlapping|possibly overlapping|unknown|pairs`; since cd0f20e the default summary inserts `, N candidate overlapping` (`render.rs:362-378`, fires on **default** runs, not just `--cross`). Running the skill's exact pipeline on 4 real corpus lines reports `pairs=598` but buckets sum to 461 — 137 candidate pairs vanish, so totals no longer reconcile and the tier the line-125 guardrail needs is invisible in aggregates. The sample line at `:56` and the baseline table (`:139-149`) are stale. **Fix:** add `[0-9]+ candidate overlapping` to the alternation plus an awk bucket (anchor/order it before the `/overlapping/`-substring buckets), refresh the sample line, re-derive the baseline table.

### F18 — [LOW, docs] Docs and the no-solver warning claim cvc5 works, but no code path probes or invokes cvc5
`reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:38` — lens: cli-build

The warning (`:38`, 0c9c69e) and docs (`QUICKSTART.md:14`, `README.md:34`, `adl-solver/Cargo.toml:11`) say a `z3` **or `cvc5`** binary on PATH works. But `subprocess_solver()` (`adl-analysis/src/lib.rs:130-138`) probes only `subprocess_available("z3")` and builds `SubprocessSolver::z3()`, invoking with z3-only flags `-in -t: -T:` that cvc5 rejects; the crate's own doc says "z3() is the supported entry point." Confirmed live: a cvc5-only user stays solver-less and loops on the same warning. Fail-safe (caps at POSSIBLY). **Fix:** either drop cvc5 from the warning/README/QUICKSTART/Cargo.toml, or add a real cvc5 probe + `cvc5 --lang smt2 - --tlimit-per=` invocation as a second Auto fallback.

### F19 — [LOW, bug] `expand_adl_inputs` silently drops directory entries whose `DirEntry` read errors
`reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:53` — lens: cli-build

`.filter_map(|e| e.ok().map(|e| e.path()))` discards any `Err` `DirEntry` without a message, while every other failure in the function fails loudly (empty dir, unreadable dir/file). In a tool whose `--cross` output reads as "this whole folder was reconciled," a silently-missing input misrepresents coverage. Trigger is rare (per-entry `readdir` I/O error mid-iteration — NFS/FUSE stale handle), so low. The project's own harness (`adl-sema/tests/snapshots.rs:33`) treats such errors as fatal. **Fix:** propagate entry errors like the `read_dir` error → `CliError::Usage`.

### F20 — [LOW, quality] `verify --json <dir>` output shape depends on directory contents
`reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:174` — lens: cli-build

`multi = files.len() > 1` is computed after expansion, and the emitter prints a bare object for one report, a JSON array for several. So `verify --json analyses/` flips between object and array depending on how many `.adl` happen to be in the folder — a scripted consumer must handle both. Introduced by 139a257 making the file count caller-invisible; `--cross` is unaffected. **Fix:** when any input arg was a directory, always emit the array form (track a `was_dir_input` flag), or document the shape rule in `--json` help.

### F21 — [LOW, quality] If z3 disappears after the Auto probe, verdicts degrade to Unknown but the no-solver warning never fires
`reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:34` — lens: cli-build

`make_solver` Auto probes z3 once and labels the report `smtlib-subprocess(z3)`; each check spawns z3 fresh. If z3 vanishes between probe and use, every check returns `Unknown("spawn z3 failed: …")` (sound — caps at POSSIBLY/UNKNOWN), but `warn_if_no_solver` triggers only on `report.solver == "none"`, so the loud all-capped warning added in 0c9c69e is skipped in exactly the scenario it was written for. Reproduced with a self-deleting fake z3 (passes `-version`, then removes itself): empty stderr, exit 0, per-pair Unknown reasons the only clue. **Fix:** also warn when the solver label is non-"none" but every check returned Unknown with a spawn/IO reason (track a spawn-failure count in the engine, mirror to stderr).

### F22 — [LOW, test-gap] `frame_sat`'s Unknown guard and `existing_size_le`'s dedup are untested and currently untestable (no mock solver)
`reimplementation/adl2/crates/adl-analysis/src/engine.rs:991` — lens: tests-coverage

No test helper implements the `Solver` trait (only `SubprocessSolver`/`NativeSolver`), so Unknown cannot be injected per-call. `frame_sat` (`:991-1003`) treats Unknown/timeout as false, but `subset` already rejects non-Unsat, so deleting the `frame_sat` call in `prove_pred_implies` (`:978`) would fail no test — the whole Unknown-classification surface of reconcile is dead to the suite. `existing_size_le` (`:1008-1037`) reverse-engineers `(sub, sup)` from SUB instances by matching ±1 coefficients without checking `rel()==Le` or `constant()==0`; a SUB-orientation change would silently break dedup and double-count XSUB — and no test observes reconciliation axiom counts. Both failure modes are conservative (lose PROVENs, never fabricate). **Fix:** add a scripted `Solver` impl returning a canned `SatResult` sequence to unit-test `reconcile()`/`prove_pred_implies()` over Unknown/Sat/Unsat combinations, plus a test asserting a SUB-covered intra-source pair produces no XR assertion and no Xsub count.

### F23 — [LOW, quality] `size(A) <= size(B)` encoding lives in three unlinked places; `existing_size_le` pattern-matches without checking `rel()`/`constant()`
`reimplementation/adl2/crates/adl-analysis/src/engine.rs:1008` — lens: code-quality

`Emit::sub` (`adl-axioms/lib.rs:707`) builds `1*F + -1*P <= 0` with f64s; `derived_size_le` (`lib.rs:293`) rebuilds it with `Rat`s, its doc claiming proximity prevents drift — but nothing in code ties them; `existing_size_le` reverse-engineers `(sub,sup)` by matching ±1 coefficients, never checking `Rel::Le`/`== 0`. A SUB reorientation would silently invert or stop matching. Correct today (single emitter, fixed shape); latent maintainability gap. **Fix:** have `Emit::sub` construct via `derived_size_le` (making the shared encoding real), and make `existing_size_le` verify candidates with `inst.formula == derived_size_le(s, p)` (LinAtom derives `PartialEq`) instead of coefficient matching.

### F24 — [LOW, quality] `subset()`'s `AssertName` parameter is dead; `prove_pred_implies` fabricates an unused "XREL" name
`reimplementation/adl2/crates/adl-analysis/src/engine.rs:981` — lens: code-quality

`subset()` calls `assert_overs(sub_overs, false)`, so `named.then(...)` is always `None` and the `AssertName` in every tuple is ignored; it never calls `core_items`/`unsat_core` either. `prove_pred_implies` therefore constructs `AssertName::new("XREL")` purely to satisfy the slice type — a name that parallels the real `XR{k}`/`AX{i}` core names but never reaches the solver, misleading anyone auditing the core machinery. **Fix:** change `subset()` to take `&[Over]`, deleting the dead names and the fake XREL.

### F25 — [LOW, docs] Golden file `features-angular_09.adl` left self-contradictory after the verdict flip
`examples/golden/features-angular_09.adl:4` — lenses: tests-coverage + code-quality *(merged)*

c8fb411 flipped line 1 to `# GOLDEN RBandA RBandB OVERLAPPING` and rewrote line 2 (realizer now validates a dR=1.5 witness), but line 4 still reads "Genuinely overlapping; analyzer cannot realize a dR witness so it reports POSSIBLY (sound)." — the exact claim the commit obsoleted. The harness ignores line 4 (inert comment; suite passes), but the pinned-verdict corpus is the hand-audited ground-truth record, and a partial edit matches the remembered "golden files overwritten mid-edit" failure mode. **Fix:** delete or rewrite line 4 to match the new OVERLAPPING-with-validated-witness ground truth.

### F26 — [LOW, docs] `verify.rs` module doc still claims cross-file analysis is "a separate, planned step"
`reimplementation/adl2/crates/adl-cli/src/cmd/verify.rs:10` — lens: code-quality

The module header (`:7-11`) says "Cross-file region relations … are a separate, planned step — see `MULTIFILE_PLAN.md`," contradicted by `run_cross` at line 190 of the same file and by the clap help that 139a257 fixed. The file was touched by 3 of the 8 in-range commits without correcting it. **Fix:** rewrite the paragraph to describe shipped behavior (multiple files independently by default; `--cross` merges into one identity space with namespaced regions and reconciliation-derived size facts; directories expand to `*.adl`).

### F27 — [LOW, hygiene] Stale untracked analysis reports at repo root
`analysis.md:1` (also `report.md`, `CMS-SUS-16-017.adl.md`) — lens: code-quality

Three untracked markdown reports sit at repo root, dated 2026-04-05 (~3 months old, predating this branch): `CMS-SUS-16-017.adl.md` and `analysis.md` are analyze-adl skill outputs (`analysis.md` covers SUS-16-048), `report.md` is an "adl-analyzer MCP server" run log. Not gitignored (`git check-ignore` matches nothing); generic root names risk being swept into an unrelated commit or overwritten by the next tool run. **Fix:** delete them (regenerable), or move the SUS-16-017 writeup under `docs/` with a dated name and add `/analysis.md` `/report.md` to `.gitignore`.

---

## 4. Improvement & fixes plan

### Phase 1 — Immediate fixes (confirmed bugs)

Ordered by severity, then blast radius. Effort: S ≈ <1h, M ≈ a few hours, L ≈ a day+.

1. **F1 — fail closed on the resolve fallback (L).** *What:* in `resolve_object`, intern a unit-unique private base (`<unit>::<name>#unresolved`) on the unsupported-take-call, no-take, and take-cycle arms instead of `Base(self_sym)`; set `Fragment::Unsupported` on the no-take case. *Why:* only confirmed false-PROVEN in the range. *Verify:* the two repro pairs (both take-call and commented-take variants) must return POSSIBLY, not PROVEN DISJOINT; add regression tests mirroring `reconcile_skips_private_base_name_collision` for all three shapes; full `cargo test --release --workspace` stays green.
2. **F5 — dedup `expand_adl_inputs` (S).** Canonicalize with fallback, keep first occurrence in order. *Verify:* `verify --cross dirA/ dirA/x.adl --fail-on=overlap` exits 0; new CLI test.
3. **F3 — unit identity, not name prefix (M).** Thread the unit ordinal into the report and partition `pair_is_cross` on it; disambiguate colliding basenames before `merge_hirs`. *Verify:* `verify --cross vA/atlas.adl vB/atlas.adl` reports the correct cross count and distinguishable region labels; new cross_file assertion.
4. **F11 — disambiguated XSUB/XEQ labels (S/M).** Render the axiom statement with the `C{id}#name` form. *Verify:* keystone `--explain` line shows `size(C1#jets) <= size(C2#jets)`.
5. **F2 — witness rows from the validated artifact (M).** Recompute mentioned quantities on the post-sort event. *Verify:* the F2 repro shows the sorted (descending) values; `[witness validated by interpreter]` never labels a loader-rejectable set.
6. **F4 — normative doc + schema catch-up (S/M).** CANDIDATE row in §2/README, XSUB/XEQ rows in §4, `--fail-on` candidate note, `SOUNDNESS_HARDENING` Status update, README directory/`--cross` sentence, and `SCHEMA_VERSION` bump-or-document. *Verify:* docs review; grep for the new rows.
7. **Quick wins (S each):** F15 (`'c' => "36"`), F18 (drop cvc5 from docs or add a real probe), F19 (propagate `DirEntry` errors), F17 (sweep-skill grep + baseline), F20 (array-shape rule), F26 (module doc), F27 (delete/relocate stale reports).

### Phase 2 — Hardening (test gaps + low-severity soundness/precision)

1. **F6 — CLI tests for directory expansion + `--cross` reconcile wiring (M).** Pins the two Phase-1 CLI bugs against regression and the reconcile flag against silent loss.
2. **F7 — XEQ both-directions test (S).** Redundant-second-cut fixture; assert both orientations + `axioms_used` XEQ count.
3. **F10 — `examples/golden/cross/` subcorpus + `GOLDEN-CROSS`/CANDIDATE pin syntax (M).** The baseline the precision unlock (Phase 3) will need.
4. **F8 — difftest oracle reconcile mode (M-L).** Generator emitting two filtered objects over one base with `reconcile: true` + a `passes(A) ⇒ passes(B)` sampling check; document the current gap in `TESTING.md` immediately (S).
5. **F9 + F13 + F14 — witness realizer unit tests and the two precision fixes (M).** Hand-built-map tests over the branch matrix and the sort; the pinned-eta fix-point correction (with the `eb - v` direction) and the dPhi-DOF realization; and quieting structurally-unrealizable INTERNAL diagnostics.
6. **F16 — CANDIDATE tier wire/gate/oracle tests (S-M).**
7. **F22 — scripted mock `Solver` (M).** Unlocks the entire Unknown-classification surface of reconcile; then pin `frame_sat` and `existing_size_le` dedup (axiom counts).
8. **F12 — `GENERIC_INDEX` defensive fix (S).** Reject/diagnose out-of-range indices at resolve (or `ElemIndex::Generic`) + fail-closed check in `reconcile::build` + regression test with index ≥ 2³².
9. **F23 + F24 — encoding-consolidation and dead-name cleanups (S each).** `Emit::sub` via `derived_size_le`; `existing_size_le` by formula equality; `subset()` to `&[Over]`.
10. **F25 — golden_09 header (S).**

### Phase 3 — Strategic (roadmap)

1. **`abs(x)<c` band encoding in `encode_pred_exact` (adl-axioms) — the top precision unlock.** *Why:* today `abs(x)<c` is opaque, so real-corpus reconciliation firing is sparse (`abs(eta)` cuts fail closed). Encoding it as the interval band `-c < x < c` on the exact-rational side is the single biggest expansion of PROVEN reach. *Blast radius:* wide — it changes what proves across the whole corpus, so it **must** ship behind the deep oracle. *Verify:* land F8 (difftest reconcile mode) and F10 (cross golden subcorpus) **first**; then require a full corpus sweep diff showing PROVEN growth with zero new unsound pairs, plus the oracle's `passes(A) ⇒ passes(B)` invariant holding on all sampled events.
2. **Rank 10 — combination certificate.** Emit a machine-checkable certificate for each PROVEN cross-file relation (the unsat core + the axiom instances), so a downstream tool can re-verify without trusting `smash2`. *Verify:* certificate replays to the same verdict under an independent checker.
3. **Rank 12 — `size()` cardinality reasoning.** Strengthen size facts beyond subset (e.g. `size(A ∩ B)` bounds) to unlock disjointness the current XSUB/XEQ pair cannot reach. *Verify:* new golden-cross pins; oracle-backed.
4. **Track B — soundness hardening.** Fold the general float-vs-real additive-boundary gap and the opaque-overlap candidate gap into the exact-rational core; documented residuals become either proven-sound or explicitly-documented limitations.

---

## 5. Appendix

### Refuted findings
None. All 34 raw findings across the six lenses survived adversarial verification (`verdict=confirmed`); 27 distinct after merging cross-lens duplicates.

### Documented residuals (restated — report only if found worse than documented)
- **Property-alias:** `constituents`/`daughters` → `ccountof`; self-consistent with the interpreter. Not observed worse in this range.
- **Base identity ("same ext name = same input" for cross-file):** the intended residual presumes both files genuinely consume the ext input. **F1 is strictly worse** than this residual (file A never consumes the input; the identity is fabricated after the take source is dropped) and is therefore reported as a new critical, not a restatement.
- **General float-vs-real additive-boundary gap:** the analyzer is exact-real, the interpreter stepwise-f64; the general additive-boundary case still needs an exact-real rewrite or a documented limitation (Track B). F13's pinned-eta round-trip is a specific, mitigated instance on the SAT side (no soundness impact).
- **Known precision limitation (planned next):** `abs(x)<c` opaque in `encode_pred_exact` → sparse real-corpus reconciliation firing. This is Phase 3 item 1, not a bug.

### Repo-hygiene notes
- Three untracked root reports (`analysis.md`, `report.md`, `CMS-SUS-16-017.adl.md`) — see F27.
- Golden corpus otherwise clean: 57 `.adl` files, all with GOLDEN pins, no conflict markers, clean git status (one stale comment line — F25).
- Suite green: `cargo test --release --workspace` (60 suites), clippy clean, per the build environment. Do **not** run the whole-`examples/Examples` cross merge (times out).
