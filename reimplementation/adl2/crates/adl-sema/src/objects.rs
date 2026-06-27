//! The object-attribute summary: one aligned row per declared collection
//! (base, filtered, union, combination), in declaration order. The modern
//! successor of the legacy `printObjectAttributes` — built from the HIR's
//! Collection identity model instead of textual take-chain walking.
//!
//! Each row carries: the bound name(s), the base chain (parents up to the
//! detector-level `Base`, pure renames on one collection collapsed with
//! `=`), the element cuts of a filtered collection rendered as a flat
//! human predicate (`pt > 25, |eta| < 2.4`), the fragment status
//! (`exact` / `partial: reason`), and — where the identity model proves
//! them by construction — derived size facts (subset of parent, union
//! bounds).
//!
//! Determinism: rows are emitted in `CollectionId` order, which is binding
//! (declaration) order. Color is opt-in (the caller decides from
//! tty + `NO_COLOR`); the plain path is byte-stable and snapshot-tested.

use crate::hir::{HKind, HNode, Hir};
use crate::intern::Symbol;
use crate::quantity::{
    Collection, CollectionId, ParticleRef, Quantity, QuantityArg, ScalarSource,
};

/// ANSI styling for the object table, no-op when disabled. Mirrors the
/// verifier's `Style` so the two reports look the same under
/// `verify --explain`.
#[derive(Clone, Copy)]
struct Style {
    on: bool,
}

impl Style {
    fn wrap(self, code: &str, s: &str) -> String {
        if self.on {
            format!("\x1b[{code}m{s}\x1b[0m")
        } else {
            s.to_owned()
        }
    }
    fn head(self, s: &str) -> String {
        self.wrap("1", s)
    }
    /// Green for `exact`, yellow for `partial`.
    fn fragment(self, exact: bool, s: &str) -> String {
        self.wrap(if exact { "32" } else { "33" }, s)
    }
}

/// One rendered row of the object table.
struct Row {
    name: String,
    chain: String,
    cuts: String,
    fragment: String,
    exact: bool,
    /// Derived-facts lines (`subset of …`, `bounded by …`); may be empty.
    facts: Vec<String>,
}

/// Build the object-attribute summary for `hir`. `color` enables ANSI
/// styling (the caller decides from tty + `NO_COLOR`).
#[must_use]
pub fn object_table(hir: &Hir, color: bool) -> String {
    let st = Style { on: color };
    let mut rows: Vec<Row> = hir
        .table
        .collections()
        .iter()
        .enumerate()
        .filter_map(|(i, coll)| {
            let id = CollectionId(u32::try_from(i).expect("collection id overflow"));
            build_row(hir, id, coll)
        })
        .collect();

    let mut out = String::new();
    out.push_str(&st.head("== objects =="));
    out.push('\n');
    if rows.is_empty() {
        out.push_str("  (no collections)\n");
        return out;
    }

    // Cap the cut column so one verbose predicate cannot blow out the
    // table; full text is always available in the HIR / quantity-table
    // dump. Names and chains stay un-truncated (they are short).
    for r in &mut rows {
        r.cuts = ellipsize(&r.cuts, CUTS_MAX);
    }
    let name_w = col_width(&rows, |r| &r.name);
    let chain_w = col_width(&rows, |r| &r.chain);
    let cuts_w = col_width(&rows, |r| &r.cuts);

    for r in &rows {
        out.push_str(&format!(
            "  {name:name_w$}  {chain:chain_w$}  {cuts:cuts_w$}  {frag}\n",
            name = r.name,
            chain = r.chain,
            cuts = r.cuts,
            frag = st.fragment(r.exact, &r.fragment),
        ));
        for fact in &r.facts {
            out.push_str(&format!("  {:name_w$}    {}\n", "", fact));
        }
    }
    out
}

/// Maximum width of the element-cuts column (char count). One verbose
/// predicate is truncated with `…`; the full text lives in the HIR dump.
const CUTS_MAX: usize = 64;

fn col_width(rows: &[Row], f: impl Fn(&Row) -> &str) -> usize {
    rows.iter().map(|r| f(r).chars().count()).max().unwrap_or(0)
}

