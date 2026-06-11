//! The reference evaluator: HIR over an [`Event`] → bool/values out.
//!
//! Semantics are exactly SPEC_LANGUAGE §4 — this crate is the executable
//! spec. Key rules implemented here:
//!
//! - **Objects** (§4.2): `object D take S <cuts>` = elements of `S`
//!   passing all cuts, order preserved; `take union(A,B)` concatenates;
//!   a pure rename shares its source's `CollectionId` (sema fact), so it
//!   evaluates identically by construction.
//! - **Regions** (§4.3): the conjunction, in order, of the statements;
//!   `select c` ⇒ `c`, `reject c` ⇒ `¬c`, a bare region name inlines that
//!   region's predicate, `trigger t` ⇒ the flag; `weight`/`histo`/`save`
//!   contribute nothing; `bin` partitions without constraining membership
//!   (`[b0,b1), …, [bn,∞)`, open last bin).
//! - **Expressions** (§4.4): `g ? a : b` ≡ `(g∧a) ∨ (¬g∧b)` (missing/ALL
//!   branch is true); `x [] lo hi` ≡ `lo ≤ x ≤ hi`; `x ][ lo hi` ≡
//!   `x ≤ lo ∨ x ≥ hi`; division by zero / non-finite arithmetic makes
//!   the **enclosing comparison false** (the event fails the cut).
//! - **Fragment honesty** (§5): out-of-fragment constructs raise a
//!   diagnosed [`EvalError`] — never a silent guess.
//!
//! Out-of-range element references and missing object properties are
//! *soft* non-values ([`NonValue`]): like non-finite arithmetic, they
//! make the enclosing comparison false (guarded references do not imply
//! existence). Missing *event-level* data (MET, scalars, trigger flags)
//! is a hard [`EvalError`]: those are structural parts of the event
//! model, so their absence is a data mismatch, not physics.

use crate::event::{Event, EventObject};
use adl_sema::{
    AngKind, Collection, CollectionId, ElemIndex, ExtDecls, Fragment, HKind, HNode, Hir,
    HirRegionStmt, ParticleRef, PropId, Quantity, QuantityArg, QuantityId, ScalarSource,
};
use adl_syntax::ast::{BandKind, CmpOp};
use adl_syntax::span::Span;
use std::collections::HashMap;
use std::fmt;
use std::rc::Rc;

/// A diagnosed evaluation error (out-of-fragment construct, ambiguous
/// semantics, or missing event-level data).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    pub span: Span,
    pub reason: String,
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "evaluation error: {}", self.reason)
    }
}

impl std::error::Error for EvalError {}

/// A soft non-value: the enclosing comparison evaluates to **false**
/// (SPEC_LANGUAGE §4.4 div-by-zero rule, extended to guarded references).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonValue {
    /// Division by zero or other non-finite arithmetic.
    NonFinite,
    /// Element index beyond the collection's size.
    MissingElement { collection: String, index: u32 },
    /// Object lacks the requested property.
    MissingProperty { property: String },
}

impl fmt::Display for NonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NonValue::NonFinite => write!(f, "non-finite arithmetic"),
            NonValue::MissingElement { collection, index } => {
                write!(f, "`{collection}[{index}]` does not exist in this event")
            }
            NonValue::MissingProperty { property } => {
                write!(f, "object has no `{property}` property")
            }
        }
    }
}

/// Result of evaluating a numeric expression.
#[derive(Debug, Clone, PartialEq)]
pub enum NumOutcome {
    /// A finite value.
    Value(f64),
    /// No usable value; any enclosing comparison is false.
    NonValue(NonValue),
}

/// Outcome of one `bin` statement for an event that passed its region.
#[derive(Debug, Clone, PartialEq)]
pub enum BinOutcome {
    /// Boundary-list bin: `bin v b0 … bn` ⇒ `[b0,b1), …, [bn,∞)`.
    /// `bin == None` means the value fell below `b0` (or was a
    /// non-value) — boundary bins do not cover `(-∞, b0)`.
    Boundary {
        label: Option<String>,
        value: Option<f64>,
        bin: Option<usize>,
    },
    /// Boolean bin: membership of the condition.
    Cond { label: Option<String>, member: bool },
    /// The bin expression could not be evaluated (hard error).
    Failed {
        label: Option<String>,
        reason: String,
    },
}

