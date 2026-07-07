//! Region/bin encoding for the analysis engine.
//!
//! Reuses `adl-formula`'s region encoder at **statement granularity**:
//! each membership statement is encoded through a synthetic
//! single-statement region, so unsat cores name individual cuts and map
//! back to source lines (SPEC_ANALYSIS §3). The conjunction of the
//! per-statement formulas is the region formula by construction
//! (region encoding ≡ conjunction of statement encodings).
//!
//! Also hosts the documented **opaque-external re-tag pass**: for the
//! verifier, a comparison over an undeclared external function is an
//! exact atom over an opaque interned quantity (the per-event value
//! exists; identity is exact by interning), per the SPEC_ANALYSIS §2
//! model caveat ("opaque external-function values … are free"). The
//! interpreter keeps refusing them — one quantity, two consumers, two
//! honest answers.

use adl_formula::{DiagTable, EncodedRegion, Formula, Over, Under, encode_region};
use adl_sema::{Fragment, HKind, HNode, Hir, HirRegion, HirRegionStmt, Quantity, QuantityId};
use adl_solver::AssertName;
use adl_syntax::ast::CmpOp;
use adl_syntax::span::{LineMap, Span};
use std::collections::BTreeSet;

/// One encoded membership statement.
#[derive(Debug, Clone)]
pub struct StmtEnc {
    pub name: AssertName,
    pub span: Span,
    pub line: u32,
    pub text: String,
    pub formula: Formula,
    pub diags: DiagTable,
}

impl StmtEnc {
    #[must_use]
    pub fn over(&self) -> Over {
        self.formula.over()
    }

    #[must_use]
    pub fn under(&self) -> Under {
        self.formula.under()
    }
}

/// One region, encoded at statement granularity.
#[derive(Debug, Clone)]
pub struct RegionEnc {
    pub idx: usize,
    pub name: String,
    pub stmts: Vec<StmtEnc>,
    pub quantities: BTreeSet<QuantityId>,
    pub leaves_total: usize,
    pub leaves_encoded: usize,
    pub or_clauses: usize,
    pub dual_hedges: usize,
    /// (line, reason) for every Unknown leaf.
    pub dropped: Vec<(u32, String)>,
}

impl RegionEnc {
    #[must_use]
    pub fn exact(&self) -> bool {
        self.stmts.iter().all(|s| s.formula.is_exact())
    }
}

/// One bin set: a boundary-list `bin` statement (bins `[b0,b1), …,
/// [bn,∞)`) or the region's boolean-bin statements taken together.
#[derive(Debug, Clone)]
pub struct BinSetEnc {
    pub region_idx: usize,
    pub variable: String,
    pub bins: Vec<Formula>,
}

/// The encoded analysis unit.
#[derive(Debug, Clone)]
pub struct UnitEnc {
    pub regions: Vec<RegionEnc>,
    pub bin_sets: Vec<BinSetEnc>,
}

/// Verifier-side re-tag of undeclared-external-function quantities (see
/// module docs). Only `HKind::Quantity(ExternalFn)` nodes whose ONLY
/// problem is "not declared in the external library" are touched.
pub fn retag_opaque_externals(hir: &mut Hir) {
    let mut regions = std::mem::take(&mut hir.regions);
    for region in &mut regions {
        for stmt in &mut region.stmts {
            match stmt {
                HirRegionStmt::Select(n) | HirRegionStmt::Reject(n) | HirRegionStmt::Trigger(n) => {
                    retag_node(hir, n);
                }
                HirRegionStmt::Bin { var, .. } => retag_node(hir, var),
                HirRegionStmt::BinCond { cond, .. } => retag_node(hir, cond),
                _ => {}
            }
        }
    }
    hir.regions = regions;
}