/// Truncate to `max` chars with an ellipsis (char-safe).
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Build a row for one collection, or `None` for a bare detector input
/// with no user object binding (it appears only as a chain tail).
fn build_row(hir: &Hir, id: CollectionId, coll: &Collection) -> Option<Row> {
    let names = &hir.coll_names[id.0 as usize];
    if names.is_empty() && matches!(coll, Collection::Base(_)) {
        return None;
    }

    let name = collapse_names(hir, names);
    let chain = base_chain(hir, id);
    let (cuts, cuts_exact) = element_cuts(hir, coll);
    let obj_tag = object_tag(hir, id);
    let exact = cuts_exact && obj_tag.is_none();
    let fragment = match (&obj_tag, exact) {
        (Some(reason), _) => format!("partial: {reason}"),
        (None, true) => "exact".to_owned(),
        (None, false) => "partial: cut out of fragment".to_owned(),
    };
    let facts = derived_facts(hir, id, coll);

    Some(Row {
        name,
        chain,
        cuts,
        fragment,
        exact,
        facts,
    })
}

/// Render a collection's bound names, pure renames collapsed with `=`
/// (the first name is canonical). A single name renders bare.
fn collapse_names(hir: &Hir, names: &[Symbol]) -> String {
    names
        .iter()
        .map(|&s| hir.symbols.display(s))
        .collect::<Vec<_>>()
        .join(" = ")
}

/// The base chain: this collection's name(s), then its parent's, up to the
/// detector-level `Base`, joined with `<-`. Union/combination have no
/// single parent chain — the chain is just this node (parts are shown as
/// derived facts).
fn base_chain(hir: &Hir, id: CollectionId) -> String {
    let mut links: Vec<String> = Vec::new();
    let mut cur = id;
    loop {
        let names = &hir.coll_names[cur.0 as usize];
        match hir.table.collection(cur) {
            Collection::Filtered { parent, .. } => {
                links.push(link_name(hir, cur, names));
                cur = *parent;
            }
            Collection::Base(sym) => {
                links.push(if names.is_empty() {
                    hir.symbols.display(*sym).to_owned()
                } else {
                    collapse_names(hir, names)
                });
                break;
            }
            // A sort/slice is a permutation/sub-range of its source — follow it.
            Collection::Sorted { source, .. } | Collection::Slice { source, .. } => {
                links.push(link_name(hir, cur, names));
                cur = *source;
            }
            Collection::Union(_)
            | Collection::Combination { .. }
            | Collection::CombProject { .. } => {
                links.push(link_name(hir, cur, names));
                break;
            }
        }
    }
    links.join(" <- ")
}

fn link_name(hir: &Hir, id: CollectionId, names: &[Symbol]) -> String {
    if names.is_empty() {
        id.to_string()
    } else {
        collapse_names(hir, names)
    }
}

/// The element cuts of a filtered collection, flattened to a human
/// predicate list (`pt > 25, |eta| < 2.4`). Returns `(text, exact)` where
/// `exact` is false if any cut leaf is out of fragment. Non-filtered
/// collections have no own cuts.
fn element_cuts(hir: &Hir, coll: &Collection) -> (String, bool) {
    let Collection::Filtered { pred, .. } = coll else {
        return ("—".to_owned(), true);
    };
    let pred = hir.elem_pred(*pred);
    let mut parts = Vec::new();
    flatten_conj(hir, &pred.node, &mut parts);
    let exact = !pred.node.has_unsupported();
    let text = if parts.is_empty() {
        "(all)".to_owned()
    } else {
        parts.join(", ")
    };
    (text, exact)
}

/// Flatten a conjunction into its human-rendered conjuncts.
fn flatten_conj(hir: &Hir, n: &HNode, out: &mut Vec<String>) {
    match &n.kind {
        HKind::And(v) => {
            for c in v {
                flatten_conj(hir, c, out);
            }
        }
        _ => out.push(render_clause(hir, n)),
    }
}

/// Render one cut clause in human form: the implicit element's `this.` is
/// dropped, `abs(x)` becomes `|x|`, comparisons carry no outer parens.
fn render_clause(hir: &Hir, n: &HNode) -> String {
    match &n.kind {
        HKind::Cmp { op, lhs, rhs } => {
            format!(
                "{} {} {}",
                render_term(hir, lhs),
                op.as_str(),
                render_term(hir, rhs)
            )
        }
        HKind::Not(e) => format!("not {}", render_clause(hir, e)),
        HKind::Or(v) => {
            let parts: Vec<String> = v.iter().map(|c| render_clause(hir, c)).collect();
            format!("({})", parts.join(" or "))
        }
        HKind::And(v) => {
            let parts: Vec<String> = v.iter().map(|c| render_clause(hir, c)).collect();
            format!("({})", parts.join(" and "))
        }
        HKind::Band { kind, expr, lo, hi } => {
            let op = match kind {
                adl_syntax::ast::BandKind::In => "in",
                adl_syntax::ast::BandKind::Out => "out",
            };
            format!("{} {op} [{lo}, {hi}]", render_term(hir, expr))
        }
        HKind::Ternary { guard, then, els } => match els {
            Some(e) => format!(
                "{} ? {} : {}",
                render_clause(hir, guard),
                render_clause(hir, then),
                render_clause(hir, e)
            ),
            None => format!("{} ? {}", render_clause(hir, guard), render_clause(hir, then)),
        },
        _ => render_term(hir, n),
    }
}