/// Per-region outcome for one event.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionResult {
    /// Region name (first-seen spelling).
    pub name: String,
    /// Membership, or a diagnosed evaluation error.
    pub pass: Result<bool, EvalError>,
    /// Bin assignments; populated only when `pass == Ok(true)`.
    pub bins: Vec<BinOutcome>,
}

/// Boundary-list bin assignment: edges `b0 … bn` denote bins
/// `[b0,b1), …, [bn-1,bn), [bn,∞)` (SPEC_LANGUAGE §4.3; open last bin).
/// Returns `None` for `v < b0` or non-finite `v`.
#[must_use]
pub fn assign_bin(v: f64, edges: &[f64]) -> Option<usize> {
    if edges.is_empty() || !v.is_finite() {
        return None;
    }
    let last = edges.len() - 1;
    if v >= edges[last] {
        return Some(last);
    }
    (0..last).find(|&i| edges[i] <= v && v < edges[i + 1])
}

type EvalResult<T> = Result<T, EvalError>;
/// Numeric result: a finite value or a soft non-value.
type NumRes = Result<f64, NonValue>;

fn fin(v: f64) -> NumRes {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(NonValue::NonFinite)
    }
}

/// Wrap an angle difference into `[-π, π)` (PHASE0 OPEN-2: oriented
/// `dPhi`, range axiom −π…π).
#[must_use]
pub fn wrap_dphi(d: f64) -> f64 {
    use std::f64::consts::PI;
    (d + PI).rem_euclid(2.0 * PI) - PI
}

/// The reference interpreter for one resolved analysis unit.
pub struct Interp<'h> {
    hir: &'h Hir,
    ext: &'h ExtDecls,
    eta_key: String,
    phi_key: String,
}

impl<'h> Interp<'h> {
    #[must_use]
    pub fn new(hir: &'h Hir, ext: &'h ExtDecls) -> Self {
        Self {
            hir,
            ext,
            eta_key: ext.prop_canon("eta").0,
            phi_key: ext.prop_canon("phi").0,
        }
    }

