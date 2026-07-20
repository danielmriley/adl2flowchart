# Multi-file / cross-analysis support (N1 + X1 + X2) — phased plan

Goal: load N ADL units, prove region relations **across** files, and emit a
cross-analysis overlap/disjointness matrix + a combination certificate. This is
the roadmap's "big bet" and the LHC community's 5-year-unfilled ask (sound
cross-analysis overlap/combination; see `docs/ADL_LHC_RESEARCH_AND_ROADMAP.md`).

> ⚠️ **Soundness-critical.** Per the `adl2-soundness` contract, a single wrong
> identity merge on the UNSAT side fabricates a *false PROVEN DISJOINT* across
> analyses — the worst outcome for this tool. The object-identity reconciliation
> (Phase 3) is not polish; it is the gate that makes Phases 4–5 trustworthy and
> MUST land before any cross-file PROVEN verdict is emitted.

## The readiness gaps (from the deep review)

Every quantity-identity leaf is currently **file-local**, so a naive table
merge would alias unrelated quantities (false PROVEN) or fail to unify the same
physical quantity (missed verdicts):

- **#5** `Collection::Base(Symbol)` — `Symbol` is a per-file interner index
  (`adl-sema/quantity.rs`); the whole `Filtered`/`Union`/`Combination` DAG roots
  here, so nothing unifies until Base does.
- **#8** `PropId` / `ElemPredId` — file-local indices; `ElemPred` identity is a
  *rendered string* over file-local ids, not structural.
- **#9** `ExternalFn { name: Symbol, args }` — file-local name + opaque-arg
  render strings.
- **#13** axioms keyed on file-local `QuantityId`s in a single per-file table
  (`adl-analysis/engine.rs`).
- **#14** object/define/region names resolved by lowercase name within one
  `Resolver`; `RegionPred`/`Inherit` carry bare per-file indices.

The model was *designed* for this (structural-not-string identity), so the work
is an extension, not a rewrite — but it is invasive and must be verified hard.

## Phased plan (each phase independently verifiable; corpus disjoint-count = 866 invariant)

### Phase 0 — portable identity keys (no behavior change; pure refactor)
Make every identity leaf carry a **portable canonical key** alongside its
file-local id. Single-file behavior must be byte-identical (corpus sweep +
golden battery are the guard; PROVEN-DISJOINT count must stay 866).
- `Collection::Base` keyed by the `ExtDecls` canonical string (not the
  per-file `Symbol`).
- Property identity via `prop_key` (already portable) instead of `PropId`.
- `ElemPred` identity via its structural HNode over portable leaf ids, not the
  render string.
- `ExternalFn` by canonical (lowercased) function name + structurally-portable
  args; opaque args from canonical keys, not display spellings.
- **Verify:** full battery green, corpus byte-identical (sweep diff = 0).

### Phase 1 — `UnitId` + multi-unit loading (additive, no cross-file analysis yet) — ✅ DONE
- `adl-cli`: `verify` now takes `Vec<PathBuf>`; each file is analyzed
  independently and reported with a per-unit `==== <name> ====` header
  (human) / a top-level JSON array (`--json`). One file → byte-identical to
  the original output (guarded by the existing 44 cli snapshots). Worst
  per-file exit code wins.
- Tests: `verify_multifile_reports_each_unit`; the single-file snapshots are
  the regression guard.
- Not yet: a shared table / cross-file pairing (Phase 2+).

