//! Deterministic rendering of resolved entities, plus the quantity-table
//! and HIR dump functions (snapshot-tested; byte-identical across runs).
//!
//! Renders are also identity-relevant: element-predicate interning and
//! `QuantityArg::Opaque` use the canonical render as their key, so every
//! label embeds the structural id (`C3#jets`) — identical text always
//! means identical resolution.

use crate::hir::{HKind, HNode, Hir, HirRegionStmt};
use crate::intern::{Symbol, SymbolTable};
use crate::quantity::{
    Collection, CollectionId, ParticleRef, Quantity, QuantityArg, QuantityTable, ScalarSource,
};
use adl_syntax::ast::BandKind;
use std::fmt::Write as _;

/// Borrowed view of everything needed to render (usable mid-resolution).
pub(crate) struct RenderCtx<'a> {
    pub symbols: &'a SymbolTable,
    pub table: &'a QuantityTable,
    pub coll_names: &'a [Vec<Symbol>],
    /// Region names in declaration order (empty mid-resolution is fine:
    /// `RegionPred` only references prior regions).
    pub region_names: &'a [Symbol],
}

impl RenderCtx<'_> {
    pub(crate) fn coll(&self, id: CollectionId) -> String {
        let name = self
            .coll_names
            .get(id.0 as usize)
            .and_then(|names| names.first())
            .map(|&s| self.symbols.display(s).to_owned())
            .or_else(|| match self.table.collection(id) {
                Collection::Base(sym) => Some(self.symbols.display(*sym).to_owned()),
                _ => None,
            });
        match name {
            Some(n) => format!("{id}#{n}"),
            None => id.to_string(),
        }
    }

    pub(crate) fn particle(&self, p: &ParticleRef) -> String {
        match p {
            ParticleRef::Elem { coll, index } => format!("{}[{index}]", self.coll(*coll)),
            ParticleRef::Whole(coll) => format!("{}[*]", self.coll(*coll)),
            ParticleRef::Met => "MET".to_owned(),
            ParticleRef::Binder { coll, name } => {
                format!("{}@{}", self.coll(*coll), self.symbols.display(*name))
            }
            ParticleRef::ThisElem => "this".to_owned(),
            ParticleRef::ReduceElem => "@elem".to_owned(),
            ParticleRef::Sum(parts) => {
                let parts: Vec<String> = parts.iter().map(|p| self.particle(p)).collect();
                format!("({})", parts.join(" + "))
            }
        }
    }

    pub(crate) fn quantity(&self, q: &Quantity) -> String {
        match q {
            Quantity::EventScalar(src) => match src {
                ScalarSource::MetProp(p) => format!("MET.{}", self.table.prop_display(*p)),
                ScalarSource::EventVar(s) => format!("evt.{}", self.symbols.display(*s)),
                ScalarSource::Trigger(s) => format!("trig({})", self.symbols.display(*s)),
            },
            Quantity::Size(c) => format!("size({})", self.coll(*c)),
            Quantity::ElemProp { coll, index, prop } => {
                format!(
                    "{}[{index}].{}",
                    self.coll(*coll),
                    self.table.prop_display(*prop)
                )
            }
            Quantity::AngularSep { kind, a, b, .. } => {
                format!(
                    "{}({}, {})",
                    kind.as_str(),
                    self.particle(a),
                    self.particle(b)
                )
            }
            Quantity::ExternalFn { name, args } => {
                let args: Vec<String> = args.iter().map(|a| self.arg(a)).collect();
                format!("{}({})", self.symbols.display(*name), args.join(", "))
            }
        }
    }

    pub(crate) fn arg(&self, a: &QuantityArg) -> String {
        match a {
            QuantityArg::Num(n) | QuantityArg::Opaque(n) => n.clone(),
            QuantityArg::Quantity(q) => self.quantity(self.table.quantity(*q)),
            QuantityArg::Particle(p) => self.particle(p),
            QuantityArg::Collection(c) => self.coll(*c),
            QuantityArg::CollProp { coll, prop } => {
                format!("{}[*].{}", self.coll(*coll), self.table.prop_display(*prop))
            }
        }
    }

    pub(crate) fn node(&self, n: &HNode) -> String {
        match &n.kind {
            HKind::Num(s) => s.clone(),
            HKind::Bool(b) => b.to_string(),
            HKind::Quantity(q) => self.quantity(self.table.quantity(*q)),
            HKind::ElemSelfProp(p) => format!("this.{}", self.table.prop_display(*p)),
            HKind::ReduceProp(p) => format!("@elem.{}", self.table.prop_display(*p)),
            HKind::Reduce { kind, coll, body, slice } => {
                let s = match slice {
                    Some((a, Some(b))) => format!("[{a}:{b}]"),
                    Some((a, None)) => format!("[{a}:]"),
                    None => String::new(),
                };
                format!("{}({}{s} of {})", kind.as_str(), self.coll(*coll), self.node(body))
            }
            HKind::CollProp { coll, prop } => {
                format!("{}[*].{}", self.coll(*coll), self.table.prop_display(*prop))
            }
            HKind::ScalarMinMax { kind, args } => {
                let inner: Vec<String> = args.iter().map(|a| self.node(a)).collect();
                format!("{}({})", kind.as_str(), inner.join(", "))
            }
            HKind::Particle(p) => self.particle(p),
            HKind::CollValue(c) => self.coll(*c),
            HKind::Neg(e) => format!("(- {})", self.node(e)),
            HKind::Not(e) => format!("(not {})", self.node(e)),
            HKind::Binary { op, lhs, rhs } => {
                format!("({} {} {})", self.node(lhs), op.as_str(), self.node(rhs))
            }
            HKind::And(v) => self.joined(v, " and "),
            HKind::Or(v) => self.joined(v, " or "),
            HKind::Cmp { op, lhs, rhs } => {
                format!("({} {} {})", self.node(lhs), op.as_str(), self.node(rhs))
            }
            HKind::Band { kind, expr, lo, hi } => {
                let op = match kind {
                    BandKind::In => "[]",
                    BandKind::Out => "][",
                };
                format!("({} {op} {lo} {hi})", self.node(expr))
            }
            HKind::Ternary { guard, then, els } => match els {
                Some(e) => format!(
                    "({} ? {} : {})",
                    self.node(guard),
                    self.node(then),
                    self.node(e)
                ),
                None => format!("({} ? {} : true)", self.node(guard), self.node(then)),
            },
            HKind::Abs(e) => format!("abs({})", self.node(e)),
            HKind::RegionPred(i) => {
                let name = self
                    .region_names
                    .get(*i)
                    .map_or("?", |&s| self.symbols.display(s));
                format!("region:{name}")
            }
            HKind::Unsupported => match &n.tag {
                crate::hir::Fragment::Unsupported(reason) => format!("<unsupported: {reason}>"),
                crate::hir::Fragment::InFragment => "<unsupported>".to_owned(),
            },
        }
    }

    fn joined(&self, v: &[HNode], sep: &str) -> String {
        let parts: Vec<String> = v.iter().map(|n| self.node(n)).collect();
        format!("({})", parts.join(sep))
    }
}

