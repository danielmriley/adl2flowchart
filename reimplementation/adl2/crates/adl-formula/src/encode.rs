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

use crate::formula::{DiagId, DiagTable, Formula};
use crate::lin::{LinAtom, Rel};
use adl_sema::{
    ArithOp, CombKind, Collection, CollectionId, CompositeBinder, ElemIndex, ElemPred, ElemPredId,
    Fragment, HKind, HNode, Hir, HirRegion, HirRegionStmt, ParticleRef, Quantity, QuantityArg,
    QuantityId, QuantityTable, Rat, ReduceKind, ScalarSource, SymbolTable,
};
use adl_syntax::ast::{BandKind, CmpOp};
use adl_syntax::span::Span;
use std::collections::{BTreeMap, BTreeSet};

/// OPEN-1 bounded-expansion depth (PHASE0_RESOLUTIONS: `k = 3`).
pub const OPEN1_BOUND: u32 = 3;

/// Per-binder index bound for the 2D composite-existence expansion (P3). A
/// `k`-binder combination expands `COMB2D_BOUND^k` index tuples within the
/// bound (e.g. `2` ⇒ one disjoint pair `(0,1)` / four cartesian pairs), with
/// a size escape disjunct beyond it so no real surviving tuple is excluded.
/// Deliberately smaller than [`OPEN1_BOUND`] to cap the `k²` blowup the plan
/// warns about; the corpus composites are all 2-binder, so `2` already
/// covers the dominant `size == 1` / `size >= 1` pattern.
pub const COMB2D_BOUND: u32 = 2;

