//! HIR → [`Formula`] region encoder (SPEC_ANALYSIS §1).
//!
//! Each region compiles to an **exact** formula over per-event
//! quantities. Implemented rows of the §1 table:
//!
//! | HIR construct | Formula |
//! |---|---|
//! | `select c` | `encode(c)` |
//! | `reject c` | `¬encode(c)` (NNF; exact) |
//! | region inheritance | inline the referenced region (cycle ⇒ `Unknown`) |
//! | `trigger t` | atom `trig(t) = 1` |
//! | `bin …` | not part of membership (skipped here) |
//! | linear-arith comparison | [`LinAtom`] (sums/diffs/const-mults; defines were HIR-inlined by sema; Int sizes coerced) |
//! | ratio `L/D ⋈ c`, `D` non-const | `(D>0 ∧ L ⋈ cD) ∨ (D<0 ∧ L ⋈̄ cD)`; `D=0` fails the cut |
//! | ternary `g ? a : b` | `(g∧a) ∨ (¬g∧b)` (missing `b` ⇒ true) |
//! | `[]` / `][` bands | conjunction / disjunction of bounds |
//! | unindexed collection cut | `Dual` bounded expansion `k = 3` with the **empty-collection case in plus** (PHASE0 OPEN-1; audit Bug 1) |
//! | anything `Unsupported` | `Unknown(diag)` |
//!
//! Per SPEC_LANGUAGE §4.4, *constant* division by zero / non-finite
//! constant arithmetic makes the enclosing comparison **false** (the
//! event fails the cut); non-finite numeric *literals* cannot construct
//! atoms and become `Unknown` instead.

use crate::formula::{DiagTable, Formula};
use crate::lin::{LinAtom, Rel};
use adl_sema::{
    ArithOp, CollectionId, ElemIndex, Fragment, HKind, HNode, Hir, HirRegion, HirRegionStmt,
    Quantity, QuantityId, QuantityTable, ScalarSource,
};
use adl_syntax::ast::{BandKind, CmpOp};
use adl_syntax::span::Span;
use std::collections::{BTreeMap, BTreeSet};

/// OPEN-1 bounded-expansion depth (PHASE0_RESOLUTIONS: `k = 3`).
pub const OPEN1_BOUND: u32 = 3;

/// One region's exact formula plus the diagnostics its `Unknown`/`Dual`
/// leaves point at (region-local [`DiagTable`]).
#[derive(Debug, Clone, PartialEq)]
pub struct EncodedRegion {
    /// Index into `Hir::regions`.
    pub region: usize,
    /// Region display name.
    pub name: String,
    pub formula: Formula,
    pub diags: DiagTable,
}

impl EncodedRegion {
    /// No `Unknown`/`Dual` anywhere: over- and under-projection coincide.
    #[must_use]
    pub fn is_exact(&self) -> bool {
        self.formula.is_exact()
    }
}

/// Encode one region of `hir`. Mutates only the quantity table (the
/// OPEN-1 expansion interns indexed element quantities).
#[must_use]
pub fn encode_region(hir: &mut Hir, region: usize) -> EncodedRegion {
    let Hir {
        table,
        regions,
        symbols,
        ..
    } = hir;
    let name = regions
        .get(region)
        .map(|r| symbols.display(r.name).to_owned())
        .unwrap_or_default();
    let span = regions.get(region).map_or_else(Span::default, |r| r.span);
    let mut enc = Encoder {
        table,
        regions,
        diags: DiagTable::default(),
        stack: Vec::new(),
    };
    let formula = enc.region(region, span);
    EncodedRegion {
        region,
        name,
        formula,
        diags: enc.diags,
    }
}

/// Encode every region of `hir`, in declaration order.
#[must_use]
pub fn encode_regions(hir: &mut Hir) -> Vec<EncodedRegion> {
    (0..hir.regions.len())
        .map(|i| encode_region(hir, i))
        .collect()
}