fn render_ctx(hir: &Hir) -> RenderCtx<'_> {
    RenderCtx {
        symbols: &hir.symbols,
        table: &hir.table,
        coll_names: &hir.coll_names,
        region_names: &hir.region_name_order,
    }
}

/// Canonical text of a single resolved expression node, over `hir`'s tables.
/// Used to render diagnostics when no source text is available (e.g. a merged
/// cross-file unit, whose spans index no single source).
#[must_use]
pub fn render_node(hir: &Hir, node: &crate::hir::HNode) -> String {
    render_ctx(hir).node(node)
}

/// Deterministic quantity-table dump: collections (id order), element
/// predicates (id order), quantities (sorted by canonical render).
#[must_use]
pub fn quantity_table_dump(hir: &Hir) -> String {
    let rc = render_ctx(hir);
    let mut out = String::new();
    let _ = writeln!(out, "unit: {}", hir.unit);

    let _ = writeln!(out, "== collections ==");
    for (i, coll) in hir.table.collections().iter().enumerate() {
        let id = CollectionId(u32::try_from(i).expect("collection id overflow"));
        let names: Vec<&str> = hir.coll_names[i]
            .iter()
            .map(|&s| hir.symbols.display(s))
            .collect();
        let structure = match coll {
            Collection::Base(sym) => format!("Base({})", hir.symbols.display(*sym)),
            Collection::Filtered { parent, pred } => {
                format!("Filtered(parent={}, pred={pred})", rc.coll(*parent))
            }
            Collection::Union(parts) => {
                let parts: Vec<String> = parts.iter().map(|&p| rc.coll(p)).collect();
                format!("Union({})", parts.join(", "))
            }
            Collection::Combination { parts, kind, .. } => {
                let parts: Vec<String> = parts.iter().map(|&p| rc.coll(p)).collect();
                format!("Combination[{kind:?}]({})", parts.join(", "))
            }
            Collection::Sorted { source, key, dir } => {
                format!("Sorted({}, {key:?}, {dir:?})", rc.coll(*source))
            }
            Collection::Slice { source, start, end } => {
                format!("Slice({}, {start}..{end:?})", rc.coll(*source))
            }
            Collection::CombProject { comb, axis } => {
                format!("CombProject({}, {axis:?})", rc.coll(*comb))
            }
        };
        let names = if names.is_empty() {
            String::new()
        } else {
            format!("  names=[{}]", names.join(", "))
        };
        let _ = writeln!(out, "{id} = {structure}{names}");
    }

    let _ = writeln!(out, "== element predicates ==");
    for (i, pred) in hir.elem_preds.iter().enumerate() {
        let _ = writeln!(out, "P{i} = {}", pred.render);
    }

    let _ = writeln!(out, "== quantities ==");
    let mut lines: Vec<String> = hir
        .table
        .quantities()
        .iter()
        .map(|q| {
            let variant = match q {
                Quantity::EventScalar(_) => "scalar ",
                Quantity::Size(_) => "size   ",
                Quantity::ElemProp { .. } => "elem   ",
                Quantity::AngularSep { oriented: true, .. } => "ang(or)",
                Quantity::AngularSep { .. } => "ang    ",
                Quantity::ExternalFn { .. } => "extfn  ",
            };
            format!("{variant} {}", rc.quantity(q))
        })
        .collect();
    lines.sort();
    for line in lines {
        let _ = writeln!(out, "{line}");
    }
    out
}

