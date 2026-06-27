//! Merge several resolved units into one [`Hir`] with a shared, **structural**
//! quantity-identity space, so the analysis engine can prove region relations
//! ACROSS files (the cross-analysis overlap matrix; see `MULTIFILE_PLAN.md`).
//!
//! Soundness rests entirely on structural identity: two collections/quantities
//! unify **iff** they are structurally identical after re-interning into the
//! shared tables — same `Base` canonical name, same filter predicate, same
//! external call over the same structural args. Different cuts (`goodjets`
//! pt>30 vs pt>20) therefore get DIFFERENT shared ids and never alias, so a
//! cross-file `PROVEN DISJOINT` can only fire on genuinely-shared quantities.
//! Conservative by design: an opaque external whose argument only survives as
//! a per-unit render string embeds source-LOCAL ids, so it is namespaced by
//! its source unit when remapped — cross-unit opaque args can never collide,
//! weakening such a cross-file verdict to `POSSIBLY` rather than risking a
//! fabricated `PROVEN` from coincidental local-id alignment.
//!
//! The remap is a memoized recursive walk; the original interning is bottom-up
//! and acyclic, so the recursion terminates without a cycle guard.

use crate::dump::RenderCtx;
use crate::hir::{ElemPred, HKind, HNode, Hir, HirRegion, HirRegionStmt};
use crate::intern::{Symbol, SymbolTable};
use crate::quantity::{
    CombAxis, Collection, CollectionId, ElemPredId, ParticleRef, PropId, Quantity, QuantityArg,
    QuantityId, QuantityTable, ScalarSource,
};
use std::collections::HashMap;

/// Per-source-unit memo tables for the structural remap.
#[derive(Default)]
struct Memo {
    coll: HashMap<u32, CollectionId>,
    quant: HashMap<u32, QuantityId>,
    prop: HashMap<u32, PropId>,
    pred: HashMap<u32, ElemPredId>,
}

/// Accumulates the shared identity space + merged regions.
struct Merger {
    /// Ordinal of the unit currently being remapped. Unique per source unit
    /// by construction, so it (not the unit *name*, which is only a file
    /// basename and can collide) namespaces opaque args without aliasing.
    unit_ord: u32,
    symbols: SymbolTable,
    table: QuantityTable,
    coll_names: Vec<Vec<Symbol>>,
    elem_preds: Vec<ElemPred>,
    elem_pred_ids: HashMap<String, ElemPredId>,
    regions: Vec<HirRegion>,
    region_name_order: Vec<Symbol>,
    histolist_regions: Vec<bool>,
}

/// Merge units into one `Hir` over a shared structural identity space. The
/// merged regions are renamed `<unit>::<region>` to disambiguate same-named
/// regions across files; `RegionPred`/`Inherit` indices are rebased onto the
/// combined region list. Callers must pass error-free units.
#[must_use]
pub fn merge_hirs(units: &[&Hir]) -> Hir {
    // The structural remap recurses one native frame per collection-DAG link
    // (a deep `Filtered` chain) and per nested expression node. Real analyses
    // are shallow, but a legal pathological unit (thousands of chained object
    // cuts) would overflow the default ~8 MiB main stack. Run the merge on a
    // worker with a large stack so a deep-but-legal input can't crash the
    // process; this protects every caller (library + CLI), not just one path.
    const MERGE_STACK: usize = 512 * 1024 * 1024;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(MERGE_STACK)
            .spawn_scoped(scope, || merge_hirs_inner(units))
            .expect("spawn merge worker")
            .join()
            .expect("merge worker panicked")
    })
}

