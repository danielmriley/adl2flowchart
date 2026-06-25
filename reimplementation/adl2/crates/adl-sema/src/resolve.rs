//! Name resolution: AST → HIR + QuantityTable (SPEC_ARCHITECTURE §4).
//!
//! Resolution facts established here, by construction:
//! - pure renames (`object X take Y`, no cuts) bind the SAME
//!   `CollectionId` as their source (transitively);
//! - filtered collections are distinct identities from their parents;
//! - indexed element properties, oriented angular pairs, and external
//!   functions intern structurally — no string canonicalization;
//! - numeric defines inline their body HIR at every reference; boolean
//!   defines inline their predicate; definition cycles are errors;
//! - every HIR node carries an `InFragment`/`Unsupported(reason)` tag.

use crate::dump::RenderCtx;
use crate::ext::ExtDecls;
use crate::hir::{
    ArithOp, DefineKind, ElemPred, Fragment, HKind, HNode, Hir, HirDefine, HirHisto, HirObject,
    HirRegion, HirRegionStmt, HirWeight, HirWeightValue, HistoSpec, ReduceKind,
};
use crate::intern::{Symbol, SymbolTable};
use crate::quantity::{
    AngKind, CombAxis, CombKind, Collection, CollectionId, CompositeBinder, CompositeCandidate,
    ElemIndex, ElemPredId, ParticleRef, PropId, Quantity, QuantityArg, QuantityId, QuantityTable,
    ScalarSource, SortDir, SortKey,
};
use adl_syntax::ast::{
    self, Arg, BinBody, BinOp, CmpOp, Expr, HistoArg, IndexVal, ObjectKw, ObjectStmt, RegionKw,
    RegionStmt, Section, TakeSource, UnaryOp,
};
use adl_syntax::diag::Diagnostic;
use adl_syntax::span::Span;
use std::collections::{HashMap, HashSet};

/// Resolve a parsed file into HIR. `unit` labels the analysis unit
/// (usually the file name); `ext` is the ingested standard library.
#[must_use]
pub fn analyze(file: &ast::File, unit: &str, ext: &ExtDecls) -> Hir {
    let mut r = Resolver::new(file, ext);
    r.run();
    r.finish(unit)
}

/// Convenience: parse `src` and resolve it; parse diagnostics are merged
/// in front of sema diagnostics.
#[must_use]
pub fn analyze_str(src: &str, unit: &str, ext: &ExtDecls) -> Hir {
    let parsed = adl_syntax::parse(src);
    let mut hir = analyze(&parsed.file, unit, ext);
    let mut diags = parsed.diags;
    diags.append(&mut hir.diags);
    hir.diags = diags;
    hir
}

#[derive(Debug, Clone, PartialEq)]
enum State<T> {
    Pending,
    InProgress,
    Done(T),
}

/// Expression-resolution context.
#[derive(Default, Clone)]
struct Ctx {
    /// Inside an object block: the source collection being filtered.
    elem_source: Option<CollectionId>,
    /// Binder names meaning "this element" (single-binder take).
    elem_aliases: HashSet<String>,
    /// Composite binder slots (multi-binder takes).
    binders: HashMap<String, ParticleRef>,
    /// Inside a `trigger` statement: bare names are trigger flags.
    in_trigger: bool,
    /// Inside a reducer body, the iteration collection whose references
    /// denote the current element (`X` in `any(pt(X) > 30)`); set on the
    /// second resolve pass once the body's single plural collection is
    /// known. Interpret-only (P1).
    reduce_coll: Option<CollectionId>,
    /// Inside a reducer body, `this` (and the enclosing object-block's own
    /// name) denotes the *outer* filtered element as a particle
    /// ([`ParticleRef::ThisElem`]) rather than the implicit subject.
    this_as_particle: bool,
}

/// What an expression denotes when used as an object/particle argument.
enum Target {
    Coll(CollectionId),
    Particle(ParticleRef),
    Met,
    /// The implicit element of the enclosing object block.
    ElemSelf,
    None,
}

struct Resolver<'a> {
    ext: &'a ExtDecls,
    symbols: SymbolTable,
    table: QuantityTable,
    coll_names: Vec<Vec<Symbol>>,
    elem_preds: Vec<ElemPred>,
    elem_pred_ids: HashMap<String, ElemPredId>,

    ast_objects: Vec<&'a ast::ObjectBlock>,
    ast_defines: Vec<&'a ast::Define>,
    ast_regions: Vec<&'a ast::RegionBlock>,
    objects_by_key: HashMap<String, usize>,
    defines_by_key: HashMap<String, usize>,

    obj_state: Vec<State<CollectionId>>,
    obj_hir: Vec<Option<HirObject>>,
    def_state: Vec<State<(DefineKind, HNode)>>,

    regions: Vec<HirRegion>,
    regions_by_key: HashMap<String, usize>,
    region_name_order: Vec<Symbol>,
    histolist_regions: Vec<bool>,
    /// Histo statements seen during region resolution; their fill
    /// expressions resolve AFTER all regions so histogram-only
    /// quantities intern at the end of the table (membership interning
    /// order — and every report keyed on it — is unchanged by histos).
    pending_histos: Vec<(usize, &'a RegionStmt)>,
    histos: Vec<HirHisto>,
    weights: Vec<HirWeight>,

    diags: Vec<Diagnostic>,
    warned_names: HashSet<String>,
}

impl<'a> Resolver<'a> {
    fn new(file: &'a ast::File, ext: &'a ExtDecls) -> Self {
        let mut ast_objects = Vec::new();
        let mut ast_defines = Vec::new();
        let mut ast_regions = Vec::new();
        for section in &file.sections {
            match section {
                Section::Object(o) => ast_objects.push(o),
                Section::Define(d) => ast_defines.push(d),
                Section::Region(r) => ast_regions.push(r),
                Section::Info(_) | Section::Table(_) | Section::CountsFormat(_) => {}
            }
        }
        let mut objects_by_key = HashMap::new();
        for (i, o) in ast_objects.iter().enumerate() {
            // First binding wins; a duplicate is diagnosed when resolved.
            objects_by_key
                .entry(o.name.name.to_ascii_lowercase())
                .or_insert(i);
        }
        let mut defines_by_key = HashMap::new();
        for (i, d) in ast_defines.iter().enumerate() {
            defines_by_key
                .entry(d.name.name.to_ascii_lowercase())
                .or_insert(i);
        }
        let obj_state = vec![State::Pending; ast_objects.len()];
        let obj_hir = vec![None; ast_objects.len()];
        let def_state = vec![State::Pending; ast_defines.len()];
        Self {
            ext,
            symbols: SymbolTable::default(),
            table: QuantityTable::default(),
            coll_names: Vec::new(),
            elem_preds: Vec::new(),
            elem_pred_ids: HashMap::new(),
            ast_objects,
            ast_defines,
            ast_regions,
            objects_by_key,
            defines_by_key,
            obj_state,
            obj_hir,
            def_state,
            regions: Vec::new(),
            regions_by_key: HashMap::new(),
            region_name_order: Vec::new(),
            histolist_regions: Vec::new(),
            pending_histos: Vec::new(),
            histos: Vec::new(),
            weights: Vec::new(),
            diags: Vec::new(),
            warned_names: HashSet::new(),
        }
    }

    fn run(&mut self) {
        for i in 0..self.ast_objects.len() {
            self.resolve_object(i);
        }
        for i in 0..self.ast_defines.len() {
            self.resolve_define(i);
        }
        for i in 0..self.ast_regions.len() {
            self.resolve_region(i);
        }
        self.resolve_pending_histos();
    }

    /// Resolve histo fill expressions last (see `pending_histos`).
    fn resolve_pending_histos(&mut self) {
        let pending = std::mem::take(&mut self.pending_histos);
        let ctx = Ctx::default();
        for (region, stmt) in pending {
            let RegionStmt::Histo {
                name,
                title,
                args,
                span,
            } = stmt
            else {
                unreachable!("pending_histos holds only Histo statements");
            };
            let spec = self.resolve_histo_spec(args, &ctx);
            self.histos.push(HirHisto {
                region,
                name: name.name.clone(),
                title: title.value.clone(),
                spec,
                span: *span,
            });
        }
    }

    fn finish(mut self, unit: &str) -> Hir {
        let objects = self
            .obj_hir
            .iter_mut()
            .filter_map(Option::take)
            .collect::<Vec<_>>();
        let defines = self
            .ast_defines
            .iter()
            .enumerate()
            .map(|(i, d)| {
                let (kind, body) = match &self.def_state[i] {
                    State::Done(done) => done.clone(),
                    // Unreachable in practice: every define is resolved in
                    // `run`; keep a defensive placeholder.
                    _ => (
                        DefineKind::Numeric,
                        HNode::unsupported(d.span, "unresolved define"),
                    ),
                };
                HirDefine {
                    name: self.symbols.intern(&d.name.name),
                    kind,
                    body,
                    span: d.span,
                }
            })
            .collect();
        Hir {
            unit: unit.to_owned(),
            symbols: self.symbols,
            table: self.table,
            coll_names: self.coll_names,
            elem_preds: self.elem_preds,
            objects,
            defines,
            regions: self.regions,
            region_name_order: self.region_name_order,
            histolist_regions: self.histolist_regions,
            histos: self.histos,
            weights: self.weights,
            diags: self.diags,
        }
    }

    // ---- shared helpers -------------------------------------------------

    fn warn_once(&mut self, key: String, d: Diagnostic) {
        if self.warned_names.insert(key) {
            self.diags.push(d);
        }
    }

    fn intern_coll(&mut self, c: Collection) -> CollectionId {
        let id = self.table.intern_collection(c);
        while self.coll_names.len() <= id.0 as usize {
            self.coll_names.push(Vec::new());
        }
        id
    }

    fn bind_coll_name(&mut self, id: CollectionId, name: &str) {
        let sym = self.symbols.intern(name);
        let names = &mut self.coll_names[id.0 as usize];
        if !names.contains(&sym) {
            names.push(sym);
        }
    }

    fn render_node(&self, node: &HNode) -> String {
        RenderCtx {
            symbols: &self.symbols,
            table: &self.table,
            coll_names: &self.coll_names,
            region_names: &self.region_name_order,
        }
        .node(node)
    }

    fn intern_prop(&mut self, name: &str) -> PropId {
        let (key, display) = self.ext.prop_canon(name);
        self.table.intern_prop(&key, &display)
    }