    #[must_use]
    pub fn hir(&self) -> &'h Hir {
        self.hir
    }

    /// Evaluate region membership by (case-insensitive) name.
    ///
    /// # Errors
    /// Returns an [`EvalError`] for unknown regions or diagnosed
    /// evaluation failures.
    pub fn eval_region_by_name(&self, name: &str, event: &Event) -> EvalResult<bool> {
        let idx = self.region_index(name).ok_or_else(|| EvalError {
            span: Span::default(),
            reason: format!("no region named `{name}`"),
        })?;
        Ev::new(self, event).region(idx)
    }

    /// Evaluate every region (in declaration order) plus bin assignments.
    #[must_use]
    pub fn run_event(&self, event: &Event) -> Vec<RegionResult> {
        let mut ev = Ev::new(self, event);
        self.hir
            .regions
            .iter()
            .enumerate()
            .map(|(idx, region)| {
                let pass = ev.region(idx);
                let bins = if pass == Ok(true) {
                    self.region_bins(&mut ev, idx)
                } else {
                    Vec::new()
                };
                RegionResult {
                    name: self.hir.symbols.display(region.name).to_owned(),
                    pass,
                    bins,
                }
            })
            .collect()
    }

    /// Evaluate a resolved expression as a predicate.
    ///
    /// # Errors
    /// Returns an [`EvalError`] for out-of-fragment constructs or missing
    /// event-level data.
    pub fn eval_bool(&self, node: &'h HNode, event: &Event) -> EvalResult<bool> {
        Ev::new(self, event).truth(node, None)
    }

    /// Evaluate a resolved expression numerically.
    ///
    /// # Errors
    /// Returns an [`EvalError`] for out-of-fragment constructs or missing
    /// event-level data. Soft failures come back as
    /// [`NumOutcome::NonValue`].
    pub fn eval_num(&self, node: &'h HNode, event: &Event) -> EvalResult<NumOutcome> {
        Ok(match Ev::new(self, event).num(node, None)? {
            Ok(v) => NumOutcome::Value(v),
            Err(nv) => NumOutcome::NonValue(nv),
        })
    }

    /// Materialize a named collection for `event` (object filtering with
    /// order preserved, union concatenation).
    ///
    /// # Errors
    /// Returns an [`EvalError`] for unknown names and out-of-fragment
    /// collections (COMB, the MET pseudo-collection).
    pub fn collection(&self, name: &str, event: &Event) -> EvalResult<Vec<EventObject>> {
        let id = self.collection_id(name).ok_or_else(|| EvalError {
            span: Span::default(),
            reason: format!("no collection named `{name}`"),
        })?;
        Ok(Ev::new(self, event).materialize(id)?.as_ref().clone())
    }

    fn region_index(&self, name: &str) -> Option<usize> {
        let sym = self.hir.symbols.lookup(name)?;
        self.hir.regions.iter().position(|r| r.name == sym)
    }

    fn collection_id(&self, name: &str) -> Option<CollectionId> {
        if let Some(id) = self.hir.collection_of(name) {
            return Some(id);
        }
        // Fall back to base collections, which carry no bound object name.
        let canon = self.ext.base_collection(name)?;
        let sym = self.hir.symbols.lookup(canon)?;
        self.hir
            .table
            .collections()
            .iter()
            .position(|c| matches!(c, Collection::Base(s) if *s == sym))
            .map(|i| CollectionId(u32::try_from(i).expect("collection id overflow")))
    }

    fn region_bins(&self, ev: &mut Ev<'h, '_>, idx: usize) -> Vec<BinOutcome> {
        let mut bins = Vec::new();
        for stmt in &self.hir.regions[idx].stmts {
            match stmt {
                HirRegionStmt::Bin {
                    label,
                    var,
                    edges,
                    span,
                } => bins.push(self.boundary_bin(ev, label, var, edges, *span)),
                HirRegionStmt::BinCond { label, cond, .. } => match ev.truth(cond, None) {
                    Ok(member) => bins.push(BinOutcome::Cond {
                        label: label.clone(),
                        member,
                    }),
                    Err(e) => bins.push(BinOutcome::Failed {
                        label: label.clone(),
                        reason: e.reason,
                    }),
                },
                _ => {}
            }
        }
        bins
    }

    fn boundary_bin(
        &self,
        ev: &mut Ev<'h, '_>,
        label: &Option<String>,
        var: &'h HNode,
        edge_texts: &[String],
        span: Span,
    ) -> BinOutcome {
        let mut edges = Vec::with_capacity(edge_texts.len());
        for t in edge_texts {
            match parse_num(t, span) {
                Ok(e) => edges.push(e),
                Err(e) => {
                    return BinOutcome::Failed {
                        label: label.clone(),
                        reason: e.reason,
                    };
                }
            }
        }
        match ev.num(var, None) {
            Err(e) => BinOutcome::Failed {
                label: label.clone(),
                reason: e.reason,
            },
            Ok(Err(_)) => BinOutcome::Boundary {
                label: label.clone(),
                value: None,
                bin: None,
            },
            Ok(Ok(v)) => BinOutcome::Boundary {
                label: label.clone(),
                value: Some(v),
                bin: assign_bin(v, &edges),
            },
        }
    }
}

fn parse_num(text: &str, span: Span) -> EvalResult<f64> {
    text.parse::<f64>().map_err(|_| EvalError {
        span,
        reason: format!("malformed numeric literal `{text}`"),
    })
}

fn compare(op: CmpOp, a: f64, b: f64) -> bool {
    match op {
        CmpOp::Gt => a > b,
        CmpOp::Lt => a < b,
        CmpOp::Ge => a >= b,
        CmpOp::Le => a <= b,
        CmpOp::Eq => a == b,
        // `~=` is mapped to `!=` by sema (OPEN-4); keep the defensive arm.
        CmpOp::Ne | CmpOp::ApproxEq => a != b,
    }
}

/// Eta/phi components of a particle reference (either may be absent).
struct Angles {
    eta: Option<f64>,
    phi: Option<f64>,
}