fn merge_hirs_inner(units: &[&Hir]) -> Hir {
    let mut m = Merger {
        unit_ord: 0,
        symbols: SymbolTable::default(),
        table: QuantityTable::default(),
        coll_names: Vec::new(),
        elem_preds: Vec::new(),
        elem_pred_ids: HashMap::new(),
        regions: Vec::new(),
        region_name_order: Vec::new(),
        histolist_regions: Vec::new(),
    };
    for (i, src) in units.iter().enumerate() {
        m.unit_ord = u32::try_from(i).expect("unit count overflow");
        m.add_unit(src);
    }
    let unit = units
        .iter()
        .map(|h| h.unit.as_str())
        .collect::<Vec<_>>()
        .join(" + ");
    Hir {
        unit,
        symbols: m.symbols,
        table: m.table,
        coll_names: m.coll_names,
        elem_preds: m.elem_preds,
        objects: Vec::new(),
        defines: Vec::new(),
        regions: m.regions,
        region_name_order: m.region_name_order,
        histolist_regions: m.histolist_regions,
        histos: Vec::new(),
        weights: Vec::new(),
        diags: Vec::new(),
    }
}

impl Merger {
    fn add_unit(&mut self, src: &Hir) {
        let mut memo = Memo::default();
        // Region indices of this unit start here in the combined list, so
        // RegionPred/Inherit can be rebased.
        let region_base = self.region_name_order.len();
        for (i, region) in src.regions.iter().enumerate() {
            let stmts = region
                .stmts
                .iter()
                .map(|s| self.remap_stmt(src, &mut memo, region_base, s))
                .collect();
            let orig = src.symbols.display(region.name);
            let name = self.symbols.intern(&format!("{}::{orig}", src.unit));
            self.regions.push(HirRegion {
                name,
                stmts,
                span: region.span,
            });
            self.region_name_order.push(name);
            self.histolist_regions
                .push(src.histolist_regions.get(i).copied().unwrap_or(false));
        }
    }

    // ---- identity-leaf remaps (memoized, structural) ---------------------

    fn remap_sym(&mut self, src: &Hir, s: Symbol) -> Symbol {
        self.symbols.intern(src.symbols.key(s))
    }

    fn remap_prop(&mut self, src: &Hir, memo: &mut Memo, p: PropId) -> PropId {
        if let Some(&id) = memo.prop.get(&p.0) {
            return id;
        }
        let id = self
            .table
            .intern_prop(src.table.prop_key(p), src.table.prop_display(p));
        memo.prop.insert(p.0, id);
        id
    }

    fn remap_coll(&mut self, src: &Hir, memo: &mut Memo, c: CollectionId) -> CollectionId {
        if let Some(&id) = memo.coll.get(&c.0) {
            return id;
        }
        let new = match src.table.collection(c) {
            Collection::Base(sym) => Collection::Base(self.remap_sym(src, *sym)),
            Collection::Filtered { parent, pred } => {
                let parent = self.remap_coll(src, memo, *parent);
                let pred = self.remap_pred(src, memo, *pred);
                Collection::Filtered { parent, pred }
            }
            Collection::Union(parts) => {
                Collection::Union(self.remap_colls(src, memo, parts))
            }
            Collection::Combination {
                parts,
                kind,
                members,
                candidate,
                cuts,
            } => {
                let (parts, kind, members, candidate, cuts) = (
                    parts.clone(),
                    *kind,
                    members.clone(),
                    candidate.clone(),
                    cuts.clone(),
                );
                let parts = self.remap_colls(src, memo, &parts);
                let members = members
                    .iter()
                    .map(|m| crate::quantity::CompositeBinder {
                        name: self.remap_sym(src, m.name),
                        source: self.remap_coll(src, memo, m.source),
                    })
                    .collect();
                let candidate = candidate.as_ref().map(|c| crate::quantity::CompositeCandidate {
                    name: self.remap_sym(src, c.name),
                    vector: self.remap_particle(src, memo, &c.vector),
                });
                let cuts = cuts
                    .iter()
                    .map(|p| self.remap_pred(src, memo, *p))
                    .collect();
                Collection::Combination {
                    parts,
                    kind,
                    members,
                    candidate,
                    cuts,
                }
            }
            Collection::Sorted { source, key, dir } => {
                let (source, key, dir) = (*source, key.clone(), *dir);
                Collection::Sorted {
                    source: self.remap_coll(src, memo, source),
                    key: self.remap_sort_key(src, memo, key),
                    dir,
                }
            }
            Collection::Slice { source, start, end } => {
                let (source, start, end) = (*source, *start, *end);
                Collection::Slice {
                    source: self.remap_coll(src, memo, source),
                    start,
                    end,
                }
            }
            Collection::CombProject { comb, axis } => {
                let (comb, axis) = (*comb, axis.clone());
                Collection::CombProject {
                    comb: self.remap_coll(src, memo, comb),
                    axis: self.remap_axis(src, axis),
                }
            }
        };
        let before = self.table.collections().len();
        let id = self.table.intern_collection(new);
        // Keep coll_names index-aligned with the shared table: a new id gets
        // this unit's names; a dedup unions them in.
        let names: Vec<Symbol> = src.coll_names[c.0 as usize]
            .iter()
            .map(|&s| self.remap_sym(src, s))
            .collect();
        if id.0 as usize == before {
            self.coll_names.push(names);
        } else {
            let slot = &mut self.coll_names[id.0 as usize];
            for s in names {
                if !slot.contains(&s) {
                    slot.push(s);
                }
            }
        }
        memo.coll.insert(c.0, id);
        id
    }

