# ADL2 Soundness & Efficiency Review — overlap/disjointness + multi-file readiness

_Multi-agent adversarial review (8 dimensions, 33 agents, 24 raw findings → 20 confirmed / 4 rejected; merged to 16 distinct items below). Solver backend reviewed: smtlib-subprocess(z3) and the native libz3 path._

---

## Executive summary

The overlap/disjointness analysis is mostly sound on its core proof paths, but adversarial verification confirmed **2 Critical, 4 High, 3 Medium, and 7 Low** findings (16 total), after rejecting 4 additional reported issues as false positives during verification — evidence that the filtering was real and not rubber-stamped. The two Critical findings are genuine false-PROVEN soundness breaks reachable from ordinary ADL input: `abs(E) == c` / `abs(E) != c` are mis-encoded when `c < 0` (yielding false PROVEN DISJOINT/SUBSET), and the ORD axiom asserts pT-descending order on filtered-union collections where that ordering is actually false (yielding false PROVEN DISJOINT/EMPTY/SUBSET into every UNSAT-direction proof). For **soundness of overlap/disjointness**, the single most important takeaway is that the dangerous defects sit on the *non-re-validated* UNSAT side (disjoint, subset, empty) where there is no witness safety net to catch them — the OVERLAP/witness path, by contrast, mostly degrades safely to POSSIBLY rather than lying. For **multi-file readiness**, the takeaway is that *every* identity leaf — `Collection::Base(Symbol)`, `PropId`, `ElemPredId`, `ExternalFn`, region/object names, and the axiom domain itself — is a file-local index or file-local rendered string, so no detector input or predicate unifies across files today; none of this is a present-day bug (nothing compares cross-file yet), but a naive raw-id merge would immediately convert the readiness gaps into soundness violations. Efficiency findings cluster on the subprocess solver backend, which re-spawns z3 and replays the entire script on every check/model/core call, with a fully serial pairwise loop on top — correct but throughput-bound, and the cost multiplies precisely at the cross-file scale the spec targets.

## Confirmed findings

### Critical

**1. `abs(E) == c` and `abs(E) != c` are unsound for negative constants — false PROVEN verdicts**
`crates/adl-formula/src/encode.rs` — `abs_cmp`, `Rel::Eq` / `Rel::Ne` arms (lines 668–675).

`abs_cmp` expands `|E| ⋈ c` into `upper := E ⋈ (c - e.k)` and `lower := E ⋈ (-c - e.k)`. For the ordering relations this is exact for all `c` (including `c < 0`), but the Eq/Ne arms silently assume `c >= 0`:

```rust
Rel::Eq => { let parts = vec![upper(self, Rel::Eq), lower(self, Rel::Eq)]; forr(parts) }
Rel::Ne => { let parts = vec![upper(self, Rel::Ne), lower(self, Rel::Ne)]; fand(parts) }
```

- `|E| = c` with `c < 0` is **unsatisfiable** (an absolute value can never equal a negative number), yet the encoder emits `E = c-e.k ∨ E = -c-e.k`, which is SAT. The encoded region is larger than truth on *both* projections. Since `Formula::over` and `Formula::under` both read this same `is_exact` formula, the under-approximation is too large, enabling a **false PROVEN SUBSET** (`A⊆B := UNSAT(A+ ∧ ¬B-)`: a too-large `B-` shrinks `¬B-` and the UNSAT spuriously succeeds) and a false PROVEN OVERLAP.
- `|E| ≠ c` with `c < 0` is **always true**, yet the encoder emits `E ≠ c-e.k ∧ E ≠ -c-e.k`, excluding two points. The over-approximation is too small, directly enabling a **false PROVEN DISJOINT** — the worst verdict class, with no re-validation to catch it.

Verified live: `select abs(MET - 200) != -5` encodes to `And([MET != 195, MET != 205])` (truth: always-True); `select abs(MET - 200) == -5` encodes to `Or([MET == 195, MET == 205])` (truth: unsatisfiable). The `c == 0` boundary is handled correctly. The doc comment at line 638–639 claims the expansion is "Exact" but only the ordering relations are tested (encoder.rs:473).

**Fix:** short-circuit the negative-constant degenerate before the match (it is uniform across relations and exact, not an approximation):
```rust
if c < 0.0 {
    return match rel {
        Rel::Lt | Rel::Le | Rel::Eq => Formula::False,
        Rel::Gt | Rel::Ge | Rel::Ne => Formula::True,
    };
}
```
Add tests for `abs(E) {==,!=,<,<=,>,>=} (-5)` and the `c == 0` boundary.

