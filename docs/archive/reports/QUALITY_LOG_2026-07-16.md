# Code-quality log — full-project ultracode review

**Date:** 2026-07-16 · **Scope:** all 13 crates (~37.6k lines) · **Method:**
6 parallel reviewers (one per crate group), every finding adversarially
verified by an independent agent instructed to refute it (checking call
sites, soundness invariants, pinned tests). **60 findings survived; 0 were
fully refuted; 5 carry verifier corrections.** Style nits and the
deliberate soundness-justification comment density were excluded by
instruction — everything here is a genuine trim/change opportunity.

## Headline items (fix-first list)

1. **`expect_tok` discriminant bug** (adl-syntax) — `TokKind::Kw(x)`
   matches ANY keyword; `table` blocks silently accept wrong keywords.
   Empirically demonstrated: `table t region x …` parses with ZERO
   diagnostics. Small fix (`expect_kw`), diagnostics-quality only. **S**
2. **Four parallel HKind dispatches in eval.rs** (adl-interp, 2,061 lines)
   — truth/truth3/num/num3 each carry a full arm set with verbatim-shared
   leaf arithmetic. The biggest single maintenance hazard in the tree. **L**
3. **`EventObject = BTreeMap<String, f64>`** — every property read in the
   interpreter's hottest path is a string-keyed tree lookup; an interned
   or fixed-slot layout is a large cheap win for `run` throughput. **L**
4. **`Engine::pair` at ~330 lines** mixing interval fast path, disjoint,
   overlap, and demotion phases (clippy suppression in place). The seams
   are already commented — split on them. **L**
5. **Stringly-typed witness-rejection classification** crossing crate
   boundaries (engine ↔ witness ↔ render `reason_signature` reverse-parses
   English strings) — introduce a typed reason enum. **M**
6. **Axiom fixpoint re-runs every emitter over the whole quantity set each
   round** (up to 32 rounds) with Display-string dedup — quadratic-ish and
   allocation-heavy on big merged units; incremental emission or
   structural dedup keys would cut `--cross` latency. **M**

## Theme summary

| Theme | Count | Notes |
|---|---|---|
| duplication | 26 | mostly small shared-helper extractions; two L-size deliberate mirrors (viz labeler, element-vs-region encoder) where the verifier found the duplication is *partly* load-bearing — consolidate carefully or document as intentional |
| dead-code | 9 | `Token.line`, `ChunkReader`, `HKind::Reduce.slice`, unused pub APIs (`ExtDecls::load_dir/describe`, solver helpers), vestigial `let _ =` scaffolding |
| complexity | 7 | parser.rs (1,691), resolve.rs (2,703), encode.rs (1,945), eval.rs (2,061), engine::pair, witness::build_event_json — all with named seams |
| performance | 7 | per-token String allocs, BTreeMap events, binder-env clones per tuple-cut, untraced-path trace recording, eager fallback model fetch |
| api-design | 6 | expect_tok, Engine 13-field literals ×4 sites, rootfile consume-self builder forcing deep clones, stringly reasons |
| structure | 5 | parallel Hir arrays, span-tuple statement joins, split report renderers |

## Suggested sequencing

- **Batch 1 (S items, ~a day):** all dead-code removals + small dedups +
  `expect_kw` + the two vestigial `let _ =`s. Pure deletion, full gate after.
- **Batch 2 (M items):** shared substitution walker in encode.rs, histo
  accumulator unification, typed witness reasons, Engine::new(),
  section-keyword single source, axiom-fixpoint incrementality.
- **Batch 3 (L items, one at a time, each with the deep oracle):** eval.rs
  dispatch consolidation, EventObject layout, engine::pair split,
  parser.rs module split.
- **Explicitly deferred:** viz-labeler/sema-renderer consolidation and the
  element-vs-region encoder mirror — the verifier showed both diverge
  deliberately; fix the stale comments now, consolidate only with a design.

---
# Full findings (by crate group)

## Front end (adl-syntax, adl-viz)

### [api-design · S] `crates/adl-syntax/src/parser.rs:226`
expect_tok compares tokens by std::mem::discriminant, so TokKind::Kw(x) matches ANY keyword — parse_table_block silently accepts any keyword where `tabletype`, `nvars`, or `errors` is required.
- **Change:** Add a dedicated `fn expect_kw(&mut self, kw: Kw, what: &str) -> bool` that matches the exact keyword, and use it in parse_table_block; alternatively make expect_tok special-case TokKind::Kw with full equality. Table blocks are metadata-only so this is a diagnostics-quality fix, not a soundness one.