    fn remap_colls(&mut self, src: &Hir, memo: &mut Memo, cs: &[CollectionId]) -> Vec<CollectionId> {
        cs.iter().map(|&c| self.remap_coll(src, memo, c)).collect()
    }

    fn remap_pred(&mut self, src: &Hir, memo: &mut Memo, p: ElemPredId) -> ElemPredId {
        if let Some(&id) = memo.pred.get(&p.0) {
            return id;
        }
        let node = self.remap_node(src, memo, 0, &src.elem_preds[p.0 as usize].node);
        // Render over the SHARED ids so structurally-identical predicates from
        // different units collapse to one ElemPredId (preds never reference
        // regions, so an empty/partial region_names is fine here).
        let render = RenderCtx {
            symbols: &self.symbols,
            table: &self.table,
            coll_names: &self.coll_names,
            region_names: &self.region_name_order,
        }
        .node(&node);
        let id = *self.elem_pred_ids.entry(render.clone()).or_insert_with(|| {
            let id = ElemPredId(u32::try_from(self.elem_preds.len()).expect("pred id overflow"));
            self.elem_preds.push(ElemPred { node, render });
            id
        });
        memo.pred.insert(p.0, id);
        id
    }

    fn remap_quant(&mut self, src: &Hir, memo: &mut Memo, q: QuantityId) -> QuantityId {
        if let Some(&id) = memo.quant.get(&q.0) {
            return id;
        }
        // AngularSep must re-canonicalize through `intern_angular`: an
        // unoriented `dR` is interned with operands ordered by their SOURCE
        // CollectionIds, but the shared ids differ, so the same physical pair
        // can otherwise land in two different orders and fail to unify across
        // units. Canonical re-ordering keeps `dR(x,y)` and `dR(y,x)` one id.
        let id = if let Quantity::AngularSep { kind, a, b, .. } = src.table.quantity(q) {
            let a = self.remap_particle(src, memo, a);
            let b = self.remap_particle(src, memo, b);
            self.table.intern_angular(*kind, a, b)
        } else {
            let new = match src.table.quantity(q) {
                Quantity::EventScalar(s) => Quantity::EventScalar(self.remap_scalar(src, memo, s)),
                Quantity::Size(c) => Quantity::Size(self.remap_coll(src, memo, *c)),
                Quantity::ElemProp { coll, index, prop } => Quantity::ElemProp {
                    coll: self.remap_coll(src, memo, *coll),
                    index: *index,
                    prop: self.remap_prop(src, memo, *prop),
                },
                Quantity::AngularSep { .. } => unreachable!("handled above"),
                Quantity::ExternalFn { name, args } => Quantity::ExternalFn {
                    name: self.remap_sym(src, *name),
                    args: args
                        .iter()
                        .map(|a| self.remap_arg(src, memo, a))
                        .collect(),
                },
            };
            self.table.intern_quantity(new)
        };
        memo.quant.insert(q.0, id);
        id
    }