**2. ORD axiom asserts pT-descending order on filtered-union collections, where it is false — false PROVEN DISJOINT/EMPTY/SUBSET**
`crates/adl-axioms/src/lib.rs` — `Emit::pt_ordered` (443–448), consumed by `Emit::ord` (474–489) and `Emit::elem_pt_quantities` (450–472).

ORD emits `pt(C[i]) >= pt(C[j])` for `i < j` on any collection where `pt_ordered(c)` is true, and:
```rust
fn pt_ordered(&self, c: CollectionId) -> bool {
    matches!(self.hir.table.collection(c),
             Collection::Base(_) | Collection::Filtered { .. })
}
```
This returns true for **every** `Collection::Filtered` regardless of its ultimate root. But `object goodleptons take union(eles, muons) select pT > 20` resolves to `Filtered { parent: Union(..) }` (resolve.rs builds `Filtered` whenever cuts are present). The interpreter materializes `Union` by plain concatenation, never a pT-merge (eval.rs:855–861, `all.extend(...)` per part; re-sort is OFF), and `Filtered` preserves source order. So `goodleptons = [eles in pT order] ++ [muons in pT order]` is **not** globally pT-descending.

Concrete falsification: `eles=[pt=30]`, `muons=[pt=50]` (each internally valid/descending) materializes to `goodleptons=[pt=30, pt=50]`, so `goodleptons[0].pt = 30 < goodleptons[1].pt = 50`, violating the asserted `goodleptons[0].pt >= goodleptons[1].pt`. That false fact is asserted into the base solver frame (engine.rs:151) and participates in every UNSAT-direction proof — yielding false PROVEN DISJOINT / EMPTY / SUBSET. The existing test (tests/axioms_hold.rs VOCAB) only exercises `take union(eles,muons)` *without* cuts (which stays a `Union`, `pt_ordered=false`), so the filtered-union case is never covered.

**Fix:** make `pt_ordered` walk the `Filtered` parent chain and return true only when the transitive root is `Collection::Base`:
```rust
fn pt_ordered(&self, mut c: CollectionId) -> bool {
    loop {
        match self.hir.table.collection(c) {
            Collection::Base(_) => return true,
            Collection::Filtered { parent, .. } => c = *parent,
            Collection::Union(_) | Collection::Combination { .. } => return false,
        }
    }
}
```
Add a filtered-union case to tests/axioms_hold.rs VOCAB. This same fix also closes Finding 4 (multi-level IDOM).

### High

**3. Opaque-quantity hard error short-circuits the membership walk, masking a violated checkable cut and producing a falsely-validated "Candidate" overlap**
`crates/adl-analysis/src/witness.rs` — `validate_witness` (75–112), interacting with `adl-interp/src/eval.rs` `region_walk` (449–476) / `quantity()` (665–668).

`validate_witness` calls `interp.eval_region_by_name(name)` and treats an `Err` containing `"no reference interpretation"` as opaque → `Validation::Candidate` → PROVEN OVERLAPPING (`witness_validated = Some(false)`):
```rust
Err(e) if e.reason.contains("no reference interpretation") => { opaque = Some(...) }
```
But an opaque external is a *hard* error in the interpreter (eval.rs:665 `self.err(...)`), and `region_walk` short-circuits on the first hard error or first `Ok(false)`, in source declaration order:
```rust
match outcome { Ok(true) => {} Ok(false) => return Ok(false), Err(e) => return Err(e) }
```
So if an opaque-external statement is declared *before* a normal, interpreter-checkable cut in the same region, the walk returns the opaque `Err` before ever reaching the later cut, and `validate_witness` sees only the opaque error — never observing that a separate, fully-checkable cut is violated. Because the realizer leaves nonlinear angulars at default (`AngKind::DR => continue` at witness.rs:530, "validation decides"), the realized event can fail a `dR > 4.0` cut while the pair is still reported PROVEN OVERLAPPING.

Verified order-dependent: with `select customSep(...) > 0` *before* `select dR(jet[0],jet[1]) > 4.0`, stmt 0's opaque `Err` short-circuits, the dr cut at stmt 1 is never evaluated, `opaque` is set, region B passes, and the pair is reported PROVEN OVERLAPPING even though the realized event has `dr ~ 0`. Reversing the two lines makes the dr cut stmt 0 → `Ok(false)` → `Validation::Rejected` → downgrade to POSSIBLY (sound). The masking is order-dependent and author-controllable.