    fn is_met_coll(&self, id: CollectionId) -> bool {
        matches!(self.table.collection(id), Collection::Base(sym)
            if self.symbols.key(*sym) == crate::ext::MET_FAMILY_KEY)
    }

    fn met_scalar(&mut self, prop_name: &str, span: Span) -> HNode {
        let prop = self.intern_prop(prop_name);
        let q = self
            .table
            .intern_quantity(Quantity::EventScalar(ScalarSource::MetProp(prop)));
        HNode::new(HKind::Quantity(q), span)
    }

    /// Wrap an interned quantity as a value node. Back-indexed elements
    /// (`coll[-k]`, OPEN-3) are in-fragment: the interpreter resolves `[-k]`
    /// to position `len - k`, and the encoder carries it as an interned
    /// `ElemProp { index: FromBack(k) }` leaf — a free per-event value with
    /// the existence guard `size(coll) >= k` (see `quantity_existence`) and
    /// no front-element ordering axioms (ORD/IDOM/SUB match `FromFront` only,
    /// so they soundly skip it).
    fn quantity_node(&self, q: QuantityId, span: Span) -> HNode {
        HNode::new(HKind::Quantity(q), span)
    }

    fn index_val(v: IndexVal) -> ElemIndex {
        let n = u32::try_from(v.value).unwrap_or(u32::MAX);
        if v.neg {
            ElemIndex::FromBack(n)
        } else {
            ElemIndex::FromFront(n)
        }
    }

    /// Canonicalize indexed access into a static slice
    /// `slice[a:b][i] → src[a+i]` (P2), so the access inherits the source's
    /// ORD/IDOM/EPRED and cross-region identity. Sound **only** for concrete
    /// `start` and an in-range front index:
    ///
    /// - concrete `end`, `i < end-start`: the slice's element `i` IS
    ///   `src[start+i]` (half-open contiguity); rebase. Its existence guard
    ///   `size(src) > start+i` is exactly the remaining condition (the slice
    ///   upper bound `i < end-start` already holds statically).
    /// - `end = None` (`[a:]`): any front index is potentially in-slice;
    ///   rebase to `src[start+i]`.
    /// - concrete `end`, `i >= end-start`: the element is **statically
    ///   absent** from the slice; keep `slice[i]` so SZSLICE
    ///   (`size(slice) ≤ end-start ≤ i`) makes its existence guard
    ///   unsatisfiable — the access is a missing element (sound).
    ///
    /// A `FromBack` index is never rebased (reserved, OPEN-3). A non-slice
    /// collection passes through unchanged.
    fn rebase_slice_index(
        &self,
        coll: CollectionId,
        index: ElemIndex,
    ) -> (CollectionId, ElemIndex) {
        let ElemIndex::FromFront(i) = index else {
            return (coll, index);
        };
        let Collection::Slice { source, start, end } = *self.table.collection(coll) else {
            return (coll, index);
        };
        if let Some(end) = end {
            // Statically out of the slice ⇒ keep the (absent) slice element.
            if i >= end.saturating_sub(start) {
                return (coll, index);
            }
        }
        match start.checked_add(i) {
            Some(abs) => (source, ElemIndex::FromFront(abs)),
            None => (coll, index),
        }
    }

    // ---- objects --------------------------------------------------------

    /// Resolve a collection-valued name (take sources, function args).
    fn resolve_collection_name(&mut self, name: &str, span: Span) -> CollectionId {
        if let Some(&idx) = self.objects_by_key.get(&name.to_ascii_lowercase()) {
            return self.resolve_object(idx);
        }
        self.resolve_base_name(name, span)
    }

    /// Resolve a name as an external (or private) base collection,
    /// bypassing user object blocks.
    fn resolve_base_name(&mut self, name: &str, span: Span) -> CollectionId {
        if let Some(canon) = self.ext.base_collection(name) {
            let canon = canon.to_owned();
            let sym = self.symbols.intern(&canon);
            return self.intern_coll(Collection::Base(sym));
        }
        self.warn_once(
            format!("coll:{}", name.to_ascii_lowercase()),
            Diagnostic::warning(
                span,
                format!("unknown collection `{name}`; treated as a private base collection"),
            ),
        );
        let sym = self.symbols.intern(name);
        self.intern_coll(Collection::Base(sym))
    }

    fn resolve_object(&mut self, idx: usize) -> CollectionId {
        match &self.obj_state[idx] {
            State::Done(id) => return *id,
            State::InProgress => {
                let obj = self.ast_objects[idx];
                let name = obj.name.name.clone();
                let span = obj.name.span;
                self.warn_once(
                    format!("objcycle:{}", name.to_ascii_lowercase()),
                    Diagnostic::error(span, format!("object take cycle involving `{name}`")),
                );
                let sym = self.symbols.intern(&name);
                return self.intern_coll(Collection::Base(sym));
            }
            State::Pending => {}
        }
        self.obj_state[idx] = State::InProgress;
        let obj = self.ast_objects[idx];
        let self_key = obj.name.name.to_ascii_lowercase();

        // Combinatorial composite block (`take comb/disjoint/cartesian`, a
        // multi-binder take, or the `composite` keyword): a collection of
        // tuples with binder axes and an optional candidate. Resolved
        // separately so its binder/candidate/per-tuple-cut model does not
        // pollute the element-filter path.
        if Self::is_composite_block(obj) {
            return self.resolve_composite(idx);
        }

        let mut sources: Vec<CollectionId> = Vec::new();
        let mut cuts: Vec<(bool, &Expr)> = Vec::new(); // (is_reject, cond)
        let mut ctx = Ctx::default();
        let mut alias_names: Vec<String> = Vec::new();
        let mut unsupported_reason: Option<String> = None;

        for stmt in &obj.stmts {
            match stmt {
                ObjectStmt::Take {
                    source,
                    binders,
                    alias,
                    ..
                } => {
                    let src = match source {
                        TakeSource::Ident(id) => {
                            // `object met take MET`: a source spelled like
                            // the block's own name refers to the external
                            // base, not to the block being defined.
                            if id.name.to_ascii_lowercase() == self_key {
                                Some(self.resolve_base_name(&id.name, id.span))
                            } else {
                                Some(self.resolve_collection_name(&id.name, id.span))
                            }
                        }
                        TakeSource::Union { members, .. } => {
                            let ids: Vec<CollectionId> = members
                                .iter()
                                .map(|m| self.resolve_collection_name(&m.name, m.span))
                                .collect();
                            Some(match ids.as_slice() {
                                [single] => *single,
                                _ => self.intern_coll(Collection::Union(ids)),
                            })
                        }
                        TakeSource::Call { name, args } => {
                            if name.name.eq_ignore_ascii_case("sort") {
                                // `sort(coll, key, dir)` is a re-sorted
                                // permutation of `coll`: a distinct collection
                                // whose element *set* is the source's, with the
                                // key's per-index order. The interpreter
                                // re-sorts; the analyzer never asserts an
                                // index-ordering fact unless P2's exact
                                // pt-descending alias gate fires (P1: opaque).
                                self.resolve_sort_source(args, &ctx)
                            } else {
                                unsupported_reason = Some(format!(
                                    "take source `{}(...)` is not supported",
                                    name.name
                                ));
                                None
                            }
                        }
                        // `take coll[2:]` / `take coll[:4]`: a sliced source.
                        TakeSource::Expr(e) => {
                            let src = self.target_collection(e, &ctx);
                            if src.is_none() {
                                unsupported_reason =
                                    Some("slice take source is not a collection".to_owned());
                            }
                            src
                        }
                    };
                    if let Some(src) = src {
                        // Single-binder take (`take jets j`): the binder is the
                        // implicit element. Multi-binder / composite takes never
                        // reach here (dispatched to `resolve_composite`).
                        if let Some(b) = binders.first() {
                            ctx.elem_aliases.insert(b.name.to_ascii_lowercase());
                        }
                        sources.push(src);
                    }
                    if let Some(a) = alias {
                        alias_names.push(a.name.clone());
                    }
                }
                ObjectStmt::Cut { cond, .. } => cuts.push((false, cond)),
                ObjectStmt::Reject { cond, .. } => cuts.push((true, cond)),
                // `Derived` only appears in composite blocks (handled by
                // `resolve_composite`); ignore it defensively here.
                ObjectStmt::Derived { .. } => {}
            }
        }

        let self_sym = self.symbols.intern(&obj.name.name);
        let combined = match sources.as_slice() {
            [] => {
                self.warn_once(
                    format!("notake:{}", obj.name.name.to_ascii_lowercase()),
                    Diagnostic::warning(
                        obj.name.span,
                        format!("object `{}` has no take statement", obj.name.name),
                    ),
                );
                self.intern_coll(Collection::Base(self_sym))
            }
            [single] => *single,
            _ => self.intern_coll(Collection::Union(sources.clone())),
        };

        let (coll, pure_alias_of) = if cuts.is_empty() {
            // No cuts: `object X take Y` is a pure rename — identity with
            // its source is a theorem of the semantics (SPEC_LANGUAGE §4.2),
            // so the analyzer unifies as a resolution fact.
            let alias = (sources.len() == 1).then_some(combined);
            (combined, alias)
        } else {
            ctx.elem_source = Some(combined);
            // Inside this block's cuts, the block's own name means the
            // implicit element (`select pdgID(OSdileptons) == 0`).
            ctx.elem_aliases.insert(self_key.clone());
            let pred_parts: Vec<HNode> = cuts
                .iter()
                .map(|(is_reject, cond)| {
                    let node = self.resolve_expr(cond, &ctx);
                    if *is_reject {
                        let span = node.span;
                        HNode::new(HKind::Not(Box::new(node)), span)
                    } else {
                        node
                    }
                })
                .collect();
            let pred = match pred_parts.len() {
                1 => pred_parts.into_iter().next().expect("len checked"),
                _ => {
                    let span = obj.span;
                    HNode::new(HKind::And(pred_parts), span)
                }
            };
            let pred_id = self.intern_elem_pred(pred);
            let id = self.intern_coll(Collection::Filtered {
                parent: combined,
                pred: pred_id,
            });
            (id, None)
        };

        self.bind_coll_name(coll, &obj.name.name);
        for alias in &alias_names {
            self.bind_coll_name(coll, alias);
        }

        let tag = match unsupported_reason {
            Some(reason) => Fragment::Unsupported(reason),
            None => Fragment::InFragment,
        };
        self.obj_hir[idx] = Some(HirObject {
            name: self_sym,
            coll,
            pure_alias_of,
            tag,
            span: obj.span,
        });
        self.obj_state[idx] = State::Done(coll);
        coll
    }