### Merge implementation notes (concrete remap for Phase 2 — de-risked)
A `merge_hirs(&[&Hir]) -> Hir` builds one shared `SymbolTable`+`QuantityTable`
and re-interns each unit by **memoized recursive structural remap** (the
original interning is bottom-up/acyclic, so memoized recursion can't loop).
Per source unit, hold `Vec<Option<Id>>` memos for collections/quantities/
props/elem-preds and remap each leaf into the shared tables:
- **Symbol** → `shared_syms.intern(src.symbols.key(sym))` (portable name text).
- **PropId** → `shared.intern_prop(src.prop_key(p), src.prop_display(p))`.
- **Collection::Base(sym)** → `Base(remap_sym)`; **Filtered{parent,pred}** →
  `Filtered{remap_coll(parent), remap_elem_pred(pred)}`; **Union/Combination**
  → remap each part. (This is the keystone fix #5: Base now keyed by canonical
  name text, so identical detector inputs unify; structurally-different filters
  stay distinct → sound.)
- **ElemPred** → deep-remap its `node: HNode` ids, then re-intern in the shared
  elem-pred store (re-renders with shared ids → portable key; fixes #8).
- **Quantity**: `EventScalar(MetProp/EventVar/Trigger)` → remap prop/sym;
  `Size(c)`→remap coll; `ElemProp{coll,index,prop}`→remap coll/prop;
  `AngularSep{kind,a,b}`→remap `ParticleRef` (coll + binder sym);
  `ExternalFn{name,args}`→remap name sym + each `QuantityArg` (Quantity/Coll/
  CollProp/Particle ids; `Opaque(text)` is already structural over resolved
  ids — re-emit from remapped ids so two units' identical calls unify; fixes #9).
- **HNode rewriter** (used for ElemPred nodes AND region/define statement
  nodes): walk `HKind`, remap embedded `Quantity(qid)`, `CollValue(cid)`,
  `CollProp{coll,prop}`, `ElemSelfProp(prop)`, `Particle(ParticleRef)`;
  `RegionPred(idx)`/`Inherit` indices are rebased onto the merged region list.
- Build the merged `Hir`: shared tables, `regions` = all units' remapped
  regions tagged with their `UnitId` (add `unit` to `HirRegion`), shared
  `elem_preds`. The existing engine then runs over it unchanged → cross-unit
  pairs fall out of the existing `i<j` loop.
- **Soundness rests entirely on structural identity:** two collections/
  quantities merge **iff** structurally identical; different cuts never alias.
  Test gates: (a) `merge_hirs([hir])` reproduces single-file verdicts exactly;
  (b) self-merge `[hir, hir]` → every region pair across the two copies is
  PROVEN identical/subset; (c) adversarial: same base name, *different* cuts in
  two units → NO false PROVEN DISJOINT; (d) corpus PROVEN-DISJOINT count
  invariant. Run the adversarial-verification workflow before shipping.

### Phase 2 — merged quantity table (re-intern units into one id space)
- `QuantityTable::merge` / a shared interner that re-interns all units'
  quantities through their **portable** keys (Phase 0), so identical detector
  inputs (jets, MET) collapse to one `CollectionId`/`QuantityId`.
- Axiom emission (`emit_axioms`, `twin_pairs`, `encode_elem_pred`) runs over the
  merged table so size/parent/twin facts span units (#13); `lookup_size` O(1)
  over the now-global table.
- **Verify:** merging a file with *itself* reproduces its single-file verdicts
  exactly (a strong idempotence check); property test: merge is associative/
  commutative on verdicts.

### Phase 3 — object-identity reconciliation (THE soundness gate) — must precede any cross-file PROVEN
- For each cross-unit collection pair, emit `IDENTICAL` / `REFINEMENT (A⊆B)` /
  `INCOMPARABLE`, reusing the existing subset machinery + the object-block
  subset axiom. `Collection::Filtered` keeps a distinct identity forever, so a
  REFINEMENT is a *derived* (proven) fact, never assumed.
- **Hard rule:** a cross-file region pair may only yield a PROVEN verdict over
  collections whose identity is `IDENTICAL` (or a proven subset on the correct
  polarity). Any unreconciled identity → the pair caps at `POSSIBLY` with a
  reason. This is what keeps Phase 4 sound.
- **Verify (adversarial):** construct two files where "jets" means *different*
  cuts and assert NO false PROVEN DISJOINT; construct two with identical jets
  and assert the expected PROVEN verdict. Run the adversarial-verification
  workflow (skeptics try to force a false cross-file PROVEN).

### Phase 2 + 4 — merged table + cross-analysis matrix — ✅ DONE
- `adl_sema::merge_hirs(&[&Hir]) -> Hir` re-interns all units into one shared
  structural identity space (`crates/adl-sema/src/merge.rs`); `verify --cross`
  runs the existing engine over the merged regions (`<file>::<region>`), so the
  verdict matrix IS the cross-analysis overlap matrix. The structural identity
  doubles as the Phase-3 gate: quantities unify iff structurally identical, so
  a cross-file PROVEN fires only on genuinely-shared quantities; same-name
  different-cut objects stay distinct (POSSIBLY).
- Tests: `adl-analysis/tests/cross_file.rs` (shared-quantity PROVEN, adversarial
  non-unify, idempotence, the two regression cases below) + `adl-sema` merge
  unit tests + `adl-cli` `verify_cross` wiring.
- **Adversarial verification (ultracode) found & fixed:**
  1. *CRITICAL false-unify* — `QuantityArg::Opaque` carried source-LOCAL
     collection ids verbatim; two units' identical-looking opaque renders over
     different cuts could intern to one id → fabricated cross-file PROVEN
     DISJOINT. **Fixed**: namespace the opaque string by source unit
     (`remap_arg`), so cross-unit opaque args never collide (conservative
     POSSIBLY). Regression: `cross_opaque_external_does_not_falsely_unify`.
  2. *HIGH (conservative)* — unoriented `dR` bypassed operand canonicalization;
     **fixed** by routing `AngularSep` through `intern_angular`. Regression:
     `cross_dr_unifies_regardless_of_object_declaration_order`.
  3. *HIGH (DoS, not soundness)* — deep `Filtered` chains overflowed the stack;
     **fixed** by running the merge on a 512 MiB worker thread.
  4. *LOW* — spurious OPEN-1 INTERNAL diagnostics on merged regions; **fixed**
     by extending the engine's INTERNAL gate.
- **Second adversarial round found & fixed:**
  5. *CRITICAL (refinement of #1)* — the opaque namespacing used `src.unit`
     (the file *basename*), so two files named `a.adl` in different dirs still
     collided → false PROVEN DISJOINT. **Fixed**: namespace by the per-unit
     positional ordinal (`unit_ord`), unique by construction for all callers.
     Regression: `cross_opaque_external_same_unit_name_attack`.
  6. *HIGH false-proven* — witness re-validation resolved regions by NAME;
     merged units can share a region name (same basename / self-merge /
     duplicate names), and a name lookup returns the first match → a region's
     decidable cut is masked → fabricated PROVEN OVERLAPPING. **Fixed**: resolve
     by region INDEX (`Interp::eval_region_membership_idx`; `validate_witness`
     and `failing_stmts` take indices; engine passes `ra.idx`/`rb.idx`). Also
     closes a pre-existing single-file duplicate-region-name ambiguity.
     Regression: `cross_colliding_region_names_do_not_mask_witness_validation`.
- **Third adversarial round: NO soundness defects** ("the merge cannot
  fabricate a false cross-file PROVEN"). One MEDIUM diagnostics defect — the
  merged Hir's empty `src` made cut text / bin labels / dropped-leaf lines
  render as blank / `[?]` / line 1. **Fixed**: render cut text and bin labels
  from the HIR when `src` is empty (`adl_sema::render_node`; `encode_unit` +
  `encode_boundary_bins`), snapshot-safe (single-file keeps non-empty `src`).
  Regression: `cross_diagnostics_render_from_hir_not_empty_src`. (Source LINE
  numbers remain non-meaningful for merged units — documented, not corrupting.)
- **Verification status: 3 adversarial rounds; soundness CLEAN.** Severity
  converged critical → high → medium-diagnostics → none. The cross-file merge
  is sound and shippable.
- Residual (conservative, sound): cross-unit opaque externals never unify (cap
  at POSSIBLY). **Known limit (DoS, not soundness):** a pathologically deep
  (~700k+) `Filtered` chain can still exhaust even the 512 MiB merge worker
  stack and abort the process; real analyses are ~tens of objects deep. A
  future iterative remap would remove the limit entirely.

### Phase 4 leftovers (optional polish, not blocking)
- A dedicated `MatrixReport`/`adl-viz` relation graph beyond the existing
  verdict matrix; richer `<unit>` provenance in the JSON schema.

### Phase 5 — combination certificate (X3)
- Identify provably-disjoint region sets (safe to statistically combine); pair
  each with its unsat-core proofs + axiom provenance, and flag any PROVEN
  OVERLAPPING pair with a concrete double-count witness. `verify --combine`.
- **Verify:** the certificate's disjoint sets re-checked pairwise; witnesses
  re-validated through the interpreter.

## Testing posture (non-negotiable for this feature)
- Corpus sweep before/after every phase: **PROVEN DISJOINT count must not rise**
  unless intended (the #1 regression signal for a fabricated fact).
- The adl-difftest encoder-vs-interpreter oracle stays green throughout.
- Phase 3 and Phase 4 each get an **adversarial-verification pass** (skeptics
  constructing inputs that try to force a false cross-file PROVEN) before being
  considered done — the same harness that found 5 rounds of masking bugs in the
  witness layer.

## Sequencing / risk
Phases 0–2 are safe, mechanical, and independently shippable (they unlock
nothing user-facing alone but carry no soundness risk). Phase 3 is the gate and
the riskiest; it must be in place before Phase 4 emits a single cross-file
PROVEN. Recommend executing Phases 0–2 as one focused run (verified against the
corpus invariant), then Phase 3 as its own run with adversarial verification,
then Phases 4–5.