fn retag_node(hir: &Hir, node: &mut HNode) {
    if let Fragment::Unsupported(reason) = &node.tag
        && reason.contains("is not declared in the external library")
        && let HKind::Quantity(q) = &node.kind
        && matches!(hir.table.quantity(*q), Quantity::ExternalFn { .. })
    {
        node.tag = Fragment::InFragment;
    }
    match &mut node.kind {
        HKind::Neg(a) | HKind::Not(a) | HKind::Abs(a) => retag_node(hir, a),
        HKind::Binary { lhs, rhs, .. } | HKind::Cmp { lhs, rhs, .. } => {
            retag_node(hir, lhs);
            retag_node(hir, rhs);
        }
        HKind::And(v) | HKind::Or(v) => {
            for n in v {
                retag_node(hir, n);
            }
        }
        HKind::Band { expr, .. } => retag_node(hir, expr),
        HKind::Ternary { guard, then, els } => {
            retag_node(hir, guard);
            retag_node(hir, then);
            if let Some(e) = els {
                retag_node(hir, e);
            }
        }
        _ => {}
    }
}

/// Encode a synthetic single-purpose region and remove it again. Only
/// the quantity table keeps the (harmless) interned growth.
fn encode_synthetic(hir: &mut Hir, stmts: Vec<HirRegionStmt>, span: Span) -> EncodedRegion {
    let name = hir.symbols.intern("__adl2_synth__");
    hir.regions.push(HirRegion { name, stmts, span });
    hir.region_name_order.push(name);
    let idx = hir.regions.len() - 1;
    let enc = encode_region(hir, idx);
    hir.regions.pop();
    hir.region_name_order.pop();
    enc
}

fn stmt_span(stmt: &HirRegionStmt) -> Span {
    match stmt {
        HirRegionStmt::Select(n) | HirRegionStmt::Reject(n) | HirRegionStmt::Trigger(n) => n.span,
        HirRegionStmt::Inherit { span, .. }
        | HirRegionStmt::Bin { span, .. }
        | HirRegionStmt::BinCond { span, .. }
        | HirRegionStmt::NonMembership { span, .. } => *span,
    }
}

fn is_membership(stmt: &HirRegionStmt) -> bool {
    match stmt {
        HirRegionStmt::Select(_)
        | HirRegionStmt::Reject(_)
        | HirRegionStmt::Inherit { .. }
        | HirRegionStmt::Trigger(_) => true,
        HirRegionStmt::Bin { .. } | HirRegionStmt::BinCond { .. } => false,
        // Unsupported non-membership statements still contribute an
        // Unknown leaf (honest coverage); in-fragment ones contribute
        // nothing.
        HirRegionStmt::NonMembership { tag, .. } => !tag.is_in_fragment(),
    }
}

/// Walk a formula counting coverage; `Dual` counts once (a hedge, not a
/// drop) and is not descended into.
fn coverage(f: &Formula, diags: &DiagTable, out: &mut RegionEnc, map: &LineMap) {
    match f {
        Formula::True | Formula::False | Formula::Atom(_) => {
            out.leaves_total += 1;
            out.leaves_encoded += 1;
        }
        Formula::And(v) => {
            for p in v {
                coverage(p, diags, out, map);
            }
        }
        Formula::Or(v) => {
            out.or_clauses += 1;
            for p in v {
                coverage(p, diags, out, map);
            }
        }
        Formula::Unknown(d) => {
            out.leaves_total += 1;
            if let Some(diag) = diags.get(*d) {
                let (line, _) = map.line_col(diag.span.start);
                out.dropped.push((line, diag.reason.clone()));
            }
        }
        Formula::Dual { .. } => {
            out.leaves_total += 1;
            out.leaves_encoded += 1;
            out.dual_hedges += 1;
        }
    }
}

/// Every quantity in a formula, both Dual branches included.
pub fn formula_quantities(f: &Formula, out: &mut BTreeSet<QuantityId>) {
    match f {
        Formula::True | Formula::False | Formula::Unknown(_) => {}
        Formula::Atom(a) => out.extend(a.terms().iter().map(|&(_, q)| q)),
        Formula::And(v) | Formula::Or(v) => {
            for p in v {
                formula_quantities(p, out);
            }
        }
        Formula::Dual { plus, minus, .. } => {
            formula_quantities(plus, out);
            formula_quantities(minus, out);
        }
    }
}