    /// A block is a combinatorial composite if it is declared `composite`, has
    /// any `take comb/disjoint/cartesian(...)`, has a multi-binder take, or
    /// declares a `candidate`/`derived` axis.
    fn is_composite_block(obj: &ast::ObjectBlock) -> bool {
        if obj.keyword == ObjectKw::Composite {
            return true;
        }
        obj.stmts.iter().any(|s| match s {
            ObjectStmt::Take {
                source, binders, ..
            } => {
                binders.len() > 1
                    || matches!(source, TakeSource::Call { name, .. }
                        if matches!(name.name.to_ascii_lowercase().as_str(),
                            "comb" | "disjoint" | "cartesian"))
            }
            ObjectStmt::Derived { .. } => true,
            _ => false,
        })
    }

    /// Resolve `sort(coll, key, dir)` as a take source into a
    /// [`Collection::Sorted`]. The source is the first collection-valued
    /// argument; the key is a per-element property when recognizable
    /// (else opaque); the direction is `descend` unless `ascend` is written.
    fn resolve_sort_source(&mut self, args: &[Arg], ctx: &Ctx) -> Option<CollectionId> {
        let exprs: Vec<&Expr> = args
            .iter()
            .filter_map(|a| if let Arg::Expr(e) = a { Some(e.as_ref()) } else { None })
            .collect();
        let source = exprs
            .iter()
            .find_map(|e| self.target_collection(e, ctx))?;
        // Direction: an `ascend` identifier anywhere flips to ascending;
        // everything else (including the common `descend`) is descending.
        let dir = if exprs.iter().any(|e| {
            matches!(e, Expr::Ident(id) if id.name.eq_ignore_ascii_case("ascend"))
        }) {
            SortDir::Ascend
        } else {
            SortDir::Descend
        };
        // Key: the first argument that is a single per-element property of the
        // source (`pt(coll)`), reduced to that `PropId`. Anything else stays
        // opaque so the analyzer never aliases a non-pt / unknown-key sort.
        let key = self
            .sort_prop_key(&exprs, source, ctx)
            .map_or_else(|| SortKey::Opaque(self.sort_key_render(&exprs, ctx)), SortKey::Prop);
        // SORT→ALIAS (P2, soundness-critical, plan §risk 1): a stable
        // descending-pT sort of an already-pT-descending source is the
        // IDENTITY permutation, so it canonicalizes to the source itself
        // (exactly the `pure_alias_of` posture) and inherits ORD/IDOM/EPRED.
        // Gated on the EXACT shape `key == Prop(pt) ∧ dir == Descend ∧
        // pt_ordered(source)` via STRUCTURAL key-quantity equality (the pT
        // PropId compared by canonical key, never a substring — the Bug-6 TAG
        // lesson), defaulting to NO alias. Any other key/dir/source ⇒ an
        // opaque `Sorted` carrying only SZPERM (size = size(source)) and
        // `pt_ordered = false`. A single wrong match here fabricates a false
        // PROVEN via ORD on the UNSAT side with no witness net.
        let pt_key = self.ext.prop_canon("pt").0;
        let is_pt_desc = dir == SortDir::Descend
            && matches!(&key, SortKey::Prop(p) if self.table.prop_key(*p) == pt_key);
        if is_pt_desc && self.table.pt_ordered(source, &pt_key) {
            return Some(source);
        }
        Some(self.intern_coll(Collection::Sorted { source, key, dir }))
    }

    /// The per-element property a sort key reduces to, when the key argument is
    /// `prop(source)` / `source.prop` over the sorted collection (so the
    /// interpreter can re-sort by it). `None` for any other key.
    fn sort_prop_key(&mut self, exprs: &[&Expr], source: CollectionId, ctx: &Ctx) -> Option<PropId> {
        for e in exprs {
            // `pt(coll)` or `{coll}pt` or `coll.pt`: a CollProp over the source.
            let node = self.resolve_expr(e, ctx);
            if let HKind::CollProp { coll, prop } = node.kind
                && coll == source
            {
                return Some(prop);
            }
        }
        None
    }

    /// Canonical render of a sort key list (the opaque-key interning identity).
    fn sort_key_render(&mut self, exprs: &[&Expr], ctx: &Ctx) -> String {
        let parts: Vec<String> = exprs
            .iter()
            .map(|e| {
                let n = self.resolve_expr(e, ctx);
                self.render_node(&n)
            })
            .collect();
        parts.join(",")
    }

    /// Resolve a combinatorial composite block (`take disjoint/cartesian/comb`,
    /// a multi-binder take, or a `candidate` axis). USER ANSWER 4: per-tuple
    /// `select`/`reject` filter the candidate collection; disjoint distinctness
    /// is by kinematic value (handled by the interpreter).
    fn resolve_composite(&mut self, idx: usize) -> CollectionId {
        let obj = self.ast_objects[idx];
        let self_sym = self.symbols.intern(&obj.name.name);

        let mut members: Vec<CompositeBinder> = Vec::new();
        let mut parts: Vec<CollectionId> = Vec::new();
        let mut binders: HashMap<String, ParticleRef> = HashMap::new();
        let mut kind = CombKind::Cartesian;
        let mut candidate_decl: Option<(&ast::Ident, &Expr)> = None;
        let mut cut_exprs: Vec<(bool, &Expr)> = Vec::new();

        for stmt in &obj.stmts {
            match stmt {
                ObjectStmt::Take {
                    source, binders: bs, ..
                } => match source {
                    // `take disjoint/cartesian/comb(src1 b1, src2 b2, ...)`.
                    TakeSource::Call { name, args } => {
                        kind = match name.name.to_ascii_lowercase().as_str() {
                            "disjoint" => CombKind::Disjoint,
                            _ => CombKind::Cartesian, // comb/cartesian
                        };
                        for a in args {
                            if let Arg::Expr(e) = a
                                && let Expr::ParticleList { items, .. } = e.as_ref()
                                && let [src_e, ast::Expr::Ident(bind)] = items.as_slice()
                                && let Some(src) = self.target_collection(src_e, &Ctx::default())
                            {
                                let bname = self.symbols.intern(&bind.name);
                                binders.insert(
                                    bind.name.to_ascii_lowercase(),
                                    ParticleRef::Binder { coll: src, name: bname },
                                );
                                members.push(CompositeBinder { name: bname, source: src });
                                parts.push(src);
                            }
                        }
                    }
                    // `take coll b1, b2` — multi-binder cartesian over one
                    // source (possibly a sliced one, `take coll[2:] a, b`).
                    TakeSource::Ident(_) | TakeSource::Union { .. } | TakeSource::Expr(_) => {
                        let src = match source {
                            TakeSource::Ident(id) => {
                                self.resolve_collection_name(&id.name, id.span)
                            }
                            TakeSource::Union { members: ms, .. } => {
                                let ids: Vec<CollectionId> = ms
                                    .iter()
                                    .map(|m| self.resolve_collection_name(&m.name, m.span))
                                    .collect();
                                match ids.as_slice() {
                                    [single] => *single,
                                    _ => self.intern_coll(Collection::Union(ids)),
                                }
                            }
                            TakeSource::Expr(e) => match self.target_collection(e, &Ctx::default()) {
                                Some(c) => c,
                                None => continue, // not a collection ⇒ skip slot
                            },
                            TakeSource::Call { .. } => unreachable!(),
                        };
                        for b in bs {
                            let bname = self.symbols.intern(&b.name);
                            binders.insert(
                                b.name.to_ascii_lowercase(),
                                ParticleRef::Binder { coll: src, name: bname },
                            );
                            members.push(CompositeBinder { name: bname, source: src });
                            parts.push(src);
                        }
                    }
                },
                ObjectStmt::Derived { name, body, .. } => {
                    candidate_decl = Some((name, body));
                }
                ObjectStmt::Cut { cond, .. } => cut_exprs.push((false, cond)),
                ObjectStmt::Reject { cond, .. } => cut_exprs.push((true, cond)),
            }
        }

        // The binder environment for the candidate body and per-tuple cuts.
        let comb_ctx = Ctx {
            binders: binders.clone(),
            ..Ctx::default()
        };

        // Candidate axis: `candidate ll = l1 + l2` becomes a `ParticleRef::Sum`
        // over the tuple binders (the only supported candidate shape).
        let candidate = candidate_decl.and_then(|(name, body)| {
            match self.resolve_target(body, &comb_ctx) {
                Target::Particle(p @ (ParticleRef::Sum(_) | ParticleRef::Binder { .. })) => {
                    Some(CompositeCandidate {
                        name: self.symbols.intern(&name.name),
                        vector: p,
                    })
                }
                _ => None,
            }
        });

        // Per-tuple cuts may reference the candidate by name (`select mass(ll)`):
        // bind it as a particle alongside the binders.
        let mut cut_ctx = comb_ctx;
        if let (Some((name, _)), Some(cand)) = (candidate_decl, &candidate) {
            cut_ctx
                .binders
                .insert(name.name.to_ascii_lowercase(), cand.vector.clone());
        }
        // The block's own name inside its cuts means the implicit tuple
        // (`select pdgID(OSdileptons) == 0`); treat it as an element alias so
        // it never re-enters `resolve_object` (which would diagnose a spurious
        // take cycle). The composite is Unsupported regardless.
        cut_ctx
            .elem_aliases
            .insert(obj.name.name.to_ascii_lowercase());

        // Per-tuple cuts: predicates over the tuple binders, interned exactly
        // like an element predicate. They filter the candidate collection.
        let cuts: Vec<ElemPredId> = cut_exprs
            .iter()
            .map(|(is_reject, cond)| {
                let node = self.resolve_expr(cond, &cut_ctx);
                let node = if *is_reject {
                    let span = node.span;
                    HNode::new(HKind::Not(Box::new(node)), span)
                } else {
                    node
                };
                self.intern_elem_pred(node)
            })
            .collect();

        let coll = self.intern_coll(Collection::Combination {
            parts,
            kind,
            members,
            candidate,
            cuts,
        });
        self.bind_coll_name(coll, &obj.name.name);

        // The composite is interpret-only (P1): the analyzer keeps it opaque
        // (size/existence-only lands in P2). Tag so the verifier treats any
        // membership reference as Unknown rather than reasoning over tuples.
        self.obj_hir[idx] = Some(HirObject {
            name: self_sym,
            coll,
            pure_alias_of: None,
            tag: Fragment::unsupported(
                "combinatorial composite is outside the checked fragment (interpret-only, P1)",
            ),
            span: obj.span,
        });
        self.obj_state[idx] = State::Done(coll);
        coll
    }