/// Deterministic HIR dump: objects, defines, regions (with resolved
/// statements and fragment tags), then sema diagnostics.
#[must_use]
pub fn hir_dump(hir: &Hir) -> String {
    let rc = render_ctx(hir);
    let mut out = String::new();
    let _ = writeln!(out, "unit: {}", hir.unit);

    let _ = writeln!(out, "== objects ==");
    for obj in &hir.objects {
        let name = hir.symbols.display(obj.name);
        let mut line = format!("object {name} -> {}", rc.coll(obj.coll));
        if let Some(src) = obj.pure_alias_of {
            let _ = write!(line, "  (pure rename of {})", rc.coll(src));
        }
        if let crate::hir::Fragment::Unsupported(reason) = &obj.tag {
            let _ = write!(line, "  [unsupported: {reason}]");
        }
        let _ = writeln!(out, "{line}");
    }

    let _ = writeln!(out, "== defines ==");
    for def in &hir.defines {
        let _ = writeln!(
            out,
            "define {} [{}] = {}",
            hir.symbols.display(def.name),
            def.kind.as_str(),
            rc.node(&def.body)
        );
    }

    let _ = writeln!(out, "== regions ==");
    for region in &hir.regions {
        let _ = writeln!(out, "region {}", hir.symbols.display(region.name));
        for stmt in &region.stmts {
            let line = match stmt {
                HirRegionStmt::Select(n) => format!("select {}", rc.node(n)),
                HirRegionStmt::Reject(n) => format!("reject {}", rc.node(n)),
                HirRegionStmt::Inherit { region: i, .. } => {
                    let name = hir
                        .region_name_order
                        .get(*i)
                        .map_or("?", |&s| hir.symbols.display(s));
                    format!("inherit {name}")
                }
                HirRegionStmt::Trigger(n) => format!("trigger {}", rc.node(n)),
                HirRegionStmt::Bin {
                    label, var, edges, ..
                } => {
                    let label = label
                        .as_ref()
                        .map(|l| format!(" {l:?}"))
                        .unwrap_or_default();
                    format!("bin{label} {} edges=[{}]", rc.node(var), edges.join(", "))
                }
                HirRegionStmt::BinCond { label, cond, .. } => {
                    let label = label
                        .as_ref()
                        .map(|l| format!(" {l:?}"))
                        .unwrap_or_default();
                    format!("bin{label} {}", rc.node(cond))
                }
                HirRegionStmt::NonMembership { kind, tag, .. } => match tag {
                    crate::hir::Fragment::InFragment => format!("({kind}: no membership effect)"),
                    crate::hir::Fragment::Unsupported(reason) => {
                        format!("({kind}: unsupported: {reason})")
                    }
                },
            };
            let _ = writeln!(out, "  {line}");
        }
    }

    let _ = writeln!(out, "== diagnostics ==");
    for d in &hir.diags {
        let _ = writeln!(out, "{}: {}", d.severity.as_str(), d.message);
    }
    out
}