/// Per-event evaluation state: memoized collections and region verdicts.
struct Ev<'h, 'e> {
    it: &'e Interp<'h>,
    event: &'e Event,
    colls: HashMap<CollectionId, Rc<Vec<EventObject>>>,
    regions: HashMap<usize, Result<bool, EvalError>>,
}

impl<'h, 'e> Ev<'h, 'e> {
    fn new(it: &'e Interp<'h>, event: &'e Event) -> Self {
        Self {
            it,
            event,
            colls: HashMap::new(),
            regions: HashMap::new(),
        }
    }

    fn err<T>(&self, span: Span, reason: impl Into<String>) -> EvalResult<T> {
        Err(EvalError {
            span,
            reason: reason.into(),
        })
    }

    // ---- regions ---------------------------------------------------------

    /// Region membership (§4.3): the conjunction, in order, of the
    /// region's statements; short-circuits at the first failing cut
    /// (cut-flow semantics).
    fn region(&mut self, idx: usize) -> EvalResult<bool> {
        if let Some(cached) = self.regions.get(&idx) {
            return cached.clone();
        }
        let result = self.region_uncached(idx);
        self.regions.insert(idx, result.clone());
        result
    }

    fn region_uncached(&mut self, idx: usize) -> EvalResult<bool> {
        let region = &self.it.hir.regions[idx];
        for stmt in &region.stmts {
            match stmt {
                HirRegionStmt::Select(n) | HirRegionStmt::Trigger(n) => {
                    if !self.truth(n, None)? {
                        return Ok(false);
                    }
                }
                HirRegionStmt::Reject(n) => {
                    if self.truth(n, None)? {
                        return Ok(false);
                    }
                }
                HirRegionStmt::Inherit { region, .. } => {
                    if !self.region(*region)? {
                        return Ok(false);
                    }
                }
                // Bins partition; they never constrain membership (§4.3).
                HirRegionStmt::Bin { .. } | HirRegionStmt::BinCond { .. } => {}
                HirRegionStmt::NonMembership { tag, span, .. } => {
                    if let Fragment::Unsupported(reason) = tag {
                        return self.err(*span, format!("cannot evaluate region: {reason}"));
                    }
                }
            }
        }
        Ok(true)
    }

    // ---- predicates ------------------------------------------------------