/// `n`-ary conjunction with constant folding: drops `true`, collapses on
/// `false`, flattens nested `And`s. Exact.
fn fand(parts: Vec<Formula>) -> Formula {
    let mut out = Vec::new();
    for p in parts {
        match p {
            Formula::True => {}
            Formula::False => return Formula::False,
            Formula::And(v) => out.extend(v),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => Formula::True,
        1 => out.into_iter().next().expect("len checked"),
        _ => Formula::And(out),
    }
}

/// `n`-ary disjunction with constant folding (dual of [`fand`]). Exact.
fn forr(parts: Vec<Formula>) -> Formula {
    let mut out = Vec::new();
    for p in parts {
        match p {
            Formula::False => {}
            Formula::True => return Formula::True,
            Formula::Or(v) => out.extend(v),
            other => out.push(other),
        }
    }
    match out.len() {
        0 => Formula::False,
        1 => out.into_iter().next().expect("len checked"),
        _ => Formula::Or(out),
    }
}

fn rel_of(op: CmpOp) -> Rel {
    match op {
        CmpOp::Gt => Rel::Gt,
        CmpOp::Lt => Rel::Lt,
        CmpOp::Ge => Rel::Ge,
        CmpOp::Le => Rel::Le,
        CmpOp::Eq => Rel::Eq,
        // `~=` is mapped to `!=` by sema (OPEN-4); defensive here.
        CmpOp::Ne | CmpOp::ApproxEq => Rel::Ne,
    }
}

fn hnode_children(node: &HNode) -> Vec<&HNode> {
    match &node.kind {
        HKind::Neg(a) | HKind::Not(a) | HKind::Abs(a) => vec![a],
        HKind::Binary { lhs, rhs, .. } | HKind::Cmp { lhs, rhs, .. } => vec![lhs, rhs],
        HKind::And(v) | HKind::Or(v) => v.iter().collect(),
        HKind::Band { expr, .. } => vec![expr],
        HKind::Ternary { guard, then, els } => {
            let mut v = vec![guard.as_ref(), then.as_ref()];
            if let Some(e) = els {
                v.push(e);
            }
            v
        }
        _ => Vec::new(),
    }
}

/// First `Unsupported` tag in the subtree, if any.
fn first_unsupported(node: &HNode) -> Option<(Span, &str)> {
    if let Fragment::Unsupported(reason) = &node.tag {
        return Some((node.span, reason));
    }
    hnode_children(node).into_iter().find_map(first_unsupported)
}

/// All distinct collections referenced by unindexed `CollProp` leaves.
fn collect_collprops(node: &HNode, out: &mut BTreeSet<CollectionId>) {
    if let HKind::CollProp { coll, .. } = &node.kind {
        out.insert(*coll);
    }
    for c in hnode_children(node) {
        collect_collprops(c, out);
    }
}

/// Element-existence requirements of one quantity: `coll[i].prop` and
/// element-anchored angular separations require `size(coll) > i`.
/// Opaque `ExternalFn` quantities contribute **nothing**: their
/// missing-element behaviour is unknown, and the SPEC_ANALYSIS §2 model
/// caveat already declares their values free.
fn quantity_existence(table: &QuantityTable, q: QuantityId, out: &mut BTreeMap<CollectionId, u32>) {
    let mut need = |coll: CollectionId, i: u32| {
        let e = out.entry(coll).or_insert(i);
        *e = (*e).max(i);
    };
    match table.quantity(q) {
        Quantity::ElemProp {
            coll,
            index: ElemIndex::FromFront(i),
            ..
        } => need(*coll, *i),
        Quantity::AngularSep { a, b, .. } => {
            for p in [a, b] {
                if let adl_sema::ParticleRef::Elem {
                    coll,
                    index: ElemIndex::FromFront(i),
                } = p
                {
                    need(*coll, *i);
                }
            }
        }
        _ => {}
    }
}

/// Every quantity referenced under `node` (the leaf's own subtree).
fn collect_quantities(node: &HNode, out: &mut BTreeSet<QuantityId>) {
    if let HKind::Quantity(q) = &node.kind {
        out.insert(*q);
    }
    for c in hnode_children(node) {
        collect_quantities(c, out);
    }
}

/// A linear expression `Σ cᵢ·qᵢ + k` under construction.
#[derive(Debug, Clone, Default)]
struct LinExpr {
    terms: BTreeMap<QuantityId, f64>,
    k: f64,
}

/// Why an expression has no [`LinExpr`] form.
#[derive(Debug, Clone)]
enum LinErr {
    /// Structurally outside linear arithmetic; comparison-level patterns
    /// (ratio, abs) may still apply, else `Unknown`.
    NonLinear(String),
    /// Constant arithmetic went non-finite (incl. division by a constant
    /// zero): the enclosing comparison is **false** (SPEC_LANGUAGE §4.4).
    NonFinite,
    /// A numeric literal itself parses non-finite: it cannot construct an
    /// atom (audit Bug 5), so the comparison is `Unknown`.
    BadLiteral,
}

impl LinExpr {
    fn constant(k: f64) -> Self {
        Self {
            terms: BTreeMap::new(),
            k,
        }
    }

    fn quantity(q: QuantityId) -> Self {
        Self {
            terms: BTreeMap::from([(q, 1.0)]),
            k: 0.0,
        }
    }

    fn all_finite(&self) -> bool {
        self.k.is_finite() && self.terms.values().all(|c| c.is_finite())
    }

    fn combine(&self, other: &Self, sign: f64) -> Option<Self> {
        let mut out = self.clone();
        out.k += sign * other.k;
        for (&q, &c) in &other.terms {
            *out.terms.entry(q).or_insert(0.0) += sign * c;
        }
        out.all_finite().then_some(out)
    }

    fn add(&self, other: &Self) -> Option<Self> {
        self.combine(other, 1.0)
    }

    fn sub(&self, other: &Self) -> Option<Self> {
        self.combine(other, -1.0)
    }

    fn scale(&self, c: f64) -> Option<Self> {
        let mut out = self.clone();
        out.k *= c;
        for v in out.terms.values_mut() {
            *v *= c;
        }
        out.all_finite().then_some(out)
    }
}

struct Encoder<'h> {
    table: &'h mut QuantityTable,
    regions: &'h [HirRegion],
    diags: DiagTable,
    /// Regions currently being inlined (inheritance cycle detection).
    stack: Vec<usize>,
}