/// Render a comparison term, dropping `this.` from self-properties and
/// rewriting `abs(x)` as `|x|`. Collection names render short (no `Cn#`).
fn render_term(hir: &Hir, n: &HNode) -> String {
    match &n.kind {
        HKind::Num(s) => s.clone(),
        HKind::Bool(b) => b.to_string(),
        HKind::ElemSelfProp(p) => hir.table.prop_display(*p).to_owned(),
        HKind::ReduceProp(p) => hir.table.prop_display(*p).to_owned(),
        HKind::Reduce { kind, coll, body, .. } => {
            format!("{}({}: {})", kind.as_str(), coll_short(hir, *coll), render_clause(hir, body))
        }
        HKind::CollProp { coll, prop } => {
            format!("{}.{}", coll_short(hir, *coll), hir.table.prop_display(*prop))
        }
        HKind::ScalarMinMax { kind, args } => {
            let inner: Vec<String> = args.iter().map(|a| render_term(hir, a)).collect();
            format!("{}({})", kind.as_str(), inner.join(", "))
        }
        HKind::Quantity(q) => render_quantity(hir, hir.table.quantity(*q)),
        HKind::Abs(e) => format!("|{}|", render_term(hir, e)),
        HKind::Neg(e) => format!("-{}", render_term(hir, e)),
        HKind::Binary { op, lhs, rhs } => {
            format!(
                "{} {} {}",
                render_term(hir, lhs),
                op.as_str(),
                render_term(hir, rhs)
            )
        }
        // Boolean structure appearing where a term is expected (rare):
        // fall back to the clause renderer.
        HKind::Cmp { .. }
        | HKind::And(_)
        | HKind::Or(_)
        | HKind::Not(_)
        | HKind::Band { .. }
        | HKind::Ternary { .. } => format!("({})", render_clause(hir, n)),
        HKind::Particle(p) => render_particle(hir, p),
        HKind::CollValue(c) => coll_short(hir, *c),
        HKind::RegionPred(_) => "<region>".to_owned(),
        HKind::Unsupported => unsupported_term(&n.tag),
    }
}

/// A readable stand-in for an out-of-fragment term: the unresolved
/// identifier when the reason names one (`` `passIso` `` → `passIso?`),
/// else the reason in angle brackets.
fn unsupported_term(tag: &crate::hir::Fragment) -> String {
    let crate::hir::Fragment::Unsupported(reason) = tag else {
        return "<?>".to_owned();
    };
    unresolved_name(reason).map_or_else(|| format!("<{reason}>"), |n| format!("{n}?"))
}

/// Collapse the verbose `<unsupported: unresolved identifier `x`>` text the
/// dump renderer embeds in opaque arguments to the same `x?` stand-in used
/// for top-level unsupported terms, so an in-fragment opaque external does
/// not leak a noisy diagnostic string into the cuts column.
fn simplify_opaque(text: &str) -> String {
    if let Some(inner) = text
        .strip_prefix("<unsupported: ")
        .and_then(|s| s.strip_suffix('>'))
        && let Some(name) = unresolved_name(inner)
    {
        return format!("{name}?");
    }
    text.to_owned()
}

/// Extract `x` from `unresolved identifier `x``, if that is the reason.
fn unresolved_name(reason: &str) -> Option<&str> {
    reason
        .strip_prefix("unresolved identifier `")
        .and_then(|rest| rest.strip_suffix('`'))
}

/// Short collection name (the canonical bound name, or the base/structural
/// id) — no `Cn#` prefix, unlike the identity-bearing dump renderer.
fn coll_short(hir: &Hir, id: CollectionId) -> String {
    if let Some(&s) = hir.coll_names[id.0 as usize].first() {
        return hir.symbols.display(s).to_owned();
    }
    match hir.table.collection(id) {
        Collection::Base(sym) => hir.symbols.display(*sym).to_owned(),
        _ => id.to_string(),
    }
}