    fn intern_elem_pred(&mut self, node: HNode) -> ElemPredId {
        let render = self.render_node(&node);
        if let Some(&id) = self.elem_pred_ids.get(&render) {
            return id;
        }
        let id = ElemPredId(u32::try_from(self.elem_preds.len()).expect("pred id overflow"));
        self.elem_pred_ids.insert(render.clone(), id);
        self.elem_preds.push(ElemPred { node, render });
        id
    }

    // ---- defines ----------------------------------------------------------

    fn resolve_define(&mut self, idx: usize) -> (DefineKind, HNode) {
        match &self.def_state[idx] {
            State::Done(done) => return done.clone(),
            State::InProgress => {
                let def = self.ast_defines[idx];
                let name = def.name.name.clone();
                self.diags.push(Diagnostic::error(
                    def.name.span,
                    format!("definition cycle involving `{name}`"),
                ));
                return (
                    DefineKind::Numeric,
                    HNode::unsupported(def.span, format!("definition cycle involving `{name}`")),
                );
            }
            State::Pending => {}
        }
        self.def_state[idx] = State::InProgress;
        let def = self.ast_defines[idx];
        let ctx = Ctx::default();
        let body = self.resolve_expr(&def.body, &ctx);
        let kind = if Self::is_boolean(&body) {
            DefineKind::Boolean
        } else {
            DefineKind::Numeric
        };
        self.def_state[idx] = State::Done((kind, body.clone()));
        (kind, body)
    }

    fn is_boolean(node: &HNode) -> bool {
        match &node.kind {
            HKind::Bool(_)
            | HKind::Cmp { .. }
            | HKind::Band { .. }
            | HKind::And(_)
            | HKind::Or(_)
            | HKind::Not(_)
            | HKind::RegionPred(_) => true,
            HKind::Ternary { then, els, .. } => {
                Self::is_boolean(then) && els.as_deref().is_none_or(Self::is_boolean)
            }
            _ => false,
        }
    }

    // ---- regions ----------------------------------------------------------

    fn resolve_region(&mut self, idx: usize) {
        let region = self.ast_regions[idx];
        let ctx = Ctx::default();
        let mut stmts = Vec::new();
        for stmt in &region.stmts {
            match stmt {
                RegionStmt::Cut { cond, .. } => {
                    stmts.push(HirRegionStmt::Select(self.resolve_expr(cond, &ctx)));
                }
                RegionStmt::Reject { cond, .. } => {
                    stmts.push(HirRegionStmt::Reject(self.resolve_expr(cond, &ctx)));
                }
                RegionStmt::RegionRef(id) => {
                    let key = id.name.to_ascii_lowercase();
                    if let Some(&prior) = self.regions_by_key.get(&key) {
                        stmts.push(HirRegionStmt::Inherit {
                            region: prior,
                            span: id.span,
                        });
                    } else if let Some(&didx) = self.defines_by_key.get(&key) {
                        let (kind, body) = self.resolve_define(didx);
                        if kind == DefineKind::Numeric {
                            self.diags.push(Diagnostic::warning(
                                id.span,
                                format!("numeric define `{}` used as a predicate", id.name),
                            ));
                        }
                        stmts.push(HirRegionStmt::Select(body));
                    } else {
                        self.diags.push(Diagnostic::error(
                            id.span,
                            format!("`{}` does not name a prior region or a define", id.name),
                        ));
                        stmts.push(HirRegionStmt::NonMembership {
                            kind: "unresolved-ref",
                            tag: Fragment::unsupported(format!(
                                "`{}` does not name a prior region or a define",
                                id.name
                            )),
                            span: id.span,
                        });
                    }
                }
                RegionStmt::Bin { label, body, span } => {
                    let label = label.as_ref().map(|l| l.value.clone());
                    match body {
                        BinBody::Boundaries { var, edges } => {
                            stmts.push(HirRegionStmt::Bin {
                                label,
                                var: self.resolve_expr(var, &ctx),
                                edges: edges.iter().map(ast::NumLit::canon).collect(),
                                span: *span,
                            });
                        }
                        BinBody::Cond(cond) => {
                            stmts.push(HirRegionStmt::BinCond {
                                label,
                                cond: self.resolve_expr(cond, &ctx),
                                span: *span,
                            });
                        }
                    }
                }
                RegionStmt::Trigger { cond, span: _ } => {
                    let tctx = Ctx {
                        in_trigger: true,
                        ..ctx.clone()
                    };
                    stmts.push(HirRegionStmt::Trigger(self.resolve_expr(cond, &tctx)));
                }
                RegionStmt::Histo { span, .. } => {
                    stmts.push(Self::non_membership("histo", *span));
                    self.pending_histos.push((idx, stmt));
                }
                RegionStmt::Weight { name, value, span } => {
                    stmts.push(Self::non_membership("weight", *span));
                    let value = match value {
                        ast::WeightValue::Num(n) => HirWeightValue::Num(n.canon()),
                        ast::WeightValue::Expr(e) => HirWeightValue::Other(match e.as_ref() {
                            Expr::Ident(id) => format!("identifier `{}`", id.name),
                            Expr::Call { name, .. } => format!("function call `{}(…)`", name.name),
                            _ => "expression argument".to_owned(),
                        }),
                    };
                    self.weights.push(HirWeight {
                        region: idx,
                        name: name.name.clone(),
                        value,
                        span: *span,
                    });
                }
                RegionStmt::Save { span, .. } => stmts.push(Self::non_membership("save", *span)),
                RegionStmt::Print { span, .. } => stmts.push(Self::non_membership("print", *span)),
                RegionStmt::Counts { span, .. } => {
                    stmts.push(Self::non_membership("counts", *span));
                }
                RegionStmt::TypeTag { span, .. } => stmts.push(Self::non_membership("type", *span)),
                RegionStmt::Sort { span, .. } => stmts.push(HirRegionStmt::NonMembership {
                    kind: "sort",
                    tag: Fragment::unsupported("`sort` is outside the checked fragment"),
                    span: *span,
                }),
            }
        }
        let name = self.symbols.intern(&region.name.name);
        self.regions.push(HirRegion {
            name,
            stmts,
            span: region.span,
        });
        self.region_name_order.push(name);
        self.histolist_regions
            .push(region.keyword == RegionKw::HistoList);
        self.regions_by_key
            .entry(region.name.name.to_ascii_lowercase())
            .or_insert(idx);
    }

    fn non_membership(kind: &'static str, span: Span) -> HirRegionStmt {
        HirRegionStmt::NonMembership {
            kind,
            tag: Fragment::InFragment,
            span,
        }
    }

    /// Classify a `histo` argument list (PLAN Phase 9 / SPEC_EVENT_PIPELINE
    /// §3). Accumulable forms: 1-D uniform `n, lo, hi, expr`, 1-D
    /// variable-bin `e0 e1 … en, expr`, and 2-D uniform
    /// `nx, xlo, xhi, ny, ylo, yhi, xexpr, yexpr`. A malformed argument
    /// list (or a non-increasing edge list) is recorded as `Unsupported`
    /// with the reason surfaced when accumulation is attempted.
    fn resolve_histo_spec(&mut self, args: &[HistoArg], ctx: &Ctx) -> HistoSpec {
        match args {
            [
                HistoArg::Num(n),
                HistoArg::Num(lo),
                HistoArg::Num(hi),
                HistoArg::Expr(e),
            ] => {
                let Some(nbins) = Self::bin_count(n) else {
                    return HistoSpec::Unsupported(format!(
                        "bin count `{}` is not a positive integer (max 1000000)",
                        n.canon()
                    ));
                };
                HistoSpec::Uniform1D {
                    nbins,
                    lo: lo.canon(),
                    hi: hi.canon(),
                    expr: self.resolve_expr_quiet(e, ctx),
                }
            }
            [HistoArg::NumList(edges), HistoArg::Expr(e)] => {
                if edges.len() < 2 {
                    return HistoSpec::Unsupported(format!(
                        "variable-bin histogram needs at least 2 edges (got {})",
                        edges.len()
                    ));
                }
                if edges.windows(2).any(|w| w[0].value >= w[1].value) {
                    return HistoSpec::Unsupported(
                        "variable-bin edges must be strictly increasing".to_owned(),
                    );
                }
                HistoSpec::Var1D {
                    edges: edges.iter().map(ast::NumLit::canon).collect(),
                    expr: self.resolve_expr_quiet(e, ctx),
                }
            }
            [
                HistoArg::Num(nx),
                HistoArg::Num(xlo),
                HistoArg::Num(xhi),
                HistoArg::Num(ny),
                HistoArg::Num(ylo),
                HistoArg::Num(yhi),
                HistoArg::Expr(ex),
                HistoArg::Expr(ey),
            ] => {
                let (Some(nx), Some(ny)) = (Self::bin_count(nx), Self::bin_count(ny)) else {
                    return HistoSpec::Unsupported(format!(
                        "2-D bin counts `{}`/`{}` must be positive integers (max 1000000)",
                        nx.canon(),
                        ny.canon()
                    ));
                };
                HistoSpec::Uniform2D {
                    nx,
                    xlo: xlo.canon(),
                    xhi: xhi.canon(),
                    ny,
                    ylo: ylo.canon(),
                    yhi: yhi.canon(),
                    xexpr: self.resolve_expr_quiet(ex, ctx),
                    yexpr: self.resolve_expr_quiet(ey, ctx),
                }
            }
            _ => HistoSpec::Unsupported("unrecognized `histo` argument shape".to_owned()),
        }
    }

    /// Bin count: a positive integer literal, capped so the accumulator
    /// never allocates absurdly (1e6 bins is far beyond any real use).
    fn bin_count(n: &ast::NumLit) -> Option<u32> {
        if n.neg || n.is_real {
            return None;
        }
        n.raw
            .parse::<u32>()
            .ok()
            .filter(|v| (1..=1_000_000).contains(v))
    }