**Fix:** do not trust a single short-circuiting region evaluation to certify a Candidate. When `eval_region_by_name` returns the opaque error, additionally evaluate every *non-opaque* membership statement individually on the witness (as `failing_stmts` already does for diagnostics): if any non-opaque Select/Trigger is `Ok(false)` or any non-opaque Reject is `Ok(true)`, return `Validation::Rejected`. Only when all interpreter-checkable cuts pass and the sole obstruction is the opaque quantity may the result be `Candidate`. Equivalently, give the interpreter a non-short-circuiting "evaluate all, collecting opaque/false/true" mode for validation.

**4. IDOM axiom over-emits for a Filtered collection whose parent is itself a Filtered-over-Union (same `pt_ordered` root-walk gap)**
`crates/adl-axioms/src/lib.rs` — `Emit::idom` (682–707), guard at line 689 `if !self.pt_ordered(parent)`.

IDOM asserts `pt(F[i]) <= pt(P[i])` for `F = Filtered{parent: P}`, guarded by `pt_ordered(parent)`, and its justification requires `P` to be genuinely pT-descending. A single-level filtered-union is correctly skipped (`pt_ordered(Union) = false`). But because `pt_ordered` returns true for *any* `Filtered`, a two-level chain `object x take goodleptons select ...` ⇒ `Filtered{parent: Filtered{parent: Union}}` passes the guard.

Verified trace: `goodleptons = [ele1 pt=100, ele2 pt=20, muon1 pt=80, muon2 pt=10]` (union, internally sorted but not globally); `x` keeps `{ele1, muon1}` ⇒ `x = [pt=100, pt=80]`. IDOM emits `pt(x[1]) <= pt(goodleptons[1])`, i.e. `80 <= 20` — **false**. This over-strong fact prunes the genuine model from `Ax ∧ A+ ∧ B+`; if a sibling region is satisfiable only on the pruned assignment, the disjointness check returns UNSAT → false PROVEN DISJOINT, with no UNSAT-side re-validation (engine.rs:431–436) to catch it.

**Fix:** the same root-walking `pt_ordered` fix from Finding 2 resolves this transitively. Add a two-level filtered-union test case.

**5. `Collection::Base` identity depends on file-local Symbol indices — shared detector inputs (jets, MET) do not unify across files**
`crates/adl-sema/src/quantity.rs` — `Collection::Base(Symbol)` (167); resolve.rs `resolve_base_name` (364–379), `is_met_coll` (295–298).

The entire object-base chain roots in `Collection::Base(Symbol)`, where `Symbol` is a `u32` index into the *per-file* `SymbolTable` (`Symbol(self.display.len())`). `Filtered`/`Union`/`Combination` carry `CollectionId` children that bottom out at these Base symbols, so the whole collection-identity DAG is file-scoped. `ExtDecls` canonicalizes spelling to a stable string, but `resolve_base_name` immediately re-interns that string into the file-local table and discards the portable string:
```rust
let canon = canon.to_owned();
let sym = self.symbols.intern(&canon);
self.intern_coll(Collection::Base(sym))
```
Verified: File1 declaring `jets` then `electrons` and File2 declaring them in the opposite order produce *different* `Symbol` indices for the same detector `JET` collection — they coincide only by accidental declaration-order matching, and distinct collections could collide on the same index across files. This is a readiness gap, not a current false-PROVEN (nothing compares cross-file yet), but it becomes Finding 5's unsoundness the instant a raw-id merge is attempted.

**Fix:** give Base collections a portable canonical key (the `ExtDecls` canonical string, or a globally-interned Symbol from a shared `SymbolTable`) as their identity rather than a per-file index. On merge, re-intern `Base(canon_string)` into the shared table so identical detector inputs map to one `CollectionId`.

**6. Subprocess solver re-spawns z3 and replays the entire SMT script on every check/model/core — no incrementality, no process reuse, no parallelism**
`crates/adl-solver/src/subprocess.rs` — `check` (276–289), `script` (144–176), `run` (179–209), `model`/`unsat_core` (322–343); driven from `engine.rs run()` per pair. *(Two reviewers reported this independently — merged here.)*

The backend is fully stateless: `check()`, `model()`, and `unsat_core()` each call `self.run(self.script(...), ...)`, and `script()` rebuilds the **complete** accumulated state from scratch (`for frame in &self.frames { for item in &frame.items { ... (assert ...) } }`) and pipes it to a freshly spawned `z3 -in`. There is no `(push)`/`(pop)` emitted to z3, no persistent process, no `(set-logic QF_LIRA)`, and no query caching. The engine's design comment and the SPEC's "incremental session" assumption hold for `NativeSolver` (real push/pop, retained lemmas) but are **false** for this backend.

