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
    AngKind, CombAxis, CombKind, Collection, CollectionId, CompositeCandidate, ElemIndex,
    ElemPredId, ExtDecls, Fragment, HKind, HNode, Hir, HirRegionStmt, ParticleRef, PropId, Quantity,
    QuantityArg, QuantityId, ReduceKind, ScalarSource, Symbol, SortDir, SortKey,
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
    MissingElement {
        collection: String,
        index: ElemIndex,
    },
    /// Object lacks the requested property.
    MissingProperty { property: String },
    /// `min`/`max` over an empty collection: no extremum exists, so the
    /// enclosing comparison is false (the empty-collection convention).
    EmptyReduction { kind: &'static str },
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
            NonValue::EmptyReduction { kind } => {
                write!(f, "`{kind}` over an empty collection has no value")
            }
        }
    }
}

/// Concrete 0-based position of `index` within a collection of `len`
/// elements, or `None` when it falls outside it. `[0]` is the first element
/// and `[-1]` the last (`[-k]` ⇒ position `len - k`, defined iff `len >= k`).
fn elem_position(index: ElemIndex, len: usize) -> Option<usize> {
    match index {
        ElemIndex::FromFront(i) => {
            let i = i as usize;
            (i < len).then_some(i)
        }
        ElemIndex::FromBack(k) => {
            let k = k as usize;
            (k >= 1 && k <= len).then(|| len - k)
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

/// Outcome of one membership-affecting statement during a traced region
/// walk (SPEC_EVENT_PIPELINE §2 cutflows). The walk short-circuits, so a
/// trace covers exactly the statements the event reached.
#[derive(Debug, Clone, PartialEq)]
pub struct StepEval {
    /// Index into the region's `stmts`.
    pub stmt: usize,
    /// `Ok(true)`: survived; `Ok(false)`: failed (walk stops);
    /// `Err`: hard evaluation error — the event counts as failing here
    /// (a faithful diagnostic, never a guessed pass; walk stops).
    pub outcome: Result<bool, EvalError>,
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
    pt_key: String,
    mass_key: String,
}

impl<'h> Interp<'h> {
    #[must_use]
    pub fn new(hir: &'h Hir, ext: &'h ExtDecls) -> Self {
        Self {
            hir,
            ext,
            eta_key: ext.prop_canon("eta").0,
            phi_key: ext.prop_canon("phi").0,
            pt_key: ext.prop_canon("pt").0,
            mass_key: ext.prop_canon("mass").0,
        }
    }

    #[must_use]
    pub fn hir(&self) -> &'h Hir {
        self.hir
    }

    /// The external declarations this interpreter resolves against — the
    /// same `ExtDecls` the streaming event reader needs (event parsing
    /// uses its canonicalization maps).
    #[must_use]
    pub fn ext(&self) -> &'h ExtDecls {
        self.ext
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

    /// Non-short-circuiting region membership, for witness validation.
    ///
    /// Same return type and pass/fail meaning as [`Self::eval_region_by_name`]
    /// (`Ok(true)` passes, `Ok(false)` is rejected, `Err` is a blocking
    /// error), but a decidable rejection is reported even when an opaque
    /// (no-reference-interpretation) statement precedes it in source order.
    /// The short-circuiting walk would surface the opaque error first and a
    /// caller could mistake an unsatisfiable region for an opaque "candidate"
    /// overlap; this method evaluates every statement and prefers a decidable
    /// `Ok(false)` over any error.
    ///
    /// # Errors
    /// Returns an [`EvalError`] for unknown regions, or the first blocking
    /// evaluation error when no statement decidably rejects the event.
    pub fn eval_region_membership(&self, name: &str, event: &Event) -> EvalResult<bool> {
        let idx = self.region_index(name).ok_or_else(|| EvalError {
            span: Span::default(),
            reason: format!("no region named `{name}`"),
        })?;
        Ev::new(self, event).region3(idx).into_result()
    }

    /// Non-short-circuiting region membership by region INDEX (not name).
    ///
    /// Resolving by index is collision-proof: name-based lookup returns the
    /// first match, so duplicate region names (e.g. same-basename units merged
    /// for cross-file analysis) would mask one region's cuts and could fabricate
    /// a "validated" overlap. Witness re-validation must use this. Semantics are
    /// otherwise those of [`Self::eval_region_membership`].
    ///
    /// # Errors
    /// Returns an [`EvalError`] for an out-of-range index, or the first blocking
    /// evaluation error when no statement decidably rejects the event.
    pub fn eval_region_membership_idx(&self, idx: usize, event: &Event) -> EvalResult<bool> {
        if idx >= self.hir.regions.len() {
            return Err(EvalError {
                span: Span::default(),
                reason: format!("region index {idx} out of range"),
            });
        }
        Ev::new(self, event).region3(idx).into_result()
    }

    /// Evaluate every region (in declaration order) plus bin assignments.
    #[must_use]
    pub fn run_event(&self, event: &Event) -> Vec<RegionResult> {
        self.run_event_traced(event).0
    }

    /// [`Self::run_event`] plus, per region, the per-statement trace of
    /// the membership walk (one [`StepEval`] per membership-affecting
    /// statement reached, in declaration order) — the cutflow input
    /// (SPEC_EVENT_PIPELINE §2). The evaluation sequence is identical to
    /// the untraced path: same short-circuiting, same memoization.
    #[must_use]
    pub fn run_event_traced(&self, event: &Event) -> (Vec<RegionResult>, Vec<Vec<StepEval>>) {
        let mut ev = Ev::new(self, event);
        let mut results = Vec::with_capacity(self.hir.regions.len());
        let mut traces = Vec::with_capacity(self.hir.regions.len());
        for (idx, region) in self.hir.regions.iter().enumerate() {
            let (pass, steps) = ev.region_traced(idx);
            let bins = if pass == Ok(true) {
                self.region_bins(&mut ev, idx)
            } else {
                Vec::new()
            };
            results.push(RegionResult {
                name: self.hir.symbols.display(region.name).to_owned(),
                pass,
                bins,
            });
            traces.push(steps);
        }
        (results, traces)
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

/// 4-vector getter for an external function name applied to a single
/// particle argument (`mass`/`m`, `pt`, `eta`, `phi`, `e`/`energy`).
fn lv_getter(name: &str) -> Option<fn(LV) -> NumRes> {
    match name {
        "mass" | "m" => Some(LV::mass),
        "pt" => Some(LV::pt),
        "eta" => Some(LV::eta),
        "phi" => Some(LV::phi),
        "e" | "energy" => Some(LV::energy),
        _ => None,
    }
}

/// Single-argument scalar function over a real (transcendental/irrational).
/// Irrational results are honest f64; a non-finite output fails the cut via
/// `fin` at the call site.
fn unary_real_fn(name: &str) -> Option<fn(f64) -> f64> {
    match name {
        "sqrt" => Some(f64::sqrt),
        "cos" => Some(f64::cos),
        "sin" => Some(f64::sin),
        "tan" => Some(f64::tan),
        "log" => Some(f64::ln),
        _ => None,
    }
}

/// Enumerate the index tuples of a composite over its slot sizes.
///
/// - **Cartesian**: the full ordered product (cross-collection repeats
///   included), in row-major slot order.
/// - **Disjoint** (USER ANSWER 4): unordered tuples. Slots sharing the SAME
///   source collection are enumerated as strictly-increasing index tuples
///   (unordered, no repeated slot index); cross-source slots keep the ordered
///   product. The kinematic value-distinctness drop is applied by the caller.
fn enumerate_index_tuples(
    kind: CombKind,
    parts: &[CollectionId],
    sizes: &[usize],
) -> Vec<Vec<usize>> {
    if parts.is_empty() {
        return Vec::new();
    }
    let mut tuples: Vec<Vec<usize>> = vec![Vec::new()];
    for &n in sizes {
        let mut next = Vec::new();
        for t in &tuples {
            for i in 0..n {
                let mut nt = t.clone();
                nt.push(i);
                next.push(nt);
            }
        }
        tuples = next;
    }
    match kind {
        CombKind::Cartesian => tuples,
        CombKind::Disjoint => {
            let same_source = parts.windows(2).all(|w| w[0] == w[1]);
            if same_source {
                tuples
                    .into_iter()
                    .filter(|idxs| idxs.windows(2).all(|w| w[0] < w[1]))
                    .collect()
            } else {
                tuples
            }
        }
    }
}

/// Swap the sides of a comparison operator (`a ⋈ b` ⇔ `b ⋈̄ a`).
fn flip_cmp(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Gt => CmpOp::Lt,
        CmpOp::Lt => CmpOp::Gt,
        CmpOp::Ge => CmpOp::Le,
        CmpOp::Le => CmpOp::Ge,
        other => other,
    }
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

/// A Lorentz 4-vector in Cartesian components, lifted from `(pt, eta, phi, m)`
/// for 4-vector-sum arithmetic (`mass(l1 + l2)`). All getters are
/// `fin()`-wrapped: a non-finite result fails the enclosing comparison.
#[derive(Debug, Clone, Copy)]
struct LV {
    px: f64,
    py: f64,
    pz: f64,
    e: f64,
}

impl LV {
    /// Lift `(pt, eta, phi, m)` to Cartesian: `px = pt·cosφ`, `py = pt·sinφ`,
    /// `pz = pt·sinhη`, `E = √(px² + py² + pz² + m²)`.
    fn from_ptetaphim(pt: f64, eta: f64, phi: f64, m: f64) -> LV {
        let px = pt * phi.cos();
        let py = pt * phi.sin();
        let pz = pt * eta.sinh();
        let e = (px * px + py * py + pz * pz + m * m).sqrt();
        LV { px, py, pz, e }
    }

    /// A massless transverse vector (`pz = 0`, `E = pt`) — the MET summand,
    /// which carries no `eta`/mass.
    fn transverse(pt: f64, phi: f64) -> LV {
        LV {
            px: pt * phi.cos(),
            py: pt * phi.sin(),
            pz: 0.0,
            e: pt,
        }
    }

    fn add(self, o: LV) -> LV {
        LV {
            px: self.px + o.px,
            py: self.py + o.py,
            pz: self.pz + o.pz,
            e: self.e + o.e,
        }
    }

    /// Invariant mass `√(max(0, E² − |p|²))` (tiny-negative clamped to 0).
    fn mass(self) -> NumRes {
        let p2 = self.px * self.px + self.py * self.py + self.pz * self.pz;
        fin((self.e * self.e - p2).max(0.0).sqrt())
    }

    fn pt(self) -> NumRes {
        fin(self.px.hypot(self.py))
    }

    fn phi(self) -> NumRes {
        fin(self.py.atan2(self.px))
    }

    fn eta(self) -> NumRes {
        let pt = self.px.hypot(self.py);
        fin((self.pz / pt).asinh())
    }

    fn energy(self) -> NumRes {
        fin(self.e)
    }
}

/// Per-event evaluation state: memoized collections and region verdicts.
/// Three-valued (Kleene) truth, used only by the non-short-circuiting
/// membership evaluation ([`Ev::region3`]/[`Ev::truth3`]): `Unknown` carries
/// the blocking reason so the witness layer can classify it (opaque vs
/// missing-data) exactly as it did the two-valued `Err`.
enum Tri {
    True,
    False,
    Unknown(EvalError),
}

impl Tri {
    fn from_bool(b: bool) -> Self {
        if b { Tri::True } else { Tri::False }
    }

    fn not(self) -> Self {
        match self {
            Tri::True => Tri::False,
            Tri::False => Tri::True,
            unknown @ Tri::Unknown(_) => unknown,
        }
    }

    fn into_result(self) -> EvalResult<bool> {
        match self {
            Tri::True => Ok(true),
            Tri::False => Ok(false),
            Tri::Unknown(e) => Err(e),
        }
    }
}

/// One surviving composite tuple: the per-slot binder elements (keyed by
/// binder symbol) and the candidate 4-vector object, if the block declared one.
#[derive(Clone)]
struct CombTuple {
    binders: HashMap<Symbol, EventObject>,
    candidate: Option<EventObject>,
}

struct Ev<'h, 'e> {
    it: &'e Interp<'h>,
    event: &'e Event,
    colls: HashMap<CollectionId, Rc<Vec<EventObject>>>,
    /// Surviving tuples of a composite (post-cut), cached per combination id.
    comb_tuples: HashMap<CollectionId, Rc<Vec<CombTuple>>>,
    regions: HashMap<usize, Result<bool, EvalError>>,
    /// Active reducer iteration elements (innermost last). A reducer body's
    /// [`ParticleRef::ReduceElem`] / [`HKind::ReduceProp`] reads the top.
    reduce_stack: Vec<EventObject>,
    /// Active composite binder environment (binder symbol → bound element),
    /// pushed while evaluating a per-tuple cut or candidate body.
    binder_env: HashMap<Symbol, EventObject>,
}

impl<'h, 'e> Ev<'h, 'e> {
    fn new(it: &'e Interp<'h>, event: &'e Event) -> Self {
        Self {
            it,
            event,
            colls: HashMap::new(),
            comb_tuples: HashMap::new(),
            regions: HashMap::new(),
            reduce_stack: Vec::new(),
            binder_env: HashMap::new(),
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
        let mut trace = Vec::new();
        let result = self.region_walk(idx, &mut trace);
        self.regions.insert(idx, result.clone());
        result
    }

    /// [`Self::region`] with the per-statement trace exposed (the cutflow
    /// input). Always re-walks — callers invoke it once per region per
    /// event, before any cross-region reference can have cached it; the
    /// final verdict lands in the cache either way.
    fn region_traced(&mut self, idx: usize) -> (EvalResult<bool>, Vec<StepEval>) {
        let mut trace = Vec::new();
        let result = self.region_walk(idx, &mut trace);
        self.regions.insert(idx, result.clone());
        (result, trace)
    }

    /// The one membership walk (single source of truth for §4.3 and the
    /// §2 cutflow): evaluates statements in order, records one
    /// [`StepEval`] per membership-affecting statement reached, and
    /// short-circuits on the first failure or hard error.
    fn region_walk(&mut self, idx: usize, trace: &mut Vec<StepEval>) -> EvalResult<bool> {
        let region = &self.it.hir.regions[idx];
        for (i, stmt) in region.stmts.iter().enumerate() {
            let outcome = match stmt {
                HirRegionStmt::Select(n) | HirRegionStmt::Trigger(n) => self.truth(n, None),
                HirRegionStmt::Reject(n) => self.truth(n, None).map(|c| !c),
                HirRegionStmt::Inherit { region, .. } => self.region(*region),
                // Bins partition; they never constrain membership (§4.3).
                HirRegionStmt::Bin { .. } | HirRegionStmt::BinCond { .. } => continue,
                HirRegionStmt::NonMembership { tag, span, .. } => {
                    if let Fragment::Unsupported(reason) = tag {
                        return self.err(*span, format!("cannot evaluate region: {reason}"));
                    }
                    continue;
                }
            };
            trace.push(StepEval {
                stmt: i,
                outcome: outcome.clone(),
            });
            match outcome {
                Ok(true) => {}
                Ok(false) => return Ok(false),
                Err(e) => return Err(e),
            }
        }
        Ok(true)
    }

    /// Three-valued (Kleene) region membership, for witness validation.
    ///
    /// Unlike [`Self::region_walk`], evaluation never short-circuits on a
    /// blocking value: a decidable rejection (`False`) is preferred over any
    /// `Unknown` (opaque / out-of-fragment / missing data) *anywhere* in the
    /// boolean structure — across statements, inside `and`/`or`/`not`/ternary,
    /// across `Inherit` edges, and across `<region>` (`RegionPred`) references
    /// (which the short-circuiting [`Self::region`] would surface as an opaque
    /// error first). This is the soundness-critical property for witness
    /// validation: a cut that definitively fails must be observed even when an
    /// opaque term is evaluated first, or an unsatisfiable region is mistaken
    /// for an opaque "candidate" overlap and the pair is falsely PROVEN
    /// OVERLAPPING. When nothing decidably rejects, the *first* `Unknown` (in
    /// evaluation order) is returned, preserving the error ordering the witness
    /// layer's patch-and-retry on missing event data relies on to converge.
    ///
    /// Membership of a region is the conjunction of its statements, so it is
    /// `False` if any statement is decidably `False`, else `Unknown` if any is
    /// `Unknown`, else `True`. The `regions` short-circuit cache is never
    /// consulted (this walk has different — sound — short-circuit rules).
    fn region3(&mut self, idx: usize) -> Tri {
        let mut unknown: Option<EvalError> = None;
        let region = &self.it.hir.regions[idx];
        for stmt in &region.stmts {
            let t = match stmt {
                HirRegionStmt::Select(n) | HirRegionStmt::Trigger(n) => self.truth3(n, None),
                HirRegionStmt::Reject(n) => self.truth3(n, None).not(),
                HirRegionStmt::Inherit { region, .. } => self.region3(*region),
                HirRegionStmt::Bin { .. } | HirRegionStmt::BinCond { .. } => continue,
                HirRegionStmt::NonMembership { tag, span, .. } => {
                    if let Fragment::Unsupported(reason) = tag {
                        Tri::Unknown(EvalError {
                            span: *span,
                            reason: format!("cannot evaluate region: {reason}"),
                        })
                    } else {
                        continue;
                    }
                }
            };
            match t {
                Tri::True => {}
                Tri::False => return Tri::False,
                Tri::Unknown(e) => {
                    if unknown.is_none() {
                        unknown = Some(e);
                    }
                }
            }
        }
        unknown.map_or(Tri::True, Tri::Unknown)
    }

    /// Three-valued (Kleene) sibling of [`Self::truth`]: boolean connectives
    /// are evaluated so a decisive `False`/`True` is never hidden behind an
    /// `Unknown` sub-term (`False ∧ Unknown = False`, `True ∨ Unknown = True`).
    /// A `RegionPred` recurses through [`Self::region3`] (NOT the
    /// short-circuiting [`Self::region`]), and comparisons / bands /
    /// numeric-as-predicate nodes evaluate their operands through the
    /// three-valued [`Self::num3`] — so a region used as a *number*
    /// (`<region> == 1`, `<region> [] lo hi`, `<region> + x > y`) is likewise
    /// never masked. Genuine leaves with no inner `RegionPred` delegate to the
    /// two-valued [`Self::truth`], whose `Err` becomes `Unknown`.
    fn truth3(&mut self, node: &'h HNode, elem: Option<&EventObject>) -> Tri {
        if let Fragment::Unsupported(reason) = &node.tag {
            return Tri::Unknown(EvalError {
                span: node.span,
                reason: reason.clone(),
            });
        }
        match &node.kind {
            HKind::Bool(b) => Tri::from_bool(*b),
            HKind::Not(a) => self.truth3(a, elem).not(),
            HKind::And(parts) => {
                let mut unknown = None;
                for p in parts {
                    match self.truth3(p, elem) {
                        Tri::True => {}
                        Tri::False => return Tri::False,
                        Tri::Unknown(e) => {
                            if unknown.is_none() {
                                unknown = Some(e);
                            }
                        }
                    }
                }
                unknown.map_or(Tri::True, Tri::Unknown)
            }
            HKind::Or(parts) => {
                let mut unknown = None;
                for p in parts {
                    match self.truth3(p, elem) {
                        Tri::False => {}
                        Tri::True => return Tri::True,
                        Tri::Unknown(e) => {
                            if unknown.is_none() {
                                unknown = Some(e);
                            }
                        }
                    }
                }
                unknown.map_or(Tri::False, Tri::Unknown)
            }
            // `g ? a : b`: decide via the guard. An undecidable guard is still
            // decidable when BOTH branches agree (the guard is then irrelevant:
            // `U?F:F = F`, `U?T:T = T`) — otherwise Unknown. Missing else is
            // `true` (§4.4).
            HKind::Ternary { guard, then, els } => match self.truth3(guard, elem) {
                Tri::True => self.truth3(then, elem),
                Tri::False => match els {
                    Some(e) => self.truth3(e, elem),
                    None => Tri::True,
                },
                Tri::Unknown(e) => {
                    let then_t = self.truth3(then, elem);
                    let else_t = match els {
                        Some(e2) => self.truth3(e2, elem),
                        None => Tri::True,
                    };
                    match (then_t, else_t) {
                        (Tri::False, Tri::False) => Tri::False,
                        (Tri::True, Tri::True) => Tri::True,
                        _ => Tri::Unknown(e),
                    }
                }
            },
            HKind::RegionPred(idx) => self.region3(*idx),
            // Boolean reducer (`any`/`all`): the Kleene fold is the truth value.
            HKind::Reduce { kind, coll, body, .. } if kind.is_boolean() => {
                self.reduce_bool(*kind, *coll, body, elem)
            }
            // §4.4: a soft non-value on EITHER side makes the comparison
            // false unconditionally — that already decides the cut, so it
            // wins even when the other operand is a blocking Unknown (checked
            // first, before the Err -> Unknown arm). A blocking operand with
            // no soft non-value makes the comparison Unknown.
            HKind::Cmp { op, lhs, rhs } => {
                match self.angular_whole_cmp(*op, lhs, rhs, node.span, elem) {
                    Ok(Some(r)) => Tri::from_bool(r),
                    Err(e) => Tri::Unknown(e),
                    Ok(None) => match (self.num3(lhs, elem), self.num3(rhs, elem)) {
                        (Ok(Err(_)), _) | (_, Ok(Err(_))) => Tri::False,
                        (Err(e), _) | (_, Err(e)) => Tri::Unknown(e),
                        (Ok(Ok(a)), Ok(Ok(b))) => Tri::from_bool(compare(*op, a, b)),
                    },
                }
            }
            HKind::Band { kind, expr, lo, hi } => match self.num3(expr, elem) {
                Err(e) => Tri::Unknown(e),
                Ok(Err(_)) => Tri::False,
                Ok(Ok(v)) => {
                    let lo = match parse_num(lo, node.span) {
                        Ok(x) => x,
                        Err(e) => return Tri::Unknown(e),
                    };
                    let hi = match parse_num(hi, node.span) {
                        Ok(x) => x,
                        Err(e) => return Tri::Unknown(e),
                    };
                    Tri::from_bool(match kind {
                        BandKind::In => lo <= v && v <= hi,
                        BandKind::Out => v <= lo || v >= hi,
                    })
                }
            },
            // Numeric value used as a predicate: nonzero is true; a soft
            // non-value fails the cut; a blocking operand is Unknown. Routed
            // through num3 because Neg/Abs/Binary can nest a RegionPred.
            HKind::Num(_)
            | HKind::Quantity(_)
            | HKind::ElemSelfProp(_)
            | HKind::ReduceProp(_)
            | HKind::Reduce { .. }
            | HKind::Neg(_)
            | HKind::Abs(_)
            | HKind::ScalarMinMax { .. }
            | HKind::Binary { .. } => match self.num3(node, elem) {
                Err(e) => Tri::Unknown(e),
                Ok(Ok(v)) => Tri::from_bool(v != 0.0),
                Ok(Err(_)) => Tri::False,
            },
            // Genuine leaves / out-of-fragment: no inner RegionPred to mask;
            // two-valued truth yields the decided value or an Err -> Unknown.
            HKind::CollProp { .. }
            | HKind::Particle(_)
            | HKind::CollValue(_)
            | HKind::Unsupported => match self.truth(node, elem) {
                Ok(true) => Tri::True,
                Ok(false) => Tri::False,
                Err(e) => Tri::Unknown(e),
            },
        }
    }

    /// Three-valued numeric sibling of [`Self::num`], used only by membership
    /// validation. Identical arithmetic to [`Self::num`] except that every
    /// node which can carry a `RegionPred` (directly, or nested under
    /// `Neg`/`Abs`/`Binary`/`Ternary`, or as a boolean-valued operand) routes
    /// through [`Self::truth3`]/[`Self::region3`] so a region used as a number
    /// with a *decidable* value (`0.0`/`1.0`) is never masked into an opaque
    /// `Err`. Leaf nodes that cannot contain a `RegionPred` (literals,
    /// quantities, externals, angles, element props) delegate to the
    /// two-valued [`Self::num`], which is exact for them.
    fn num3(&mut self, node: &'h HNode, elem: Option<&EventObject>) -> EvalResult<NumRes> {
        if let Fragment::Unsupported(reason) = &node.tag {
            return self.err(node.span, reason.clone());
        }
        match &node.kind {
            HKind::Neg(a) => Ok(self.num3(a, elem)?.map(|v| -v)),
            HKind::Abs(a) => Ok(self.num3(a, elem)?.map(f64::abs)),
            HKind::Binary { op, lhs, rhs } => {
                // §4.4: a soft non-value is ABSORBING in arithmetic — it
                // propagates even past a blocking (opaque) operand. Checked
                // before the blocking Err so a cut over a missing element stays
                // a decidable rejection (`softNV * opaque > k` is False) rather
                // than being masked into Unknown. (Two-valued `num` `?`-aborts
                // on the blocking operand first; this is the sound refinement.)
                let (a, b) = match (self.num3(lhs, elem), self.num3(rhs, elem)) {
                    (Ok(Err(nv)), _) | (_, Ok(Err(nv))) => return Ok(Err(nv)),
                    (Err(e), _) | (_, Err(e)) => return Err(e),
                    (Ok(Ok(a)), Ok(Ok(b))) => (a, b),
                };
                let v = match op {
                    adl_sema::ArithOp::Add => a + b,
                    adl_sema::ArithOp::Sub => a - b,
                    adl_sema::ArithOp::Mul => a * b,
                    adl_sema::ArithOp::Div => a / b,
                    adl_sema::ArithOp::Pow => a.powf(b),
                };
                Ok(fin(v))
            }
            HKind::ScalarMinMax { kind, args } => {
                let mut acc: Option<f64> = None;
                for a in args {
                    let v = match self.num3(a, elem)? {
                        Ok(v) => v,
                        Err(nv) => return Ok(Err(nv)), // soft non-value is absorbing
                    };
                    acc = Some(match acc {
                        None => v,
                        Some(p) if matches!(kind, ReduceKind::Min) => p.min(v),
                        Some(p) => p.max(v),
                    });
                }
                match acc {
                    Some(v) => Ok(fin(v)),
                    None => Ok(Err(NonValue::EmptyReduction { kind: kind.as_str() })),
                }
            }
            HKind::Ternary { guard, then, els } => match self.truth3(guard, elem) {
                Tri::True => self.num3(then, elem),
                Tri::False => match els {
                    Some(e) => self.num3(e, elem),
                    None => Ok(Ok(1.0)),
                },
                // Undecidable guard: still decidable when both branches yield
                // the same value (or both the same kind of soft non-value), so
                // a `g?v:v` feeding a comparison is not masked into Unknown.
                //
                // RESIDUAL (documented, sound): when the branches differ
                // numerically but the *enclosing* comparison would be False on
                // both (e.g. `(opaque ? missing : 5) > 1000`), this returns the
                // guard's Unknown rather than the decidable False — closing it
                // requires distributing the comparison over the branches
                // (supervaluation / guard path-splitting), which three-valued
                // Kleene evaluation does not do. This is strictly conservative:
                // it can only weaken a verdict to POSSIBLY / a caveated overlap
                // candidate (the accepted `OVERLAP_CAVEAT` behavior), never
                // turn a non-passing witness into a passing one.
                Tri::Unknown(e) => {
                    let then_v = self.num3(then, elem)?;
                    let else_v = match els {
                        Some(e2) => self.num3(e2, elem)?,
                        None => Ok(1.0),
                    };
                    match (then_v, else_v) {
                        (Ok(a), Ok(b)) if a == b => Ok(Ok(a)),
                        (Err(nv), Err(_)) => Ok(Err(nv)),
                        _ => Err(e),
                    }
                }
            },
            // Boolean-valued nodes used numerically -> 1.0/0.0, three-valued.
            HKind::Not(_)
            | HKind::And(_)
            | HKind::Or(_)
            | HKind::Cmp { .. }
            | HKind::Band { .. }
            | HKind::RegionPred(_) => match self.truth3(node, elem) {
                Tri::True => Ok(Ok(1.0)),
                Tri::False => Ok(Ok(0.0)),
                Tri::Unknown(e) => Err(e),
            },
            // Boolean reducer used as a number ⇒ 1.0/0.0, three-valued.
            HKind::Reduce { kind, .. } if kind.is_boolean() => match self.truth3(node, elem) {
                Tri::True => Ok(Ok(1.0)),
                Tri::False => Ok(Ok(0.0)),
                Tri::Unknown(e) => Err(e),
            },
            // Numeric reducer: the Kleene fold (uses num3 on the body).
            HKind::Reduce { kind, coll, body, .. } => self.reduce_num(*kind, *coll, body, elem),
            // Leaves with no inner RegionPred: the exact two-valued evaluator.
            HKind::Num(_)
            | HKind::Bool(_)
            | HKind::Quantity(_)
            | HKind::ElemSelfProp(_)
            | HKind::ReduceProp(_)
            | HKind::CollProp { .. }
            | HKind::Particle(_)
            | HKind::CollValue(_)
            | HKind::Unsupported => self.num(node, elem),
        }
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
            HKind::Cmp { op, lhs, rhs } => {
                if let Some(r) = self.angular_whole_cmp(*op, lhs, rhs, node.span, elem)? {
                    return Ok(r);
                }
                match (self.num(lhs, elem)?, self.num(rhs, elem)?) {
                    (Ok(a), Ok(b)) => Ok(compare(*op, a, b)),
                    _ => Ok(false),
                }
            }
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
            // Boolean reducer (`any`/`all`) as a predicate.
            HKind::Reduce { kind, coll, body, .. } if kind.is_boolean() => {
                self.reduce_bool(*kind, *coll, body, elem).into_result()
            }
            HKind::Num(_)
            | HKind::Quantity(_)
            | HKind::ElemSelfProp(_)
            | HKind::ReduceProp(_)
            | HKind::Reduce { .. }
            | HKind::Neg(_)
            | HKind::Abs(_)
            | HKind::ScalarMinMax { .. }
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
            HKind::Quantity(q) => self.quantity(*q, node.span, elem),
            HKind::ElemSelfProp(prop) => {
                let Some(obj) = elem else {
                    return self.err(
                        node.span,
                        "implicit element property used outside an object block",
                    );
                };
                Ok(self.object_prop(obj, *prop))
            }
            HKind::ReduceProp(prop) => self.reduce_prop(node.span, *prop),
            // A numeric reducer used as a value (`sum`/`min`/`max`). A boolean
            // reducer used numerically is 1.0/0.0 via `truth`.
            HKind::Reduce { kind, coll, body, .. } if !kind.is_boolean() => {
                self.reduce_num(*kind, *coll, body, elem)
            }
            HKind::Reduce { .. } => Ok(Ok(f64::from(self.truth(node, elem)?))),
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
            // Scalar n-ary min/max: fold the arg values; a missing-element arg
            // is a non-value that makes the enclosing comparison false (§4.4).
            HKind::ScalarMinMax { kind, args } => {
                let mut acc: Option<f64> = None;
                for a in args {
                    let v = match self.num(a, elem)? {
                        Ok(v) => v,
                        Err(nv) => return Ok(Err(nv)),
                    };
                    acc = Some(match acc {
                        None => v,
                        Some(p) if matches!(kind, ReduceKind::Min) => p.min(v),
                        Some(p) => p.max(v),
                    });
                }
                match acc {
                    Some(v) => Ok(fin(v)),
                    None => Ok(Err(NonValue::EmptyReduction { kind: kind.as_str() })),
                }
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

    /// Property of the reducer iteration element by `PropId`.
    fn reduce_prop(&self, span: Span, prop: PropId) -> EvalResult<NumRes> {
        match self.reduce_stack.last() {
            Some(obj) => Ok(self.object_prop(obj, prop)),
            None => self.err(span, "reducer element property used outside a reducer body"),
        }
    }

    /// Kinematic value-equality (USER ANSWER 4): two elements are "the same"
    /// iff their canonical `(pt, eta, phi, mass)` agree (a missing component on
    /// one must be missing on the other too).
    fn kinematic_eq(&self, a: &EventObject, b: &EventObject) -> bool {
        [
            &self.it.pt_key,
            &self.it.eta_key,
            &self.it.phi_key,
            &self.it.mass_key,
        ]
        .iter()
        .all(|k| a.get(k) == b.get(k))
    }

    /// Eta/phi components of an event object (either may be absent).
    fn obj_angles(&self, obj: &EventObject) -> Angles {
        Angles {
            eta: obj.get(&self.it.eta_key),
            phi: obj.get(&self.it.phi_key),
        }
    }

    /// Lift an event object to a Lorentz 4-vector from its `(pt, eta, phi,
    /// mass)`. A missing `pt`/`eta`/`phi` is a soft non-value (the
    /// comparison fails); a missing `mass` is `MissingProperty` (we never
    /// assume massless).
    fn obj_lorentz(&self, obj: &EventObject) -> Result<LV, NonValue> {
        let need = |key: &str, name: &str| {
            obj.get(key).ok_or_else(|| NonValue::MissingProperty {
                property: name.to_owned(),
            })
        };
        let pt = need(&self.it.pt_key, "pt")?;
        let eta = need(&self.it.eta_key, "eta")?;
        let phi = need(&self.it.phi_key, "phi")?;
        let m = need(&self.it.mass_key, "mass")?;
        Ok(LV::from_ptetaphim(pt, eta, phi, m))
    }

    /// Build the Lorentz 4-vector of a particle reference (sum, reducer
    /// element, `this`, indexed element, or MET summand).
    fn lorentz(
        &mut self,
        p: &ParticleRef,
        span: Span,
        elem: Option<&EventObject>,
    ) -> EvalResult<Result<LV, NonValue>> {
        match p {
            ParticleRef::Sum(parts) => {
                let parts = parts.clone();
                let mut acc: Option<LV> = None;
                for part in &parts {
                    let lv = match self.lorentz(part, span, elem)? {
                        Ok(lv) => lv,
                        Err(nv) => return Ok(Err(nv)),
                    };
                    acc = Some(acc.map_or(lv, |a| a.add(lv)));
                }
                match acc {
                    Some(lv) => Ok(Ok(lv)),
                    None => self.err(span, "empty 4-vector sum"),
                }
            }
            ParticleRef::Elem { coll, index } => {
                let (coll, index) = (*coll, *index);
                let objs = self.materialize(coll)?;
                match elem_position(index, objs.len()) {
                    Some(pos) => Ok(self.obj_lorentz(&objs[pos])),
                    None => Ok(Err(NonValue::MissingElement {
                        collection: self.coll_label(coll),
                        index,
                    })),
                }
            }
            ParticleRef::ReduceElem => match self.reduce_stack.last() {
                Some(obj) => Ok(self.obj_lorentz(obj)),
                None => self.err(span, "reducer element used outside a reducer body"),
            },
            ParticleRef::ThisElem => match elem {
                Some(obj) => Ok(self.obj_lorentz(obj)),
                None => self.err(span, "`this` used outside an object block"),
            },
            // MET summand: massless transverse vector (pz = 0, E = pt).
            ParticleRef::Met => {
                if self.event.met.is_empty() {
                    return self.err(span, "event has no MET vector");
                }
                let pt = self.event.met.get(&self.it.pt_key).copied();
                let phi = self.event.met.get(&self.it.phi_key).copied();
                match (pt, phi) {
                    (Some(pt), Some(phi)) => Ok(Ok(LV::transverse(pt, phi))),
                    _ => Ok(Err(NonValue::MissingProperty {
                        property: "MET pt/phi".to_owned(),
                    })),
                }
            }
            // A composite binder (`l1` inside a tuple): the bound element's
            // Lorentz vector, read from the active binder environment.
            ParticleRef::Binder { name, .. } => match self.binder_env.get(name) {
                Some(obj) => Ok(self.obj_lorentz(obj)),
                None => self.err(span, "composite binder used outside a tuple environment"),
            },
            ParticleRef::Whole(_) => self.err(
                span,
                "4-vector over an unindexed collection is unsupported",
            ),
        }
    }

    // ---- reducers ---------------------------------------------------------

    /// Evaluate a boolean reducer (`any`/`all`) as Kleene three-valued. The
    /// body is evaluated per iteration element with that element pushed onto
    /// the reduce stack; `elem` stays the outer object-filter element.
    /// Empty-collection convention: `any ⇒ false`, `all ⇒ true` (vacuous).
    /// Any element evaluating to `Unknown` poisons the fold to `Unknown`.
    fn reduce_bool(
        &mut self,
        kind: ReduceKind,
        coll: CollectionId,
        body: &'h HNode,
        elem: Option<&EventObject>,
    ) -> Tri {
        let objs = match self.materialize(coll) {
            Ok(o) => o,
            Err(e) => return Tri::Unknown(e),
        };
        let mut unknown: Option<EvalError> = None;
        for obj in objs.iter() {
            self.reduce_stack.push(obj.clone());
            let t = self.truth3(body, elem);
            self.reduce_stack.pop();
            match (kind, t) {
                // any: short-circuit on the first True.
                (ReduceKind::Any, Tri::True) => return Tri::True,
                (ReduceKind::Any, Tri::False) => {}
                // all: short-circuit on the first False.
                (ReduceKind::All, Tri::False) => return Tri::False,
                (ReduceKind::All, Tri::True) => {}
                (_, Tri::Unknown(e)) => {
                    if unknown.is_none() {
                        unknown = Some(e);
                    }
                }
                // Unreachable: numeric kinds never call reduce_bool.
                _ => {}
            }
        }
        // No decisive element: an Unknown anywhere poisons the result;
        // otherwise the vacuous identity (any ⇒ false, all ⇒ true).
        match unknown {
            Some(e) => Tri::Unknown(e),
            None => Tri::from_bool(kind == ReduceKind::All),
        }
    }

    /// Evaluate a numeric reducer (`sum`/`min`/`max`) over `coll`. Empty:
    /// `sum ⇒ 0`, `min`/`max ⇒ EmptyReduction` (comparison-false). Any
    /// element body that is a blocking `Unknown` poisons to a hard error; a
    /// soft non-value element is absorbed per the fold's rule (skipped for
    /// `sum`; propagated as comparison-false for `min`/`max`).
    fn reduce_num(
        &mut self,
        kind: ReduceKind,
        coll: CollectionId,
        body: &'h HNode,
        elem: Option<&EventObject>,
    ) -> EvalResult<NumRes> {
        let objs = self.materialize(coll)?;
        let mut acc: Option<f64> = None;
        let mut sum = 0.0_f64;
        let mut any = false;
        for obj in objs.iter() {
            self.reduce_stack.push(obj.clone());
            let v = self.num3(body, elem);
            self.reduce_stack.pop();
            let v = match v? {
                Ok(v) => v,
                // A soft non-value element: for min/max it makes the whole
                // comparison false (an element with no value); for sum it is
                // absorbing too (the spec's div-by-zero rule extends here).
                Err(nv) => return Ok(Err(nv)),
            };
            any = true;
            sum += v;
            acc = Some(match (kind, acc) {
                (ReduceKind::Min, Some(a)) => a.min(v),
                (ReduceKind::Max, Some(a)) => a.max(v),
                (_, None) => v,
                (_, Some(a)) => a,
            });
        }
        Ok(match kind {
            ReduceKind::Sum => fin(sum),
            ReduceKind::Min | ReduceKind::Max => {
                if any {
                    fin(acc.unwrap_or(0.0))
                } else {
                    Err(NonValue::EmptyReduction {
                        kind: kind.as_str(),
                    })
                }
            }
            // Unreachable: boolean kinds never call reduce_num.
            ReduceKind::Any | ReduceKind::All => fin(sum),
        })
    }

    fn quantity(
        &mut self,
        q: QuantityId,
        span: Span,
        elem: Option<&EventObject>,
    ) -> EvalResult<NumRes> {
        match self.it.hir.table.quantity(q) {
            Quantity::EventScalar(src) => self.event_scalar(src, span),
            Quantity::Size(coll) => {
                let objs = self.materialize(*coll)?;
                #[allow(clippy::cast_precision_loss)] // realistic sizes are tiny
                Ok(Ok(objs.len() as f64))
            }
            Quantity::ElemProp { coll, index, prop } => {
                let (index, coll, prop) = (*index, *coll, *prop);
                let objs = self.materialize(coll)?;
                match elem_position(index, objs.len()) {
                    Some(pos) => Ok(self.object_prop(&objs[pos], prop)),
                    None => Ok(Err(NonValue::MissingElement {
                        collection: self.coll_label(coll),
                        index,
                    })),
                }
            }
            Quantity::AngularSep { kind, a, b, .. } => {
                let (kind, a, b) = (*kind, a.clone(), b.clone());
                self.angular(kind, &a, &b, span, elem)
            }
            Quantity::ExternalFn { name, args } => {
                let fname = self.it.hir.symbols.key(*name).to_owned();
                // 4-vector-sum / element getter: `mass(l1+l2)`, `pt(jet)` inside
                // a reducer body, etc. The single argument is a particle whose
                // Lorentz vector we can build.
                if let [QuantityArg::Particle(p)] = args.as_slice()
                    && let Some(getter) = lv_getter(&fname)
                {
                    let p = p.clone();
                    return Ok(match self.lorentz(&p, span, elem)? {
                        Ok(lv) => getter(lv),
                        Err(nv) => Err(nv),
                    });
                }
                // Single-argument transcendental / irrational scalar functions.
                if let [arg] = args.as_slice()
                    && let Some(f) = unary_real_fn(&fname)
                {
                    let arg = arg.clone();
                    let v = match self.arg_num(&arg, span, elem)? {
                        Ok(v) => v,
                        Err(nv) => return Ok(Err(nv)),
                    };
                    // A non-finite result (e.g. sqrt of a negative) ⇒ the
                    // comparison-false rule via `fin`.
                    return Ok(fin(f(v)));
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

    fn arg_num(
        &mut self,
        arg: &QuantityArg,
        span: Span,
        elem: Option<&EventObject>,
    ) -> EvalResult<NumRes> {
        match arg {
            QuantityArg::Num(text) => Ok(fin(parse_num(text, span)?)),
            QuantityArg::Quantity(q) => self.quantity(*q, span, elem),
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
        elem: Option<&EventObject>,
    ) -> EvalResult<NumRes> {
        let pa = match self.angles(a, span, elem)? {
            Ok(x) => x,
            Err(nv) => return Ok(Err(nv)),
        };
        let pb = match self.angles(b, span, elem)? {
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

    /// OPEN-1 (operator-scoped ∀/∃): if `lhs ⋈ rhs` is an unindexed angular
    /// separation over two WHOLE collections compared to a constant, evaluate
    /// it by folding the FULL Cartesian product A×B — `∀` pairs for `>`/`≥`,
    /// `∃` a pair for `<`/`≤`. A missing-element or non-finite pair counts as
    /// false; an empty product is vacuously ∀-true / ∃-false. Returns `None`
    /// for any other shape (caller falls through to the normal comparison) and
    /// for `==`/`!=` (no monotone reading — matches the encoder's opaque
    /// fall-through).
    fn angular_whole_cmp(
        &mut self,
        op: CmpOp,
        lhs: &'h HNode,
        rhs: &'h HNode,
        span: Span,
        elem: Option<&EventObject>,
    ) -> EvalResult<Option<bool>> {
        let (q, op, other) = if let Some(q) = self.whole_angular(lhs) {
            (q, op, rhs)
        } else if let Some(q) = self.whole_angular(rhs) {
            (q, flip_cmp(op), lhs)
        } else {
            return Ok(None);
        };
        let (kind, a, b) = match self.it.hir.table.quantity(q) {
            Quantity::AngularSep {
                kind,
                a: ParticleRef::Whole(a),
                b: ParticleRef::Whole(b),
                ..
            } => (*kind, *a, *b),
            _ => return Ok(None),
        };
        if matches!(op, CmpOp::Eq | CmpOp::Ne | CmpOp::ApproxEq) {
            return Ok(None);
        }
        let forall = matches!(op, CmpOp::Gt | CmpOp::Ge);
        // A non-value threshold makes the comparison false (§4.4).
        let c = match self.num(other, elem)? {
            Ok(v) => v,
            Err(_) => return Ok(Some(false)),
        };
        let na = self.materialize(a)?.len();
        let nb = self.materialize(b)?.len();
        if na == 0 || nb == 0 {
            return Ok(Some(forall)); // ∀ vacuous-true / ∃ false over ∅
        }
        for i in 0..na {
            for j in 0..nb {
                let pa = ParticleRef::Elem {
                    coll: a,
                    index: ElemIndex::FromFront(i as u32),
                };
                let pb = ParticleRef::Elem {
                    coll: b,
                    index: ElemIndex::FromFront(j as u32),
                };
                let holds = match self.angular(kind, &pa, &pb, span, elem)? {
                    Ok(d) => compare(op, d, c),
                    Err(_) => false, // missing / non-finite pair ⇒ false
                };
                if forall && !holds {
                    return Ok(Some(false));
                }
                if !forall && holds {
                    return Ok(Some(true));
                }
            }
        }
        Ok(Some(forall)) // ∀: all held ⇒ true; ∃: none held ⇒ false
    }

    /// The `AngularSep { Whole, Whole }` quantity of `node`, if it is one.
    fn whole_angular(&self, node: &HNode) -> Option<QuantityId> {
        if let HKind::Quantity(q) = &node.kind
            && matches!(
                self.it.hir.table.quantity(*q),
                Quantity::AngularSep {
                    a: ParticleRef::Whole(_),
                    b: ParticleRef::Whole(_),
                    ..
                }
            )
        {
            return Some(*q);
        }
        None
    }

    fn angles(
        &mut self,
        p: &ParticleRef,
        span: Span,
        elem: Option<&EventObject>,
    ) -> EvalResult<Result<Angles, NonValue>> {
        match p {
            ParticleRef::Elem { coll, index } => {
                let (coll, index) = (*coll, *index);
                let objs = self.materialize(coll)?;
                match elem_position(index, objs.len()) {
                    Some(pos) => Ok(Ok(self.obj_angles(&objs[pos]))),
                    None => Ok(Err(NonValue::MissingElement {
                        collection: self.coll_label(coll),
                        index,
                    })),
                }
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
            // Reducer iteration element (top of the reduce stack).
            ParticleRef::ReduceElem => match self.reduce_stack.last() {
                Some(obj) => Ok(Ok(self.obj_angles(obj))),
                None => self.err(span, "reducer element used outside a reducer body"),
            },
            // The outer object-filter element (`this`) used as a particle.
            ParticleRef::ThisElem => match elem {
                Some(obj) => Ok(Ok(self.obj_angles(obj))),
                None => self.err(span, "`this` used outside an object block"),
            },
            // A 4-vector sum: its eta/phi come from the summed Lorentz vector.
            ParticleRef::Sum(_) => Ok(match self.lorentz(p, span, elem)? {
                Ok(lv) => match (lv.eta(), lv.phi()) {
                    (Ok(eta), Ok(phi)) => Ok(Angles {
                        eta: Some(eta),
                        phi: Some(phi),
                    }),
                    _ => Err(NonValue::NonFinite),
                },
                Err(nv) => Err(nv),
            }),
            ParticleRef::Whole(_) => self.err(
                span,
                "angular separation over an unindexed collection is ambiguous (OPEN-1 unresolved)",
            ),
            // A composite binder: the bound element's angles from the env.
            ParticleRef::Binder { name, .. } => match self.binder_env.get(name) {
                Some(obj) => Ok(Ok(self.obj_angles(obj))),
                None => self.err(span, "composite binder used outside a tuple environment"),
            },
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
            // A re-sorted permutation of the source (same element set, key's
            // order). `Prop` keys re-sort by that property; an `Opaque` key
            // keeps source order (its indexed access is tagged Unsupported at
            // resolve, so a possibly-wrong order is never observed as a value).
            Collection::Sorted { source, key, dir } => {
                let (source, key, dir) = (*source, key.clone(), *dir);
                let src = self.materialize(source)?;
                let mut out = src.as_ref().clone();
                if let SortKey::Prop(prop) = key {
                    let pkey = self.it.hir.table.prop_key(prop).to_owned();
                    // Stable sort by the key property; missing values sort last.
                    // `descend` reverses the comparison. Identity no-op when the
                    // source already has this order (e.g. pt-descending input).
                    out.sort_by(|a, b| {
                        let (va, vb) = (a.get(&pkey), b.get(&pkey));
                        let ord = match (va, vb) {
                            (Some(x), Some(y)) => {
                                x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(_), None) => std::cmp::Ordering::Less,
                            (None, Some(_)) => std::cmp::Ordering::Greater,
                            (None, None) => std::cmp::Ordering::Equal,
                        };
                        match dir {
                            SortDir::Ascend => ord,
                            SortDir::Descend => ord.reverse(),
                        }
                    });
                }
                Ok(Rc::new(out))
            }
            // A contiguous half-open sub-range `src[start..end]`, clamped.
            Collection::Slice { source, start, end } => {
                let (source, start, end) = (*source, *start as usize, *end);
                let src = self.materialize(source)?;
                let lo = start.min(src.len());
                let hi = end.map_or(src.len(), |e| (e as usize).clamp(lo, src.len()));
                Ok(Rc::new(src[lo..hi].to_vec()))
            }
            // The candidate-or-empty projection of a composite (length = tuple
            // count). Axis-specific access goes through `CombProject`.
            Collection::Combination { .. } => {
                let tuples = self.comb_tuples(id)?;
                Ok(Rc::new(
                    tuples
                        .iter()
                        .map(|t| t.candidate.clone().unwrap_or_default())
                        .collect(),
                ))
            }
            // Projection onto a member or candidate axis: one object per tuple.
            Collection::CombProject { comb, axis } => {
                let (comb, axis) = (*comb, axis.clone());
                let tuples = self.comb_tuples(comb)?;
                let mut out = Vec::with_capacity(tuples.len());
                for t in tuples.iter() {
                    let obj = match &axis {
                        CombAxis::Candidate(_) => t.candidate.clone().unwrap_or_default(),
                        CombAxis::Member(name) => {
                            t.binders.get(name).cloned().unwrap_or_default()
                        }
                    };
                    out.push(obj);
                }
                Ok(Rc::new(out))
            }
        }
    }

    /// Materialize the surviving tuples of a composite (cached). Enumerates
    /// tuples over the parts, applies the disjoint value-distinctness rule
    /// (USER ANSWER 4), binds each tuple's candidate 4-vector, and drops
    /// tuples failing any per-tuple cut (the cuts FILTER the candidate
    /// collection, USER ANSWER 4).
    fn comb_tuples(&mut self, id: CollectionId) -> EvalResult<Rc<Vec<CombTuple>>> {
        if let Some(t) = self.comb_tuples.get(&id) {
            return Ok(Rc::clone(t));
        }
        let Collection::Combination {
            parts,
            kind,
            members,
            candidate,
            cuts,
        } = self.it.hir.table.collection(id).clone()
        else {
            return self.err(Span::default(), "internal: comb_tuples on a non-composite");
        };
        // Materialize each slot's source collection.
        let mut slots: Vec<Rc<Vec<EventObject>>> = Vec::with_capacity(parts.len());
        for p in &parts {
            slots.push(self.materialize(*p)?);
        }
        // Enumerate index tuples per the combinator.
        let sizes: Vec<usize> = slots.iter().map(|s| s.len()).collect();
        let index_tuples = enumerate_index_tuples(kind, &parts, &sizes);
        let drop_value_equal = kind == CombKind::Disjoint;

        let mut out: Vec<CombTuple> = Vec::new();
        'tuple: for idxs in index_tuples {
            // USER ANSWER 4: a disjoint tuple with two kinematically value-equal
            // members forms 0 valid pairs — drop it.
            if drop_value_equal {
                for a in 0..idxs.len() {
                    for b in (a + 1)..idxs.len() {
                        if self.kinematic_eq(&slots[a][idxs[a]], &slots[b][idxs[b]]) {
                            continue 'tuple;
                        }
                    }
                }
            }
            // Bind each slot's element to its binder name.
            let mut env: HashMap<Symbol, EventObject> = HashMap::new();
            for (slot, &i) in idxs.iter().enumerate() {
                env.insert(members[slot].name, slots[slot][i].clone());
            }
            // Build the candidate 4-vector object (if declared) under this env.
            let cand_obj = match &candidate {
                Some(c) => match self.candidate_object(c, &env)? {
                    Some(o) => Some(o),
                    None => continue, // candidate non-value ⇒ drop tuple
                },
                None => None,
            };
            // Per-tuple cuts filter the candidate collection.
            if !self.tuple_passes_cuts(&cuts, &env)? {
                continue;
            }
            out.push(CombTuple {
                binders: env,
                candidate: cand_obj,
            });
        }
        let rc = Rc::new(out);
        self.comb_tuples.insert(id, Rc::clone(&rc));
        Ok(rc)
    }

    /// Build the candidate 4-vector object for one tuple from the binder env.
    /// Returns `None` when the candidate's Lorentz vector is a soft non-value
    /// (a missing property), so the tuple is dropped.
    fn candidate_object(
        &mut self,
        cand: &CompositeCandidate,
        env: &HashMap<Symbol, EventObject>,
    ) -> EvalResult<Option<EventObject>> {
        let prev = std::mem::replace(&mut self.binder_env, env.clone());
        let lv = self.lorentz(&cand.vector, Span::default(), None);
        self.binder_env = prev;
        let lv = match lv? {
            Ok(lv) => lv,
            Err(_) => return Ok(None),
        };
        let (Ok(pt), Ok(eta), Ok(phi), Ok(mass)) = (lv.pt(), lv.eta(), lv.phi(), lv.mass()) else {
            return Ok(None);
        };
        Ok(Some(EventObject::from_props([
            (self.it.pt_key.clone(), pt),
            (self.it.eta_key.clone(), eta),
            (self.it.phi_key.clone(), phi),
            (self.it.mass_key.clone(), mass),
        ])))
    }

    /// Evaluate every per-tuple cut under the binder env; `true` iff all pass.
    fn tuple_passes_cuts(
        &mut self,
        cuts: &[ElemPredId],
        env: &HashMap<Symbol, EventObject>,
    ) -> EvalResult<bool> {
        for &pred in cuts {
            let node: &'h HNode = &self.it.hir.elem_pred(pred).node;
            let prev = std::mem::replace(&mut self.binder_env, env.clone());
            let pass = self.truth(node, None);
            self.binder_env = prev;
            if !pass? {
                return Ok(false);
            }
        }
        Ok(true)
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
