//! Graphviz DOT emitters built from the resolved HIR (SPEC_ARCHITECTURE
//! §1/§9). Output is deterministic and byte-identical across runs: every
//! iteration is in declaration order and node ids are derived from stable
//! HIR indices, never from hashing or pointer order.
//!
//! Two graphs:
//! - [`flowchart_dot`] — the analysis structure: object collections with
//!   their `take`/inheritance lineage, and regions with their ordered
//!   membership statements and inheritance edges.
//! - [`ast_dot`] — the resolved expression trees of every region cut,
//!   object predicate and define, as a node-per-subexpression graph.

use crate::label::Labeler;
use adl_sema::{Collection, CollectionId, Fragment, HKind, HNode, Hir, HirRegionStmt};
use std::fmt::Write as _;

/// Escape a string for use inside a DOT double-quoted label.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            _ => out.push(c),
        }
    }
    out
}

/// Flowchart DOT: collections (with lineage) and regions (with ordered
/// statements + inheritance).
#[must_use]
pub fn flowchart_dot(hir: &Hir) -> String {
    let lbl = Labeler::new(hir);
    let mut s = String::new();
    let _ = writeln!(s, "digraph flowchart {{");
    let _ = writeln!(s, "  rankdir=LR;");
    let _ = writeln!(s, "  node [shape=box, fontname=\"monospace\"];");
    let _ = writeln!(s, "  label=\"{}\";", esc(&hir.unit));
    let _ = writeln!(s, "  labelloc=t;");

    // --- Collections (objects subgraph). One node per CollectionId that is
    // either bound to a name or referenced as an object's collection. We
    // emit in CollectionId order for determinism, but only for collections
    // that carry a bound name (the ones a reader recognizes); the rest are
    // reachable only as lineage parents and are drawn on demand below.
    let _ = writeln!(s, "  subgraph cluster_objects {{");
    let _ = writeln!(s, "    label=\"objects\";");
    let _ = writeln!(s, "    color=gray70;");
    for (i, _coll) in hir.table.collections().iter().enumerate() {
        let id = CollectionId(u32::try_from(i).expect("collection id overflow"));
        if !collection_has_node(hir, id) {
            continue;
        }
        let kind = collection_kind(hir, id);
        let label = format!("{}\\n[{kind}]", esc(&lbl.collection(id)));
        let style = if collection_unsupported(hir, id) {
            ", style=filled, fillcolor=\"#ffe0e0\""
        } else {
            ", style=filled, fillcolor=\"#e0f0ff\""
        };
        let _ = writeln!(s, "    coll{i} [label=\"{label}\"{style}];");
    }
    let _ = writeln!(s, "  }}");

    // Lineage edges (take / union / combination). A `take Jet` shows the
    // base `Jet` collection feeding the filtered child; unions/combinations
    // fan in from every part. Every endpoint has a node (named or base).
    for (i, coll) in hir.table.collections().iter().enumerate() {
        if !collection_has_node(hir, CollectionId(u32::try_from(i).expect("id overflow"))) {
            continue;
        }
        match coll {
            Collection::Filtered { parent, .. } => {
                edge_if_node(&mut s, hir, *parent, i, "take");
            }
            Collection::Union(parts) | Collection::Combination { parts, .. } => {
                let kw = if matches!(coll, Collection::Union(_)) {
                    "union"
                } else {
                    "comb"
                };
                for p in parts {
                    edge_if_node(&mut s, hir, *p, i, kw);
                }
            }
            Collection::Sorted { source, .. } => edge_if_node(&mut s, hir, *source, i, "sort"),
            Collection::Slice { source, .. } => edge_if_node(&mut s, hir, *source, i, "slice"),
            Collection::CombProject { comb, .. } => {
                edge_if_node(&mut s, hir, *comb, i, "->");
            }
            Collection::Base(_) => {}
        }
    }

    // --- Regions subgraph: one node per region, ordered statement list as
    // the label, inheritance drawn as edges between region nodes.
    let _ = writeln!(s, "  subgraph cluster_regions {{");
    let _ = writeln!(s, "    label=\"regions\";");
    let _ = writeln!(s, "    color=gray70;");
    for (ri, region) in hir.regions.iter().enumerate() {
        let name = hir.symbols.display(region.name);
        let mut lines = vec![esc(name)];
        for stmt in &region.stmts {
            lines.push(esc(&render_stmt_short(&lbl, stmt, hir)));
        }
        let label = lines.join("\\l");
        let _ = writeln!(
            s,
            "    region{ri} [label=\"{label}\\l\", style=filled, fillcolor=\"#f0fff0\"];"
        );
    }
    let _ = writeln!(s, "  }}");

    // Inheritance edges between regions. Both statement forms inherit:
    // the bare-name form (`Inherit`) and the region-as-predicate form
    // (`select presel`, `HKind::RegionPred`) chain the parent's cuts.
    for (ri, region) in hir.regions.iter().enumerate() {
        let mut parents: Vec<usize> = Vec::new();
        for stmt in &region.stmts {
            match stmt {
                HirRegionStmt::Inherit { region: parent, .. } => parents.push(*parent),
                // Select only: a region predicate under `reject` negates
                // the parent's cuts and is not inheritance.
                HirRegionStmt::Select(n) => collect_region_preds(n, &mut parents),
                _ => {}
            }
        }
        parents.sort_unstable();
        parents.dedup();
        for parent in parents {
            if let Some(pi) = region_index_of(hir, parent) {
                let _ = writeln!(
                    s,
                    "  region{pi} -> region{ri} [label=\"inherit\", style=dashed];"
                );
            }
        }
    }

    // Object → region usage edges: a region statement mentioning a named
    // collection draws a light edge from that collection to the region, so
    // the reader sees which objects feed which selections.
    for (ri, region) in hir.regions.iter().enumerate() {
        let mut seen: Vec<usize> = Vec::new();
        for stmt in &region.stmts {
            for node in stmt_nodes(stmt) {
                collect_used_collections(hir, node, &mut seen);
            }
        }
        seen.sort_unstable();
        seen.dedup();
        for ci in seen {
            let _ = writeln!(s, "  coll{ci} -> region{ri} [style=dotted, color=gray60];");
        }
    }

    let _ = writeln!(s, "}}");
    s
}