/// Encode every region (statement granularity) and every bin set.
pub fn encode_unit(hir: &mut Hir, src: &str) -> UnitEnc {
    let map = LineMap::new(src);
    let mut regions = Vec::new();
    let mut bin_sets = Vec::new();

    for ridx in 0..hir.regions.len() {
        let name = hir.symbols.display(hir.regions[ridx].name).to_owned();
        let stmt_list: Vec<HirRegionStmt> = hir.regions[ridx].stmts.clone();
        let mut enc = RegionEnc {
            idx: ridx,
            name,
            stmts: Vec::new(),
            quantities: BTreeSet::new(),
            leaves_total: 0,
            leaves_encoded: 0,
            or_clauses: 0,
            dual_hedges: 0,
            dropped: Vec::new(),
        };

        let mut cond_bins: Vec<Formula> = Vec::new();
        for (sidx, stmt) in stmt_list.iter().enumerate() {
            let span = stmt_span(stmt);
            if is_membership(stmt) {
                let e = encode_synthetic(hir, vec![stmt.clone()], span);
                let (line, _) = map.line_col(span.start);
                // Source-line text, or — for a merged cross-file unit with no
                // single `src` — the canonical HIR render of the cut, so the
                // unsat-core explanation stays meaningful instead of blank.
                let text = if src.is_empty() {
                    match stmt {
                        HirRegionStmt::Select(n) | HirRegionStmt::Trigger(n) => {
                            adl_sema::render_node(hir, n)
                        }
                        HirRegionStmt::Reject(n) => {
                            format!("reject {}", adl_sema::render_node(hir, n))
                        }
                        _ => String::new(),
                    }
                } else {
                    map.line_text(src, span.start).trim().to_owned()
                };
                coverage(&e.formula, &e.diags, &mut enc, &map);
                formula_quantities(&e.formula, &mut enc.quantities);
                enc.stmts.push(StmtEnc {
                    name: AssertName::new(format!("R{ridx}S{sidx}")),
                    span,
                    line,
                    text,
                    formula: e.formula,
                    diags: e.diags,
                });
            } else {
                match stmt {
                    HirRegionStmt::Bin { var, edges, .. } => {
                        if let Some(set) = encode_boundary_bins(hir, ridx, var, edges, span, src) {
                            bin_sets.push(set);
                        }
                    }
                    HirRegionStmt::BinCond { cond, .. } => {
                        let e =
                            encode_synthetic(hir, vec![HirRegionStmt::Select(cond.clone())], span);
                        cond_bins.push(e.formula);
                    }
                    _ => {}
                }
            }
        }
        if !cond_bins.is_empty() {
            bin_sets.push(BinSetEnc {
                region_idx: ridx,
                variable: "boolean bins".to_owned(),
                bins: cond_bins,
            });
        }
        regions.push(enc);
    }

    UnitEnc { regions, bin_sets }
}

/// `bin v b0 … bn` ⇒ formulas for `[b0,b1), …, [bn-1,bn), [bn,∞)`
/// (real-valued edges, open last bin — SPEC_LANGUAGE §4.3, divergence 5).
fn encode_boundary_bins(
    hir: &mut Hir,
    region_idx: usize,
    var: &HNode,
    edges: &[String],
    span: Span,
    src: &str,
) -> Option<BinSetEnc> {
    if edges.is_empty() {
        return None;
    }
    // Bin variable label from source text, or — for a merged unit with no
    // single `src` — the canonical HIR render (instead of a useless `?`).
    let variable = src
        .get(var.span.start as usize..var.span.end as usize)
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| adl_sema::render_node(hir, var));
    let num = |text: &str| HNode::new(HKind::Num(text.to_owned()), span);
    let cmp = |op: CmpOp, rhs: HNode| {
        HNode::new(
            HKind::Cmp {
                op,
                lhs: Box::new(var.clone()),
                rhs: Box::new(rhs),
            },
            span,
        )
    };
    let mut bins = Vec::new();
    for i in 0..edges.len() {
        let mut stmts = vec![HirRegionStmt::Select(cmp(CmpOp::Ge, num(&edges[i])))];
        if i + 1 < edges.len() {
            stmts.push(HirRegionStmt::Select(cmp(CmpOp::Lt, num(&edges[i + 1]))));
        }
        let e = encode_synthetic(hir, stmts, span);
        bins.push(e.formula);
    }
    Some(BinSetEnc {
        region_idx,
        variable,
        bins,
    })
}