/// Which quantifier the bounded expansion encodes. `Open1` is the
/// region-level unindexed-collection cut whose ∀/∃ reading is *unresolved*
/// (the over-approx unions both readings, the under-approx intersects
/// them). `Any`/`All` are the reducer keywords, whose quantifier is
/// **resolved**, giving a strictly tighter (never looser) Dual.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DualKind {
    /// Unresolved ∀/∃ (OPEN-1, the legacy unindexed-collection cut).
    Open1,
    /// `any(P)` ≡ ∃ element with `P`.
    Any,
    /// `all(P)` ≡ ∀ present elements have `P`.
    All,
}

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
        elem_preds,
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
        symbols,
        elem_preds,
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
        // Scalar min/max args ARE formula-visible quantities (unlike a
        // reducer body), so the existence/axiom collectors must see them.
        HKind::ScalarMinMax { args, .. } => args.iter().collect(),
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
    // The guard emitted per collection is `size > out[coll]`. A front element
    // `[i]` exists iff `size > i`; a back element `[-k]` iff `size >= k`, i.e.
    // `size > k - 1`. Both reduce to a `size > N` threshold, and the per-coll
    // max correctly conjoins them. `[-0]` is degenerate (never exists) and
    // imposes no guard — sound, the interpreter fails the cut anyway.
    let floor_need = |index: &ElemIndex| -> Option<u32> {
        match index {
            ElemIndex::FromFront(i) => Some(*i),
            ElemIndex::FromBack(k) => k.checked_sub(1),
        }
    };
    match table.quantity(q) {
        Quantity::ElemProp { coll, index, .. } => {
            if let Some(n) = floor_need(index) {
                need(*coll, n);
            }
        }
        Quantity::AngularSep { a, b, .. } => {
            for p in [a, b] {
                if let adl_sema::ParticleRef::Elem { coll, index } = p
                    && let Some(n) = floor_need(index)
                {
                    need(*coll, n);
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

/// A canonical, injective structural key for a value-position numeric
/// reducer body (`sum`/`min`/`max`), rendered over **already-interned** ids
/// (`QuantityId`, `CollectionId`, `PropId`) so identical text always means
/// identical resolution. `None` for any node shape that cannot be rendered
/// injectively — the caller then falls back to a fresh `Unknown`/non-linear
/// leaf (sound: only ever weakens to POSSIBLY, never a false PROVEN).
///
/// Only the shapes a numeric reducer body can actually take are rendered:
/// the iteration element's property (`ReduceProp`), interned per-event
/// quantities, region-level collection properties, numeric literals, and
/// the closed-under-arithmetic operators (`+ - * / ^`, neg, abs) plus
/// nested reducers. Everything else (booleans, comparisons, particles,
/// composite binders, the implicit-element `ElemSelfProp`) bails to `None`.
fn reduce_body_key(node: &HNode) -> Option<String> {
    if !node.tag.is_in_fragment() {
        return None;
    }
    Some(match &node.kind {
        HKind::Num(s) => format!("#{s}"),
        HKind::Quantity(q) => format!("Q{}", q.0),
        HKind::ReduceProp(p) => format!("RP{}", p.0),
        HKind::CollProp { coll, prop } => format!("CP{}.{}", coll.0, prop.0),
        HKind::Neg(a) => format!("(neg {})", reduce_body_key(a)?),
        HKind::Abs(a) => format!("(abs {})", reduce_body_key(a)?),
        HKind::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            op.as_str(),
            reduce_body_key(lhs)?,
            reduce_body_key(rhs)?
        ),
        HKind::Reduce {
            kind,
            coll,
            body,
            slice,
        } => format!(
            "{}[C{}{} {}]",
            kind.as_str(),
            coll.0,
            slice_key(*slice),
            reduce_body_key(body)?
        ),
        HKind::ScalarMinMax { kind, args } => {
            let mut s = format!("{}<", kind.as_str());
            for a in args {
                s.push_str(&reduce_body_key(a)?);
                s.push(',');
            }
            s.push('>');
            s
        }
        _ => return None,
    })
}

/// Number of `Add`/`Sub` nodes anywhere in an arithmetic source tree
/// (`Mul`/`Div`/`Pow` are not counted — only additive associativity and
/// cancellation are the f64-faithfulness hazard).
fn count_add_sub(node: &HNode) -> u32 {
    match &node.kind {
        HKind::Binary {
            op: ArithOp::Add | ArithOp::Sub,
            lhs,
            rhs,
        } => 1 + count_add_sub(lhs) + count_add_sub(rhs),
        HKind::Binary { lhs, rhs, .. } => count_add_sub(lhs) + count_add_sub(rhs),
        HKind::Neg(a) | HKind::Abs(a) => count_add_sub(a),
        _ => 0,
    }
}

/// Every numeric literal that is an operand of an `Add`/`Sub` must be dyadic
/// (`fl(c) == c`), else folding it across the comparison is not f64-faithful:
/// `MET + 0.1 > 0.3` folds to the exact atom `MET > 0.2`, but the interpreter
/// computes `fl(MET + 0.1) > 0.3`, which diverges at the boundary.
fn additive_consts_dyadic(node: &HNode) -> bool {
    match &node.kind {
        HKind::Binary {
            op: ArithOp::Add | ArithOp::Sub,
            lhs,
            rhs,
        } => {
            for side in [lhs.as_ref(), rhs.as_ref()] {
                if let HKind::Num(s) = &side.kind {
                    match parse_rat(s) {
                        Some(c) if c.is_dyadic() => {}
                        _ => return false,
                    }
                }
            }
            additive_consts_dyadic(lhs) && additive_consts_dyadic(rhs)
        }
        HKind::Binary { lhs, rhs, .. } => {
            additive_consts_dyadic(lhs) && additive_consts_dyadic(rhs)
        }
        HKind::Neg(a) | HKind::Abs(a) => additive_consts_dyadic(a),
        _ => true,
    }
}

/// Whether a comparison-operand source may be flattened to a shared exact
/// linear atom (`Σcᵢqᵢ`) without the analyzer's exact value diverging from the
/// interpreter's stepwise-f64 evaluation. Sound f64-faithfulness guard: at
/// most one additive op AND no non-dyadic additive constant. A non-faithful
/// source is routed to `intern_opaque_scalar` (structure-keyed) so two regions
/// whose sources f64-evaluate differently never unify into a false PROVEN
/// DISJOINT/SUBSET/EMPTY (the UNSAT side has no witness oracle).
fn is_f64_faithful(node: &HNode) -> bool {
    count_add_sub(node) <= 1 && additive_consts_dyadic(node)
}

/// Canonical text of a reducer's static slice (`None` ⇒ empty).
fn slice_key(slice: Option<(u32, Option<u32>)>) -> String {
    match slice {
        Some((a, Some(b))) => format!("[{a}:{b}]"),
        Some((a, None)) => format!("[{a}:]"),
        None => String::new(),
    }
}

/// A linear expression `Σ cᵢ·qᵢ + k` under construction, over exact
/// rationals — folding is exact, so the atom boundary matches the
/// interpreter's evaluation of the same cut to the bit.
#[derive(Debug, Clone, Default)]
struct LinExpr {
    terms: BTreeMap<QuantityId, Rat>,
    k: Rat,
}

/// Why an expression has no [`LinExpr`] form.
#[derive(Debug, Clone)]
enum LinErr {
    /// Structurally outside linear arithmetic; comparison-level patterns
    /// (ratio, abs) may still apply, else `Unknown`.
    NonLinear(String),
    /// Division by a constant zero: the enclosing comparison is **false**
    /// (SPEC_LANGUAGE §4.4).
    NonFinite,
    /// A numeric literal itself parses non-finite: it cannot construct an
    /// atom (audit Bug 5), so the comparison is `Unknown`.
    BadLiteral,
}

impl LinExpr {
    fn constant(k: Rat) -> Self {
        Self {
            terms: BTreeMap::new(),
            k,
        }
    }

    fn quantity(q: QuantityId) -> Self {
        Self {
            terms: BTreeMap::from([(q, Rat::one())]),
            k: Rat::zero(),
        }
    }

    /// `self + sign·other`, exact (rationals never overflow).
    fn combine(&self, other: &Self, negate: bool) -> Self {
        let mut out = self.clone();
        let adj = |c: &Rat| if negate { -c } else { c.clone() };
        out.k = &out.k + &adj(&other.k);
        for (q, c) in &other.terms {
            let entry = out.terms.entry(*q).or_insert_with(Rat::zero);
            *entry = &*entry + &adj(c);
        }
        out
    }

    fn sub(&self, other: &Self) -> Self {
        self.combine(other, true)
    }

    fn scale(&self, c: &Rat) -> Self {
        let mut out = self.clone();
        out.k = &out.k * c;
        for v in out.terms.values_mut() {
            *v = &*v * c;
        }
        out
    }
}

struct Encoder<'h> {
    table: &'h mut QuantityTable,
    regions: &'h [HirRegion],
    /// Symbol interner, needed to synthesize the opaque-reducer function name
    /// (a dotted key — `reduce.sum` — that no source identifier can collide
    /// with, since identifiers never contain a `.`).
    symbols: &'h mut SymbolTable,
    /// Interned per-tuple cuts of composite blocks (indexed by `ElemPredId`),
    /// read by the 2D composite-existence dual-expansion.
    elem_preds: &'h [ElemPred],
    diags: DiagTable,
    /// Regions currently being inlined (inheritance cycle detection).
    stack: Vec<usize>,
}

impl Encoder<'_> {
    fn unknown(&mut self, span: Span, reason: impl Into<String>) -> Formula {
        Formula::Unknown(self.diags.push(span, reason))
    }

    /// Small-constant atom used by the OPEN-1 expansion and triggers;
    /// constants are tiny integers.
    fn simple_atom(&mut self, q: QuantityId, rel: Rel, k: i64) -> Formula {
        Formula::Atom(LinAtom::single(q, rel, Rat::from_i64(k)))
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
                    self.simple_atom(*q, Rel::Eq, 1)
                }
                _ => self.unknown(node.span, "numeric quantity used as a boolean condition"),
            },
            HKind::RegionPred(idx) => self.region(*idx, node.span),
            HKind::Num(_) => self.unknown(node.span, "numeric literal used as a boolean condition"),
            HKind::CollProp { .. } => self.unknown(
                node.span,
                "unindexed collection property used as a bare boolean",
            ),
            // Boolean reducers (`any`/`all`) get a per-kind Dual bounded
            // expansion (P2); numeric reducers (`sum`/`min`/`max`) used as a
            // bare boolean are not conditions. (`min`/`max ⋈ c` is desugared
            // to `any`/`all` at resolve time, so a surviving numeric Reduce
            // here is genuinely value-position.)
            HKind::Reduce {
                kind, coll, body, ..
            } if kind.is_boolean() => self.encode_reduce(*kind, *coll, body, node.span),
            HKind::Reduce { kind, .. } => self.unknown(
                node.span,
                format!("`{}` reducer is not a boolean condition", kind.as_str()),
            ),
            HKind::ElemSelfProp(_)
            | HKind::ReduceProp(_)
            | HKind::Particle(_)
            | HKind::CollValue(_)
            | HKind::Neg(_)
            | HKind::Binary { .. }
            | HKind::Abs(_)
            | HKind::ScalarMinMax { .. }
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
            (Some(coll), None) => {
                let size_q = self.table.intern_quantity(Quantity::Size(coll));
                let instances: Vec<Formula> = (0..OPEN1_BOUND)
                    .map(|i| {
                        let inst = self.subst(node, coll, i);
                        self.leaf(&inst)
                    })
                    .collect();
                let why = self.diags.push(
                    node.span,
                    format!(
                        "unindexed collection cut: \u{2200}/\u{2203} reading unresolved (OPEN-1); \
                         Dual bounded expansion k={OPEN1_BOUND}"
                    ),
                );
                self.build_dual(DualKind::Open1, size_q, instances, why)
            }
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
                self.simple_atom(sq, Rel::Gt, i64::from(i))
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

    /// Build the per-kind `Dual` bounded expansion from the already-encoded
    /// per-element instances `P(i)` (`i = 0..k`), the collection's `size_q`,
    /// and a diagnostic `why`. `instances[i]` is `P(i)`.
    ///
    /// The three quantifier readings (SPEC_ANALYSIS §1, audit Bug 1):
    ///
    /// - **`Open1`** (∀/∃ unresolved): `plus` unions both readings'
    ///   over-approximations (`size=0 ∨ ⋁ᵢ P(i) ∨ size>k`); `minus`
    ///   intersects both readings' under-approximations
    ///   (`1≤size≤k ∧ ⋀ᵢ(size≤i ∨ P(i))`).
    /// - **`Any`** (∃): `plus = ⋁ᵢ P(i) ∨ size>k` (a witness exists at
    ///   some present index, or beyond the bound); `minus = ⋁ᵢ(size>i ∧
    ///   P(i))` (a present element provably satisfies `P`). Empty ⇒ false,
    ///   correctly NOT in `plus`.
    /// - **`All`** (∀): `plus = ⋀ᵢ(size≤i ∨ P(i))` (every present element
    ///   within the bound satisfies `P`; admits `size>k`); `minus = size=0
    ///   ∨ (1≤size≤k ∧ ⋀ᵢ(size≤i ∨ P(i)))` — the `size=0` disjunct is the
    ///   **vacuous-true** empty case, which belongs to `All`, never `Any`.
    fn build_dual(
        &mut self,
        kind: DualKind,
        size_q: QuantityId,
        instances: Vec<Formula>,
        why: DiagId,
    ) -> Formula {
        let k = i64::from(OPEN1_BOUND);
        // `⋀ᵢ (size ≤ i ∨ P(i))` — "every present element up to the bound
        // satisfies P". Shared by Open1-minus, All-plus and All-minus.
        let all_within_bound = |enc: &mut Self, insts: &[Formula]| {
            let mut parts = Vec::new();
            for (i, p) in insts.iter().enumerate() {
                let guard = enc.simple_atom(size_q, Rel::Le, i as i64);
                parts.push(forr(vec![guard, p.clone()]));
            }
            fand(parts)
        };
        let (plus, minus) = match kind {
            DualKind::Open1 => {
                let mut plus_parts = vec![self.simple_atom(size_q, Rel::Eq, 0)];
                plus_parts.extend(instances.iter().cloned());
                plus_parts.push(self.simple_atom(size_q, Rel::Gt, k));
                let plus = forr(plus_parts);

                let mut minus_parts = vec![
                    self.simple_atom(size_q, Rel::Ge, 1),
                    self.simple_atom(size_q, Rel::Le, k),
                ];
                minus_parts.push(all_within_bound(self, &instances));
                (plus, fand(minus_parts))
            }
            DualKind::Any => {
                let mut plus_parts: Vec<Formula> = instances.clone();
                plus_parts.push(self.simple_atom(size_q, Rel::Gt, k));
                let plus = forr(plus_parts);

                let mut minus_parts = Vec::new();
                for (i, p) in instances.iter().enumerate() {
                    let guard = self.simple_atom(size_q, Rel::Gt, i as i64);
                    minus_parts.push(fand(vec![guard, p.clone()]));
                }
                (plus, forr(minus_parts))
            }
            DualKind::All => {
                let plus = all_within_bound(self, &instances);
                let empty = self.simple_atom(size_q, Rel::Eq, 0);
                let lo = self.simple_atom(size_q, Rel::Ge, 1);
                let hi = self.simple_atom(size_q, Rel::Le, k);
                let bounded = fand(vec![lo, hi, all_within_bound(self, &instances)]);
                (plus, forr(vec![empty, bounded]))
            }
        };
        Formula::Dual {
            plus: Box::new(plus),
            minus: Box::new(minus),
            why,
        }
    }

    /// Encode a boolean reducer `any(P)` / `all(P)` (P2). The iteration
    /// collection's element `i` is substituted into the body to get `P(i)`,
    /// each `P(i)` is encoded as a boolean formula, and the per-kind `Dual`
    /// bounded expansion folds them. When the iteration collection is a
    /// concrete-bound static slice, the indices rebase onto the source (so
    /// `min(jets[:4]…)` reasons at source indices `0..4`, EPRED/ORD coming
    /// for free) and the encoding is exact (no Dual).
    fn encode_reduce(
        &mut self,
        kind: ReduceKind,
        coll: CollectionId,
        body: &HNode,
        span: Span,
    ) -> Formula {
        let dual_kind = match kind {
            ReduceKind::Any => DualKind::Any,
            ReduceKind::All => DualKind::All,
            // Numeric reducers never reach here (boolean-only callers).
            _ => {
                return self.unknown(span, "numeric reducer used as a boolean condition");
            }
        };

        // Static slice with a concrete upper bound (`jets[:4]`): the element
        // count is fixed at `end - start` (clamped by the source), so emit an
        // EXACT conjunction over exactly those indices, no Dual. Reducer
        // indices rebase onto the source (`slice[j] ≡ src[start+j]`),
        // inheriting ORD/IDOM/EPRED. An open-ended slice (`jets[2:]`) has no
        // static count — it falls through to the bounded Dual over the slice
        // id (whose SZSLICE bounds its size).
        if let Collection::Slice {
            source,
            start,
            end: Some(end),
        } = *self.table.collection(coll)
        {
            let n = end.saturating_sub(start);
            return self.encode_static_slice_reduce(dual_kind, source, start, n, body);
        }

        let size_q = self.table.intern_quantity(Quantity::Size(coll));
        let instances: Vec<Formula> = (0..OPEN1_BOUND)
            .map(|i| {
                let inst = self.subst_reduce(body, coll, i);
                self.boolean(&inst)
            })
            .collect();
        let why = self.diags.push(
            span,
            format!(
                "`{}` reducer: bounded expansion k={OPEN1_BOUND}",
                kind.as_str()
            ),
        );
        self.build_dual(dual_kind, size_q, instances, why)
    }

    /// Static-slice reducer over `src[start .. start+n]`: the slice has
    /// **exactly** `n` elements once the source is long enough, so emit an
    /// exact (no-Dual) conjunction/disjunction guarded by `size(src) > i`.
    /// Each present index `start+j` is encoded directly against the source —
    /// ORD/IDOM/EPRED ride the source's pT order if it has one.
    ///
    /// `all`: `⋀_{j<n} (size(src) ≤ start+j ∨ P(start+j))` (vacuous on a
    /// short source — sound, matches the interpreter's clamp+empty rule).
    /// `any`: `⋁_{j<n} (size(src) > start+j ∧ P(start+j))`.
    fn encode_static_slice_reduce(
        &mut self,
        kind: DualKind,
        source: CollectionId,
        start: u32,
        n: u32,
        body: &HNode,
    ) -> Formula {
        let size_q = self.table.intern_quantity(Quantity::Size(source));
        let mut parts = Vec::new();
        for j in 0..n {
            let abs = start.saturating_add(j);
            let inst = self.subst_reduce(body, source, abs);
            let p = self.boolean(&inst);
            let idx = i64::from(abs);
            match kind {
                DualKind::All | DualKind::Open1 => {
                    let guard = self.simple_atom(size_q, Rel::Le, idx);
                    parts.push(forr(vec![guard, p]));
                }
                DualKind::Any => {
                    let guard = self.simple_atom(size_q, Rel::Gt, idx);
                    parts.push(fand(vec![guard, p]));
                }
            }
        }
        match kind {
            DualKind::All | DualKind::Open1 => fand(parts),
            DualKind::Any => forr(parts),
        }
    }

    /// Clone a reducer body, replacing every reference to the iteration
    /// element with the interned `coll[index]` element. The iteration
    /// element appears as `HKind::ReduceProp(prop)` (`pt(X)`) and as
    /// `ParticleRef::ReduceElem` inside angular/external quantities
    /// (`dR(this, X)`). A body part that still references an opaque element
    /// (`ThisElem`, a `Sum`, …) stays an opaque quantity, so the leaf's
    /// encoder falls to `Unknown` — sound.
    fn subst_reduce(&mut self, node: &HNode, coll: CollectionId, index: u32) -> HNode {
        let kind = match &node.kind {
            HKind::ReduceProp(prop) => {
                let q = self.table.intern_quantity(Quantity::ElemProp {
                    coll,
                    index: ElemIndex::FromFront(index),
                    prop: *prop,
                });
                HKind::Quantity(q)
            }
            HKind::Quantity(q) => HKind::Quantity(self.subst_reduce_quantity(*q, coll, index)),
            HKind::Neg(a) => HKind::Neg(Box::new(self.subst_reduce(a, coll, index))),
            HKind::Not(a) => HKind::Not(Box::new(self.subst_reduce(a, coll, index))),
            HKind::Abs(a) => HKind::Abs(Box::new(self.subst_reduce(a, coll, index))),
            HKind::Binary { op, lhs, rhs } => HKind::Binary {
                op: *op,
                lhs: Box::new(self.subst_reduce(lhs, coll, index)),
                rhs: Box::new(self.subst_reduce(rhs, coll, index)),
            },
            HKind::Cmp { op, lhs, rhs } => HKind::Cmp {
                op: *op,
                lhs: Box::new(self.subst_reduce(lhs, coll, index)),
                rhs: Box::new(self.subst_reduce(rhs, coll, index)),
            },
            HKind::And(v) => HKind::And(v.iter().map(|n| self.subst_reduce(n, coll, index)).collect()),
            HKind::Or(v) => HKind::Or(v.iter().map(|n| self.subst_reduce(n, coll, index)).collect()),
            HKind::Band { kind, expr, lo, hi } => HKind::Band {
                kind: *kind,
                expr: Box::new(self.subst_reduce(expr, coll, index)),
                lo: lo.clone(),
                hi: hi.clone(),
            },
            HKind::Ternary { guard, then, els } => HKind::Ternary {
                guard: Box::new(self.subst_reduce(guard, coll, index)),
                then: Box::new(self.subst_reduce(then, coll, index)),
                els: els.as_ref().map(|e| Box::new(self.subst_reduce(e, coll, index))),
            },
            other => other.clone(),
        };
        HNode {
            kind,
            span: node.span,
            tag: node.tag.clone(),
        }
    }

    /// Rewrite an interned quantity, replacing `ParticleRef::ReduceElem`
    /// with `coll[index]`. Re-interns the rewritten quantity; if it has no
    /// `ReduceElem` it returns the same id. Quantities that still carry an
    /// opaque `ReduceElem` (e.g. nested in a `Sum`) are left untouched —
    /// they remain free, and the leaf encoder keeps them opaque.
    fn subst_reduce_quantity(
        &mut self,
        q: QuantityId,
        coll: CollectionId,
        index: u32,
    ) -> QuantityId {
        let elem = ParticleRef::Elem {
            coll,
            index: ElemIndex::FromFront(index),
        };
        let subst_p = |p: &ParticleRef| -> ParticleRef {
            if matches!(p, ParticleRef::ReduceElem) {
                elem.clone()
            } else {
                p.clone()
            }
        };
        match self.table.quantity(q).clone() {
            Quantity::AngularSep { kind, a, b, .. } => {
                let (na, nb) = (subst_p(&a), subst_p(&b));
                if na == a && nb == b {
                    return q;
                }
                self.table.intern_angular(kind, na, nb)
            }
            Quantity::ExternalFn { name, args } => {
                let mut changed = false;
                let new_args: Vec<QuantityArg> = args
                    .iter()
                    .map(|arg| match arg {
                        QuantityArg::Particle(ParticleRef::ReduceElem) => {
                            changed = true;
                            QuantityArg::Particle(elem.clone())
                        }
                        other => other.clone(),
                    })
                    .collect();
                if !changed {
                    return q;
                }
                self.table.intern_quantity(Quantity::ExternalFn {
                    name,
                    args: new_args,
                })
            }
            _ => q,
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

    // ---- composite per-candidate cut existence (2D dual, P3) --------------

    /// Refine a positive lower bound on a composite tuple count with the
    /// per-tuple cut structure (plan P3 / Tier 2). The exact membership of
    /// `size(K) >= 1` is `∃ surviving tuple`, i.e. `∃ binder-index tuple t :
    /// P(t) ∧ (disjoint value-distinctness)`. We refine only the **OVER**
    /// side, which is the sound, valuable direction:
    ///
    /// - **Over**: `atom ∧ (⋁_t P_over(t)  ∨  size-escape)`. Conjoining a
    ///   fact *implied by membership* (a surviving tuple is within the bound
    ///   and passes `P`, or lies beyond the bound) never drops a real member,
    ///   so the result stays a superset. When every `P_over(t)` is `true`
    ///   (opaque cut — `mass`, `dR`), the disjunction folds to `true` and the
    ///   refinement is a no-op: the encoding degrades exactly to `atom`,
    ///   which the COMBSIZE axioms already bound. The gain materializes only
    ///   when a per-tuple cut is built from analyzable per-element quantities
    ///   and is unsatisfiable for every bounded tuple.
    /// - **Under**: just `atom`. USER ANSWER 4: same-source `disjoint`
    ///   value-distinctness makes the existence *lower* bound opaque (two
    ///   value-equal elements form 0 pairs), so we never strengthen the
    ///   under-approximation here. (The cartesian both-nonempty lower bound
    ///   already lives in COMBSIZE and is unaffected.)
    ///
    /// Applies only to a `size(K) ⋈ k` atom that is a positive lower bound
    /// (`>= c`/`> c`/`== c` with the satisfying region requiring `size >= 1`),
    /// where `K` is a `Combination`/`CombProject` carrying per-tuple `cuts`.
    /// Returns `None` (fall through to the plain atom) otherwise.
    fn try_comb_existence(
        &mut self,
        terms: &BTreeMap<QuantityId, Rat>,
        rel: Rel,
        k: &Rat,
        span: Span,
    ) -> Option<Formula> {
        // Single positive-coefficient size term, comparing to a constant that
        // forces at least one surviving tuple.
        let q = match terms.iter().next() {
            Some((q, c)) if terms.len() == 1 && c.is_one() => *q,
            _ => return None,
        };
        // A lower bound that implies `size >= 1`: `size >= k` (k>=1),
        // `size > k` (k>=0), `size == k` (k>=1). Other relations (`<`, `<=`,
        // `!=`, or a non-positive bound) do not assert existence.
        let forces_existence = match rel {
            Rel::Ge => k >= &Rat::one(),
            Rel::Gt => k >= &Rat::zero(),
            Rel::Eq => k >= &Rat::one(),
            Rel::Lt | Rel::Le | Rel::Ne => false,
        };
        if !forces_existence {
            return None;
        }
        let Quantity::Size(coll) = *self.table.quantity(q) else {
            return None;
        };
        // Resolve `K` to the underlying combination (projection ⇒ its comb).
        let comb_id = match self.table.collection(coll).clone() {
            Collection::Combination { .. } => coll,
            Collection::CombProject { comb, .. } => comb,
            _ => return None,
        };
        let Collection::Combination {
            parts,
            kind,
            members,
            cuts,
            ..
        } = self.table.collection(comb_id).clone()
        else {
            return None;
        };
        if cuts.is_empty() || parts.is_empty() {
            // Nothing to refine: COMBSIZE already covers the cuts-free case
            // (and asserts its own existence lower bound where sound).
            return None;
        }
        // Build the bounded set of per-binder index tuples (i<j for a
        // same-source disjoint; the full product otherwise), capped at
        // `COMB2D_BOUND` per binder.
        let tuples = self.binder_index_tuples(&parts, kind);
        if tuples.is_empty() {
            return None;
        }
        // `P(t)` = conjunction of every per-tuple cut, encoded over the
        // tuple's bound elements. Encoded as a boolean Formula whose OVER
        // projection is what the existence disjunct uses.
        let mut existence: Vec<Formula> = Vec::new();
        for t in &tuples {
            let p = self.encode_tuple_cuts(&cuts, &members, &parts, t);
            existence.push(p);
        }
        // Size escape: a surviving tuple could use a binder index at or beyond
        // the bound. Sound over-approximation — `size(part_s) > B` for any
        // slot admits the possibility, so we union one disjunct per slot.
        for &part in &parts {
            let sq = self.table.intern_quantity(Quantity::Size(part));
            existence.push(self.simple_atom(sq, Rel::Gt, i64::from(COMB2D_BOUND)));
        }
        let atom = self.atom_of(terms.clone(), rel, k.clone());
        if atom == Formula::False {
            return Some(atom);
        }
        let why = self.diags.push(
            span,
            format!(
                "composite tuple-count lower bound: 2D per-candidate-cut existence \
                 expansion (bound={COMB2D_BOUND}); under-approx kept opaque (USER ANSWER 4)"
            ),
        );
        // Over: atom ∧ (⋁ P_over(t) ∨ escape). Under: atom (no strengthening).
        let plus = fand(vec![atom.clone(), forr(existence)]);
        Some(Formula::Dual {
            plus: Box::new(plus),
            minus: Box::new(atom),
            why,
        })
    }

    /// Bounded per-binder index tuples for the 2D expansion. Same-source
    /// `disjoint` over `>= 2` binders enumerates strictly-increasing tuples
    /// (matching the interpreter's `i < j` rule — no value-equal repeats);
    /// every other shape (cross-source disjoint, cartesian) takes the full
    /// product. Each binder index ranges `0..COMB2D_BOUND`.
    fn binder_index_tuples(&self, parts: &[CollectionId], kind: CombKind) -> Vec<Vec<u32>> {
        let b = COMB2D_BOUND;
        let mut tuples: Vec<Vec<u32>> = vec![Vec::new()];
        for _ in parts {
            let mut next = Vec::new();
            for t in &tuples {
                for i in 0..b {
                    let mut nt = t.clone();
                    nt.push(i);
                    next.push(nt);
                }
            }
            tuples = next;
        }
        let same_source = parts.len() >= 2 && parts.windows(2).all(|w| w[0] == w[1]);
        if kind == CombKind::Disjoint && same_source {
            tuples
                .into_iter()
                .filter(|idxs| idxs.windows(2).all(|w| w[0] < w[1]))
                .collect()
        } else {
            tuples
        }
    }

    /// Encode the conjunction of a composite's per-tuple `cuts` for one bound
    /// index tuple, substituting each binder name with `parts[slot][idx]`.
    /// Each cut is a boolean predicate over the tuple binders; a cut that
    /// references an opaque quantity (`mass(cand)`, `dR(l1,l2)`) folds to an
    /// `Unknown` leaf whose OVER projection is `true` — so an opaque cut
    /// contributes no tightening (sound).
    fn encode_tuple_cuts(
        &mut self,
        cuts: &[ElemPredId],
        members: &[CompositeBinder],
        parts: &[CollectionId],
        idx: &[u32],
    ) -> Formula {
        let mut parts_f: Vec<Formula> = Vec::new();
        // Existence guard: every bound element must be present.
        for (slot, &i) in idx.iter().enumerate() {
            let sq = self.table.intern_quantity(Quantity::Size(parts[slot]));
            parts_f.push(self.simple_atom(sq, Rel::Gt, i64::from(i)));
        }
        for &cut in cuts {
            let node = self.elem_preds[cut.0 as usize].node.clone();
            let inst = self.subst_binders(&node, members, parts, idx);
            // The plan keeps candidate mass/pt-of-sum OPAQUE: a cut that did
            // NOT fully ground to indexed per-element quantities (it still
            // references a binder, or a `Sum` candidate over binders —
            // `mass(jj)`, `pt(jj)`) becomes Unknown for this tuple, whose OVER
            // projection is `true`. Only cuts built from analyzable
            // per-element quantities (indexed ElemProp, sizes, tags, angular
            // seps between binder elements) reach the over side.
            if self.has_residual_binder(&inst) {
                parts_f.push(self.unknown(
                    inst.span,
                    "composite per-tuple cut references an opaque candidate \
                     (mass/pt of a 4-vector sum) — kept opaque (P3)",
                ));
            } else {
                parts_f.push(self.boolean(&inst));
            }
        }
        fand(parts_f)
    }

    /// Does `node` still reference a composite binder (directly or inside a
    /// candidate `Sum`)? Such a reference means the 2D substitution could not
    /// ground the quantity to an indexed per-element quantity — it stays
    /// opaque (mass/pt of a sum), so the whole leaf must be Unknown.
    fn has_residual_binder(&self, node: &HNode) -> bool {
        fn particle_has_binder(p: &ParticleRef) -> bool {
            match p {
                ParticleRef::Binder { .. } => true,
                ParticleRef::Sum(parts) => parts.iter().any(particle_has_binder),
                _ => false,
            }
        }
        let quantity_has_binder = |table: &QuantityTable, q: QuantityId| -> bool {
            match table.quantity(q) {
                Quantity::AngularSep { a, b, .. } => {
                    particle_has_binder(a) || particle_has_binder(b)
                }
                Quantity::ExternalFn { args, .. } => args.iter().any(|arg| {
                    matches!(arg, QuantityArg::Particle(p) if particle_has_binder(p))
                }),
                _ => false,
            }
        };
        match &node.kind {
            HKind::Quantity(q) => quantity_has_binder(self.table, *q),
            HKind::ReduceProp(_) | HKind::ElemSelfProp(_) => false,
            _ => hnode_children(node)
                .into_iter()
                .any(|c| self.has_residual_binder(c)),
        }
    }

    /// Clone a per-tuple cut node, replacing every `ParticleRef::Binder{name}`
    /// (and `ReduceProp`/binder-anchored quantities) with the indexed source
    /// element `parts[slot][idx[slot]]`, where `slot` is the binder's position
    /// in `members`. A reference to a binder the tuple does not bind, or to an
    /// opaque sub-term, is left untouched (it stays opaque ⇒ Unknown leaf).
    fn subst_binders(
        &mut self,
        node: &HNode,
        members: &[CompositeBinder],
        parts: &[CollectionId],
        idx: &[u32],
    ) -> HNode {
        let kind = match &node.kind {
            HKind::Quantity(q) => {
                HKind::Quantity(self.subst_binder_quantity(*q, members, parts, idx))
            }
            HKind::Neg(a) => HKind::Neg(Box::new(self.subst_binders(a, members, parts, idx))),
            HKind::Not(a) => HKind::Not(Box::new(self.subst_binders(a, members, parts, idx))),
            HKind::Abs(a) => HKind::Abs(Box::new(self.subst_binders(a, members, parts, idx))),
            HKind::Binary { op, lhs, rhs } => HKind::Binary {
                op: *op,
                lhs: Box::new(self.subst_binders(lhs, members, parts, idx)),
                rhs: Box::new(self.subst_binders(rhs, members, parts, idx)),
            },
            HKind::Cmp { op, lhs, rhs } => HKind::Cmp {
                op: *op,
                lhs: Box::new(self.subst_binders(lhs, members, parts, idx)),
                rhs: Box::new(self.subst_binders(rhs, members, parts, idx)),
            },
            HKind::And(v) => {
                HKind::And(v.iter().map(|n| self.subst_binders(n, members, parts, idx)).collect())
            }
            HKind::Or(v) => {
                HKind::Or(v.iter().map(|n| self.subst_binders(n, members, parts, idx)).collect())
            }
            HKind::Band { kind, expr, lo, hi } => HKind::Band {
                kind: *kind,
                expr: Box::new(self.subst_binders(expr, members, parts, idx)),
                lo: lo.clone(),
                hi: hi.clone(),
            },
            HKind::Ternary { guard, then, els } => HKind::Ternary {
                guard: Box::new(self.subst_binders(guard, members, parts, idx)),
                then: Box::new(self.subst_binders(then, members, parts, idx)),
                els: els.as_ref().map(|e| Box::new(self.subst_binders(e, members, parts, idx))),
            },
            other => other.clone(),
        };
        HNode {
            kind,
            span: node.span,
            tag: node.tag.clone(),
        }
    }

    /// The source element a binder name resolves to in this tuple, or `None`
    /// if the name is not a bound slot.
    fn binder_elem(
        members: &[CompositeBinder],
        parts: &[CollectionId],
        idx: &[u32],
        name: adl_sema::Symbol,
    ) -> Option<ParticleRef> {
        let slot = members.iter().position(|m| m.name == name)?;
        Some(ParticleRef::Elem {
            coll: *parts.get(slot)?,
            index: ElemIndex::FromFront(*idx.get(slot)?),
        })
    }

    /// Rewrite an interned quantity, replacing every `ParticleRef::Binder`
    /// with its bound source element. Re-interns; an unchanged quantity keeps
    /// its id. A `Sum`/opaque candidate over binders stays opaque (the
    /// existing `ExternalFn`/`Sum` posture) — sound: it folds to an Unknown
    /// leaf whose OVER projection is `true`.
    fn subst_binder_quantity(
        &mut self,
        q: QuantityId,
        members: &[CompositeBinder],
        parts: &[CollectionId],
        idx: &[u32],
    ) -> QuantityId {
        let subst_p = |p: &ParticleRef| -> ParticleRef {
            match p {
                ParticleRef::Binder { name, .. } => {
                    Self::binder_elem(members, parts, idx, *name).unwrap_or_else(|| p.clone())
                }
                other => other.clone(),
            }
        };
        match self.table.quantity(q).clone() {
            Quantity::AngularSep { kind, a, b, .. } => {
                let (na, nb) = (subst_p(&a), subst_p(&b));
                if na == a && nb == b {
                    return q;
                }
                self.table.intern_angular(kind, na, nb)
            }
            Quantity::ExternalFn { name, args } => {
                let mut changed = false;
                let new_args: Vec<QuantityArg> = args
                    .iter()
                    .map(|arg| match arg {
                        QuantityArg::Particle(p @ ParticleRef::Binder { .. }) => {
                            let np = subst_p(p);
                            if np != *p {
                                changed = true;
                            }
                            QuantityArg::Particle(np)
                        }
                        other => other.clone(),
                    })
                    .collect();
                if !changed {
                    return q;
                }
                self.table.intern_quantity(Quantity::ExternalFn {
                    name,
                    args: new_args,
                })
            }
            _ => q,
        }
    }

    // ---- comparisons --------------------------------------------------------

    fn cmp(&mut self, op: CmpOp, lhs: &HNode, rhs: &HNode, span: Span) -> Formula {
        let rel = rel_of(op);
        let l = self.lin_guarded(lhs);
        let r = self.lin_guarded(rhs);
        match (l, r) {
            (Ok(l), Ok(r)) => {
                // l ⋈ r  ⇔  Σ terms ⋈ −k. Exact: rationals never overflow.
                let d = l.sub(&r);
                let k = -&d.k;
                // P3: a positive lower bound on a composite tuple count
                // (`size(K) >= 1`, `size(K->cand) == 1`, …) gains a 2D
                // per-candidate-cut existence refinement on the OVER side.
                if let Some(f) = self.try_comb_existence(&d.terms, rel, &k, span) {
                    return f;
                }
                self.atom_of(d.terms, rel, k)
            }
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
    /// rewrites first (they encode the operator's real structure); failing
    /// those, a deterministic non-linear scalar (`Rsq`, `MCT`, a charge
    /// product, a chi2 define) is interned as one opaque free quantity so the
    /// comparison still becomes a real atom `O ⋈ c` instead of dropping. This
    /// is the loosest sound over-approximation: `O` is an unconstrained real,
    /// so a model always exists, yet two regions that compare the *same*
    /// expression to different thresholds share one `O` and so decide by
    /// threshold (`Rsq > 0.08` vs `Rsq < 0.05` ⇒ disjoint). Anything that
    /// cannot be rendered injectively stays `Unknown`.
    fn pattern(&mut self, side: &HNode, rel: Rel, c: Rat, why: &str, span: Span) -> Formula {
        match &side.kind {
            HKind::Binary {
                op: ArithOp::Div,
                lhs: num,
                rhs: den,
            } => self.ratio(num, den, rel, c, span),
            HKind::Abs(inner) => self.abs_cmp(inner, rel, c, span),
            // Scalar min/max against a constant — the EXACT monotone identity:
            // `min(a,…) < c ⇔ ∃ aᵢ < c`, `min(a,…) > c ⇔ ∀ aᵢ > c`, max dual
            // (Le/Ge alike). Each `aᵢ ⋈ c` recurses through the full
            // comparison machinery (linear → atom, ratio → two-branch, nested
            // min → this same rule). `==`/`!=` have no monotone reading, so
            // they fall to the opaque leaf below.
            HKind::ScalarMinMax { kind, args } => {
                let or_branch = matches!(
                    (kind, rel),
                    (ReduceKind::Min, Rel::Lt | Rel::Le) | (ReduceKind::Max, Rel::Gt | Rel::Ge)
                );
                let and_branch = matches!(
                    (kind, rel),
                    (ReduceKind::Min, Rel::Gt | Rel::Ge) | (ReduceKind::Max, Rel::Lt | Rel::Le)
                );
                if or_branch || and_branch {
                    let parts: Vec<Formula> = args
                        .iter()
                        .map(|a| self.cmp_node_const(a, rel, c.clone(), span))
                        .collect();
                    let combined = if or_branch { forr(parts) } else { fand(parts) };
                    // `min`/`max` is a value only when EVERY argument is — a
                    // missing-element arg makes the interpreter's comparison
                    // false. So conjoin the existence of all args OUTSIDE the
                    // disjunction (the `Or` reading must not fire via a present
                    // arg while another is absent). `guard_existence` collects
                    // the args' element quantities (via `hnode_children`).
                    self.guard_existence(side, combined)
                } else {
                    // `==`/`!=` have no monotone reading. Interning the whole
                    // min as an opaque free leaf would be sound for the UNSAT
                    // side, but a free leaf in an OVERLAP makes the witness
                    // unvalidatable and metamorphically order-dependent
                    // (Candidate vs Rejected). `==` on a min is rare and absent
                    // from the corpus, so keep it an honest Unknown.
                    let _ = why;
                    self.unknown(span, "scalar min/max compared by equality is opaque")
                }
            }
            _ => self.opaque_atom(side, rel, c, why, span),
        }
    }

    /// `side ⋈ c` where `side` is non-linear-but-deterministic: intern it as
    /// one opaque free scalar (sound over-approximation) or, failing that,
    /// keep it `Unknown`.
    fn opaque_atom(&mut self, side: &HNode, rel: Rel, c: Rat, why: &str, span: Span) -> Formula {
        match self.intern_opaque_scalar(side) {
            Some(q) => self.atom_of(BTreeMap::from([(q, Rat::one())]), rel, c),
            None => self.unknown(span, format!("comparison is not linear arithmetic: {why}")),
        }
    }

    /// Encode `node ⋈ c` against a constant, reusing the full comparison
    /// dispatch (linear atom / ratio / abs / opaque / nested min-max). Mirrors
    /// [`Self::cmp`] with a constant right-hand side.
    fn cmp_node_const(&mut self, node: &HNode, rel: Rel, c: Rat, span: Span) -> Formula {
        match self.lin_guarded(node) {
            Ok(l) => {
                let k = &c - &l.k;
                if let Some(f) = self.try_comb_existence(&l.terms, rel, &k, span) {
                    return f;
                }
                self.atom_of(l.terms, rel, k)
            }
            Err(LinErr::NonFinite) => Formula::False,
            Err(LinErr::BadLiteral) => {
                self.unknown(span, "non-finite numeric literal cannot construct an atom")
            }
            Err(LinErr::NonLinear(why)) => self.pattern(node, rel, c, &why, span),
        }
    }

    /// Exact two-branch ratio encoding (SPEC_ANALYSIS §1):
    /// `L/D ⋈ c` (D non-constant) ⇒ `(D>0 ∧ L ⋈ cD) ∨ (D<0 ∧ L ⋈̄ cD)`.
    /// `D = 0` fails the cut (neither branch admits it; §4.4).
    fn ratio(&mut self, num: &HNode, den: &HNode, rel: Rel, c: Rat, span: Span) -> Formula {
        let l = match self.lin_or_opaque(num) {
            Ok(v) => v,
            Err(e) => return self.lin_err(e, "ratio numerator is not linear", span),
        };
        let d = match self.lin_or_opaque(den) {
            Ok(v) => v,
            Err(e) => return self.lin_err(e, "ratio denominator is not linear", span),
        };
        if d.terms.is_empty() {
            // Constant denominator: clear it EXACTLY. `L/d ⋈ c` ⇔ `L ⋈ c·d`
            // (d>0) or `L ⋈̄ c·d` (d<0); `d=0` fails the cut (§4.4). Rational
            // arithmetic keeps the boundary on the interpreter's exactly.
            if d.k.is_zero() {
                return Formula::False; // §4.4
            }
            let cd = d.scale(&c);
            let e = l.sub(&cd);
            let rel = if d.k.is_negative() { rel.flipped() } else { rel };
            let k = -&e.k;
            return self.atom_of(e.terms, rel, k);
        }
        let cd = d.scale(&c);
        let e = l.sub(&cd);
        let neg_d_k = -&d.k;
        let neg_e_k = -&e.k;
        let d_pos = self.atom_of(d.terms.clone(), Rel::Gt, neg_d_k.clone());
        let e_pos = self.atom_of(e.terms.clone(), rel, neg_e_k.clone());
        let d_neg = self.atom_of(d.terms, Rel::Lt, neg_d_k);
        let e_neg = self.atom_of(e.terms, rel.flipped(), neg_e_k);
        forr(vec![fand(vec![d_pos, e_pos]), fand(vec![d_neg, e_neg])])
    }

    /// Exact absolute-value expansion against a constant:
    /// `|E| < c ⇔ E < c ∧ E > −c`, `|E| > c ⇔ E > c ∨ E < −c`, etc.
    fn abs_cmp(&mut self, inner: &HNode, rel: Rel, c: Rat, span: Span) -> Formula {
        let e = match self.lin(inner) {
            Ok(v) => v,
            Err(err) => {
                return self.lin_err(err, "absolute value of a non-linear expression", span);
            }
        };
        // `|E| >= 0` always, so a comparison against a negative constant is
        // itself constant — exact for every relation, no approximation. The
        // expansion below is only correct for `c >= 0`; without this guard
        // `|E| == c` (c<0) would encode as SAT and `|E| != c` (c<0) as a
        // two-point exclusion — both unsound (false PROVEN verdicts).
        if c.is_negative() {
            return match rel {
                Rel::Lt | Rel::Le | Rel::Eq => Formula::False,
                Rel::Gt | Rel::Ge | Rel::Ne => Formula::True,
            };
        }
        let hi = &c - &e.k;
        let neg_c = -&c;
        let lo = &neg_c - &e.k;
        let upper = |enc: &mut Self, r: Rel| enc.atom_of(e.terms.clone(), r, hi.clone());
        let lower = |enc: &mut Self, r: Rel| enc.atom_of(e.terms.clone(), r, lo.clone());
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
        let (Some(lo), Some(hi)) = (parse_rat(lo), parse_rat(hi)) else {
            return self.unknown(span, "non-finite numeric literal cannot construct an atom");
        };
        // `lin_guarded` (not `lin`): a non-f64-faithful band expression
        // (`MET+HT-HT [] lo hi`) must route to the opaque/per-bound path, not
        // flatten — else the cancellation false-PROVEN resurfaces in bands.
        let e = match self.lin_guarded(expr) {
            Ok(v) => v,
            // A non-linear band expression (`MET/HT [] lo hi`, `MCT ][ lo hi`)
            // is exactly its two bounds: `lo ≤ x ≤ hi` ⇔ `x ≥ lo ∧ x ≤ hi`,
            // `x ≤ lo ∨ x ≥ hi` for Out. Route each bound through the
            // comparison machinery (`pattern` → exact ratio / abs / opaque
            // free leaf), reusing the same encoding `x ⋈ lo` would get.
            Err(LinErr::NonLinear(why)) => {
                let (lo_rel, hi_rel) = match kind {
                    BandKind::In => (Rel::Ge, Rel::Le),
                    BandKind::Out => (Rel::Le, Rel::Ge),
                };
                let lo_bound = self.pattern(expr, lo_rel, lo, &why, span);
                let hi_bound = self.pattern(expr, hi_rel, hi, &why, span);
                return match kind {
                    BandKind::In => fand(vec![lo_bound, hi_bound]),
                    BandKind::Out => forr(vec![lo_bound, hi_bound]),
                };
            }
            Err(err) => return self.lin_err(err, "band expression is not linear", span),
        };
        let lo_k = &lo - &e.k;
        let hi_k = &hi - &e.k;
        let lo_bound = self.atom_of(
            e.terms.clone(),
            if kind == BandKind::In {
                Rel::Ge
            } else {
                Rel::Le
            },
            lo_k,
        );
        let hi_bound = self.atom_of(
            e.terms,
            if kind == BandKind::In {
                Rel::Le
            } else {
                Rel::Ge
            },
            hi_k,
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
    fn atom_of(&mut self, terms: BTreeMap<QuantityId, Rat>, mut rel: Rel, mut k: Rat) -> Formula {
        if terms.is_empty() {
            // Constant comparison: fold exactly.
            return if rel.eval(&Rat::zero(), &k) {
                Formula::True
            } else {
                Formula::False
            };
        }
        // Int-size coercion: a sum of integer multiples of collection
        // sizes is an integer, so fractional bounds tighten exactly.
        let int_valued = terms
            .iter()
            .all(|(q, c)| matches!(self.table.quantity(*q), Quantity::Size(_)) && c.is_integer());
        if int_valued && !k.is_integer() {
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
        Formula::Atom(LinAtom::new(terms.into_iter().map(|(q, c)| (c, q)), rel, k))
    }

    // ---- linear extraction ----------------------------------------------

    /// Intern a value-position numeric reducer (`sum`/`min`/`max`) as a
    /// **structurally-interned free quantity**: two structurally-identical
    /// reducers share one `QuantityId` (so the interval/solver engine cancels
    /// their bands across regions), while structurally-distinct reducers get
    /// distinct ids. Modeled as an opaque `ExternalFn` — a free var with NO
    /// axiom (the loosest sound over-approximation; NNEG on a sign-indefinite
    /// body like `pt*cos(phi)` would be unsound, so none is added).
    ///
    /// The identity is keyed on (reducer kind, iteration-collection id, body
    /// structure, slice): the iteration collection rides in a real
    /// `QuantityArg::Collection` (so cross-file merge remaps it), and the body
    /// shape rides in a `QuantityArg::Opaque` over interned ids (merge
    /// namespaces it per-unit, so it only ever fails to cross-merge — sound).
    ///
    /// Returns `None` (caller falls back to the opaque non-linear leaf) if the
    /// body cannot be rendered injectively — the conservative posture.
    fn intern_reduce(
        &mut self,
        kind: ReduceKind,
        coll: CollectionId,
        body: &HNode,
        slice: Option<(u32, Option<u32>)>,
    ) -> Option<QuantityId> {
        let body_key = reduce_body_key(body)?;
        // The `.` makes this name unrepresentable as a source identifier
        // (`[A-Za-z][A-Za-z0-9]*`, `_`-joined — no dots), so it can never
        // collide with a user/ext function symbol, while still rendering
        // cleanly in verdict labels (`reduce.sum(...)`).
        let name = self
            .symbols
            .intern(&format!("reduce.{}", kind.as_str()));
        let args = vec![
            QuantityArg::Collection(coll),
            QuantityArg::Opaque(format!("{}{}", slice_key(slice), body_key)),
        ];
        Some(
            self.table
                .intern_quantity(Quantity::ExternalFn { name, args }),
        )
    }

    /// Intern a deterministic non-linear scalar sub-expression (`Rsq`,
    /// `MCT`, a `q1*q2` charge product, …) as a single opaque free quantity,
    /// keyed by its canonical structure so identical expressions across
    /// regions share one `QuantityId` and cancel by threshold. Same axiom
    /// posture as [`Self::intern_reduce`]: a `.`-named `ExternalFn` carries no
    /// axiom (a free real — `Rsq` is really `>= 0`, but assuming so is not
    /// needed for soundness and a sign-indefinite body like `pt*cos(phi)`
    /// would make NNEG unsound, so none is added). `None` when the body is not
    /// injectively renderable — the caller then keeps the leaf `Unknown`.
    fn intern_opaque_scalar(&mut self, node: &HNode) -> Option<QuantityId> {
        let body_key = reduce_body_key(node)?;
        let name = self.symbols.intern("opaque.scalar");
        let args = vec![QuantityArg::Opaque(body_key)];
        Some(
            self.table
                .intern_quantity(Quantity::ExternalFn { name, args }),
        )
    }

    /// Like [`Self::lin`], but a non-linear-yet-deterministic operand is
    /// interned as one opaque free scalar (the sound over-approximation of
    /// [`Self::intern_opaque_scalar`]) instead of failing. Used for a ratio's
    /// numerator/denominator so `MET / (HT^0.5) ⋈ c` reduces to the exact
    /// two-branch encoding over a single free quantity rather than dropping.
    fn lin_or_opaque(&mut self, node: &HNode) -> Result<LinExpr, LinErr> {
        // A non-f64-faithful source (≥2 additive ops / non-dyadic additive
        // constant) must NOT flatten even in a ratio operand — intern it whole
        // as a structure-keyed opaque scalar so cancellation/reassociation
        // can't fabricate a false PROVEN through the ratio path.
        if !is_f64_faithful(node) {
            return match self.intern_opaque_scalar(node) {
                Some(q) => Ok(LinExpr::quantity(q)),
                None => Err(LinErr::NonLinear("source not f64-faithful".to_owned())),
            };
        }
        match self.lin(node) {
            Ok(v) => Ok(v),
            Err(LinErr::NonLinear(why)) => match self.intern_opaque_scalar(node) {
                Some(q) => Ok(LinExpr::quantity(q)),
                None => Err(LinErr::NonLinear(why)),
            },
            Err(e) => Err(e),
        }
    }

    /// [`Self::lin`] for a TOP-LEVEL comparison operand, gated by the
    /// f64-faithfulness guard: a source that is not [`is_f64_faithful`]
    /// (multiple additive ops, or a non-dyadic additive constant) is refused
    /// as `NonLinear` so `cmp`/`pattern` route it to `intern_opaque_scalar`
    /// (structure-keyed) instead of flattening it into a shared linear atom.
    /// This prevents two regions whose sources f64-evaluate differently — yet
    /// canonicalize to the same `Σcᵢqᵢ` — from fabricating a false PROVEN
    /// DISJOINT/SUBSET/EMPTY. NOT used for recursive sub-linearization, which
    /// stays exact (the `0.5*HT` inside a faithful `MET + 0.5*HT` is fine).
    fn lin_guarded(&mut self, node: &HNode) -> Result<LinExpr, LinErr> {
        if !is_f64_faithful(node) {
            return Err(LinErr::NonLinear(
                "source not f64-faithful (multiple additive ops or non-dyadic additive constant)"
                    .to_owned(),
            ));
        }
        self.lin(node)
    }

    fn lin(&mut self, node: &HNode) -> Result<LinExpr, LinErr> {
        if let Fragment::Unsupported(reason) = &node.tag {
            return Err(LinErr::NonLinear(reason.clone()));
        }
        match &node.kind {
            HKind::Num(s) => match parse_rat(s) {
                Some(v) => Ok(LinExpr::constant(v)),
                None => Err(LinErr::BadLiteral),
            },
            HKind::Quantity(q) => Ok(LinExpr::quantity(*q)),
            HKind::Neg(a) => Ok(self.lin(a)?.scale(&Rat::from_i64(-1))),
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
            HKind::ReduceProp(_) => Err(LinErr::NonLinear(
                "reducer-element property is interpret-only".to_owned(),
            )),
            // A reducer in arithmetic position is a value, not a condition:
            // boolean reducers (`any`/`all`) are encoded at the boolean layer
            // (non-linear here). A numeric reducer (`sum`, or a `min`/`max`
            // outside a monotone comparison) is a value: intern it as a
            // structurally-interned FREE quantity so structurally-identical
            // reducers cancel across regions (`HT > 400` vs `HT in [60,400]`
            // on a shared `sum(jets.pT)`), while distinct ones get distinct
            // ids. If the body cannot be rendered injectively, fall back to
            // the opaque non-linear leaf (sound).
            HKind::Reduce {
                kind,
                coll,
                body,
                slice,
            } if !kind.is_boolean() => match self.intern_reduce(*kind, *coll, body, *slice) {
                Some(q) => Ok(LinExpr::quantity(q)),
                None => Err(LinErr::NonLinear(format!(
                    "`{}` reducer body is not injectively renderable",
                    kind.as_str()
                ))),
            },
            HKind::Reduce { kind, .. } => Err(LinErr::NonLinear(format!(
                "`{}` reducer value is opaque to linear arithmetic",
                kind.as_str()
            ))),
            // Scalar min/max is not linear; the comparison path (`pattern`)
            // desugars it monotonically against a constant, or interns it as
            // one opaque free scalar for `==`/`!=` and value position.
            HKind::ScalarMinMax { kind, .. } => Err(LinErr::NonLinear(format!(
                "`{}` of scalars is not linear arithmetic",
                kind.as_str()
            ))),
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
                Ok(l.combine(&r, false))
            }
            ArithOp::Sub => {
                let l = self.lin(lhs)?;
                let r = self.lin(rhs)?;
                Ok(l.sub(&r))
            }
            ArithOp::Mul => {
                let l = self.lin(lhs)?;
                let r = self.lin(rhs)?;
                if l.terms.is_empty() {
                    Ok(r.scale(&l.k))
                } else if r.terms.is_empty() {
                    Ok(l.scale(&r.k))
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
                if r.k.is_zero() {
                    return Err(LinErr::NonFinite); // §4.4: division by zero
                }
                let l = self.lin(lhs)?;
                if l.terms.is_empty() {
                    // constant / constant: exact rational division.
                    return match l.k.checked_div(&r.k) {
                        Some(v) => Ok(LinExpr::constant(v)),
                        None => Err(LinErr::NonFinite), // r.k != 0, so unreachable
                    };
                }
                // variable numerator / constant denominator: deferred to the
                // comparison level, where multiply-through clears the
                // denominator with the numerator's exact coefficients; a
                // nested occurrence has no comparison to clear it -> Unknown.
                Err(LinErr::NonLinear(
                    "division by a constant (cleared at the comparison level)".to_owned(),
                ))
            }
            ArithOp::Pow => {
                let l = self.lin(lhs)?;
                let r = self.lin(rhs)?;
                if l.terms.is_empty() && r.terms.is_empty() {
                    // Only INTEGER powers stay rational; a fractional exponent
                    // is generally irrational, so it leaves the linear fragment
                    // (Unknown) rather than being folded to an inexact f64.
                    match r.k.to_i64().and_then(|n| i32::try_from(n).ok()) {
                        Some(n) => match l.k.powi(n) {
                            Some(v) => Ok(LinExpr::constant(v)),
                            None => Err(LinErr::NonFinite), // 0^negative (§4.4)
                        },
                        None => Err(LinErr::NonLinear("non-integer power".to_owned())),
                    }
                } else {
                    Err(LinErr::NonLinear("non-constant power".to_owned()))
                }
            }
        }
    }
}

/// Parse a canonical numeral as an exact decimal rational (the value the
/// physicist wrote); `None` if it does not parse finite.
fn parse_rat(s: &str) -> Option<Rat> {
    s.parse::<f64>().ok().and_then(Rat::from_decimal_f64)
}