fn collection_is_named(hir: &Hir, id: CollectionId) -> bool {
    hir.coll_names
        .get(id.0 as usize)
        .is_some_and(|names| !names.is_empty())
}

/// A collection gets its own flowchart node iff it is bound to a name or
/// is a detector-level base (bases always render with their own symbol, so
/// `take Jet` lineage stays visible even when `Jet` is never re-bound).
fn collection_has_node(hir: &Hir, id: CollectionId) -> bool {
    collection_is_named(hir, id) || matches!(hir.table.collection(id), Collection::Base(_))
}

fn collection_kind(hir: &Hir, id: CollectionId) -> &'static str {
    match hir.table.collection(id) {
        Collection::Base(_) => "base",
        Collection::Filtered { .. } => "filtered",
        Collection::Union(_) => "union",
        Collection::Combination { .. } => "comb",
        Collection::Sorted { .. } => "sorted",
        Collection::Slice { .. } => "slice",
        Collection::CombProject { .. } => "projection",
    }
}

fn collection_unsupported(hir: &Hir, id: CollectionId) -> bool {
    hir.objects
        .iter()
        .find(|o| o.coll == id)
        .is_some_and(|o| matches!(o.tag, Fragment::Unsupported(_)))
}

fn edge_if_node(s: &mut String, hir: &Hir, parent: CollectionId, child_idx: usize, kw: &str) {
    if collection_has_node(hir, parent) {
        let pi = parent.0 as usize;
        let _ = writeln!(s, "  coll{pi} -> coll{child_idx} [label=\"{kw}\"];");
    }
}

fn region_index_of(hir: &Hir, order_idx: usize) -> Option<usize> {
    let sym = hir.region_name_order.get(order_idx)?;
    hir.regions.iter().position(|r| r.name == *sym)
}