    fn remap_scalar(&mut self, src: &Hir, memo: &mut Memo, s: &ScalarSource) -> ScalarSource {
        match s {
            ScalarSource::MetProp(p) => ScalarSource::MetProp(self.remap_prop(src, memo, *p)),
            ScalarSource::EventVar(sym) => ScalarSource::EventVar(self.remap_sym(src, *sym)),
            ScalarSource::Trigger(sym) => ScalarSource::Trigger(self.remap_sym(src, *sym)),
        }
    }

    fn remap_sort_key(
        &mut self,
        src: &Hir,
        memo: &mut Memo,
        key: crate::quantity::SortKey,
    ) -> crate::quantity::SortKey {
        match key {
            // The opaque key is a self-contained render string (identity),
            // carrying no unit-local ids — kept verbatim.
            crate::quantity::SortKey::Opaque(s) => crate::quantity::SortKey::Opaque(s),
            // `Prop` carries a unit-local PropId; re-intern by key/display.
            crate::quantity::SortKey::Prop(p) => {
                crate::quantity::SortKey::Prop(self.remap_prop(src, memo, p))
            }
        }
    }

    fn remap_axis(&mut self, src: &Hir, axis: CombAxis) -> CombAxis {
        match axis {
            CombAxis::Member(s) => CombAxis::Member(self.remap_sym(src, s)),
            CombAxis::Candidate(s) => CombAxis::Candidate(self.remap_sym(src, s)),
        }
    }

    fn remap_particle(&mut self, src: &Hir, memo: &mut Memo, p: &ParticleRef) -> ParticleRef {
        match p {
            ParticleRef::Elem { coll, index } => ParticleRef::Elem {
                coll: self.remap_coll(src, memo, *coll),
                index: *index,
            },
            ParticleRef::Whole(c) => ParticleRef::Whole(self.remap_coll(src, memo, *c)),
            ParticleRef::Met => ParticleRef::Met,
            ParticleRef::Binder { coll, name } => ParticleRef::Binder {
                coll: self.remap_coll(src, memo, *coll),
                name: self.remap_sym(src, *name),
            },
            ParticleRef::ThisElem => ParticleRef::ThisElem,
            ParticleRef::ReduceElem => ParticleRef::ReduceElem,
            ParticleRef::Sum(parts) => {
                ParticleRef::sum(parts.iter().map(|p| self.remap_particle(src, memo, p)))
            }
        }
    }

    fn remap_arg(&mut self, src: &Hir, memo: &mut Memo, a: &QuantityArg) -> QuantityArg {
        match a {
            QuantityArg::Num(s) => QuantityArg::Num(s.clone()),
            QuantityArg::Quantity(q) => QuantityArg::Quantity(self.remap_quant(src, memo, *q)),
            QuantityArg::Particle(p) => QuantityArg::Particle(self.remap_particle(src, memo, p)),
            QuantityArg::Collection(c) => QuantityArg::Collection(self.remap_coll(src, memo, *c)),
            QuantityArg::CollProp { coll, prop } => QuantityArg::CollProp {
                coll: self.remap_coll(src, memo, *coll),
                prop: self.remap_prop(src, memo, *prop),
            },
            // SOUNDNESS-CRITICAL: an opaque render string embeds source-unit-
            // LOCAL collection ids (`C{id}#name` from dump.rs), so identical
            // text across two units can mean DIFFERENT physical quantities
            // whenever the same object happens to land at the same local id.
            // Keeping it verbatim would intern two structurally-distinct
            // externals to one shared id → a fabricated cross-file PROVEN.
            // Namespace it by the source unit (control-char separator that can
            // never occur in a render) so cross-unit opaque args never collide;
            // within a unit they still share the prefix and unify as before.
            QuantityArg::Opaque(s) => {
                QuantityArg::Opaque(format!("{}\u{1}{s}", self.unit_ord))
            }
        }
    }

    // ---- HNode rewriter (region statements + elem-pred bodies) -----------