    fn truth(&mut self, node: &'h HNode, elem: Option<&EventObject>) -> EvalResult<bool> {
        if let Fragment::Unsupported(reason) = &node.tag {
            return self.err(node.span, reason.clone());
        }
        match &node.kind {
            HKind::Bool(b) => Ok(*b),
            HKind::Not(a) => Ok(!self.truth(a, elem)?),
            HKind::And(parts) => {
                for p in parts {
                    if !self.truth(p, elem)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            HKind::Or(parts) => {
                for p in parts {
                    if self.truth(p, elem)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            // §4.4: a soft non-value on either side makes the comparison
            // false — the event fails the cut.
            HKind::Cmp { op, lhs, rhs } => match (self.num(lhs, elem)?, self.num(rhs, elem)?) {
                (Ok(a), Ok(b)) => Ok(compare(*op, a, b)),
                _ => Ok(false),
            },
            HKind::Band { kind, expr, lo, hi } => {
                let Ok(v) = self.num(expr, elem)? else {
                    return Ok(false);
                };
                let lo = parse_num(lo, node.span)?;
                let hi = parse_num(hi, node.span)?;
                Ok(match kind {
                    BandKind::In => lo <= v && v <= hi,
                    BandKind::Out => v <= lo || v >= hi,
                })
            }
            // `g ? a : b` ≡ `(g∧a) ∨ (¬g∧b)`; missing branch is true.
            HKind::Ternary { guard, then, els } => {
                if self.truth(guard, elem)? {
                    self.truth(then, elem)
                } else {
                    match els {
                        Some(e) => self.truth(e, elem),
                        None => Ok(true),
                    }
                }
            }
            HKind::RegionPred(idx) => self.region(*idx),
            HKind::Num(_)
            | HKind::Quantity(_)
            | HKind::ElemSelfProp(_)
            | HKind::Neg(_)
            | HKind::Abs(_)
            | HKind::Binary { .. } => {
                // Numeric value used as a predicate: nonzero is true; a
                // soft non-value fails the cut.
                Ok(matches!(self.num(node, elem)?, Ok(v) if v != 0.0))
            }
            HKind::CollProp { .. } => self.err(
                node.span,
                "unindexed per-element cut at region level is ambiguous (OPEN-1 unresolved)",
            ),
            HKind::Particle(_) | HKind::CollValue(_) | HKind::Unsupported => {
                self.err(node.span, "expression is outside the checked fragment")
            }
        }
    }

    // ---- numeric evaluation ------------------------------------------------

    fn num(&mut self, node: &'h HNode, elem: Option<&EventObject>) -> EvalResult<NumRes> {
        if let Fragment::Unsupported(reason) = &node.tag {
            return self.err(node.span, reason.clone());
        }
        match &node.kind {
            HKind::Num(text) => Ok(fin(parse_num(text, node.span)?)),
            HKind::Bool(b) => Ok(Ok(f64::from(*b))),
            HKind::Quantity(q) => self.quantity(*q, node.span),
            HKind::ElemSelfProp(prop) => {
                let Some(obj) = elem else {
                    return self.err(
                        node.span,
                        "implicit element property used outside an object block",
                    );
                };
                Ok(self.object_prop(obj, *prop))
            }
            HKind::Neg(a) => Ok(self.num(a, elem)?.map(|v| -v)),
            HKind::Abs(a) => Ok(self.num(a, elem)?.map(f64::abs)),
            HKind::Binary { op, lhs, rhs } => {
                let (a, b) = match (self.num(lhs, elem)?, self.num(rhs, elem)?) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(nv), _) | (_, Err(nv)) => return Ok(Err(nv)),
                };
                let v = match op {
                    adl_sema::ArithOp::Add => a + b,
                    adl_sema::ArithOp::Sub => a - b,
                    adl_sema::ArithOp::Mul => a * b,
                    // Division by zero yields a non-finite value, which
                    // `fin` turns into the comparison-false rule (§4.4).
                    adl_sema::ArithOp::Div => a / b,
                    adl_sema::ArithOp::Pow => a.powf(b),
                };
                Ok(fin(v))
            }
            HKind::Ternary { guard, then, els } => {
                if self.truth(guard, elem)? {
                    self.num(then, elem)
                } else {
                    match els {
                        Some(e) => self.num(e, elem),
                        // Missing branch is `true` (§4.4).
                        None => Ok(Ok(1.0)),
                    }
                }
            }
            HKind::Not(_)
            | HKind::And(_)
            | HKind::Or(_)
            | HKind::Cmp { .. }
            | HKind::Band { .. }
            | HKind::RegionPred(_) => Ok(Ok(f64::from(self.truth(node, elem)?))),
            HKind::CollProp { .. } => self.err(
                node.span,
                "unindexed per-element cut at region level is ambiguous (OPEN-1 unresolved)",
            ),
            HKind::Particle(_) | HKind::CollValue(_) | HKind::Unsupported => {
                self.err(node.span, "expression is outside the checked fragment")
            }
        }
    }

    fn object_prop(&self, obj: &EventObject, prop: PropId) -> NumRes {
        let key = self.it.hir.table.prop_key(prop);
        match obj.get(key) {
            Some(v) => fin(v),
            None => Err(NonValue::MissingProperty {
                property: self.it.hir.table.prop_display(prop).to_owned(),
            }),
        }
    }

    fn quantity(&mut self, q: QuantityId, span: Span) -> EvalResult<NumRes> {
        match self.it.hir.table.quantity(q) {
            Quantity::EventScalar(src) => self.event_scalar(src, span),
            Quantity::Size(coll) => {
                let objs = self.materialize(*coll)?;
                #[allow(clippy::cast_precision_loss)] // realistic sizes are tiny
                Ok(Ok(objs.len() as f64))
            }
            Quantity::ElemProp { coll, index, prop } => {
                let ElemIndex::FromFront(i) = index else {
                    return self.err(span, "negative index `[-n]` is reserved (OPEN-3)");
                };
                let (i, coll, prop) = (*i, *coll, *prop);
                let objs = self.materialize(coll)?;
                match objs.get(i as usize) {
                    Some(obj) => Ok(self.object_prop(obj, prop)),
                    None => Ok(Err(NonValue::MissingElement {
                        collection: self.coll_label(coll),
                        index: i,
                    })),
                }
            }
            Quantity::AngularSep { kind, a, b, .. } => {
                let (kind, a, b) = (*kind, a.clone(), b.clone());
                self.angular(kind, &a, &b, span)
            }
            Quantity::ExternalFn { name, args } => {
                let fname = self.it.hir.symbols.key(*name).to_owned();
                if fname == "sqrt"
                    && let [arg] = args.as_slice()
                {
                    let arg = arg.clone();
                    let v = match self.arg_num(&arg, span)? {
                        Ok(v) => v,
                        Err(nv) => return Ok(Err(nv)),
                    };
                    // sqrt of a negative is NaN ⇒ comparison-false rule.
                    return Ok(fin(v.sqrt()));
                }
                self.err(
                    span,
                    format!("external function `{fname}` has no reference interpretation"),
                )
            }
        }
    }

    fn event_scalar(&self, src: &ScalarSource, span: Span) -> EvalResult<NumRes> {
        match src {
            ScalarSource::MetProp(prop) => {
                if self.event.met.is_empty() {
                    return self.err(span, "event has no MET vector");
                }
                let key = self.it.hir.table.prop_key(*prop);
                match self.event.met.get(key) {
                    Some(&v) => Ok(fin(v)),
                    None => self.err(
                        span,
                        format!(
                            "event MET has no `{}` component",
                            self.it.hir.table.prop_display(*prop)
                        ),
                    ),
                }
            }
            ScalarSource::EventVar(sym) => {
                let key = self.it.hir.symbols.key(*sym);
                match self.event.scalars.get(key) {
                    Some(&v) => Ok(fin(v)),
                    None => self.err(
                        span,
                        format!(
                            "event has no scalar `{}`",
                            self.it.hir.symbols.display(*sym)
                        ),
                    ),
                }
            }
            ScalarSource::Trigger(sym) => {
                let key = self.it.hir.symbols.key(*sym);
                match self.event.triggers.get(key) {
                    Some(&v) => Ok(fin(v)),
                    None => self.err(
                        span,
                        format!(
                            "event has no trigger flag `{}`",
                            self.it.hir.symbols.display(*sym)
                        ),
                    ),
                }
            }
        }
    }

    fn arg_num(&mut self, arg: &QuantityArg, span: Span) -> EvalResult<NumRes> {
        match arg {
            QuantityArg::Num(text) => Ok(fin(parse_num(text, span)?)),
            QuantityArg::Quantity(q) => self.quantity(*q, span),
            _ => self.err(span, "function argument is outside the checked fragment"),
        }
    }

    // ---- angular separations ----------------------------------------------

    fn angular(
        &mut self,
        kind: AngKind,
        a: &ParticleRef,
        b: &ParticleRef,
        span: Span,
    ) -> EvalResult<NumRes> {
        let pa = match self.angles(a, span)? {
            Ok(x) => x,
            Err(nv) => return Ok(Err(nv)),
        };
        let pb = match self.angles(b, span)? {
            Ok(x) => x,
            Err(nv) => return Ok(Err(nv)),
        };
        let missing = |property: &str| NonValue::MissingProperty {
            property: property.to_owned(),
        };
        let dphi = || -> NumRes {
            match (pa.phi, pb.phi) {
                (Some(x), Some(y)) => fin(wrap_dphi(x - y)),
                _ => Err(missing("phi")),
            }
        };
        let deta = || -> NumRes {
            match (pa.eta, pb.eta) {
                (Some(x), Some(y)) => fin(x - y),
                _ => Err(missing("eta")),
            }
        };
        Ok(match kind {
            // Oriented, signed, range [-π, π) (PHASE0 OPEN-2).
            AngKind::DPhi => dphi(),
            AngKind::DEta => deta(),
            AngKind::DR => match (deta(), dphi()) {
                (Ok(de), Ok(dp)) => fin(de.hypot(dp)),
                (Err(nv), _) | (_, Err(nv)) => Err(nv),
            },
        })
    }

    fn angles(&mut self, p: &ParticleRef, span: Span) -> EvalResult<Result<Angles, NonValue>> {
        match p {
            ParticleRef::Elem {
                coll,
                index: ElemIndex::FromFront(i),
            } => {
                let (coll, i) = (*coll, *i);
                let objs = self.materialize(coll)?;
                match objs.get(i as usize) {
                    Some(obj) => Ok(Ok(Angles {
                        eta: obj.get(&self.it.eta_key),
                        phi: obj.get(&self.it.phi_key),
                    })),
                    None => Ok(Err(NonValue::MissingElement {
                        collection: self.coll_label(coll),
                        index: i,
                    })),
                }
            }
            ParticleRef::Elem { .. } => {
                self.err(span, "negative index `[-n]` is reserved (OPEN-3)")
            }
            ParticleRef::Met => {
                if self.event.met.is_empty() {
                    return self.err(span, "event has no MET vector");
                }
                Ok(Ok(Angles {
                    eta: None, // MET has no pseudorapidity
                    phi: self.event.met.get(&self.it.phi_key).copied(),
                }))
            }
            ParticleRef::Whole(_) => self.err(
                span,
                "angular separation over an unindexed collection is ambiguous (OPEN-1 unresolved)",
            ),
            ParticleRef::Binder { .. } => {
                self.err(span, "composite binder is outside the checked fragment")
            }
        }
    }

    // ---- collections ---------------------------------------------------------

    /// Materialize a collection for this event (§4.2): filtering keeps
    /// order; union concatenates; pure renames share the id upstream.
    fn materialize(&mut self, id: CollectionId) -> EvalResult<Rc<Vec<EventObject>>> {
        if let Some(objs) = self.colls.get(&id) {
            return Ok(Rc::clone(objs));
        }
        let objs = self.materialize_uncached(id)?;
        self.colls.insert(id, Rc::clone(&objs));
        Ok(objs)
    }

    fn materialize_uncached(&mut self, id: CollectionId) -> EvalResult<Rc<Vec<EventObject>>> {
        match self.it.hir.table.collection(id) {
            Collection::Base(sym) => {
                let key = self.it.hir.symbols.key(*sym);
                if key == adl_sema::ext::MET_FAMILY_KEY {
                    return self.err(
                        Span::default(),
                        "the MET family is an event vector, not an object list",
                    );
                }
                // An absent collection is an empty one: events can
                // legitimately have zero objects of a kind.
                Ok(Rc::new(
                    self.event.collections.get(key).cloned().unwrap_or_default(),
                ))
            }
            Collection::Filtered { parent, pred } => {
                let (parent, pred) = (*parent, *pred);
                let source = self.materialize(parent)?;
                let pred_node: &'h HNode = &self.it.hir.elem_pred(pred).node;
                let mut kept = Vec::new();
                for obj in source.iter() {
                    // Per-element predicate, the element is the implicit
                    // subject; order preserved (§4.2).
                    if self.truth(pred_node, Some(obj))? {
                        kept.push(obj.clone());
                    }
                }
                Ok(Rc::new(kept))
            }
            Collection::Union(parts) => {
                let parts = parts.clone();
                let mut all = Vec::new();
                for part in parts {
                    all.extend(self.materialize(part)?.iter().cloned());
                }
                Ok(Rc::new(all))
            }
            Collection::Combination { .. } => self.err(
                Span::default(),
                "combinatorial composite (COMB) is outside the checked fragment",
            ),
        }
    }

    /// Human label for a collection (first bound name, else base name).
    fn coll_label(&self, id: CollectionId) -> String {
        let names = &self.it.hir.coll_names[id.0 as usize];
        if let Some(sym) = names.first() {
            return self.it.hir.symbols.display(*sym).to_owned();
        }
        match self.it.hir.table.collection(id) {
            Collection::Base(sym) => self.it.hir.symbols.display(*sym).to_owned(),
            _ => format!("{id}"),
        }
    }
}