/// Collections (by id) used inside an expression node, named ones only.
fn collect_used_collections(hir: &Hir, node: &HNode, out: &mut Vec<usize>) {
    use adl_sema::{ParticleRef, Quantity};
    let push = |id: CollectionId, out: &mut Vec<usize>| {
        if collection_has_node(hir, id) {
            out.push(id.0 as usize);
        }
    };
    match &node.kind {
        HKind::Quantity(q) => {
            let particle_coll = |p: &ParticleRef| match p {
                ParticleRef::Elem { coll, .. }
                | ParticleRef::Whole(coll)
                | ParticleRef::Binder { coll, .. } => Some(*coll),
                // No single fixed collection: skip for graph-edge collection.
                ParticleRef::Met
                | ParticleRef::ThisElem
                | ParticleRef::ReduceElem
                | ParticleRef::Sum(_) => None,
            };
            match hir.table.quantity(*q) {
                Quantity::Size(c) => push(*c, out),
                Quantity::ElemProp { coll, .. } => push(*coll, out),
                Quantity::AngularSep { a, b, .. } => {
                    if let Some(c) = particle_coll(a) {
                        push(c, out);
                    }
                    if let Some(c) = particle_coll(b) {
                        push(c, out);
                    }
                }
                Quantity::EventScalar(_) | Quantity::ExternalFn { .. } => {}
            }
        }
        HKind::CollProp { coll, .. } | HKind::CollValue(coll) => push(*coll, out),
        HKind::Reduce { coll, body, .. } => {
            push(*coll, out);
            collect_used_collections(hir, body, out);
        }
        HKind::Neg(a) | HKind::Not(a) | HKind::Abs(a) | HKind::Band { expr: a, .. } => {
            collect_used_collections(hir, a, out);
        }
        HKind::Binary { lhs, rhs, .. } | HKind::Cmp { lhs, rhs, .. } => {
            collect_used_collections(hir, lhs, out);
            collect_used_collections(hir, rhs, out);
        }
        HKind::And(v) | HKind::Or(v) => {
            for c in v {
                collect_used_collections(hir, c, out);
            }
        }
        HKind::Ternary { guard, then, els } => {
            collect_used_collections(hir, guard, out);
            collect_used_collections(hir, then, out);
            if let Some(e) = els {
                collect_used_collections(hir, e, out);
            }
        }
        _ => {}
    }
}

/// Prior-region order-indices referenced as predicates (`select presel`)
/// anywhere inside an expression node.
fn collect_region_preds(node: &HNode, out: &mut Vec<usize>) {
    match &node.kind {
        HKind::RegionPred(idx) => out.push(*idx),
        HKind::Neg(a) | HKind::Not(a) | HKind::Abs(a) => collect_region_preds(a, out),
        HKind::Binary { lhs, rhs, .. } | HKind::Cmp { lhs, rhs, .. } => {
            collect_region_preds(lhs, out);
            collect_region_preds(rhs, out);
        }
        HKind::And(v) | HKind::Or(v) => {
            for n in v {
                collect_region_preds(n, out);
            }
        }
        HKind::Band { expr, .. } => collect_region_preds(expr, out),
        HKind::Ternary { guard, then, els } => {
            collect_region_preds(guard, out);
            collect_region_preds(then, out);
            if let Some(e) = els {
                collect_region_preds(e, out);
            }
        }
        _ => {}
    }
}

/// Expression nodes a region statement contributes (for usage edges).
fn stmt_nodes(stmt: &HirRegionStmt) -> Vec<&HNode> {
    match stmt {
        HirRegionStmt::Select(n)
        | HirRegionStmt::Reject(n)
        | HirRegionStmt::Trigger(n)
        | HirRegionStmt::BinCond { cond: n, .. } => vec![n],
        HirRegionStmt::Bin { var, .. } => vec![var],
        HirRegionStmt::Inherit { .. } | HirRegionStmt::NonMembership { .. } => Vec::new(),
    }
}