    fn remap_node(&mut self, src: &Hir, memo: &mut Memo, region_base: usize, n: &HNode) -> HNode {
        let kind = match &n.kind {
            HKind::Num(s) => HKind::Num(s.clone()),
            HKind::Bool(b) => HKind::Bool(*b),
            HKind::Quantity(q) => HKind::Quantity(self.remap_quant(src, memo, *q)),
            HKind::ElemSelfProp(p) => HKind::ElemSelfProp(self.remap_prop(src, memo, *p)),
            HKind::ReduceProp(p) => HKind::ReduceProp(self.remap_prop(src, memo, *p)),
            HKind::Reduce {
                kind,
                coll,
                body,
                slice,
            } => HKind::Reduce {
                kind: *kind,
                coll: self.remap_coll(src, memo, *coll),
                body: self.remap_box(src, memo, region_base, body),
                slice: *slice,
            },
            HKind::CollProp { coll, prop } => HKind::CollProp {
                coll: self.remap_coll(src, memo, *coll),
                prop: self.remap_prop(src, memo, *prop),
            },
            HKind::ScalarMinMax { kind, args } => HKind::ScalarMinMax {
                kind: *kind,
                args: args
                    .iter()
                    .map(|a| self.remap_node(src, memo, region_base, a))
                    .collect(),
            },
            HKind::Particle(p) => HKind::Particle(self.remap_particle(src, memo, p)),
            HKind::CollValue(c) => HKind::CollValue(self.remap_coll(src, memo, *c)),
            HKind::Neg(a) => HKind::Neg(self.remap_box(src, memo, region_base, a)),
            HKind::Not(a) => HKind::Not(self.remap_box(src, memo, region_base, a)),
            HKind::Abs(a) => HKind::Abs(self.remap_box(src, memo, region_base, a)),
            HKind::Binary { op, lhs, rhs } => HKind::Binary {
                op: *op,
                lhs: self.remap_box(src, memo, region_base, lhs),
                rhs: self.remap_box(src, memo, region_base, rhs),
            },
            HKind::Cmp { op, lhs, rhs } => HKind::Cmp {
                op: *op,
                lhs: self.remap_box(src, memo, region_base, lhs),
                rhs: self.remap_box(src, memo, region_base, rhs),
            },
            HKind::And(v) => HKind::And(self.remap_vec(src, memo, region_base, v)),
            HKind::Or(v) => HKind::Or(self.remap_vec(src, memo, region_base, v)),
            HKind::Band {
                kind,
                expr,
                lo,
                hi,
            } => HKind::Band {
                kind: *kind,
                expr: self.remap_box(src, memo, region_base, expr),
                lo: lo.clone(),
                hi: hi.clone(),
            },
            HKind::Ternary { guard, then, els } => HKind::Ternary {
                guard: self.remap_box(src, memo, region_base, guard),
                then: self.remap_box(src, memo, region_base, then),
                els: els
                    .as_ref()
                    .map(|e| self.remap_box(src, memo, region_base, e)),
            },
            // Prior-region reference: rebase onto the combined region list.
            HKind::RegionPred(idx) => HKind::RegionPred(region_base + idx),
            HKind::Unsupported => HKind::Unsupported,
        };
        HNode {
            kind,
            span: n.span,
            tag: n.tag.clone(),
        }
    }

    fn remap_box(
        &mut self,
        src: &Hir,
        memo: &mut Memo,
        region_base: usize,
        n: &HNode,
    ) -> Box<HNode> {
        Box::new(self.remap_node(src, memo, region_base, n))
    }

    fn remap_vec(
        &mut self,
        src: &Hir,
        memo: &mut Memo,
        region_base: usize,
        v: &[HNode],
    ) -> Vec<HNode> {
        v.iter()
            .map(|n| self.remap_node(src, memo, region_base, n))
            .collect()
    }