impl Encoder<'_> {
    fn unknown(&mut self, span: Span, reason: impl Into<String>) -> Formula {
        Formula::Unknown(self.diags.push(span, reason))
    }

    /// Small-constant atom used by the OPEN-1 expansion and triggers;
    /// constants are tiny literals, so construction cannot fail.
    fn simple_atom(&mut self, q: QuantityId, rel: Rel, k: f64) -> Formula {
        LinAtom::single(q, rel, k).map_or(Formula::False, Formula::Atom)
    }

    // ---- regions --------------------------------------------------------

    fn region(&mut self, idx: usize, span: Span) -> Formula {
        let Some(region) = self.regions.get(idx) else {
            return self.unknown(span, "reference to an unknown region");
        };
        if self.stack.contains(&idx) {
            return self.unknown(span, "region inheritance cycle");
        }
        self.stack.push(idx);
        let mut parts = Vec::new();
        for stmt in &region.stmts {
            if let Some(f) = self.stmt(stmt) {
                parts.push(f);
            }
        }
        self.stack.pop();
        fand(parts)
    }

    fn stmt(&mut self, stmt: &HirRegionStmt) -> Option<Formula> {
        match stmt {
            HirRegionStmt::Select(n) | HirRegionStmt::Trigger(n) => Some(self.boolean(n)),
            // `reject c` is the exact negation of `c` (NNF).
            HirRegionStmt::Reject(n) => Some(self.boolean(n).not()),
            HirRegionStmt::Inherit { region, span } => Some(self.region(*region, *span)),
            // Bins partition the region's events; they do not constrain
            // membership (SPEC_ANALYSIS §1/§5).
            HirRegionStmt::Bin { .. } | HirRegionStmt::BinCond { .. } => None,
            HirRegionStmt::NonMembership { tag, span, .. } => match tag {
                Fragment::Unsupported(reason) => {
                    let reason = reason.clone();
                    Some(self.unknown(*span, reason))
                }
                Fragment::InFragment => None,
            },
        }
    }

    // ---- boolean structure ------------------------------------------------

    fn boolean(&mut self, node: &HNode) -> Formula {
        if let Fragment::Unsupported(reason) = &node.tag {
            let reason = reason.clone();
            return self.unknown(node.span, reason);
        }
        match &node.kind {
            HKind::Bool(true) => Formula::True,
            HKind::Bool(false) => Formula::False,
            HKind::And(v) => {
                let parts = v.iter().map(|n| self.boolean(n)).collect();
                fand(parts)
            }
            HKind::Or(v) => {
                let parts = v.iter().map(|n| self.boolean(n)).collect();
                forr(parts)
            }
            HKind::Not(inner) => self.boolean(inner).not(),
            HKind::Cmp { .. } | HKind::Band { .. } => self.leaf(node),
            // `g ? a : b` ≡ `(g∧a) ∨ (¬g∧b)`; missing/ALL branch is true
            // (SPEC_LANGUAGE §4.4).
            HKind::Ternary { guard, then, els } => {
                let g = self.boolean(guard);
                let t = self.boolean(then);
                let e = els
                    .as_deref()
                    .map_or(Formula::True, |els| self.boolean(els));
                forr(vec![fand(vec![g.clone(), t]), fand(vec![g.not(), e])])
            }
            // `trigger t` ⇒ atom `trig(t) = 1`.
            HKind::Quantity(q) => match self.table.quantity(*q) {
                Quantity::EventScalar(ScalarSource::Trigger(_)) => {
                    self.simple_atom(*q, Rel::Eq, 1.0)
                }
                _ => self.unknown(node.span, "numeric quantity used as a boolean condition"),
            },
            HKind::RegionPred(idx) => self.region(*idx, node.span),
            HKind::Num(_) => self.unknown(node.span, "numeric literal used as a boolean condition"),
            HKind::CollProp { .. } => self.unknown(
                node.span,
                "unindexed collection property used as a bare boolean",
            ),
            HKind::ElemSelfProp(_)
            | HKind::Particle(_)
            | HKind::CollValue(_)
            | HKind::Neg(_)
            | HKind::Binary { .. }
            | HKind::Abs(_)
            | HKind::Unsupported => {
                self.unknown(node.span, "expression is not a boolean condition")
            }
        }
    }

    // ---- comparison leaves (Cmp / Band) ------------------------------------

    fn leaf(&mut self, node: &HNode) -> Formula {
        if let Some((span, reason)) = first_unsupported(node) {
            let reason = reason.to_owned();
            return self.unknown(span, reason);
        }
        let mut colls = BTreeSet::new();
        collect_collprops(node, &mut colls);
        let mut iter = colls.into_iter();
        match (iter.next(), iter.next()) {
            (None, _) => {
                let inner = self.leaf_inner(node);
                self.guard_existence(node, inner)
            }
            (Some(coll), None) => self.dual_expand(coll, node),
            (Some(_), Some(_)) => self.unknown(
                node.span,
                "comparison references more than one unindexed collection (OPEN-1)",
            ),
        }
    }

    /// Make a comparison leaf **exact under the missing-element rule**
    /// (SPEC_LANGUAGE §4.4 extended: a comparison over a non-existent
    /// element is false): conjoin `size(C) > i` for every element-indexed
    /// quantity the leaf references. Without the guards, NNF negation
    /// (`reject`, `not`) would wrongly claim the comparison's complement
    /// holds on events where the element does not exist — the legacy
    /// "guarded references do not imply existence" lesson, this time on
    /// the negative polarity.
    ///
    /// Only applied to exact leaves: an `Unknown` leaf stays an honest
    /// refusal, and the `Dual` expansion carries its own size structure.
    fn guard_existence(&mut self, node: &HNode, inner: Formula) -> Formula {
        if !inner.is_exact() {
            return inner;
        }
        let mut qids = BTreeSet::new();
        collect_quantities(node, &mut qids);
        let mut needs: BTreeMap<CollectionId, u32> = BTreeMap::new();
        for q in qids {
            quantity_existence(self.table, q, &mut needs);
        }
        if needs.is_empty() {
            return inner;
        }
        let mut parts: Vec<Formula> = needs
            .into_iter()
            .map(|(coll, i)| {
                let sq = self.table.intern_quantity(Quantity::Size(coll));
                self.simple_atom(sq, Rel::Gt, f64::from(i))
            })
            .collect();
        parts.push(inner);
        fand(parts)
    }

    fn leaf_inner(&mut self, node: &HNode) -> Formula {
        match &node.kind {
            HKind::Cmp { op, lhs, rhs } => self.cmp(*op, lhs, rhs, node.span),
            HKind::Band { kind, expr, lo, hi } => self.band(*kind, expr, lo, hi, node.span),
            _ => self.unknown(node.span, "expression is not a comparison"),
        }
    }

    /// OPEN-1 (PHASE0): unindexed collection cut at region level. The
    /// ∀/∃ reading is unresolved, so encode a `Dual` bounded expansion
    /// with `k = 3`, where `P(i)` is the cut applied to element `i`:
    ///
    /// - `plus`  ⊇ both readings:
    ///   `size=0 ∨ P(0) ∨ P(1) ∨ P(2) ∨ size>3`
    ///   — the `size=0` disjunct is the **empty-collection case** the
    ///   legacy ∀-plus dropped (audit Bug 1: ∀ over an empty collection
    ///   is vacuously true, so the over-approximation must admit it);
    ///   `size>3` admits a witness beyond the expansion bound.
    /// - `minus` ⊆ both readings:
    ///   `1≤size≤3 ∧ ⋀ᵢ (size≤i ∨ P(i))`
    ///   — every present element satisfies the cut (⊆ ∀) and at least
    ///   one element exists (⊆ ∃).
    fn dual_expand(&mut self, coll: CollectionId, node: &HNode) -> Formula {
        let size_q = self.table.intern_quantity(Quantity::Size(coll));
        let instances: Vec<Formula> = (0..OPEN1_BOUND)
            .map(|i| {
                let inst = self.subst(node, coll, i);
                self.leaf(&inst)
            })
            .collect();

        let mut plus_parts = vec![self.simple_atom(size_q, Rel::Eq, 0.0)];
        plus_parts.extend(instances.iter().cloned());
        plus_parts.push(self.simple_atom(size_q, Rel::Gt, f64::from(OPEN1_BOUND)));
        let plus = forr(plus_parts);

        let mut minus_parts = vec![
            self.simple_atom(size_q, Rel::Ge, 1.0),
            self.simple_atom(size_q, Rel::Le, f64::from(OPEN1_BOUND)),
        ];
        for (i, p) in (0..OPEN1_BOUND).zip(instances) {
            let guard = self.simple_atom(size_q, Rel::Le, f64::from(i));
            minus_parts.push(forr(vec![guard, p]));
        }
        let minus = fand(minus_parts);

        let why = self.diags.push(
            node.span,
            format!(
                "unindexed collection cut: \u{2200}/\u{2203} reading unresolved (OPEN-1); \
                 Dual bounded expansion k={OPEN1_BOUND}"
            ),
        );
        Formula::Dual {
            plus: Box::new(plus),
            minus: Box::new(minus),
            why,
        }
    }

    /// Clone `node`, replacing every `CollProp` of `coll` with the
    /// interned indexed element property `coll[index].prop`.
    fn subst(&mut self, node: &HNode, coll: CollectionId, index: u32) -> HNode {
        let kind = match &node.kind {
            HKind::CollProp { coll: c, prop } if *c == coll => {
                let q = self.table.intern_quantity(Quantity::ElemProp {
                    coll,
                    index: ElemIndex::FromFront(index),
                    prop: *prop,
                });
                HKind::Quantity(q)
            }
            HKind::Neg(a) => HKind::Neg(Box::new(self.subst(a, coll, index))),
            HKind::Not(a) => HKind::Not(Box::new(self.subst(a, coll, index))),
            HKind::Abs(a) => HKind::Abs(Box::new(self.subst(a, coll, index))),
            HKind::Binary { op, lhs, rhs } => HKind::Binary {
                op: *op,
                lhs: Box::new(self.subst(lhs, coll, index)),
                rhs: Box::new(self.subst(rhs, coll, index)),
            },
            HKind::Cmp { op, lhs, rhs } => HKind::Cmp {
                op: *op,
                lhs: Box::new(self.subst(lhs, coll, index)),
                rhs: Box::new(self.subst(rhs, coll, index)),
            },
            HKind::And(v) => HKind::And(v.iter().map(|n| self.subst(n, coll, index)).collect()),
            HKind::Or(v) => HKind::Or(v.iter().map(|n| self.subst(n, coll, index)).collect()),
            HKind::Band { kind, expr, lo, hi } => HKind::Band {
                kind: *kind,
                expr: Box::new(self.subst(expr, coll, index)),
                lo: lo.clone(),
                hi: hi.clone(),
            },
            HKind::Ternary { guard, then, els } => HKind::Ternary {
                guard: Box::new(self.subst(guard, coll, index)),
                then: Box::new(self.subst(then, coll, index)),
                els: els.as_ref().map(|e| Box::new(self.subst(e, coll, index))),
            },
            other => other.clone(),
        };
        HNode {
            kind,
            span: node.span,
            tag: node.tag.clone(),
        }
    }

    // ---- comparisons --------------------------------------------------------

    fn cmp(&mut self, op: CmpOp, lhs: &HNode, rhs: &HNode, span: Span) -> Formula {
        let rel = rel_of(op);
        let l = self.lin(lhs);
        let r = self.lin(rhs);
        match (l, r) {
            (Ok(l), Ok(r)) => match l.sub(&r) {
                // l ⋈ r  ⇔  Σ terms ⋈ −k.
                Some(d) => self.atom_of(d.terms, rel, -d.k),
                None => Formula::False, // §4.4: non-finite constant arithmetic
            },
            (Err(LinErr::NonFinite), _) | (_, Err(LinErr::NonFinite)) => Formula::False,
            (Err(LinErr::BadLiteral), _) | (_, Err(LinErr::BadLiteral)) => {
                self.unknown(span, "non-finite numeric literal cannot construct an atom")
            }
            (Err(LinErr::NonLinear(why)), Ok(c)) if c.terms.is_empty() => {
                self.pattern(lhs, rel, c.k, &why, span)
            }
            (Ok(c), Err(LinErr::NonLinear(why))) if c.terms.is_empty() => {
                self.pattern(rhs, rel.flipped(), c.k, &why, span)
            }
            (Err(LinErr::NonLinear(why)), _) | (_, Err(LinErr::NonLinear(why))) => {
                self.unknown(span, format!("comparison is not linear arithmetic: {why}"))
            }
        }
    }

    /// Non-linear side vs constant `c`: exact ratio and absolute-value
    /// rewrites; anything else is `Unknown`.
    fn pattern(&mut self, side: &HNode, rel: Rel, c: f64, why: &str, span: Span) -> Formula {
        match &side.kind {
            HKind::Binary {
                op: ArithOp::Div,
                lhs: num,
                rhs: den,
            } => self.ratio(num, den, rel, c, span),
            HKind::Abs(inner) => self.abs_cmp(inner, rel, c, span),
            _ => self.unknown(span, format!("comparison is not linear arithmetic: {why}")),
        }
    }

    /// Exact two-branch ratio encoding (SPEC_ANALYSIS §1):
    /// `L/D ⋈ c` (D non-constant) ⇒ `(D>0 ∧ L ⋈ cD) ∨ (D<0 ∧ L ⋈̄ cD)`.
    /// `D = 0` fails the cut (neither branch admits it; §4.4).
    fn ratio(&mut self, num: &HNode, den: &HNode, rel: Rel, c: f64, span: Span) -> Formula {
        let l = match self.lin(num) {
            Ok(v) => v,
            Err(e) => return self.lin_err(e, "ratio numerator is not linear", span),
        };
        let d = match self.lin(den) {
            Ok(v) => v,
            Err(e) => return self.lin_err(e, "ratio denominator is not linear", span),
        };
        if d.terms.is_empty() {
            // A constant denominator is handled by plain linear folding;
            // reaching here means the numerator was rejected upstream.
            return self.unknown(span, "ratio numerator is not linear");
        }
        let Some(cd) = d.scale(c) else {
            return Formula::False; // §4.4
        };
        let Some(e) = l.sub(&cd) else {
            return Formula::False; // §4.4
        };
        let d_pos = self.atom_of(d.terms.clone(), Rel::Gt, -d.k);
        let e_pos = self.atom_of(e.terms.clone(), rel, -e.k);
        let d_neg = self.atom_of(d.terms, Rel::Lt, -d.k);
        let e_neg = self.atom_of(e.terms, rel.flipped(), -e.k);
        forr(vec![fand(vec![d_pos, e_pos]), fand(vec![d_neg, e_neg])])
    }

    /// Exact absolute-value expansion against a constant:
    /// `|E| < c ⇔ E < c ∧ E > −c`, `|E| > c ⇔ E > c ∨ E < −c`, etc.
    fn abs_cmp(&mut self, inner: &HNode, rel: Rel, c: f64, span: Span) -> Formula {
        let e = match self.lin(inner) {
            Ok(v) => v,
            Err(err) => {
                return self.lin_err(err, "absolute value of a non-linear expression", span);
            }
        };
        let hi = c - e.k;
        let lo = -c - e.k;
        let upper = |enc: &mut Self, r: Rel| enc.atom_of(e.terms.clone(), r, hi);
        let lower = |enc: &mut Self, r: Rel| enc.atom_of(e.terms.clone(), r, lo);
        match rel {
            Rel::Lt => {
                let parts = vec![upper(self, Rel::Lt), lower(self, Rel::Gt)];
                fand(parts)
            }
            Rel::Le => {
                let parts = vec![upper(self, Rel::Le), lower(self, Rel::Ge)];
                fand(parts)
            }
            Rel::Gt => {
                let parts = vec![upper(self, Rel::Gt), lower(self, Rel::Lt)];
                forr(parts)
            }
            Rel::Ge => {
                let parts = vec![upper(self, Rel::Ge), lower(self, Rel::Le)];
                forr(parts)
            }
            Rel::Eq => {
                let parts = vec![upper(self, Rel::Eq), lower(self, Rel::Eq)];
                forr(parts)
            }
            Rel::Ne => {
                let parts = vec![upper(self, Rel::Ne), lower(self, Rel::Ne)];
                fand(parts)
            }
        }
    }

    /// `x [] lo hi ⇔ lo ≤ x ∧ x ≤ hi`; `x ][ lo hi ⇔ x ≤ lo ∨ x ≥ hi`
    /// (SPEC_LANGUAGE §4.4).
    fn band(&mut self, kind: BandKind, expr: &HNode, lo: &str, hi: &str, span: Span) -> Formula {
        let (Some(lo), Some(hi)) = (parse_finite(lo), parse_finite(hi)) else {
            return self.unknown(span, "non-finite numeric literal cannot construct an atom");
        };
        let e = match self.lin(expr) {
            Ok(v) => v,
            Err(err) => return self.lin_err(err, "band expression is not linear", span),
        };
        let lo_bound = self.atom_of(
            e.terms.clone(),
            if kind == BandKind::In {
                Rel::Ge
            } else {
                Rel::Le
            },
            lo - e.k,
        );
        let hi_bound = self.atom_of(
            e.terms,
            if kind == BandKind::In {
                Rel::Le
            } else {
                Rel::Ge
            },
            hi - e.k,
        );
        match kind {
            BandKind::In => fand(vec![lo_bound, hi_bound]),
            BandKind::Out => forr(vec![lo_bound, hi_bound]),
        }
    }

    fn lin_err(&mut self, e: LinErr, what: &str, span: Span) -> Formula {
        match e {
            LinErr::NonFinite => Formula::False, // §4.4
            LinErr::BadLiteral => {
                self.unknown(span, "non-finite numeric literal cannot construct an atom")
            }
            LinErr::NonLinear(why) => self.unknown(span, format!("{what}: {why}")),
        }
    }

    /// Build `Σ terms ⋈ k` with constant folding and Int-size coercion.
    fn atom_of(&mut self, terms: BTreeMap<QuantityId, f64>, mut rel: Rel, mut k: f64) -> Formula {
        if terms.is_empty() {
            // Constant comparison: fold. Non-finite k comes from constant
            // arithmetic, so the comparison is false (§4.4).
            if !k.is_finite() {
                return Formula::False;
            }
            return if rel.eval(0.0, k) {
                Formula::True
            } else {
                Formula::False
            };
        }
        // Int-size coercion: a sum of integer multiples of collection
        // sizes is an integer, so fractional bounds tighten exactly.
        let int_valued = terms
            .iter()
            .all(|(q, c)| matches!(self.table.quantity(*q), Quantity::Size(_)) && c.fract() == 0.0);
        if int_valued && k.is_finite() && k.fract() != 0.0 {
            match rel {
                Rel::Lt | Rel::Le => {
                    rel = Rel::Le;
                    k = k.floor();
                }
                Rel::Gt | Rel::Ge => {
                    rel = Rel::Ge;
                    k = k.ceil();
                }
                Rel::Eq => return Formula::False, // integer ≠ fractional constant
                Rel::Ne => return Formula::True,
            }
        }
        match LinAtom::new(terms.into_iter().map(|(q, c)| (c, q)), rel, k) {
            Ok(a) => Formula::Atom(a),
            // Non-finite coefficients/constants here arose from constant
            // arithmetic (literals were screened): comparison false (§4.4).
            Err(_) => Formula::False,
        }
    }

    // ---- linear extraction ----------------------------------------------

    fn lin(&mut self, node: &HNode) -> Result<LinExpr, LinErr> {
        if let Fragment::Unsupported(reason) = &node.tag {
            return Err(LinErr::NonLinear(reason.clone()));
        }
        match &node.kind {
            HKind::Num(s) => match parse_finite(s) {
                Some(v) => Ok(LinExpr::constant(v)),
                None => Err(LinErr::BadLiteral),
            },
            HKind::Quantity(q) => Ok(LinExpr::quantity(*q)),
            HKind::Neg(a) => self.lin(a)?.scale(-1.0).ok_or(LinErr::NonFinite),
            HKind::Abs(_) => Err(LinErr::NonLinear(
                "absolute value (only `|E| ⋈ const` is expanded)".to_owned(),
            )),
            HKind::Binary { op, lhs, rhs } => self.lin_binary(*op, lhs, rhs),
            HKind::CollProp { .. } => Err(LinErr::NonLinear(
                "unindexed collection property".to_owned(),
            )),
            HKind::ElemSelfProp(_) => Err(LinErr::NonLinear(
                "implicit-element property outside an object block".to_owned(),
            )),
            HKind::Bool(_)
            | HKind::Cmp { .. }
            | HKind::And(_)
            | HKind::Or(_)
            | HKind::Not(_)
            | HKind::Band { .. }
            | HKind::Ternary { .. }
            | HKind::RegionPred(_) => Err(LinErr::NonLinear(
                "boolean value used in arithmetic".to_owned(),
            )),
            HKind::Particle(_) | HKind::CollValue(_) | HKind::Unsupported => Err(
                LinErr::NonLinear("unsupported value in arithmetic".to_owned()),
            ),
        }
    }

    fn lin_binary(&mut self, op: ArithOp, lhs: &HNode, rhs: &HNode) -> Result<LinExpr, LinErr> {
        match op {
            ArithOp::Add => {
                let l = self.lin(lhs)?;
                let r = self.lin(rhs)?;
                l.add(&r).ok_or(LinErr::NonFinite)
            }
            ArithOp::Sub => {
                let l = self.lin(lhs)?;
                let r = self.lin(rhs)?;
                l.sub(&r).ok_or(LinErr::NonFinite)
            }
            ArithOp::Mul => {
                let l = self.lin(lhs)?;
                let r = self.lin(rhs)?;
                if l.terms.is_empty() {
                    r.scale(l.k).ok_or(LinErr::NonFinite)
                } else if r.terms.is_empty() {
                    l.scale(r.k).ok_or(LinErr::NonFinite)
                } else {
                    Err(LinErr::NonLinear(
                        "product of two event quantities".to_owned(),
                    ))
                }
            }
            ArithOp::Div => {
                let r = self.lin(rhs)?;
                if !r.terms.is_empty() {
                    // Non-constant denominator: the exact two-branch ratio
                    // encoding applies at the comparison level.
                    return Err(LinErr::NonLinear(
                        "ratio with a non-constant denominator".to_owned(),
                    ));
                }
                if r.k == 0.0 {
                    return Err(LinErr::NonFinite); // §4.4: division by zero
                }
                let l = self.lin(lhs)?;
                l.scale(1.0 / r.k).ok_or(LinErr::NonFinite)
            }
            ArithOp::Pow => {
                let l = self.lin(lhs)?;
                let r = self.lin(rhs)?;
                if l.terms.is_empty() && r.terms.is_empty() {
                    let v = l.k.powf(r.k);
                    if v.is_finite() {
                        Ok(LinExpr::constant(v))
                    } else {
                        Err(LinErr::NonFinite)
                    }
                } else {
                    Err(LinErr::NonLinear("non-constant power".to_owned()))
                }
            }
        }
    }
}

/// Parse a canonical numeral; `None` if it does not parse finite.
fn parse_finite(s: &str) -> Option<f64> {
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}