/// One-line label for a region statement (the flowchart node body).
fn render_stmt_short(lbl: &Labeler, stmt: &HirRegionStmt, hir: &Hir) -> String {
    match stmt {
        HirRegionStmt::Select(n) => format!("select {}", lbl.node(n)),
        HirRegionStmt::Reject(n) => format!("reject {}", lbl.node(n)),
        HirRegionStmt::Trigger(n) => format!("trigger {}", lbl.node(n)),
        HirRegionStmt::Inherit { region, .. } => {
            let name = hir
                .region_name_order
                .get(*region)
                .map_or("?", |&s| hir.symbols.display(s));
            format!("inherit {name}")
        }
        HirRegionStmt::Bin {
            label, var, edges, ..
        } => {
            let l = label
                .as_deref()
                .map(|l| format!("{l} "))
                .unwrap_or_default();
            format!("bin {l}{} {}", lbl.node(var), edges.join(" "))
        }
        HirRegionStmt::BinCond { label, cond, .. } => {
            let l = label
                .as_deref()
                .map(|l| format!("{l} "))
                .unwrap_or_default();
            format!("bin {l}{}", lbl.node(cond))
        }
        HirRegionStmt::NonMembership { kind, .. } => format!("{kind} (no membership)"),
    }
}

/// AST DOT: the resolved expression trees. Each region cut, object element
/// predicate and define body becomes a labeled subtree rooted at a header
/// node; every subexpression is its own node with edges to its children.
///
/// The trees are a forest: without constraints dot lays the components
/// side by side, and large files render as a 100k+ pt flat ribbon. The
/// per-item roots are therefore chained with invisible edges whose
/// `minlen` is the previous tree's depth, so each component starts below
/// the deepest rank of the one before it — the diagram grows in height
/// and its width is bounded by the widest single tree.
#[must_use]
pub fn ast_dot(hir: &Hir) -> String {
    let lbl = Labeler::new(hir);
    let mut s = String::new();
    let _ = writeln!(s, "digraph ast {{");
    let _ = writeln!(s, "  node [shape=box, fontname=\"monospace\"];");
    let _ = writeln!(s, "  label=\"{} (AST)\";", esc(&hir.unit));
    let _ = writeln!(s, "  labelloc=t;");

    let mut ctr: usize = 0;
    // (root id, subtree depth in ranks) per emitted component.
    let mut roots: Vec<(String, usize)> = Vec::new();

    // Defines first.
    for def in &hir.defines {
        let name = hir.symbols.display(def.name);
        let root = next_id(&mut ctr);
        let _ = writeln!(
            s,
            "  {root} [label=\"define {} ({})\", style=filled, fillcolor=\"#fff0e0\"];",
            esc(name),
            def.kind.as_str()
        );
        let (child, depth) = emit_node(&mut s, &lbl, &def.body, &mut ctr);
        let _ = writeln!(s, "  {root} -> {child};");
        roots.push((root, depth + 1));
    }

    // Object element predicates.
    for obj in &hir.objects {
        let Collection::Filtered { pred, .. } = hir.table.collection(obj.coll) else {
            continue;
        };
        let name = hir.symbols.display(obj.name);
        let root = next_id(&mut ctr);
        let _ = writeln!(
            s,
            "  {root} [label=\"object {}\", style=filled, fillcolor=\"#e0f0ff\"];",
            esc(name)
        );
        let ep = hir.elem_pred(*pred);
        let (child, depth) = emit_node(&mut s, &lbl, &ep.node, &mut ctr);
        let _ = writeln!(s, "  {root} -> {child} [label=\"predicate\"];");
        roots.push((root, depth + 1));
    }

    // Region statements.
    for region in &hir.regions {
        let name = hir.symbols.display(region.name);
        let root = next_id(&mut ctr);
        let _ = writeln!(
            s,
            "  {root} [label=\"region {}\", style=filled, fillcolor=\"#f0fff0\"];",
            esc(name)
        );
        let mut max_depth = 0usize;
        for stmt in &region.stmts {
            for node in stmt_nodes(stmt) {
                let kw = stmt_keyword(stmt);
                let (child, depth) = emit_node(&mut s, &lbl, node, &mut ctr);
                let _ = writeln!(s, "  {root} -> {child} [label=\"{kw}\"];");
                max_depth = max_depth.max(depth);
            }
        }
        roots.push((root, max_depth + 1));
    }

    // Invisible vertical chain between component roots (layout only).
    for w in roots.windows(2) {
        let (prev, depth) = (&w[0].0, w[0].1);
        let next = &w[1].0;
        let _ = writeln!(
            s,
            "  {prev} -> {next} [style=invis, weight=100, minlen={}];",
            depth.max(1)
        );
    }

    let _ = writeln!(s, "}}");
    s
}