    /// Resolve a histogram fill expression without contributing sema
    /// diagnostics: histograms have no membership effect, so their
    /// problems are reported (once) by the accumulator at run time via
    /// the node's `Unsupported` tags. Defines and objects are already
    /// resolved before regions, so no legitimate diagnostic can be
    /// swallowed here; the warn-once name set is restored so a later
    /// membership statement still warns.
    fn resolve_expr_quiet(&mut self, e: &Expr, ctx: &Ctx) -> HNode {
        let n_diags = self.diags.len();
        let warned = self.warned_names.clone();
        let node = self.resolve_expr(e, ctx);
        self.diags.truncate(n_diags);
        self.warned_names = warned;
        node
    }

    // ---- expressions -------------------------------------------------------

    /// What does `e` denote as an object/particle argument?
    fn resolve_target(&mut self, e: &Expr, ctx: &Ctx) -> Target {
        match e {
            Expr::Ident(id) => {
                let key = id.name.to_ascii_lowercase();
                if ctx.elem_aliases.contains(&key) || (ctx.this_as_particle && key == "this") {
                    // Inside a reducer body, the outer element is a particle
                    // (`dR(this, X)`); elsewhere it is the implicit subject.
                    if ctx.this_as_particle {
                        return Target::Particle(ParticleRef::ThisElem);
                    }
                    return Target::ElemSelf;
                }
                if let Some(p) = ctx.binders.get(&key) {
                    return Target::Particle(p.clone());
                }
                if let Some(&oidx) = self.objects_by_key.get(&key) {
                    let c = self.resolve_object(oidx);
                    if ctx.reduce_coll == Some(c) {
                        return Target::Particle(ParticleRef::ReduceElem);
                    }
                    return if self.is_met_coll(c) {
                        Target::Met
                    } else {
                        Target::Coll(c)
                    };
                }
                if let Some(&didx) = self.defines_by_key.get(&key) {
                    // A define is a scope-free alias for its body expression.
                    // Resolve THROUGH it to the body's target so an aliased
                    // particle (`define leadjet = jets[0]`) makes
                    // `f(leadjet)` and `f(jets[0])` the SAME quantity — else
                    // they intern as two distinct opaque args and fabricate a
                    // false PROVEN OVERLAPPING between contradictory cuts.
                    // Resolve the body in the default (scope-free) context,
                    // exactly as `resolve_define` does, and guard cycles.
                    if matches!(self.def_state[didx], State::InProgress) {
                        return Target::None;
                    }
                    let def = self.ast_defines[didx];
                    let prev =
                        std::mem::replace(&mut self.def_state[didx], State::InProgress);
                    let body_ctx = Ctx::default();
                    let target = self.resolve_target(&def.body, &body_ctx);
                    self.def_state[didx] = prev;
                    return target;
                }
                if self.ext.base_collection(&id.name).is_some()
                    && !self.ext.is_event_scalar(&id.name)
                {
                    if self.ext.is_met_family(&id.name) {
                        return Target::Met;
                    }
                    let c = self.resolve_collection_name(&id.name, id.span);
                    if ctx.reduce_coll == Some(c) {
                        return Target::Particle(ParticleRef::ReduceElem);
                    }
                    return Target::Coll(c);
                }
                Target::None
            }
            Expr::Index { base, index, .. } | Expr::UnderscoreIndex { base, index, .. } => {
                match self.resolve_target(base, ctx) {
                    Target::Met => Target::Met, // METLV_0 is the MET vector
                    Target::Coll(c) => {
                        let (coll, index) = self.rebase_slice_index(c, Self::index_val(*index));
                        Target::Particle(ParticleRef::Elem { coll, index })
                    }
                    _ => Target::None,
                }
            }
            Expr::UnderscoreAll { base, .. } => match self.resolve_target(base, ctx) {
                Target::Met => Target::Met,
                Target::Coll(c) => Target::Particle(ParticleRef::Whole(c)),
                t @ (Target::Particle(_) | Target::ElemSelf) => t,
                Target::None => Target::None,
            },
            // `coll[a:b]` — a contiguous sub-range; a new collection identity
            // (USER ANSWER 3: ordered, element-indexable; never assumed
            // pt-descending). `[-n]`-bounded slices stay unsupported (OPEN-3).
            Expr::Slice { base, start, end, .. } => {
                let Target::Coll(c) = self.resolve_target(base, ctx) else {
                    return Target::None;
                };
                let bound = |v: &Option<IndexVal>| match v {
                    Some(iv) if iv.neg => None, // back-index slice ⇒ unsupported
                    Some(iv) => Some(Some(u32::try_from(iv.value).unwrap_or(u32::MAX))),
                    None => Some(None),
                };
                let (Some(start_opt), Some(end_opt)) = (bound(start), bound(end)) else {
                    return Target::None;
                };
                let id = self.intern_coll(Collection::Slice {
                    source: c,
                    start: start_opt.unwrap_or(0),
                    end: end_opt,
                });
                Self::coll_or_reduce_elem(id, ctx)
            }
            // `X->axis` — projection of a composite onto a member or candidate
            // axis (USER ANSWER 3: the projected collection is element-indexable).
            Expr::Member { base, field, .. } => {
                let Target::Coll(c) = self.resolve_target(base, ctx) else {
                    return Target::None;
                };
                // Snapshot the axis names so the immutable table borrow ends
                // before `intern_coll` mutates.
                let (member_syms, cand_sym): (Vec<Symbol>, Option<Symbol>) =
                    match self.table.collection(c) {
                        Collection::Combination { members, candidate, .. } => (
                            members.iter().map(|m| m.name).collect(),
                            candidate.as_ref().map(|c| c.name),
                        ),
                        _ => return Target::None,
                    };
                let fkey = field.name.to_ascii_lowercase();
                let axis = if let Some(&m) =
                    member_syms.iter().find(|&&s| self.symbols.key(s) == fkey)
                {
                    CombAxis::Member(m)
                } else if cand_sym.is_some_and(|s| self.symbols.key(s) == fkey) {
                    CombAxis::Candidate(cand_sym.expect("checked"))
                } else {
                    return Target::None;
                };
                let id = self.intern_coll(Collection::CombProject { comb: c, axis });
                Self::coll_or_reduce_elem(id, ctx)
            }
            // 4-vector sum (`l1 + l2`): both sides must denote particles. The
            // result interns canonically (flattened, operand-sorted) so
            // association and order do not create distinct identities.
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => match (self.target_particle(lhs, ctx), self.target_particle(rhs, ctx)) {
                (Some(a), Some(b)) => Target::Particle(ParticleRef::sum([a, b])),
                _ => Target::None,
            },
            _ => Target::None,
        }
    }

    /// A collection target, demoted to the reducer iteration element when it
    /// IS the reducer's iteration collection (`pt(eles[:2])` inside
    /// `min(pt(eles[:2]))`, where `eles[:2]` is the iterated slice).
    fn coll_or_reduce_elem(id: CollectionId, ctx: &Ctx) -> Target {
        if ctx.reduce_coll == Some(id) {
            Target::Particle(ParticleRef::ReduceElem)
        } else {
            Target::Coll(id)
        }
    }

    fn target_collection(&mut self, e: &Expr, ctx: &Ctx) -> Option<CollectionId> {
        match self.resolve_target(e, ctx) {
            Target::Coll(c) | Target::Particle(ParticleRef::Whole(c)) => Some(c),
            Target::Particle(ParticleRef::Elem { coll, .. } | ParticleRef::Binder { coll, .. }) => {
                Some(coll)
            }
            _ => None,
        }
    }