Verified cost of one overlapping pair reaching the witness loop: disjoint check (1 spawn) + `unsat_core` (1) + two subset checks (2) + overlap check (1) + up to 6 witness attempts, each calling `refined_model` (base `model()` = 1, plus up to 4 `try_with` = check+model = up to 9) plus a blocking-clause check (1) — **40+ z3 spawns for a single hard pair**, each re-parsing the full axiom set with no `(set-logic)`. The pairwise loop (engine.rs:202–216) is single-threaded; the only parallelism in the codebase is per-event in `adl-cli/src/cmd/parallel.rs`, not the solver analysis. Correctness is unaffected (deterministic replay).

**Fix (ranked, sound and determinism-preserving):** (a) emit `(get-model)`/`(get-unsat-core)` in the *same* script as `(check-sat)` so a Sat/Unsat pair costs one spawn not two; (b) keep one long-lived `z3 -in` child and stream `(declare-const)`/`(assert)`/`(push)`/`(pop)`/`(check-sat)` 1:1 with the trait calls, collapsing ~40 spawns/pair to ~10 streamed commands and restoring the warm learned-clause reuse the engine already assumes; (c) emit `(set-logic QF_LIRA)` once; (d) parallelize engine.rs:203–216 across `available_parallelism()` workers, each owning its own per-thread solver (the native backend is already per-thread by its doc). If a persistent session is too invasive, at minimum cache the immutable base-frame prefix string and concatenate `[cached prefix] + [delta]` per check.

### Medium

**7. PROVEN OVERLAPPING emitted with `witness_validated = Some(false)` — conflicts with the stated contract**
`crates/adl-analysis/src/engine.rs` — `pair()`, Candidate arm (556–563).

The review's soundness contract states `PROVEN OVERLAP := SAT(A- ∧ B-)` with the witness *re-validated* (`witness_validated == Some(true)`), and "every witness must be re-validated before being shown." The Candidate arm instead sets:
```rust
Some((model, Validation::Candidate(why))) => {
    report.kind = VerdictKind::ProvenOverlapping;
    report.witness_validated = Some(false);
}
```
i.e. the interpreter did *not* confirm membership (it refused on an opaque external), yet the verdict carries the full PROVEN OVERLAPPING label and is counted as such (`report.rs:399 counts.1 += 1`). The implementation is internally consistent and visibly caveated (module docs witness.rs:9–13, `OVERLAP_CAVEAT`, render.rs:166 / report.rs:369 "witness is a candidate only"), but under the strict contract handed to this review it mislabels a not-fully-proven verdict. Verified: two regions both cutting on the same undeclared external plus a shared-dimension cut land in this arm with no real event known to satisfy both.

**Fix:** either (a) reclassify the Candidate outcome as a distinct weaker kind (e.g. `OVERLAPPING (candidate)` / `PossiblyOverlapping`) so it is not aggregated with fully-validated PROVEN OVERLAPPING; or (b) if the relaxation is intended, update SPEC_ANALYSIS so the invariant matches the code (`PROVEN OVERLAPPING ⇒ witness_validated ∈ {Some(true), Some(false)}` with documented opaque-caveat semantics). Keep the caveat rendering either way.

**8. `PropId` and `ElemPredId` identities are file-local — structurally identical properties and filtered cuts will not match across files**
`crates/adl-sema/src/quantity.rs` — `intern_prop` (232–240), prop store (188–189); resolve.rs `intern_elem_pred` (582–591).

Two more identity leaves are file-scoped. (1) `PropId` is the index into `QuantityTable.props`; the canonical *key* string is stable but the `PropId` value is not portable, and `ElemProp`/`CollProp`/`MetProp` embed `PropId`. (2) `ElemPredId` — the identity of a Filtered collection's cut set — is interned by the *rendered string* of the predicate HNode (`self.render_node(&node)`) via a `RenderCtx` over file-local symbols/table/coll_names/region_names. Verified: `object F take Jet with cut pt>20` renders to a string containing the file-local CollectionId index (e.g. `"C3#Jet[0].pt"`); a second file with a different collection-id ordering produces a different render and thus a different `ElemPredId`. So Filtered identity is neither structural nor portable — it is a per-file string hash. This is a Phase-8 readiness gap but currently inert and partially mitigated, since the spec'd cross-unit mechanism is solver-proven predicate equivalence over the preserved HNode (`ElemPred.node`), not render equality, and the portable `prop_key` is already available and used in adl-axioms.