    fn remap_stmt(
        &mut self,
        src: &Hir,
        memo: &mut Memo,
        region_base: usize,
        s: &HirRegionStmt,
    ) -> HirRegionStmt {
        match s {
            HirRegionStmt::Select(n) => HirRegionStmt::Select(self.remap_node(src, memo, region_base, n)),
            HirRegionStmt::Reject(n) => HirRegionStmt::Reject(self.remap_node(src, memo, region_base, n)),
            HirRegionStmt::Trigger(n) => {
                HirRegionStmt::Trigger(self.remap_node(src, memo, region_base, n))
            }
            HirRegionStmt::Inherit { region, span } => HirRegionStmt::Inherit {
                region: region_base + region,
                span: *span,
            },
            HirRegionStmt::Bin {
                label,
                var,
                edges,
                span,
            } => HirRegionStmt::Bin {
                label: label.clone(),
                var: self.remap_node(src, memo, region_base, var),
                edges: edges.clone(),
                span: *span,
            },
            HirRegionStmt::BinCond { label, cond, span } => HirRegionStmt::BinCond {
                label: label.clone(),
                cond: self.remap_node(src, memo, region_base, cond),
                span: *span,
            },
            HirRegionStmt::NonMembership { kind, tag, span } => HirRegionStmt::NonMembership {
                kind,
                tag: tag.clone(),
                span: *span,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::{Quantity, ScalarSource};
    use crate::resolve::analyze_str;

    fn hir(src: &str, unit: &str) -> Hir {
        let ext = crate::ext::ExtDecls::legacy();
        analyze_str(src, unit, &ext)
    }

    #[test]
    fn merge_doubles_regions_and_prefixes_names() {
        let a = hir("region R\n  select MET.pt > 10\n", "a");
        let b = hir("region S\n  select MET.pt > 20\n", "b");
        let m = merge_hirs(&[&a, &b]);
        assert_eq!(m.regions.len(), 2);
        let names: Vec<&str> = m
            .region_name_order
            .iter()
            .map(|s| m.symbols.display(*s))
            .collect();
        assert!(names.contains(&"a::R"), "{names:?}");
        assert!(names.contains(&"b::S"), "{names:?}");
    }

    #[test]
    fn merge_unifies_a_shared_event_scalar() {
        // MET.pt in both units must collapse to ONE EventScalar(MetProp) in
        // the merged table (the shared-identity property the engine relies on).
        let a = hir("region R\n  select MET.pt > 10\n", "a");
        let b = hir("region S\n  select MET.pt > 20\n", "b");
        let m = merge_hirs(&[&a, &b]);
        let met = m
            .table
            .quantities()
            .iter()
            .filter(|q| matches!(q, Quantity::EventScalar(ScalarSource::MetProp(_))))
            .count();
        assert_eq!(met, 1, "MET.pt must be a single shared quantity");
    }

    #[test]
    fn merge_keeps_differently_cut_objects_distinct() {
        // `goodjets` with different cuts in two units must remain two distinct
        // Filtered collections (no structural aliasing).
        let a = hir(
            "object goodjets\n  take Jet\n  select pt > 30\nregion R\n  select size(goodjets) >= 1\n",
            "a",
        );
        let b = hir(
            "object goodjets\n  take Jet\n  select pt > 100\nregion S\n  select size(goodjets) >= 1\n",
            "b",
        );
        let m = merge_hirs(&[&a, &b]);
        let filtered = m
            .table
            .collections()
            .iter()
            .filter(|c| matches!(c, Collection::Filtered { .. }))
            .count();
        assert_eq!(filtered, 2, "different cuts must not merge to one Filtered");

        // Identical cuts → exactly one shared Filtered collection.
        let b_same = hir(
            "object goodjets\n  take Jet\n  select pt > 30\nregion S\n  select size(goodjets) >= 1\n",
            "b",
        );
        let m = merge_hirs(&[&a, &b_same]);
        let filtered = m
            .table
            .collections()
            .iter()
            .filter(|c| matches!(c, Collection::Filtered { .. }))
            .count();
        assert_eq!(filtered, 1, "identical cuts must unify to one Filtered");
    }

    #[test]
    fn merge_of_one_preserves_region_count() {
        let a = hir(
            "region R\n  select MET.pt > 10\nregion S\n  select MET.pt < 5\n",
            "solo",
        );
        let m = merge_hirs(&[&a]);
        assert_eq!(m.regions.len(), a.regions.len());
    }
}