    fn resolve_expr(&mut self, e: &Expr, ctx: &Ctx) -> HNode {
        match e {
            Expr::Num(n) => HNode::new(HKind::Num(n.canon()), n.span),
            Expr::True(s) | Expr::All(s) => HNode::new(HKind::Bool(true), *s),
            Expr::False(s) | Expr::NoneKw(s) => HNode::new(HKind::Bool(false), *s),
            Expr::Error(s) => HNode::unsupported(*s, "parse error"),
            Expr::Ident(id) => self.resolve_value_ident(id, ctx),
            Expr::Unary { op, expr, span } => {
                let inner = self.resolve_expr(expr, ctx);
                let kind = match op {
                    UnaryOp::Neg => HKind::Neg(Box::new(inner)),
                    UnaryOp::Not => HKind::Not(Box::new(inner)),
                };
                HNode::new(kind, *span)
            }
            Expr::Binary { op, lhs, rhs, span } => self.resolve_binary(*op, lhs, rhs, *span, ctx),
            Expr::Cmp { op, lhs, rhs, span } => {
                // OPEN-4: `~=` is treated as `!=` downstream (parser warned).
                let op = if *op == CmpOp::ApproxEq {
                    CmpOp::Ne
                } else {
                    *op
                };
                // min/max → any/all desugar (P2 analyzer): a monotone
                // comparison `max(e) > c` ⇔ `any(e > c)` (etc) becomes a
                // boolean reducer, sharing ONE body with the interpreter
                // (which evaluates the equivalent fold — no drift). Only the
                // monotone pairings desugar; `==`/`!=` and anti-monotone keep
                // the numeric Reduce (opaque ⇒ Unknown). Note `min`/`max` are
                // never boolean, so this and the boolean hoists are disjoint.
                // The min/max desugar runs AFTER both sides resolve (below),
                // so it uniformly catches direct calls AND inlined-define
                // reducers from one place — no syntactic special-case here.
                //
                // Boolean-reducer comparison hoist (P1, interpret-only):
                // `any(<scalar>) ⋈ c` ⇒ `any over X of (<scalar> ⋈ c)`. Only
                // fires when the reducer's body is a bare scalar; an
                // already-boolean body (`any(pt(X) > 10)`) is left intact.
                if let Some((kind, rargs, rspan)) = Self::as_boolean_reduce(lhs) {
                    let other = self.resolve_expr(rhs, ctx);
                    let hoist = move |_s: &mut Self, body: HNode| {
                        let bspan = body.span;
                        HNode::new(
                            HKind::Cmp {
                                op,
                                lhs: Box::new(body),
                                rhs: Box::new(other.clone()),
                            },
                            bspan,
                        )
                    };
                    return self.resolve_reduce(kind, rargs, rspan, ctx, Some(&hoist));
                }
                if let Some((kind, rargs, rspan)) = Self::as_boolean_reduce(rhs) {
                    let other = self.resolve_expr(lhs, ctx);
                    let hoist = move |_s: &mut Self, body: HNode| {
                        let bspan = body.span;
                        HNode::new(
                            HKind::Cmp {
                                op,
                                lhs: Box::new(other.clone()),
                                rhs: Box::new(body),
                            },
                            bspan,
                        )
                    };
                    return self.resolve_reduce(kind, rargs, rspan, ctx, Some(&hoist));
                }
                let lhs = self.resolve_expr(lhs, ctx);
                let rhs = self.resolve_expr(rhs, ctx);
                // Post-resolution min/max desugar: catches a numeric `min`/`max`
                // reducer that arrived via an INLINED define (`define dphimin =
                // min(...)`; `select dphimin > 0.4`), where the syntactic call
                // is hidden behind an identifier. Same monotone-pairing rule,
                // same shared body (the comparison is hoisted into the existing
                // reducer body — no re-resolution, so it cannot drift).
                if let Some(node) = self.desugar_minmax_node(&lhs, op, &rhs, *span) {
                    return node;
                }
                if let Some(node) = self.desugar_minmax_node(&rhs, op.flipped(), &lhs, *span) {
                    return node;
                }
                HNode::new(
                    HKind::Cmp {
                        op,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    *span,
                )
            }
            Expr::Band {
                kind,
                expr,
                lo,
                hi,
                span,
            } => {
                // Boolean-reducer band hoist: `any(<scalar>) [] lo hi`.
                if let Some((rkind, rargs, rspan)) = Self::as_boolean_reduce(expr) {
                    let (bkind, lo, hi) = (*kind, lo.canon(), hi.canon());
                    let hoist = move |_s: &mut Self, body: HNode| {
                        let bspan = body.span;
                        HNode::new(
                            HKind::Band {
                                kind: bkind,
                                expr: Box::new(body),
                                lo: lo.clone(),
                                hi: hi.clone(),
                            },
                            bspan,
                        )
                    };
                    return self.resolve_reduce(rkind, rargs, rspan, ctx, Some(&hoist));
                }
                let inner = self.resolve_expr(expr, ctx);
                HNode::new(
                    HKind::Band {
                        kind: *kind,
                        expr: Box::new(inner),
                        lo: lo.canon(),
                        hi: hi.canon(),
                    },
                    *span,
                )
            }
            Expr::Ternary {
                guard,
                then,
                els,
                span,
            } => {
                let guard = self.resolve_expr(guard, ctx);
                let then = self.resolve_expr(then, ctx);
                let els = els.as_ref().map(|e| Box::new(self.resolve_expr(e, ctx)));
                HNode::new(
                    HKind::Ternary {
                        guard: Box::new(guard),
                        then: Box::new(then),
                        els,
                    },
                    *span,
                )
            }
            Expr::Abs { expr, span } => {
                let inner = self.resolve_expr(expr, ctx);
                HNode::new(HKind::Abs(Box::new(inner)), *span)
            }
            Expr::Call { name, args, span } => self.resolve_call(name, args, *span, ctx),
            Expr::Dot { base, field, span } => self.resolve_dot(base, field, *span, ctx),
            Expr::Member { base, field, span } => {
                // `X->axis` projects a composite onto a member/candidate
                // collection. In *value* position a bare projection is a
                // collection used as a scalar — resolve to the projected id so
                // `size`/index paths reuse it, but tag it unsupported-as-scalar.
                match self.resolve_target(e, ctx) {
                    Target::Coll(c) => {
                        let mut node = HNode::new(HKind::CollValue(c), *span);
                        node.tag =
                            Fragment::unsupported("composite axis used as a scalar value");
                        node
                    }
                    _ => {
                        // Recurse for diagnostics; `field.name` keeps distinct
                        // accesses (`->j1` vs `->j2`) rendering distinctly.
                        let _ = self.resolve_expr(base, ctx);
                        HNode::unsupported(
                            *span,
                            format!(
                                "member access `->{}` of a composite candidate is outside the checked fragment",
                                field.name
                            ),
                        )
                    }
                }
            }
            Expr::Index { span, .. } | Expr::UnderscoreIndex { span, .. } => {
                match self.resolve_target(e, ctx) {
                    Target::Met => self.met_scalar("pt", *span),
                    Target::Particle(p) => {
                        let back = matches!(
                            p,
                            ParticleRef::Elem {
                                index: ElemIndex::FromBack(_),
                                ..
                            }
                        );
                        let mut node = HNode::new(HKind::Particle(p), *span);
                        node.tag = Fragment::unsupported(if back {
                            "negative index `[-n]` is reserved (OPEN-3)"
                        } else {
                            "particle value used as a scalar"
                        });
                        node
                    }
                    _ => HNode::unsupported(*span, "unsupported indexed expression"),
                }
            }
            Expr::UnderscoreAll { span, .. } => match self.resolve_target(e, ctx) {
                Target::Met => self.met_scalar("pt", *span),
                Target::Particle(p) => {
                    let mut node = HNode::new(HKind::Particle(p), *span);
                    node.tag = Fragment::unsupported("collection value used as a scalar");
                    node
                }
                _ => HNode::unsupported(*span, "unsupported `_` reference"),
            },
            Expr::Slice { span, .. } => match self.resolve_target(e, ctx) {
                Target::Coll(c) => {
                    let mut node = HNode::new(HKind::CollValue(c), *span);
                    node.tag = Fragment::unsupported("slice collection used as a scalar value");
                    node
                }
                _ => HNode::unsupported(
                    *span,
                    "slice expression is outside the checked fragment",
                ),
            },
            Expr::Braced { args, prop, span } => self.resolve_braced(args, prop, *span, ctx),
            Expr::ParticleList { span, .. } => HNode::unsupported(
                *span,
                "particle-list value is only supported as a function argument",
            ),
        }
    }

    fn resolve_binary(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        span: Span,
        ctx: &Ctx,
    ) -> HNode {
        let l = self.resolve_expr(lhs, ctx);
        let r = self.resolve_expr(rhs, ctx);
        let kind = match op {
            BinOp::And | BinOp::Or => {
                let mut parts = Vec::new();
                for side in [l, r] {
                    match (op, side.kind) {
                        (BinOp::And, HKind::And(v)) | (BinOp::Or, HKind::Or(v)) => parts.extend(v),
                        (_, kind) => parts.push(HNode {
                            kind,
                            span: side.span,
                            tag: side.tag,
                        }),
                    }
                }
                if op == BinOp::And {
                    HKind::And(parts)
                } else {
                    HKind::Or(parts)
                }
            }
            BinOp::Add => Self::arith(ArithOp::Add, l, r),
            BinOp::Sub => Self::arith(ArithOp::Sub, l, r),
            BinOp::Mul => Self::arith(ArithOp::Mul, l, r),
            BinOp::Div => Self::arith(ArithOp::Div, l, r),
            BinOp::Pow => Self::arith(ArithOp::Pow, l, r),
        };
        HNode::new(kind, span)
    }

    fn arith(op: ArithOp, l: HNode, r: HNode) -> HKind {
        HKind::Binary {
            op,
            lhs: Box::new(l),
            rhs: Box::new(r),
        }
    }

    fn resolve_value_ident(&mut self, id: &ast::Ident, ctx: &Ctx) -> HNode {
        let key = id.name.to_ascii_lowercase();
        let span = id.span;

        if ctx.elem_aliases.contains(&key) {
            return HNode::unsupported(span, "bare element reference used as a scalar");
        }
        if let Some(p) = ctx.binders.get(&key) {
            let mut node = HNode::new(HKind::Particle(p.clone()), span);
            node.tag = Fragment::unsupported("composite binder used as a scalar");
            return node;
        }
        if let Some(&didx) = self.defines_by_key.get(&key) {
            // Defines resolve to their body HIR: inline by construction.
            let (_, body) = self.resolve_define(didx);
            return body;
        }
        if let Some(&oidx) = self.objects_by_key.get(&key) {
            let c = self.resolve_object(oidx);
            if self.is_met_coll(c) {
                // A bare MET-family value means its .pt magnitude.
                return self.met_scalar("pt", span);
            }
            let mut node = HNode::new(HKind::CollValue(c), span);
            node.tag = Fragment::unsupported("collection used as a scalar value");
            return node;
        }
        if let Some(&ridx) = self.regions_by_key.get(&key) {
            return HNode::new(HKind::RegionPred(ridx), span);
        }
        if ctx.elem_source.is_some() && self.ext.is_property(&id.name) {
            let prop = self.intern_prop(&id.name);
            return HNode::new(HKind::ElemSelfProp(prop), span);
        }
        if self.ext.is_met_family(&id.name) {
            return self.met_scalar("pt", span);
        }
        if self.ext.is_event_scalar(&id.name) {
            let sym = self.symbols.intern(&id.name);
            let q = self
                .table
                .intern_quantity(Quantity::EventScalar(ScalarSource::EventVar(sym)));
            return HNode::new(HKind::Quantity(q), span);
        }
        if self.ext.base_collection(&id.name).is_some() {
            let c = self.resolve_collection_name(&id.name, span);
            let mut node = HNode::new(HKind::CollValue(c), span);
            node.tag = Fragment::unsupported("collection used as a scalar value");
            return node;
        }
        if ctx.in_trigger {
            let sym = self.symbols.intern(&id.name);
            let q = self
                .table
                .intern_quantity(Quantity::EventScalar(ScalarSource::Trigger(sym)));
            return HNode::new(HKind::Quantity(q), span);
        }
        self.warn_once(
            format!("ident:{key}"),
            Diagnostic::warning(span, format!("unresolved identifier `{}`", id.name)),
        );
        HNode::unsupported(span, format!("unresolved identifier `{}`", id.name))
    }

    fn resolve_dot(&mut self, base: &Expr, field: &ast::Ident, span: Span, ctx: &Ctx) -> HNode {
        match self.resolve_target(base, ctx) {
            Target::Met => self.met_scalar(&field.name, span),
            Target::ElemSelf => {
                let prop = self.intern_prop(&field.name);
                HNode::new(HKind::ElemSelfProp(prop), span)
            }
            Target::Coll(c) | Target::Particle(ParticleRef::Whole(c)) => {
                if field.name.eq_ignore_ascii_case("size") {
                    let q = self.table.intern_quantity(Quantity::Size(c));
                    return self.quantity_node(q, span);
                }
                let prop = self.intern_prop(&field.name);
                if ctx.elem_source == Some(c) {
                    return HNode::new(HKind::ElemSelfProp(prop), span);
                }
                HNode::new(HKind::CollProp { coll: c, prop }, span)
            }
            Target::Particle(ParticleRef::Elem { coll, index }) => {
                let prop = self.intern_prop(&field.name);
                let q = self
                    .table
                    .intern_quantity(Quantity::ElemProp { coll, index, prop });
                self.quantity_node(q, span)
            }
            Target::Particle(
                p @ (ParticleRef::ReduceElem
                | ParticleRef::ThisElem
                | ParticleRef::Sum(_)
                | ParticleRef::Binder { .. }),
            ) => self.reduce_particle_prop(p, &field.name, span),
            Target::Particle(ParticleRef::Met) | Target::None => HNode::unsupported(
                span,
                format!("property `.{}` on an unsupported base", field.name),
            ),
        }
    }

    /// Property access on a reducer iteration element (`ReduceElem`), the
    /// outer reducer element (`ThisElem`), a 4-vector sum, or a composite
    /// binder. Interpret-only (P1): `ReduceElem` props become
    /// [`HKind::ReduceProp`]; `ThisElem` props are the outer element's
    /// [`HKind::ElemSelfProp`]; a `Sum`/`Binder` property interns as an opaque
    /// getter (`InFragment` so `run` evaluates it via the 4-vector / binder
    /// env), kept a free var for the analyzer.
    fn reduce_particle_prop(&mut self, p: ParticleRef, prop_name: &str, span: Span) -> HNode {
        match p {
            ParticleRef::ReduceElem => {
                let prop = self.intern_prop(prop_name);
                HNode::new(HKind::ReduceProp(prop), span)
            }
            ParticleRef::ThisElem => {
                let prop = self.intern_prop(prop_name);
                HNode::new(HKind::ElemSelfProp(prop), span)
            }
            // 4-vector sum getter (`mass(l1+l2)`): interns as an opaque
            // external function over the canonical sum. The interpreter
            // computes the real Lorentz value; the analyzer keeps it as a
            // free interned quantity (P1: no axiom — sound, just no
            // cross-region cancellation leverage yet). Stays `InFragment` so
            // `run` evaluates it instead of hard-erroring.
            p => {
                let name = self.symbols.intern(prop_name);
                let q = self.table.intern_quantity(Quantity::ExternalFn {
                    name,
                    args: vec![QuantityArg::Particle(p)],
                });
                self.quantity_node(q, span)
            }
        }
    }

    fn resolve_braced(&mut self, args: &[Arg], prop: &ast::Ident, span: Span, ctx: &Ctx) -> HNode {
        if let [Arg::Expr(e)] = args {
            return self.resolve_prop_access(e, prop, span, ctx);
        }
        // Multi-argument braced property ({a b}m): opaque external fn.
        let name = self.symbols.intern(&prop.name);
        let qargs: Vec<QuantityArg> = args.iter().map(|a| self.quantity_arg(a, ctx)).collect();
        let q = self
            .table
            .intern_quantity(Quantity::ExternalFn { name, args: qargs });
        self.quantity_node(q, span)
    }

    /// Property applied to a target: `{X}prop`, `prop(X)`, `X.prop`.
    fn resolve_prop_access(
        &mut self,
        target_expr: &Expr,
        prop: &ast::Ident,
        span: Span,
        ctx: &Ctx,
    ) -> HNode {
        match self.resolve_target(target_expr, ctx) {
            Target::Met => self.met_scalar(&prop.name, span),
            Target::ElemSelf => {
                let p = self.intern_prop(&prop.name);
                HNode::new(HKind::ElemSelfProp(p), span)
            }
            Target::Coll(c) | Target::Particle(ParticleRef::Whole(c)) => {
                let p = self.intern_prop(&prop.name);
                if ctx.elem_source == Some(c) {
                    HNode::new(HKind::ElemSelfProp(p), span)
                } else {
                    HNode::new(HKind::CollProp { coll: c, prop: p }, span)
                }
            }
            Target::Particle(ParticleRef::Elem { coll, index }) => {
                let p = self.intern_prop(&prop.name);
                let q = self.table.intern_quantity(Quantity::ElemProp {
                    coll,
                    index,
                    prop: p,
                });
                self.quantity_node(q, span)
            }
            Target::Particle(
                p @ (ParticleRef::ReduceElem
                | ParticleRef::ThisElem
                | ParticleRef::Sum(_)
                | ParticleRef::Binder { .. }),
            ) => self.reduce_particle_prop(p, &prop.name, span),
            Target::Particle(ParticleRef::Met) | Target::None => {
                // Not a typed target (e.g. `{jets[0] jets[1]}m` handled by
                // caller; arbitrary expressions stay opaque).
                let name = self.symbols.intern(&prop.name);
                let arg = self.opaque_arg(target_expr, ctx);
                let q = self.table.intern_quantity(Quantity::ExternalFn {
                    name,
                    args: vec![arg],
                });
                self.quantity_node(q, span)
            }
        }
    }

    /// If `e` is a *boolean* reducer call (`any`/`all`), return its kind,
    /// argument list and span — the operand a surrounding comparison/band
    /// hoists into. Numeric reducers (`sum`/`min`/`max`) are NOT hoisted:
    /// they evaluate to a number and the comparison applies to that number.
    fn as_boolean_reduce(e: &Expr) -> Option<(ReduceKind, &[Arg], Span)> {
        if let Expr::Call { name, args, span } = e {
            let kind = Self::reduce_kind(&name.name.to_ascii_lowercase())?;
            if kind.is_boolean() {
                return Some((kind, args, *span));
            }
        }
        None
    }

    /// The boolean reducer a `min`/`max(e) ⋈ c` comparison desugars to, for
    /// the **monotone** pairings only (P2 analyzer desugar):
    ///
    /// - `max(e) > c` / `max(e) >= c`  ⇔ `any(e ⋈ c)`
    /// - `max(e) < c` / `max(e) <= c`  ⇔ `all(e ⋈ c)`
    /// - `min(e) < c` / `min(e) <= c`  ⇔ `any(e ⋈ c)`
    /// - `min(e) > c` / `min(e) >= c`  ⇔ `all(e ⋈ c)`
    ///
    /// `==`/`!=` and the anti-monotone pairings return `None` (the numeric
    /// fold stays opaque ⇒ Unknown — sound, just not desugared). `op` is the
    /// relation applied with the reducer on the **left** (callers flip it
    /// when the reducer is on the right).
    fn minmax_desugar_kind(reduce: ReduceKind, op: CmpOp) -> Option<ReduceKind> {
        use CmpOp::{Ge, Gt, Le, Lt};
        match (reduce, op) {
            (ReduceKind::Max, Gt | Ge) | (ReduceKind::Min, Lt | Le) => Some(ReduceKind::Any),
            (ReduceKind::Max, Lt | Le) | (ReduceKind::Min, Gt | Ge) => Some(ReduceKind::All),
            _ => None,
        }
    }

    /// Desugar an already-resolved numeric `min`/`max` reducer compared
    /// against `other` (`reduce_node <rule_op> other`) into the equivalent
    /// boolean reducer, for the monotone pairings only. `rule_op` is the
    /// relation **with the reducer on the left** (callers flip it for a
    /// right-hand reducer). The comparison is hoisted into the existing
    /// reducer body (`min(e) > c → all(e > c)`), reusing the SAME resolved
    /// body so the interpreter and analyzer share one node — no drift.
    ///
    /// **Empty-collection boundary** (USER ANSWER 2: `min`/`max` over an
    /// empty collection is a `NonValue`, i.e. the comparison is *false*).
    /// `any` matches this for free (`any` over empty is false), but `all`
    /// over empty is vacuously *true* — so an `All`-target desugar
    /// (`min(e) > c`, `max(e) < c`) is conjoined with `size(coll) > 0` to
    /// keep the empty case cut-false. `Any`-target desugars need no guard.
    fn desugar_minmax_node(
        &mut self,
        reduce_node: &HNode,
        rule_op: CmpOp,
        other: &HNode,
        span: Span,
    ) -> Option<HNode> {
        let HKind::Reduce {
            kind: mkind,
            coll,
            body,
            slice,
        } = &reduce_node.kind
        else {
            return None;
        };
        if !matches!(mkind, ReduceKind::Min | ReduceKind::Max) {
            return None;
        }
        let (coll, slice) = (*coll, *slice);
        let dkind = Self::minmax_desugar_kind(*mkind, rule_op)?;
        let bspan = body.span;
        let cmp_body = HNode::new(
            HKind::Cmp {
                op: rule_op,
                lhs: body.clone(),
                rhs: Box::new(other.clone()),
            },
            bspan,
        );
        let mut reduce = HNode::new(
            HKind::Reduce {
                kind: dkind,
                coll,
                body: Box::new(cmp_body),
                slice,
            },
            span,
        );
        reduce.tag = Fragment::InFragment;
        // `all` is vacuously true on an empty collection, but min/max over an
        // empty collection is cut-false — guard the All-target desugar so the
        // empty case agrees with the numeric fold.
        if dkind == ReduceKind::All {
            let size_q = self.table.intern_quantity(Quantity::Size(coll));
            let size_node = self.quantity_node(size_q, span);
            let nonempty = HNode::new(
                HKind::Cmp {
                    op: CmpOp::Gt,
                    lhs: Box::new(size_node),
                    rhs: Box::new(HNode::new(HKind::Num("0".to_owned()), span)),
                },
                span,
            );
            return Some(HNode::new(HKind::And(vec![nonempty, reduce]), span));
        }
        Some(reduce)
    }

    /// Reducer kind for a call name, if any (`any`/`all`/`sum`/`min`/`max`).
    fn reduce_kind(lc: &str) -> Option<ReduceKind> {
        match lc {
            "any" => Some(ReduceKind::Any),
            "all" => Some(ReduceKind::All),
            "sum" => Some(ReduceKind::Sum),
            "min" => Some(ReduceKind::Min),
            "max" => Some(ReduceKind::Max),
            _ => None,
        }
    }

    /// Resolve a reducer call (`any`/`all`/`sum`/`min`/`max`). Interpret-only
    /// (P1): the iteration collection is the *single* plural collection
    /// appearing in the body; a body with zero or more than one plural
    /// occurrence (including a self-cross like `dR(leptons, leptons)`) is
    /// `Unsupported`, which is sound. `cmp_hoist` carries the surrounding
    /// comparison/band for a boolean reducer whose body is a bare scalar
    /// (`any(dR(this, X)) < 0.4` ⇒ `any over X of dR(this, X) < 0.4`).
    // The `cmp_hoist` closure is `&dyn Fn(&mut Self, HNode) -> HNode`; the
    // alias form trips a higher-ranked-lifetime mismatch, so it is inlined.
    #[allow(clippy::type_complexity)]
    fn resolve_reduce(
        &mut self,
        kind: ReduceKind,
        args: &[Arg],
        span: Span,
        ctx: &Ctx,
        cmp_hoist: Option<&dyn Fn(&mut Self, HNode) -> HNode>,
    ) -> HNode {
        let [Arg::Expr(body_expr)] = args else {
            return HNode::unsupported(
                span,
                format!("`{}` expects a single expression argument", kind.as_str()),
            );
        };
        // Probe pass: resolve the body with `this` as the outer element so we
        // can find the iteration collection without yet substituting it.
        let probe_ctx = Ctx {
            this_as_particle: true,
            reduce_coll: None,
            ..ctx.clone()
        };
        let probe = self.resolve_expr(body_expr, &probe_ctx);
        // Every plural occurrence (NOT deduplicated): the body must reference
        // exactly ONE collection exactly ONCE. Zero leaves nothing to iterate;
        // a self-cross (`dR(leptons, leptons)`) or two distinct collections is
        // a tuple iteration we do not model here ⇒ Unsupported (sound).
        let mut occurrences = Vec::new();
        self.collect_plural_colls(&probe, &mut occurrences);
        if occurrences.len() != 1 {
            return HNode::unsupported(
                span,
                format!(
                    "`{}` body must reference exactly one collection once (found {} plural references)",
                    kind.as_str(),
                    occurrences.len()
                ),
            );
        }
        let coll = occurrences[0];
        // Resolve pass: references to the iteration collection become the
        // current element; `this` stays the outer element.
        let body_ctx = Ctx {
            this_as_particle: true,
            reduce_coll: Some(coll),
            ..ctx.clone()
        };
        let mut body = self.resolve_expr(body_expr, &body_ctx);

        if kind.is_boolean() {
            // Boolean reducers fold a predicate. A bare-scalar body hoists the
            // surrounding comparison/band; an already-boolean body uses itself.
            if !Self::is_boolean(&body) {
                match cmp_hoist {
                    Some(hoist) => body = hoist(self, body),
                    None => {
                        return HNode::unsupported(
                            span,
                            format!(
                                "`{}` of a scalar needs a comparison (e.g. `{}(...) < c`)",
                                kind.as_str(),
                                kind.as_str()
                            ),
                        );
                    }
                }
            }
        } else if Self::is_boolean(&body) {
            return HNode::unsupported(
                span,
                format!("`{}` expects a numeric body", kind.as_str()),
            );
        }

        let mut node = HNode::new(
            HKind::Reduce {
                kind,
                coll,
                body: Box::new(body),
                slice: None,
            },
            span,
        );
        // The body's own out-of-fragment leaves propagate via `has_unsupported`;
        // the Reduce node itself is in-fragment for the interpreter (P1).
        node.tag = Fragment::InFragment;
        node
    }

    /// Collect every *plural* collection reference inside a resolved body
    /// (`Whole(C)` particle args, `CollProp{C}`, `CollValue(C)`) — the
    /// candidates for a reducer's iteration collection. Fixed references
    /// (indexed elements, `size`, MET) are not collected.
    fn collect_plural_colls(&self, node: &HNode, out: &mut Vec<CollectionId>) {
        match &node.kind {
            HKind::CollProp { coll, .. } | HKind::CollValue(coll) => out.push(*coll),
            HKind::Quantity(q) => {
                if let Quantity::AngularSep { a, b, .. } = self.table.quantity(*q) {
                    if let ParticleRef::Whole(c) = a {
                        out.push(*c);
                    }
                    if let ParticleRef::Whole(c) = b {
                        out.push(*c);
                    }
                } else if let Quantity::ExternalFn { args, .. } = self.table.quantity(*q) {
                    for arg in args {
                        if let QuantityArg::Particle(ParticleRef::Whole(c))
                        | QuantityArg::Collection(c)
                        | QuantityArg::CollProp { coll: c, .. } = arg
                        {
                            out.push(*c);
                        }
                    }
                }
            }
            HKind::Neg(a) | HKind::Not(a) | HKind::Abs(a) => self.collect_plural_colls(a, out),
            HKind::Binary { lhs, rhs, .. } | HKind::Cmp { lhs, rhs, .. } => {
                self.collect_plural_colls(lhs, out);
                self.collect_plural_colls(rhs, out);
            }
            HKind::And(v) | HKind::Or(v) => {
                for n in v {
                    self.collect_plural_colls(n, out);
                }
            }
            HKind::Band { expr, .. } => self.collect_plural_colls(expr, out),
            HKind::Ternary { guard, then, els } => {
                self.collect_plural_colls(guard, out);
                self.collect_plural_colls(then, out);
                if let Some(e) = els {
                    self.collect_plural_colls(e, out);
                }
            }
            HKind::Reduce { body, .. } => self.collect_plural_colls(body, out),
            _ => {}
        }
    }

    fn resolve_call(&mut self, name: &ast::Ident, args: &[Arg], span: Span, ctx: &Ctx) -> HNode {
        let lc = name.name.to_ascii_lowercase();

        if lc == "abs"
            && let [Arg::Expr(e)] = args
        {
            let inner = self.resolve_expr(e, ctx);
            return HNode::new(HKind::Abs(Box::new(inner)), span);
        }

        if let Some(kind) = Self::reduce_kind(&lc) {
            return self.resolve_reduce(kind, args, span, ctx, None);
        }

        let ang = match lc.as_str() {
            "dr" => Some(AngKind::DR),
            "dphi" => Some(AngKind::DPhi),
            "deta" => Some(AngKind::DEta),
            _ => None,
        };
        if let Some(kind) = ang
            && let [Arg::Expr(a), Arg::Expr(b)] = args
            && let Some(pa) = self.target_particle(a, ctx)
            && let Some(pb) = self.target_particle(b, ctx)
        {
            let q = self.table.intern_angular(kind, pa, pb);
            return self.quantity_node(q, span);
        }

        if matches!(lc.as_str(), "size" | "count")
            && let [Arg::Expr(e)] = args
        {
            match self.resolve_target(e, ctx) {
                Target::Coll(c) | Target::Particle(ParticleRef::Whole(c)) => {
                    let q = self.table.intern_quantity(Quantity::Size(c));
                    return self.quantity_node(q, span);
                }
                _ => {}
            }
        }

        if self.ext.is_property(&name.name)
            && let [Arg::Expr(e)] = args
        {
            if let Expr::ParticleList { items, .. } = e.as_ref() {
                let parts: Option<Vec<ParticleRef>> = items
                    .iter()
                    .map(|item| self.target_particle(item, ctx))
                    .collect();
                if let Some(parts) = parts {
                    let fname = self.symbols.intern(&name.name);
                    let qargs = parts.into_iter().map(QuantityArg::Particle).collect();
                    let q = self.table.intern_quantity(Quantity::ExternalFn {
                        name: fname,
                        args: qargs,
                    });
                    return self.quantity_node(q, span);
                }
            }
            if !matches!(self.resolve_target(e, ctx), Target::None) {
                return self.resolve_prop_access(e, name, span, ctx);
            }
        }

        let declared = self.ext.is_function(&name.name) || self.ext.is_property(&name.name);
        let fname = self.symbols.intern(&name.name);
        let qargs: Vec<QuantityArg> = args.iter().map(|a| self.quantity_arg(a, ctx)).collect();
        let q = self.table.intern_quantity(Quantity::ExternalFn {
            name: fname,
            args: qargs,
        });
        let mut node = self.quantity_node(q, span);
        if !declared {
            self.warn_once(
                format!("fn:{lc}"),
                Diagnostic::warning(
                    name.span,
                    format!(
                        "function `{}` is not declared in the external library",
                        name.name
                    ),
                ),
            );
            node.tag = Fragment::unsupported(format!(
                "function `{}` is not declared in the external library",
                name.name
            ));
        }
        node
    }

    fn target_particle(&mut self, e: &Expr, ctx: &Ctx) -> Option<ParticleRef> {
        match self.resolve_target(e, ctx) {
            Target::Met => Some(ParticleRef::Met),
            Target::Coll(c) => Some(ParticleRef::Whole(c)),
            Target::Particle(p) => Some(p),
            Target::ElemSelf | Target::None => None,
        }
    }

    fn quantity_arg(&mut self, arg: &Arg, ctx: &Ctx) -> QuantityArg {
        match arg {
            Arg::Str(s) => QuantityArg::Opaque(format!("{:?}", s.value)),
            Arg::Path(p) => QuantityArg::Opaque(p.value.clone()),
            Arg::Expr(e) => {
                if let Expr::Num(n) = e.as_ref() {
                    return QuantityArg::Num(n.canon());
                }
                match self.resolve_target(e, ctx) {
                    Target::Met => return QuantityArg::Particle(ParticleRef::Met),
                    Target::Coll(c) => return QuantityArg::Collection(c),
                    Target::Particle(p) => return QuantityArg::Particle(p),
                    Target::ElemSelf | Target::None => {}
                }
                self.opaque_arg(e, ctx)
            }
        }
    }

    /// Resolve an expression argument to the most precise `QuantityArg`.
    fn opaque_arg(&mut self, e: &Expr, ctx: &Ctx) -> QuantityArg {
        if let Expr::ParticleList { items, .. } = e {
            let parts: Vec<String> = items
                .iter()
                .map(|item| {
                    let node = self.resolve_expr(item, ctx);
                    self.render_node(&node)
                })
                .collect();
            return QuantityArg::Opaque(format!("[{}]", parts.join(" ")));
        }
        let node = self.resolve_expr(e, ctx);
        match node.kind {
            HKind::Quantity(q) if node.tag.is_in_fragment() => QuantityArg::Quantity(q),
            HKind::CollProp { coll, prop } => QuantityArg::CollProp { coll, prop },
            _ => QuantityArg::Opaque(self.render_node(&node)),
        }
    }
}