**Fix:** key `ElemPred` identity on the structural HNode over already-portable leaf ids (after Findings 5/8), not on a label-dependent render; or canonicalize the render against portable canonical keys rather than display spellings and region-order indices. Carry `prop_key` (not `PropId`) as the portable property identity in the shared table.

**9. `ExternalFn` identity embeds a file-local Symbol name and child ids; Opaque args render via the file-local table — opaque externals will not unify across files and could mis-unify on merge**
`crates/adl-sema/src/quantity.rs` — `Quantity::ExternalFn { name: Symbol, args: Vec<QuantityArg> }` (158–162), `QuantityArg::Opaque(String)` (137); resolve.rs `opaque_arg` (1396–1413), `quantity_arg` (1376–1393).

`ExternalFn` identity is `(Symbol name, Vec<QuantityArg>)`: `name` is a file-local Symbol index, and args contain file-local `QuantityId`/`CollectionId`/`ParticleRef` plus `QuantityArg::Opaque(String)` produced by `self.render_node(&node)` over the file-local `RenderCtx`. The comment claims "identical text means identical resolution, so interning cannot over-merge" — true *within* one file but not across files: the rendered text depends on file-local display spellings and child labels, so (a) the same external call in two files may render differently (no unification) and (b) after a naive table merge, two Opaque strings coinciding textually but from different resolution contexts could over-merge into one atom, yielding a false PROVEN DISJOINT/SUBSET. `ExtDecls` itself is fine (string-keyed); the interned `ExternalFn` inherits the file-local leaves. Not a present-day false-PROVEN (no cross-file merge pass exists — grep for merge/remap/reintern returns nothing).

**Fix:** identify `ExternalFn` by the canonical (lowercased) function-name string and structurally-portable args; build Opaque strings from portable canonical keys, not display spellings. Re-intern external-fn quantities through the shared table on merge rather than trusting per-file Opaque text.

### Low

**10. `classify()` only recognizes `(error` / `error "` — z3's `unsupported` and other non-`(error)` diagnostics slip through**
`crates/adl-solver/src/subprocess.rs` — `classify` (211–233).

The audit-Bug-5 guard rests on `output.contains("(error") || output.contains("error \"")`. Reproduced on z3 4.12.2: a bare malformed top-level term (e.g. `(this_is_not_a_function q0)` with no `(assert ...)` wrapper) prints `unsupported` + a `; ...` comment then answers `sat`; `classify` misses both tokens, falls through to the line scan, finds `sat`, and returns `SatResult::Sat` from a script where a command was effectively dropped. **Currently unreachable** — the encoder only emits malformed content *inside* `(assert ...)` (which does yield `(error ...)`, correctly caught), and the only raw-injection hook is test-only (`conformance.rs:185`). A real latent robustness gap on the worst bug class, but not presently triggerable; severity reduced medium→low.

**Fix:** treat any of z3's diagnostic tokens as forcing `Unknown` before reading the answer — scan for a trimmed line equal to `unsupported`, starting with `;`, or matching `(error`/`error:`/`warning:` case-insensitively. Better: whitelist `sat`/`unsat`/`unknown`/`timeout`/the getter s-expr and treat any other non-empty line as `Unknown`.

**11. Native model extraction silently falls back to lossy `approx_f64` for rationals with num/den ≥ 2^53 — risks spurious POSSIBLY downgrades (not a soundness break)**
`crates/adl-solver/src/native.rs` — `model` (209–214).

```rust
(QSort::Real, Var::R(r)) => model.eval(r, true).map(|x| match x.as_rational() {
    Some((n, d)) if d != 0 && n.abs() < (1i64 << 53) && d.abs() < (1i64 << 53) => n as f64 / d as f64,
    _ => x.approx_f64(),
}),
```
For large num/den the fallback goes through a truncated decimal string and can be several ulps off. Witness re-validation re-runs the model through the reference interpreter, so a lossy value **cannot** fabricate a PROVEN OVERLAPPING verdict — at worst it fails re-validation and downgrades to POSSIBLY. Confirmed not a soundness issue: a wrong-verdict trigger would require a witness whose realizability hinges on a denominator ≥ 2^53 surviving the ε-interior preference, the dyadic `snap_model`, and all 6 blocking-clause retries, then still rounding wrong — exactly what the engine's anti-boundary machinery prevents. A precision/quality nit.

**Fix:** carry the exact rational into the witness layer when num/den exceed 2^53 (store an exact rational alongside the f64 for Real vars), or parse z3's big-integer `as_rational` losslessly rather than `approx_f64`. The backend should not be the place that drops precision.