### [duplication · L] `crates/adl-viz/src/label.rs:2` *(partially confirmed — see note)*
The entire Labeler (~200 lines: collection/particle/quantity/arg/node renderers) duplicates adl-sema's dump renderer, and its justifying comment is stale — sema's renderer is public, not crate-private.
- **Change:** Give adl-sema's RenderCtx a display mode (with/without `C<id>#` prefixes) and expose a `display_node(hir, node)` entry point; delete Labeler's rendering bodies and keep it as a thin wrapper (or drop it). At minimum, fix the stale crate-private comment so the duplication is honestly documented.
- **Verifier's correction:** The stale-comment half is true and worth fixing: adl-sema's render_node/collection_ref ARE public today (label.rs:2's "crate-private" claim was accurate when written, invalidated by commit 36aa067). But the duplication is not pure: Labeler intentionally diverges from sema's canonical renderer beyond C<id># prefixes — structural rendering of unnamed derived collections (filter/union/comb/sort/slice/CombProject), "elem" vs "@elem", and slice-less Reduce formatting — and the structural-collection labels cannot be obtained from sema output at all (sema renders unnamed derived collections as bare "

### [complexity · M] `crates/adl-syntax/src/parser.rs:1076`
parser.rs is 1691 lines in one impl block covering cursor management, statement parsing, expression parsing, and lookahead heuristics; it has a clean, already-marked seam to split on.
- **Change:** Split into parser/mod.rs (Parser struct + cursor/recovery helpers + parse()), parser/stmt.rs (sections and statements), parser/expr.rs (expressions + arguments), using multiple `impl Parser` blocks with pub(crate)/pub(super) fields. No behavior change; snapshot tests already lock output.

### [dead-code · S] `crates/adl-syntax/src/token.rs:226`
Token.line is written by the lexer but never read by the parser or any consumer; its doc comment ("lets the parser do same-line checks cheaply") describes a mechanism that was replaced.
- **Change:** Delete Token.line and the Lexer.line counter (and the one test assertion, replacing it with a span-based check if desired). Shrinks every token by 4 bytes and removes a misleading comment.

### [performance · M] `crates/adl-syntax/src/lexer.rs:66`
Every token — including every Newline, operator, and Eof — allocates an owned String for Token.text, and Ident/Str tokens store their text twice (in the kind and in text).
- **Change:** Drop Token.text and slice `&src[span.start..span.end]` on demand (the parser already holds `src`, and describe() can take the source or be moved to a method on Parser). If that churns too much, populate text only for Int/Real/Ident tokens and leave it empty elsewhere.

### [duplication · S] `crates/adl-syntax/src/dump.rs:197`
The Define dump block is duplicated verbatim between Section::Define and ObjectStmt::Define, and the Take arm does a needless header.clone().
- **Change:** Extract `fn define(&mut self, def: &Define)` on Dumper and call it from both arms; restructure the Take arm so the header String is moved into one code path instead of cloned.

### [structure · S] `crates/adl-syntax/src/parser.rs:176`
The set of section-starting keywords is maintained in two places: the parse_file dispatch and an inline Kw list in recover_block_stmt; adding a section keyword requires updating both or block recovery mis-skips.
- **Change:** Add `pub const SECTION_KEYWORDS: &[Kw]` next to STMT_KEYWORDS in token.rs, use `SECTION_KEYWORDS.contains(&kw)` in recover_block_stmt, and (optionally) note in parse_file that its dispatch must cover the same set.

### [duplication · M] `crates/adl-viz/src/dot.rs:271`
collect_used_collections and collect_region_preds duplicate the same HKind child-recursion skeleton, and node_label_and_children re-enumerates children a third time.
- **Change:** Add one `fn hnode_children(n: &HNode) -> Vec<&HNode>` (or reuse node_label_and_children's child list) and reduce both collectors to a generic walk plus a leaf-matching closure. Purely viz-side; no soundness surface.

### [duplication · S] `crates/adl-syntax/src/parser.rs:1520`
Three separate comma-separated argument-list loops exist: parse_arg_list_to_eol, the body of parse_paren_args, and the Braced arm of parse_primary.
- **Change:** Have the Braced arm call self.parse_arg_list_to_eol() (rename to parse_comma_args since EOL plays no role in it), and let parse_paren_args reuse it for its non-empty case.

### [dead-code · S] `crates/adl-syntax/src/lexer.rs:178`
lex_number binds exp_start and then discards it with `let _ = exp_start;` — vestigial scaffolding from an unimplemented recovery path.
- **Change:** Delete the exp_start binding and the `let _ =` line; keep the recovery comment on the pos-advance itself.


## Semantic layer (adl-sema)

### [duplication · S] `crates/adl-sema/src/merge.rs:291`
Element-predicate interning is implemented twice — resolve.rs intern_elem_pred fails closed on Unsupported nodes (always a fresh, never-shared ElemPredId, per soundness review S1), but merge.rs remap_pred re-implements the interning with a plain render-keyed dedup and has silently dropped that fail-closed rule, so two physically different unsupported cuts from different units that render to the same '<unsupported: reason>' string unify to one shared ElemPredId (and hence one Filtered CollectionId) in the merged table.
- **Change:** Extract one shared intern-elem-pred helper (render-key dedup + the unsupported fail-closed branch) used by both Resolver and Merger, e.g. a free fn or a small ElemPredInterner struct owning elem_preds/elem_pred_ids; add a merge test with two units whose cuts are distinct but share an unsupported render, asserting two Filtered collections survive.

### [duplication · M] `crates/adl-sema/src/objects.rs:375`
objects.rs contains a second, near line-for-line renderer for Quantity/ParticleRef/QuantityArg/HNode (render_particle, render_quantity, render_arg, render_term, render_clause) parallel to dump.rs RenderCtx (particle, quantity, arg, node), differing only in collection-name style ('C3#jets' vs short name) and a few cosmetics ('|x|', dropped 'this.').
- **Change:** Parameterize RenderCtx with a name-style (Identity vs Short) plus an optional 'human' flag for the cosmetic clause rewrites, and have objects.rs consume it. Keep the identity-bearing render byte-stable (it is an interning key) — the flavor only changes the human path, so no soundness surface moves.

### [dead-code · M] `crates/adl-sema/src/hir.rs:168`
HKind::Reduce's slice field is vestigial: it is documented 'always None today', no code in the workspace ever constructs Some, yet it is pattern-matched, rendered, remapped, and keyed through three crates.
- **Change:** Delete the slice field from HKind::Reduce and the dead handling in dump.rs, merge.rs, and adl-formula's slice_key/intern_reduce plumbing. If P1-part-B ever needs it, the Collection::Slice route already covers it.

### [structure · M] `crates/adl-sema/src/hir.rs:392`
Hir carries two parallel arrays that duplicate or shadow per-region state: region_name_order is always exactly regions[i].name (pushed in lockstep at every site, including the synthetic push/pop), and histolist_regions is a Vec<bool> that 'may be shorter than regions' and must be read with .get(i) by convention — a one-source-of-truth violation and a fragile index contract spread across five consumer crates.
- **Change:** Drop region_name_order in favor of regions[i].name (or a region_names() accessor), and move the histoList flag onto HirRegion as a bool field so a region can never lose its flag by index skew. Mechanical change across adl-interp/adl-viz/adl-formula/adl-difftest consumers.

### [duplication · S] `crates/adl-sema/src/resolve.rs:1349`
The soundness-critical pT-descending sort-alias gate is duplicated verbatim in two places: resolve_sort_source (take-level sort) and resolve_region's Sort arm (region-level sort) each recompute pt_key, the 'dir == Descend && key is Prop(pt)' match, and the pt_ordered check — exactly the 'no second copy may diverge' hazard quantity.rs warns about for this gate.
- **Change:** Extract fn sorted_view(&mut self, source, key, dir, suspect: bool) -> CollectionId on Resolver that applies the alias gate and interns Collection::Sorted otherwise; both call sites use it (take-sort passes dir_suspect, region-sort passes false).

### [duplication · S] `crates/adl-sema/src/resolve.rs:977`
TakeSource::Union resolution (map members through resolve_collection_name, collapse a single-element list, else intern Collection::Union) is copy-pasted between resolve_object and resolve_composite.
- **Change:** Extract fn resolve_union_members(&mut self, members: &[ast::Ident]) -> CollectionId and call it from both take paths.

### [complexity · M] `crates/adl-sema/src/resolve.rs:1`
resolve.rs is the crate's monolith at 2,703 lines / 60 fns (note: not the ~6k the maintainer believed — the crate total is 5,940). It is currently navigable thanks to explicit section markers, but it mixes four separable concerns in one impl: object/composite resolution, define resolution, region lowering, and expression/target resolution.
- **Change:** Split along the existing banners into a resolve/ module directory: mod.rs (Resolver struct, Ctx, run/finish, shared helpers), objects.rs (resolve_object/resolve_composite/sort sources, ~650 lines), regions.rs (resolve_region/histo/sort cascade, ~330 lines), exprs.rs (resolve_expr/resolve_target/reducers/args, ~1,140 lines), each an impl Resolver block. Pure code motion; do it before the file grows further rather than as an urgent refactor.

### [dead-code · S] `crates/adl-sema/src/ext.rs:75`
ExtDecls::load_dir and ExtDecls::describe are public API with zero callers anywhere in the workspace (every consumer, including all six CLI commands, uses ExtDecls::legacy()).
- **Change:** Delete describe(); delete load_dir (and its embedded-fallback logic) or wire it to a real --ext-dir CLI flag if loading a custom standard library is actually wanted — keeping an untested I/O path pub is the worst of both.

### [performance · S] `crates/adl-sema/src/hir.rs:127`
HNode::children() allocates a Vec<&HNode> at every node, and the recursive predicates built on it (has_unsupported, context_tainted, mentions_indexed_element) therefore allocate O(nodes) vectors per query; has_unsupported is called on every elem-pred intern, every opaque_arg, and every objects-table row.
- **Change:** Add fn for_each_child(&self, f: &mut impl FnMut(&HNode)) (or an internal try-fold visitor returning ControlFlow) and rewrite the three recursive predicates on it; keep children() for callers that genuinely need the collected list (reconciliation's residual-binder scan).

### [api-design · S] `crates/adl-sema/src/resolve.rs:2373`
The cmp_hoist callback is typed &dyn Fn(&mut Self, HNode) -> HNode but no hoist closure ever uses the &mut Self parameter (all three name it _s), and the lhs/rhs hoist blocks in the Cmp arm are copy-paste mirror images differing only in operand order.
- **Change:** Change cmp_hoist to Option<&dyn Fn(HNode) -> HNode> (dropping the Self param removes the HRTB workaround and the #[allow(clippy::type_complexity)]), and fold the two mirrored Cmp hoist blocks into one helper taking a reducer-side flag that flips operand order.


## Proof layer (adl-formula, adl-axioms)

### [duplication · L] `crates/adl-axioms/src/lib.rs:1340` *(partially confirmed — see note)*
The element-predicate encoder in adl-axioms is a hand-maintained mirror of the region encoder in adl-formula: clear_ratio<->ratio(), abs_pred<->abs_cmp(), the Band arm of encode_pred_exact<->band(), lin_pred<->lin()/lin_binary(), lin_atom<->atom_of() constant-fold, PredLin<->LinExpr, plus a re-implemented CmpOp->Rel map (lines 1367-1374) duplicating adl-formula's private rel_of().
- **Change:** Extract one comparison-lowering core into adl-formula (ratio clearing, abs expansion, band, relational folding, the LinExpr type) parameterized over a leaf-linearizer callback (region path resolves HKind::Quantity/Num; the element path additionally grounds ElemSelfProp to coll[index] and rejects leaked-context externals). adl-axioms already depends on adl-formula, so PredLin can be deleted in favor of an exported LinExpr. Keep the pinning test as a regression net for the shared core. This strengthens (not weakens) soundness: drift between the two copies is the current risk the comments warn about.
- **Verifier's correction:** The duplication of the soundness-critical core (abs expansion, constant-denominator clearing, relational constant folding, linear-combination type, rel mapping) is confirmed and only informally synchronized. But the encoders are not arm-for-arm mirrors: Pow constant-folding, nested div-by-zero semantics, Int-size coercion in atom_of, band's non-linear fallback, and ratio()'s non-constant-denominator branch all differ deliberately, and the region path emits Formula (with Unknown/diagnostics) while the element path emits Option<QFormula> (None = sound drop). A shared core is viable but must abst

### [duplication · M] `crates/adl-formula/src/encode.rs:940`
Three near-identical ~50-line HNode substitution walkers (subst_reduce at 834, subst at 940, subst_binders at 1220) copy the same recursive scaffolding for Neg/Not/Abs/Binary/Cmp/And/Or/ScalarMinMax/Band/Ternary, differing only in the leaf arm; the two quantity-rewrite helpers (subst_reduce_quantity at 889, subst_binder_quantity at 1297) likewise duplicate the AngularSep/ExternalFn re-intern pattern with a different ParticleRef substitution closure.
- **Change:** One generic walker `fn map_hnode(&mut self, node: &HNode, leaf: &mut impl FnMut(&mut Self, &HKind) -> Option<HKind>) -> HNode` handling the structural arms once; the three substitutions become leaf closures. Similarly one `rewrite_quantity_particles(&mut self, q, subst_p: impl Fn(&ParticleRef) -> ParticleRef)` shared by both quantity rewriters. Removes ~180 lines and guarantees a future HKind variant is handled in one place instead of three (today, a new variant silently falls to `other.clone()` in all three, which is easy to miss in one of them).

### [dead-code · S] `crates/adl-formula/src/encode.rs:811`
The `DualKind::Open1` arms inside encode_static_slice_reduce (lines 811 and 822) are unreachable: its only caller, encode_reduce, maps ReduceKind::Any/All to DualKind::Any/All and returns Unknown for everything else, so Open1 can never reach this function.
- **Change:** Take the quantifier as a two-variant enum (or a bool `is_all`) in encode_static_slice_reduce, or replace the `| DualKind::Open1` with `DualKind::Open1 => unreachable!("resolved reducers only")` so the invariant is explicit instead of silently aliased to All.

### [performance · M] `crates/adl-axioms/src/lib.rs:512`
The emit_axioms fixpoint re-runs every emitter over the ENTIRE quantity set each round (up to 32 rounds) and dedups by Debug-formatting each instance's whole QFormula into a String key, so each round redoes all prior rounds' work: ORD alone is O(n^2) pairs per collection per round, re-emitted and re-formatted every round.
- **Change:** Emit each round only for the delta quantities (new since last round) for the per-quantity emitters (SZ0/NNEG/TRIG/DPHI/TAG/SUB/UNI/SZSLICE/SZPERM/COMBSIZE), keeping full re-emission only for the pairwise families (ORD/TWIN/IDOM) or making those pair against (delta x all). Also key `seen` on a structural (AxiomId, LinAtom-based) key instead of a Debug string. Behavior-preserving; verify with the corpus-sweep skill.

### [duplication · S] `crates/adl-axioms/src/lib.rs:644`
elem_pt_quantities (617) and elem_pt_back_quantities (644) are identical 22-line functions differing only in the matched ElemIndex variant (FromFront vs FromBack); the three ORD families (674-727) then repeat the same pair-loop + guarded + label-format + push block three times.
- **Change:** One collector taking a `fn(&ElemIndex) -> Option<u32>` projection (or returning both maps in a single pass over qs), and a private `fn push_ord(&mut self, q_hi: QuantityId, q_lo: QuantityId)` used by all three families. Pure factoring; the family-selection conditions (i<j, k1<k2, i==0||k==1) stay exactly where they are.

### [complexity · M] `crates/adl-formula/src/encode.rs:1`
encode.rs is 1945 lines and the single Encoder impl mixes five separable concerns: region/boolean structure, the Dual bounded expansions, the composite 2D existence refinement, three substitution walkers, and linear extraction; adl-axioms/lib.rs (2101 lines) similarly packs the emitter set AND a second full element-predicate encoder into one file.
- **Change:** Split adl-formula/src/encode.rs on the existing section comments: comb.rs (2D composite existence), subst.rs (the walkers, or the shared walker from the other finding), leaving encode.rs with region/boolean/comparison logic. Move adl-axioms lines 1152-1652 (encode_elem_pred, encode_elem_pred_generic, encode_pred_exact, lin_pred, clear_ratio, abs_pred) into their own module (or into adl-formula per the mirroring finding) — they are consumed by EPRED and by adl-analysis/reconcile.rs, not by the other emitters.

### [duplication · S] `crates/adl-formula/src/encode.rs:1349`
cmp (1349) and cmp_node_const (1483) duplicate both the post-linearization finish (try_comb_existence then atom_of) and the LinErr mapping (NonFinite -> False, BadLiteral -> unknown, NonLinear -> pattern/unknown) that already exists as the lin_err helper (1645), which neither uses for those arms.
- **Change:** Give cmp a shared tail helper `fn finish_lin(&mut self, e: LinExpr, rel: Rel, span) -> Formula` used by both, and route the error arms through lin_err (pattern-dispatch for the const-side NonLinear case stays in the callers). Removes the triplicated unknown-message literal.

### [dead-code · S] `crates/adl-axioms/src/lib.rs:716`
Vestigial `let _ = coll;` in the front-to-back ORD loop (coll is genuinely used one line above in back.get(coll)), and a similar `let _ = why;` in encode.rs's ScalarMinMax equality arm (line 1462) where the binding could simply not be captured.
- **Change:** Delete `let _ = coll;`. In encode.rs, drop the `let _ = why;` (the parameter is used by the opaque_atom fallback arm, so it stays; the discard line is just noise).

### [duplication · S] `crates/adl-formula/src/encode.rs:173`
hnode_children in encode.rs duplicates adl_sema::HNode::children() arm-for-arm except for one deliberate difference (it omits the Reduce body), leaving two structural-walk child lists that must be kept in sync when HKind grows.
- **Change:** Express the intent once: implement hnode_children as `if matches!(kind, HKind::Reduce{..}) { vec![] } else { node.children() }`, or add a named `formula_visible_children()` next to children() in adl-sema with the soundness comment attached. Either way a new HKind variant gets one child list to update, and the Reduce exclusion stays explicit.

### [duplication · S] `crates/adl-axioms/src/lib.rs:839`
trig (839) and dphi (857) emit the same two-sided And(x <= B, x >= -B) shape with only the quantity filter and bound differing; and the Emit struct carries `tag_keys: [&'static str; 3]`, a pure compile-time constant stored as an instance field and re-built every emit_round.
- **Change:** Add `fn push_symmetric_bound(&mut self, q: QuantityId, id: AxiomId, bound: f64, label: &str)` used by both emitters, and make tag_keys a `const TAG_KEYS: [&str; 3]` matching the NNEG_EXTFN_KEYS pattern (dropping the struct field).


## Engine (adl-analysis, adl-solver, adl-certify)

### [complexity · L] `crates/adl-analysis/src/engine.rs:457`
Engine::pair is ~330 lines (with clippy::too_many_lines suppressed) mixing four distinct phases: interval fast path, disjointness+certify, subset checks, and a large inline overlap-witness retry loop.
- **Change:** Extract the overlap-witness search (lines 598-759 plus refined_model, tightened, snap_model, blocking_clause, MAX_WITNESS_ATTEMPTS, WITNESS_EPS) into a witness_search.rs (or a submodule of witness.rs) returning an enum {Validated(Model,String), Candidate(Model,String), Failed(Option<String>)}; pair() then only maps that to PairReport fields. Similarly move reconcile()/prove_pred_implies()/frame_sat()/existing_size_le() (lines 1002-1271) next to reconcile.rs so engine.rs is pure pairwise orchestration.

### [api-design · M] `crates/adl-analysis/src/engine.rs:730`
Witness-rejection classification is stringly typed across crate boundaries: engine.rs decides quiet-downgrade vs internal-bug by substring-matching error text produced in adl-interp and witness.rs.
- **Change:** Give adl-interp's EvalError a structured kind (Opaque, Open1, MissingData, Other) alongside the human reason, thread it through Validation::Rejected/Candidate as an enum field, and match on the enum in engine.rs. For spawn failures, add a distinct SatResult::Unknown reason kind (or a bool on the variant) instead of substring sniffing. This does not weaken any fail-closed path; it makes the existing classification robust.

### [complexity · M] `crates/adl-analysis/src/witness.rs:288`
build_event_json is a ~280-line monolith covering six commented phases; the phase comments already name the seams but everything shares one scope.
- **Change:** Introduce a small EventBuilder struct holding {hir, ext, model, mentioned, sizes, elem_pins, built} and turn each commented phase into a method (collect_pins, plan_families, build_objects, apply_defaults, normalize_pt, serialize). Pure mechanical split; behavior and phase order unchanged, each phase independently readable/testable.

### [duplication · S] `crates/adl-analysis/src/engine.rs:1378`
validated_witness_values and witness_values duplicate the same two-loop row-building logic (mentioned rows, then derived size rows) differing only in the value source — and inconsistently, only witness_values sorts its rows.
- **Change:** One helper fn witness_rows(hir, mentioned, value_of: impl Fn(QuantityId) -> Option<f64>) -> Vec<WitnessValue> used by both call sites; decide the sort question once (either both sort or neither, with a comment).

### [dead-code · S] `crates/adl-solver/src/lib.rs:141`
Unused public API in adl-solver: native_available(), SatResult::is_sat()/is_unsat(), and Model::is_empty() have no callers anywhere in the workspace.
- **Change:** Delete the four unused functions (or, if native_available is meant as external API for downstream consumers, say so in a doc comment; the CLI selects backends via SolverChoice/make_solver and never consults it).

### [duplication · S] `crates/adl-analysis/src/engine.rs:1480`
The Scripted test-double Solver is implemented twice, nearly identically, in the two #[cfg(test)] modules of engine.rs.
- **Change:** One #[cfg(test)] pub(crate) mod test_support with a single Scripted { seq, on_exhausted: enum } (or a bool), used by both modules — future soundness tests that need Unknown injection get it for free.

### [api-design · S] `crates/adl-analysis/src/lib.rs:230`
Engine is constructed as a bare 13-field struct literal at four sites, so every new field (recon_facts, gate_events, certify were all recent additions) forces edits to all constructors, most of which just want defaults.
- **Change:** Give Engine a fn new(hir, ext, unit, axioms, solver, solver_label, timeout, unit_name) that zero-initializes the accumulators (spawn_failures, recon_facts) and defaults the options (recon: None, gate_events: vec![], certify: false), with setters/with_* for the options. Shrinks each test constructor to 3-4 lines and removes a churn point.

### [structure · M] `crates/adl-analysis/src/render.rs:122`
reason_signature reverse-parses engine-generated English reason strings by prefix matching to reconstruct grouping keys — an implicit string protocol between engine.rs and render.rs that silently degrades on any wording change.
- **Change:** Have PairReport carry the structured pieces render needs (it already has kind, core, subset flags, witness_validated; add e.g. an optional interval-disjoint {quantity, iv_a, iv_b} detail or a small ReasonKind enum) and derive both the reason string and the signature from that one source, instead of formatting in engine and parsing in render.

### [performance · M] `crates/adl-analysis/src/engine.rs:847`
refined_model fetches the fallback base model eagerly before trying any wish layer, which under the subprocess backend costs a full extra z3 process spawn + complete re-solve per witness attempt even when the first wish layer succeeds (the common case).
- **Change:** Make the base-model fetch lazy (final `.or_else(|| s.model())`), and/or teach the subprocess backend to append (get-value ...) to the check-sat script and cache the last output so model() after check() parses the cached text instead of re-solving. Pure search-side work; no verdict path changes.

### [structure · S] `crates/adl-analysis/src/report.rs:347`
Two renderings live in two files: the ~145-line --explain renderer Report::human() sits inside the data-model file report.rs while the default renderer is in render.rs; both duplicate the verdict-count summary logic in different shapes.
- **Change:** Move Report::human()'s body into render.rs (render_explain), leaving report.rs as pure schema + FailOn gating, and share one verdict_counts(&[PairReport]) -> [usize; 6] helper between the two renderers.


## Runtime (adl-interp, adl-ingest)

### [complexity · L] `crates/adl-interp/src/eval.rs:674`
eval.rs is 2061 lines with one `Ev` impl carrying FOUR parallel full HKind dispatches (truth @1084, truth3 @831, num @1170, num3 @971) plus regions, reducers, collection materialization, composite-tuple enumeration, and angular/Lorentz kinematics — too much in one reasoning unit.
- **Change:** Split on the existing banner seams into submodules keeping `Ev` as the shared state type: `kleene.rs` (Tri, region3, truth3, num3 — isolated so the soundness-critical three-valued rules are reviewable alone), `collections.rs` (materialize*, comb_tuples, candidate_object, tuple_passes_cuts, enumerate_index_tuples), `kinematics.rs` (LV, Angles, angular*, lorentz, wrap_dphi). Do NOT merge the two- and three-valued evaluators — keep them parallel by design.

### [duplication · S] `crates/adl-interp/src/eval.rs:999`
The leaf arithmetic of the two-valued and three-valued evaluators is duplicated verbatim: the ArithOp match, the ScalarMinMax fold, and the Band lo/hi parse+check each appear twice.
- **Change:** Extract pure helpers used by both evaluators: `fn apply_arith(op: ArithOp, a: f64, b: f64) -> NumRes`, `fn fold_minmax(kind, acc, v)`, and `fn band_holds(kind, v, lo, hi) -> bool`. The soundness-relevant absorption/Unknown ordering stays in each caller; only the shared f64 arithmetic is deduplicated.

### [performance · L] `crates/adl-interp/src/event.rs:40`
EventObject is `BTreeMap<String, f64>`, so every property read in the interpreter's hottest path is a string-keyed tree lookup, and every filter/reducer/composite operation deep-clones the map.
- **Change:** Intern canonical property keys once (per Interp or per ExtDecls) to a small integer id and store objects as a sorted `Vec<(KeyId, f64)>` (or keep BTreeMap<KeyId, f64>); alternatively make collections `Rc<[Rc<EventObject>]>` so filter/union/reduce/binder paths share elements instead of cloning. Either change is transparent to semantics and removes both the string lookups and the hot-loop clones.

### [performance · S] `crates/adl-interp/src/eval.rs:2040`
tuple_passes_cuts clones the whole binder environment (HashMap<Symbol, EventObject>) once per CUT per tuple; hoisting the env swap out of the cut loop makes it once per tuple.
- **Change:** In comb_tuples, swap `env` into `self.binder_env` once per tuple, evaluate the candidate body and all cuts under it, then swap back and reuse the map for the stored CombTuple — one env installation per tuple, zero extra clones for cuts.

### [dead-code · S] `crates/adl-interp/src/event.rs:218`
`ChunkReader` (the inline-parsing streaming reader) is never constructed anywhere in the workspace; production uses RawChunkReader + RawChunk::parse. `EventObject::properties()` and `Interp::hir()` also have zero callers.
- **Change:** Delete `ChunkReader` and its Iterator impl (fix the two doc-comment references to say RawChunkReader), delete `EventObject::properties()` and `Interp::hir()`. Consider demoting the name-based `eval_region_membership` to `#[doc(hidden)]` or replacing its test uses with the idx variant so the collision-prone entry point disappears.

### [duplication · M] `crates/adl-interp/src/histo.rs:171`
Hist1D and Hist1DVar duplicate the entire 1-D accumulator body — 10 identical stat/flow fields, line-for-line identical merge() and fill() tails — and the reconciliation forces an 11-argument h1_tail_json.
- **Change:** Introduce `struct H1Body { sumw, sumw2, underflow_w, underflow_w2, overflow_w, overflow_w2, entries, tsumw, tsumw2, tsumwx, tsumwx2 }` with one merge() and one `fill_at(idx: Option<usize> /* None=under, .. */, x, w)`; Hist1D and Hist1DVar become a binning rule (`bin_of(x) -> Flow|Idx`) plus an H1Body, and h1_tail_json takes `&H1Body`.

### [structure · M] `crates/adl-interp/src/weights.rs:39`
HIR payloads are joined to region statements by (span.start, span.end) tuple equality in two independent places — weights.rs and HistoSet::new — a fragile implicit foreign key that should be an explicit index on the HIR marker.
- **Change:** Have adl-sema put the payload index on the marker (`HirRegionStmt::NonMembership { kind, payload: Option<u32>, .. }` pointing into hir.weights / hir.histos), then delete both span-keyed maps. The silent fallbacks become unreachable instead of load-bearing.

### [duplication · M] `crates/adl-ingest/src/reader.rs:679`
load_one_element and load_one_element_pair duplicate the entire one-element-branch protocol: counter read, non-finite scan, first-element walk, and the empty/multi diagnostic bookkeeping.
- **Change:** Factor a generic `load_one_element_n(tree, profile, branch, leaves: &[&str], ...) -> Option<Vec<Option<SmallVec<f64>>>>` (or take a closure per entry) that owns the counter walk and diagnostics once; the scalar and pair loaders become thin wrappers.

### [performance · S] `crates/adl-interp/src/eval.rs:714`
The untraced region() path still records a full StepEval trace (cloning every outcome, including EvalError strings) into a Vec that is immediately discarded.
- **Change:** Make region_walk take `Option<&mut Vec<StepEval>>` (or a small sink trait) and pass None from region(); region_traced passes Some. No semantic change — the walk order is identical.

### [api-design · S] `crates/adl-ingest/src/profile.rs:186` *(partially confirmed — see note)*
Profile::decides() reverse-engineers the [DECIDE] entries out of the data table by hardcoded name introspection (tag list ["btag","tautag"], prop == "m", branch == "FatJet"), so a new profile or renamed property silently drops its provenance entry.
- **Change:** Make decides explicit data: add `decides: Vec<(String, String)>` (or a small enum) to Profile, populated where delphes()/nanoaod() already document each [DECIDE] in comments; keep only weight_branch derived (it genuinely mirrors the table). This keeps the 'the table records the choice' claim in the module docs true by construction.
- **Verifier's correction:** decides() is hardcoded-introspective as described and does feed §6 provenance, but: (a) nanoaod's continuous b-tag discriminants correctly produce no `btag_bit` decide because NanoAOD makes no working-point-bit decision (documented in the profile), so that example is not a dropped entry; (b) drops are not fully silent — delphes decides are exactly pinned by a unit test; only an as-yet-nonexistent third profile is exposed; (c) the proposed explicit `decides` field contradicts the module's documented "never a second copy" single-source-of-truth invariant and would introduce a table-vs-decides di


## IO & harness (rootfile, adl-cli, adl-difftest)

### [duplication · M] `crates/adl-cli/src/cmd/dot.rs:14` *(partially confirmed — see note)*
The load-and-resolve prologue (read_file -> unit_name -> ExtDecls::legacy() -> analyze_str -> render diags to stderr -> has_errors -> "cannot X — resolve errors above" -> exit 1) is copy-pasted across six call sites.
- **Change:** Add one helper in cmd/mod.rs, e.g. `fn load_hir(file: &Path, what: &str) -> Result<Result<(String, String, adl_sema::Hir), ExitCode>, CliError>` that reads, resolves, prints diagnostics, and returns the exit-1 case; have all six sites call it. ExtDecls::legacy() can be built inside it (or passed) since every subcommand uses the same one.
- **Verifier's correction:** Real duplication exists at exactly 3 sites (dot.rs, objects.rs, run.rs) — a shared helper there is a sound suggestion. check.rs run continues across files instead of exiting (prints "FAILED", aggregates exit code), check.rs run_json emits diagnostics as JSON on stdout with silent stderr (documented schema), and verify.rs run_cross differs in warning rendering, unit naming (unit_labels), ext provenance, and message text. The helper should target only the three single-file human-mode subcommands; folding the other three in as suggested would alter documented CLI behavior.

### [complexity · M] `crates/adl-difftest/src/casegen.rs:728`
casegen.rs is 1197 lines and grows with every phase (Phase-6a alone added GExtra, MinMaxTernary, OrdPair, three pools, and new strategies); it already contains three self-labeled sections that are natural module seams.
- **Change:** Split into casegen/vocab.rs (GQuant/GNum/GCond/GExtra/GCase + pools + pool_for), casegen/render.rs (RenderCtx, render, cond_str and friends), casegen/strategy.rs (arb_*), re-exported from casegen/mod.rs. Do it before the next phase adds more shapes.

### [api-design · M] `crates/adl-cli/src/cmd/run.rs:392`
rootfile's consume-self builder forces write_root_file to deep-clone the entire accumulated RootFile before every single add just to recover from a per-object rejection — O(n^2) copying of all histogram payloads.
- **Change:** Give RootFile `&mut self` add methods (validation already happens before any mutation in add_h1/add_th2d_at, except dir_mut side effects in check_key — hoist the dir walk so a failed add leaves the builder untouched). Keep the chaining `self` methods as thin wrappers if the doctest style is worth preserving. Then write_root_file's snapshot/take dance deletes entirely.

### [duplication · S] `crates/adl-cli/src/cmd/verify.rs:310` *(partially confirmed — see note)*
run() and run_cross() duplicate the whole report-emission tail: json-vs-explain-vs-human_default output, internal_diagnostics mirroring, --fail-on findings printing, and exit-code derivation.
- **Change:** Extract `fn emit_report(report: &Report, label: &str, json: bool, explain: bool, color: bool, fail_on: &FailOn) -> u8` returning the exit contribution; run() keeps its per-unit header and the array-join, run_cross() calls it once. The explain-mode object_table append stays in run() as its extra step.
- **Verifier's correction:** run() and run_cross() genuinely duplicate the explain/human_default output branch, the `internal:` diagnostics mirroring, the `--fail-on fired:` block, and the exit-code derivation — an extraction is valid. But the JSON path differs materially: run() collects per-file JSON into an array with a documented shape invariant (array form whenever a directory was given, verify.rs:208-217) while run_cross() prints one JSON object directly, so the proposed emit_report(json: bool) signature that prints JSON itself would break run()'s array contract. A correct refactor keeps JSON handling in the callers 

### [duplication · S] `crates/rootfile/src/file.rs:434`
The offset-interleaved merge of object records and subdirectory records is written out twice as identical four-arm peekable match loops — once for record emission and once for the keys-list children.
- **Change:** Extract `fn interleave_by_offset<O, D>(objs: impl Iterator<Item=O>, dirs: impl Iterator<Item=D>, obj_off: impl Fn(&O)->usize, dir_off: impl Fn(&D)->usize, mut on_obj: impl FnMut(O), mut on_dir: impl FnMut(D))` (or a small merged-iterator helper) and call it from both places.

### [duplication · M] `crates/rootfile/src/lib.rs:324`
add_h1 and add_th1d_var_at duplicate the sumw/sumw2-length check, the `bad` closure, check_key, and an 18-field Th1d push; H1Spec and H1VarSpec themselves duplicate nine stat/flow fields differing only in axis description.
- **Change:** Have add_th1d_var_at validate its edges then delegate to a single internal `add_h1_inner(dir, name, axis: AxisSpec, stats..., labels)`; alternatively fold the shared nine fields into an `H1Stats` struct both specs embed. Keeps the public API, removes one of the two Th1d construction sites.

### [dead-code · S] `crates/adl-difftest/src/lib.rs:24`
The `CRATE_NAME` constant plus its `crate_is_wired` smoke test is vestigial bootstrap scaffolding — nothing outside each crate's own trivial test reads it, and the pattern is replicated across the workspace.
- **Change:** Delete `CRATE_NAME` and the `crate_is_wired` test in adl-difftest (and, in a separate sweep, the sibling crates). Anything that genuinely needs the crate name has env!("CARGO_PKG_NAME").

### [duplication · S] `crates/adl-cli/src/cmd/objects.rs:33`
The color-detection expression `std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()` is copy-pasted three times.
- **Change:** Add `pub fn stdout_color() -> bool` to cmd/mod.rs next to read_file/unit_name and call it from all three sites, so a future color policy change (e.g. CLICOLOR_FORCE) has one home.

### [duplication · S] `crates/adl-cli/src/cmd/run.rs:113`
The profile-lookup-or-usage-error block (including the exact "unknown profile `{}` (known: ...)" message) is duplicated between run.rs and ingest.rs.
- **Change:** Add `fn profile_or_usage(name: &str) -> Result<Profile, CliError>` in cmd/ingest.rs (run.rs already calls two other pub helpers there: print_profile_choices, print_diags) and use it from both commands.

### [duplication · S] `crates/adl-difftest/src/casegen.rs:364`
quant_str inlines a GProp -> "pT"/"Eta"/"BTag" match that duplicates the standalone prop_fn helper 50 lines below it.
- **Change:** Replace the inline match in quant_str with `prop_fn(prop)`.