fn stmt_keyword(stmt: &HirRegionStmt) -> &'static str {
    match stmt {
        HirRegionStmt::Select(_) => "select",
        HirRegionStmt::Reject(_) => "reject",
        HirRegionStmt::Trigger(_) => "trigger",
        HirRegionStmt::Bin { .. } | HirRegionStmt::BinCond { .. } => "bin",
        HirRegionStmt::Inherit { .. } | HirRegionStmt::NonMembership { .. } => "",
    }
}

fn next_id(ctr: &mut usize) -> String {
    let id = format!("n{ctr}");
    *ctr += 1;
    id
}

/// Emit a node and its children, returning the node's DOT id and the
/// subtree depth in ranks (a leaf is depth 1). The label is the node's
/// own operator/leaf; children are separate nodes with edges.
fn emit_node(s: &mut String, lbl: &Labeler, n: &HNode, ctr: &mut usize) -> (String, usize) {
    let id = next_id(ctr);
    let (label, children) = node_label_and_children(lbl, n);
    let fill = if matches!(n.tag, Fragment::Unsupported(_)) {
        ", style=filled, fillcolor=\"#ffe0e0\""
    } else {
        ""
    };
    let _ = writeln!(s, "  {id} [label=\"{}\"{fill}];", esc(&label));
    let mut depth = 0usize;
    for child in children {
        let (cid, cdepth) = emit_node(s, lbl, child, ctr);
        let _ = writeln!(s, "  {id} -> {cid};");
        depth = depth.max(cdepth);
    }
    (id, depth + 1)
}

/// Operator/leaf label plus the children we recurse into. Leaves render
/// their full quantity/literal text (via the shared labeler); operators
/// render only the symbol and lean on child nodes for operands.
fn node_label_and_children<'n>(lbl: &Labeler, n: &'n HNode) -> (String, Vec<&'n HNode>) {
    match &n.kind {
        HKind::Num(_)
        | HKind::Bool(_)
        | HKind::Quantity(_)
        | HKind::ElemSelfProp(_)
        | HKind::ReduceProp(_)
        | HKind::CollProp { .. }
        | HKind::Particle(_)
        | HKind::CollValue(_)
        | HKind::RegionPred(_)
        | HKind::Unsupported => (lbl.node(n), Vec::new()),
        HKind::Reduce { kind, body, .. } => (kind.as_str().to_owned(), vec![body.as_ref()]),
        HKind::ScalarMinMax { kind, args } => (kind.as_str().to_owned(), args.iter().collect()),
        HKind::Neg(a) => ("neg".to_owned(), vec![a.as_ref()]),
        HKind::Not(a) => ("not".to_owned(), vec![a.as_ref()]),
        HKind::Abs(a) => ("abs".to_owned(), vec![a.as_ref()]),
        HKind::Binary { op, lhs, rhs } => {
            (op.as_str().to_owned(), vec![lhs.as_ref(), rhs.as_ref()])
        }
        HKind::Cmp { op, lhs, rhs } => (op.as_str().to_owned(), vec![lhs.as_ref(), rhs.as_ref()]),
        HKind::And(v) => ("and".to_owned(), v.iter().collect()),
        HKind::Or(v) => ("or".to_owned(), v.iter().collect()),
        HKind::Band { kind, expr, lo, hi } => {
            let op = match kind {
                adl_syntax::ast::BandKind::In => "[]",
                adl_syntax::ast::BandKind::Out => "][",
            };
            (format!("{op} {lo} {hi}"), vec![expr.as_ref()])
        }
        HKind::Ternary { guard, then, els } => {
            let mut c = vec![guard.as_ref(), then.as_ref()];
            if let Some(e) = els {
                c.push(e.as_ref());
            }
            ("ternary ?:".to_owned(), c)
        }
    }
}