**12. `parse_get_value` / `unsat_core` parse by raw `split("sat")` / `split("unsat")` and first-paren scanning — fragile but currently non-triggerable**
`crates/adl-solver/src/subprocess.rs` — `parse_get_value` (422–441), `unsat_core` (322–326).

`output.split("sat").nth(1)` and `output.split("unsat").nth(1)` then grab the first `(`..`)`. `"unsat"` contains `"sat"`, and any preceding diagnostic/echo line containing those substrings would split at the wrong point. **Could not construct a trigger:** z3 4.12.2 `-in` batch mode emits no preamble before the answer, strips `;` comments from stdout, routes warnings to stderr (appended *after* stdout in `run`), and `script()` emits no echo/`:print-success`/comments. A mis-parsed core only weakens explanations (safe); a mis-parsed model returning `None` downgrades to POSSIBLY (safe). Real but non-triggerable and soundness-neutral.

**Fix:** anchor on the answer line — find the first line whose trimmed value is exactly `sat`/`unsat` and parse the trailing s-expressions with the existing `tokenize`/`parse_sexp`, ignoring earlier lines.

**13. Axiom set is keyed on file-local QuantityIds in a single per-file table — the axiom domain cannot span two files**
`crates/adl-analysis/src/engine.rs` — `emit_axioms` / `AxiomSet` usage; base frame (133–150), `twin_pairs(&self.hir.table, &combined)` (459), size-parent intern (505–531), TAG/PT axioms (561–665).

`emit_axioms(hir: &mut Hir, ...)` interns new quantities into the same single table (`self.hir.table.intern_quantity(Quantity::Size(parent))`, `encode_elem_pred` over `&mut self.hir.table`) and emits `AxiomSet` instances over file-local QuantityIds; `twin_pairs` takes `&self.hir.table`; the base-frame `all_q` union mixes region and axiom quantities from one table. There is no way to assert that file-1's `Size(jets)` and file-2's `Size(jets)` are the same variable. No present false-PROVEN (cross-file analysis is unimplemented; `analyze_hir` takes a single `&mut Hir`; grep for CrossLink/assume_same_events/merged-table is empty; SPEC_ANALYSIS §7 defers this to Phase 8).

**Fix:** run axiom emission over the shared merged table after re-interning all units' quantities into one id space, so size/parent/twin axioms are stated over global ids; ensure `twin_pairs` and `encode_elem_pred` operate on the merged table.

**14. Object/define/region name collisions across files are not modeled — resolution is keyed by lowercase name within one Resolver**
`crates/adl-sema/src/resolve.rs` — `objects_by_key`/`defines_by_key`/`regions_by_key` (94–95, 130–142, 756–758), `resolve_collection_name` (355–360), `RegionPred(ridx)`/`Inherit` (655–685).

All cross-reference resolution is by lowercase name into per-Resolver HashMaps, and `RegionPred`/`Inherit` carry a bare `ridx` into one file's `region_name_order`. Two files sharing a name (region `SR`, object `goodJets`) refer to unrelated definitions, and a region index from file 1 is meaningless against file 2. No present false-PROVEN: `check.rs` analyzes each file via a separate `analyze_str()` (separate Resolver, HashMaps, region order) with no merge; grep for `UnitId`/`Vec<Hir>`/merge is empty.

**Fix:** namespace identities by `UnitId` at the cross-file layer — keep each Hir's resolution intact and have the driver carry `(UnitId, regionIndex)` / `(UnitId, name)` for any cross-unit reference, never reusing a bare index/name. Portable identity comes from the structural re-intern (Findings 5/8).

**15. Pairwise loop is fully serial; O(regions²) independent solver queries are not parallelized**
`crates/adl-analysis/src/engine.rs` — `run()` pairwise loop (202–216), bin loop (219–223).

A single `Option<Box<dyn Solver>>` is threaded through every pair sequentially via `&mut self`; all `R(R-1)/2` pairs run on one solver. Each pair is an independent query set sharing only the immutable axiom base frame — embarrassingly parallel — and there is no rayon/thread usage in the crate. Confirmed serial cost is real; verification *rejected* the finding's "minutes / hundreds of cross-file regions" framing as unsubstantiated (no multi-unit union exists, the cited SPEC files are absent, only 2-region fixtures are present), so severity is low at the current scale. No soundness claim.

**Fix:** make the solver cloneable-per-worker (each seeded with the immutable axiom/declaration base frame built once and cloned) and parallelize the pair loop with rayon over a core-sized pool; collect `PairReport`s into a `Vec` indexed by `(i,j)` for stable ordering. Sound — pairs are independent and verdict functions take quantities, not shared mutable state.