fn render_particle(hir: &Hir, p: &ParticleRef) -> String {
    match p {
        ParticleRef::Elem { coll, index } => format!("{}[{index}]", coll_short(hir, *coll)),
        ParticleRef::Whole(coll) => coll_short(hir, *coll),
        ParticleRef::Met => "MET".to_owned(),
        ParticleRef::Binder { coll, name } => {
            format!("{}@{}", coll_short(hir, *coll), hir.symbols.display(*name))
        }
        ParticleRef::ThisElem => "this".to_owned(),
        ParticleRef::ReduceElem => "elem".to_owned(),
        ParticleRef::Sum(parts) => {
            let parts: Vec<String> = parts.iter().map(|p| render_particle(hir, p)).collect();
            format!("({})", parts.join(" + "))
        }
    }
}

/// Short quantity render for cut clauses (no `Cn#` identity prefix).
fn render_quantity(hir: &Hir, q: &Quantity) -> String {
    match q {
        Quantity::EventScalar(src) => match src {
            ScalarSource::MetProp(p) => format!("MET.{}", hir.table.prop_display(*p)),
            ScalarSource::EventVar(s) => hir.symbols.display(*s).to_owned(),
            ScalarSource::Trigger(s) => format!("trig({})", hir.symbols.display(*s)),
        },
        Quantity::Size(c) => format!("size({})", coll_short(hir, *c)),
        Quantity::ElemProp { coll, index, prop } => format!(
            "{}[{index}].{}",
            coll_short(hir, *coll),
            hir.table.prop_display(*prop)
        ),
        Quantity::AngularSep { kind, a, b, .. } => format!(
            "{}({}, {})",
            kind.as_str(),
            render_particle(hir, a),
            render_particle(hir, b)
        ),
        Quantity::ExternalFn { name, args } => {
            let args: Vec<String> = args.iter().map(|a| render_arg(hir, a)).collect();
            format!("{}({})", hir.symbols.display(*name), args.join(", "))
        }
    }
}

fn render_arg(hir: &Hir, a: &QuantityArg) -> String {
    match a {
        QuantityArg::Num(n) | QuantityArg::Opaque(n) => simplify_opaque(n),
        QuantityArg::Quantity(q) => render_quantity(hir, hir.table.quantity(*q)),
        QuantityArg::Particle(p) => render_particle(hir, p),
        QuantityArg::Collection(c) => coll_short(hir, *c),
        QuantityArg::CollProp { coll, prop } => {
            format!("{}.{}", coll_short(hir, *coll), hir.table.prop_display(*prop))
        }
    }
}

/// The fragment reason from the declared object backing this collection, if
/// any (the object block was tagged `Unsupported`). `None` means the block
/// itself is in-fragment (cut-level coverage is judged separately).
fn object_tag(hir: &Hir, id: CollectionId) -> Option<String> {
    hir.objects
        .iter()
        .find(|o| o.coll == id)
        .and_then(|o| match &o.tag {
            crate::hir::Fragment::Unsupported(reason) => Some(reason.clone()),
            crate::hir::Fragment::InFragment => None,
        })
}

/// Derived size facts the identity model proves by construction: a filtered
/// collection is a subset of its parent; a union's size is bounded by the
/// sum of its parts' sizes (and below by each part).
fn derived_facts(hir: &Hir, id: CollectionId, coll: &Collection) -> Vec<String> {
    match coll {
        Collection::Filtered { parent, .. } => {
            vec![format!(
                "size({}) ≤ size({})  (subset of parent)",
                coll_short(hir, id),
                coll_short(hir, *parent)
            )]
        }
        Collection::Union(parts) => {
            let names: Vec<String> = parts.iter().map(|&p| coll_short(hir, p)).collect();
            let sum = names
                .iter()
                .map(|n| format!("size({n})"))
                .collect::<Vec<_>>()
                .join(" + ");
            vec![format!(
                "size({}) = {}  (disjoint parts ⇒ exact; else ≤)",
                coll_short(hir, id),
                sum
            )]
        }
        Collection::Combination { parts, kind, .. } => {
            let names: Vec<String> = parts.iter().map(|&p| coll_short(hir, p)).collect();
            vec![format!("{kind:?} combination of {}", names.join(", "))]
        }
        // A sort is a permutation of its source: equal size, same element set.
        Collection::Sorted { source, .. } => vec![format!(
            "size({}) = size({})  (permutation of source)",
            coll_short(hir, id),
            coll_short(hir, *source)
        )],
        // A slice is a contiguous sub-range: bounded by its source's size.
        Collection::Slice { source, .. } => vec![format!(
            "size({}) ≤ size({})  (contiguous sub-range)",
            coll_short(hir, id),
            coll_short(hir, *source)
        )],
        Collection::CombProject { comb, axis } => vec![format!(
            "size({}) = size({})  ({axis:?} axis)",
            coll_short(hir, id),
            coll_short(hir, *comb)
        )],
        Collection::Base(_) => Vec::new(),
    }
}