**16. `lookup_size` does an O(quantities) linear scan inside the per-attempt witness hot loop — a cross-file scaling hazard**
`crates/adl-analysis/src/engine.rs` — `lookup_size` (898–904), called from `refined_model`'s `need_elem` (637) and `witness_values` (924).

```rust
hir.table.quantities().iter().position(|q| matches!(q, Quantity::Size(c) if *c == coll)).map(...)
```
Called O(pairs × MAX_WITNESS_ATTEMPTS × mentioned) times via `refined_model` plus O(mentioned) via `witness_values`, scanning the *global* quantity table — which grows with the number of files under cross-file unification, turning a cheap helper into a quadratic-ish hot path. Negligible at single-file scale (table size in the tens); correctly flagged as a readiness/scaling hazard, not a present hot spot.

**Fix:** the existing private `quant_ids: HashMap<Quantity, QuantityId>` already maps `Quantity::Size(coll)` to its id in O(1) — expose a `get_size(coll) -> Option<QuantityId>` accessor on `QuantityTable`, or build a `CollectionId → QuantityId` map once at Engine construction. Identical results, no scan.

*(Note: a related Low finding — `refined_model`'s up-to-4-level `try_with` ladder re-run on every witness retry, ~24 checks per hard pair — was also reported; it is genuine and efficiency-only, but its concrete cost folds entirely into Finding 6's spawn accounting. Its standalone fix: cache the winning layer per pair and start subsequent retries from it, since the blocking clause removes only one point.)*

## Multi-file readiness

Cross-file analysis is **not implemented today** — `analyze_hir` consumes a single `&mut Hir`, `check.rs` analyzes each file through an independent `analyze_str()`, and grep across the crates finds no `UnitId`, `Vec<Hir>`, merge, remap, or `assume_same_events` machinery. None of the gaps below is a current soundness violation; each becomes one the instant a naive raw-id merge is attempted. Ranked **least to most invasive**:

1. **Namespace names and region indices by `UnitId` (Finding 14, Low).** Leave each Hir's resolution intact; have the cross-file driver carry `(UnitId, regionIndex)` and `(UnitId, name)` so a bare `ridx`/lowercase name from one file is never resolved against another. This is purely additive at the driver layer — no change to the identity model.

2. **Make `Collection::Base` identity portable (Finding 5, High).** Replace the file-local `Symbol` index with the `ExtDecls` canonical string (or a globally-interned shared `Symbol`) so identical detector inputs (jets, MET) collapse to one `CollectionId` on merge. This is the keystone: the entire `Filtered`/`Union`/`Combination` DAG bottoms out here, so nothing else can unify until Base does.

3. **Carry portable property and external-fn identity (Findings 8 and 9, Medium).** Use `prop_key` (not `PropId`) as the property identity, and identify `ExternalFn` by canonical lowercased name + structurally-portable args, building Opaque strings from canonical keys rather than file-local display spellings — closing both the no-unification (A− side) and the over-merge (false-DISJOINT) directions.

4. **Make `ElemPred` (Filtered cut) identity structural, not render-string (Finding 8, Medium).** Key on the preserved HNode over portable leaf ids — which the spec'd Phase-8 mechanism (solver-proven predicate equivalence over `ElemPred.node`) already anticipates — instead of the file-local rendered string, so identically-defined filtered objects match across units.

5. **Re-intern all units into one shared table and emit axioms over global ids (Finding 13, Low; most invasive).** Run `emit_axioms`, `twin_pairs`, and `encode_elem_pred` over the merged table so size/parent/twin/monotonicity axioms span units, and ensure `lookup_size` (Finding 16) is O(1) over the now-large global table. This is the deepest change because it touches the axiom emission pipeline and the engine base frame, and it depends on steps 2–4 having made every leaf id portable first.

## Recommended improvements (prioritized)

Ordered by soundness-impact first, then effort.

1. Special-case `c < 0` in `abs_cmp` before the relation match (exact, not approximate) — fixes false PROVEN DISJOINT/SUBSET. `crates/adl-formula/src/encode.rs`.
2. Make `pt_ordered` walk the `Filtered` parent chain to a `Base` root — fixes the false ORD and IDOM axiom facts in one change. `crates/adl-axioms/src/lib.rs`.
3. On opaque-error validation, evaluate every non-opaque membership statement individually and return `Rejected` on any `Ok(false)`/`Ok(true)` violation — closes the order-dependent overlap masking. `crates/adl-analysis/src/witness.rs`.
4. Reclassify the Candidate overlap as a distinct weaker verdict kind (or reconcile the SPEC) so it is not aggregated with fully-validated PROVEN OVERLAPPING. `crates/adl-analysis/src/engine.rs`.
5. Harden `classify()` to force `Unknown` on any non-answer diagnostic line (`unsupported`, `;`, `error:`, `warning:`) — defense-in-depth on the worst bug class. `crates/adl-solver/src/subprocess.rs`.
6. Carry exact rationals into the witness layer (or parse z3's big-int form losslessly) instead of `approx_f64` for num/den ≥ 2^53 — removes spurious POSSIBLY downgrades. `crates/adl-solver/src/native.rs`.
7. Anchor `parse_get_value`/`unsat_core` on the exact `sat`/`unsat` answer line rather than bare-substring `split` — robustness. `crates/adl-solver/src/subprocess.rs`.
8. Drive the subprocess backend as a persistent `z3 -in` session with streamed `(push)`/`(pop)`/incremental commands, emit getters in the same script as `(check-sat)`, and emit `(set-logic QF_LIRA)` once — collapses ~40 spawns/pair to ~10 commands. `crates/adl-solver/src/subprocess.rs`.
9. Run interval/bin disjointness and a shared-dimension guard before the solver for the bin-disjoint path (the interval spine is already the trusted no-solver fallback) — eliminates O(bins²) interval-decidable solver calls. `crates/adl-analysis/src/engine.rs`.
10. Parallelize the pairwise loop with rayon, each worker owning a per-thread solver seeded with the immutable base frame. `crates/adl-analysis/src/engine.rs`.
11. Cache the winning `refined_model` layer per pair so witness retries skip the full 4-level ladder. `crates/adl-analysis/src/engine.rs`.
12. Replace `lookup_size`'s linear scan with the existing `quant_ids` map (or a `CollectionId → QuantityId` map built once at Engine construction). `crates/adl-analysis/src/engine.rs`.

## What looks sound

Several proof paths held up under adversarial verification and were explicitly checked:

- **OPEN-1 `dual_expand` plus/minus bounds (Finding noted as confirmation, no defect).** The most intricate bound was verified sound for *both* the ∀ and ∃ readings by exhaustive size-class case analysis. `plus = size=0 ∨ P(0) ∨ P(1) ∨ P(2) ∨ size>3` over-approximates both readings (n=0 via the vacuous-forall `size=0` disjunct — the audit-Bug-1 fix; n∈{1,2,3} forces some `P(i)`; n≥4 admitted via `size>3`). `minus = size>=1 ∧ size<=3 ∧ ⋀ᵢ(size<=i ∨ P(i))` under-approximates both (the `size<=i` guard forces `P(i)` iff element `i` is present; the `i=0` clause forces `P(e_0)` for every `size>=1`). The `size=0` atom is a genuine atom that survives the `!=0` coefficient filter, so the empty-collection over-admission is not silently dropped. No assignment lets a false PROVEN verdict through on either polarity. `crates/adl-formula/src/encode.rs`, `dual_expand` (481–517).
- **The `abs_cmp` ordering relations (`Lt`/`Le`/`Gt`/`Ge`) are exact for all `c`**, including `c < 0` and the `c == 0` boundary — verified case by case (e.g. `|E| < c` with `c < 0` correctly collapses to False). Only the Eq/Ne arms are defective.
- **Opaque-external interning does not over-merge distinct externals** within a file (`quantity.rs:157–158`): same name+args ⇒ same `QuantityId`, consistent across regions of one file. The Candidate issue (Finding 7) is purely a verdict-label/aggregation question, not an identity unsoundness.
- **The UNDER/OVER polarity contract and the empty-dimension guard hold** on the paths exercised: the interval fast path before the solver disjoint check is sound, and `bins_disjoint`'s interval spine (the no-solver fallback) only ever proves disjointness via over-approximations, deferring to the solver otherwise.
- **No present-day cross-file soundness violation exists.** Every multi-file finding was verified to be currently inert: there is no merge/union pass, the conformance test `subprocess_error_output_is_unknown_not_weaker` passes, and the witness re-validation safety net on the OVERLAP path means lossy-rational and parse-fragility issues can only downgrade to POSSIBLY, never fabricate a PROVEN verdict.

Finally, of the issues raised during review, **4 were rejected as false positives** under adversarial verification, which is itself a signal that the surviving 16 findings cleared a real falsification bar rather than being accepted on assertion.