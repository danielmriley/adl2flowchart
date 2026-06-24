//! HIR: the resolved, typed program form shared by the interpreter, the
//! verifier and the visualizer (SPEC_ARCHITECTURE §4).
//!
//! Every node carries a [`Fragment`] tag: `InFragment` or
//! `Unsupported(reason)` — one diagnosis, two consumers.

use crate::intern::{Symbol, SymbolTable};
use crate::quantity::{CollectionId, ElemPredId, ParticleRef, PropId, QuantityId, QuantityTable};
use adl_syntax::ast::{BandKind, CmpOp};
use adl_syntax::diag::Diagnostic;
use adl_syntax::span::Span;

/// Fragment-membership tag (SPEC_LANGUAGE §5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fragment {
    InFragment,
    Unsupported(String),
}

impl Fragment {
    #[must_use]
    pub fn unsupported(reason: impl Into<String>) -> Self {
        Fragment::Unsupported(reason.into())
    }

    #[must_use]
    pub fn is_in_fragment(&self) -> bool {
        matches!(self, Fragment::InFragment)
    }
}

/// A reducer over a collection (`any`/`all`/`min`/`max`/`sum`). Booleans
/// (`Any`/`All`) fold a predicate body; numerics (`Sum`/`Min`/`Max`) fold a
/// scalar body. Interpret-only in P1 (no analyzer encoding).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReduceKind {
    Any,
    All,
    Sum,
    Min,
    Max,
}

impl ReduceKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ReduceKind::Any => "any",
            ReduceKind::All => "all",
            ReduceKind::Sum => "sum",
            ReduceKind::Min => "min",
            ReduceKind::Max => "max",
        }
    }

    /// `Any`/`All` fold a boolean body; `Sum`/`Min`/`Max` fold a scalar.
    #[must_use]
    pub fn is_boolean(self) -> bool {
        matches!(self, ReduceKind::Any | ReduceKind::All)
    }
}

/// Arithmetic operator (boolean structure is separate: `And`/`Or`/`Not`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl ArithOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ArithOp::Add => "+",
            ArithOp::Sub => "-",
            ArithOp::Mul => "*",
            ArithOp::Div => "/",
            ArithOp::Pow => "^",
        }
    }
}

/// A resolved expression node. `tag` describes THIS node; consumers walk
/// the subtree for coverage (an `InFragment` parent may contain
/// `Unsupported` leaves).
#[derive(Debug, Clone, PartialEq)]
pub struct HNode {
    pub kind: HKind,
    pub span: Span,
    pub tag: Fragment,
}

impl HNode {
    #[must_use]
    pub fn new(kind: HKind, span: Span) -> Self {
        Self {
            kind,
            span,
            tag: Fragment::InFragment,
        }
    }

    #[must_use]
    pub fn unsupported(span: Span, reason: impl Into<String>) -> Self {
        Self {
            kind: HKind::Unsupported,
            span,
            tag: Fragment::unsupported(reason),
        }
    }

    /// Does this subtree contain any `Unsupported` node?
    #[must_use]
    pub fn has_unsupported(&self) -> bool {
        if !self.tag.is_in_fragment() {
            return true;
        }
        self.children().iter().any(|c| c.has_unsupported())
    }

    fn children(&self) -> Vec<&HNode> {
        match &self.kind {
            HKind::Neg(a) | HKind::Not(a) | HKind::Abs(a) => vec![a],
            HKind::Binary { lhs, rhs, .. } | HKind::Cmp { lhs, rhs, .. } => vec![lhs, rhs],
            HKind::And(v) | HKind::Or(v) => v.iter().collect(),
            HKind::Band { expr, .. } => vec![expr],
            HKind::Reduce { body, .. } => vec![body],
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
}

/// Resolved expression kinds.
#[derive(Debug, Clone, PartialEq)]
pub enum HKind {
    /// Numeric literal; canonical source text (sign included).
    Num(String),
    Bool(bool),
    /// A typed, interned event quantity.
    Quantity(QuantityId),
    /// Property of the implicit element (object-block cut context).
    ElemSelfProp(PropId),
    /// Property of the current reducer iteration element (`pt(jets)` inside
    /// `any(pt(jets) > 30)`). Interpret-only (P1).
    ReduceProp(PropId),
    /// A reducer over a collection (`any`/`all`/`min`/`max`/`sum`).
    /// Interpret-only in P1: the formula/axiom layers tag it `Unsupported`.
    /// `slice` is reserved for P1-part-B static slices (`coll[:n]`); always
    /// `None` today.
    Reduce {
        kind: ReduceKind,
        coll: CollectionId,
        body: Box<HNode>,
        slice: Option<(u32, Option<u32>)>,
    },
    /// Unindexed per-element property at region level (OPEN-1: the
    /// formula layer applies the Dual bounded expansion).
    CollProp {
        coll: CollectionId,
        prop: PropId,
    },
    /// A bare particle value (meaningful only inside function arguments;
    /// in value position the node is tagged `Unsupported`).
    Particle(ParticleRef),
    /// A bare collection value (same caveat as `Particle`).
    CollValue(CollectionId),
    Neg(Box<HNode>),
    Not(Box<HNode>),
    Binary {
        op: ArithOp,
        lhs: Box<HNode>,
        rhs: Box<HNode>,
    },
    And(Vec<HNode>),
    Or(Vec<HNode>),
    Cmp {
        op: CmpOp,
        lhs: Box<HNode>,
        rhs: Box<HNode>,
    },
    Band {
        kind: BandKind,
        expr: Box<HNode>,
        lo: String,
        hi: String,
    },
    Ternary {
        guard: Box<HNode>,
        then: Box<HNode>,
        els: Option<Box<HNode>>,
    },
    Abs(Box<HNode>),
    /// A prior region used as a predicate (`select presel`).
    RegionPred(usize),
    /// Placeholder for out-of-fragment constructs; `tag` carries the reason.
    Unsupported,
}

/// An interned element predicate (cut set of a filtered collection).
#[derive(Debug, Clone, PartialEq)]
pub struct ElemPred {
    pub node: HNode,
    /// Canonical render (also the interning key).
    pub render: String,
}

/// A resolved `object` block.
#[derive(Debug, Clone, PartialEq)]
pub struct HirObject {
    pub name: Symbol,
    pub coll: CollectionId,
    /// `Some(source)` when this block is a pure rename (`object X take Y`
    /// with no cuts): the name binds the SAME `CollectionId` as the
    /// source — unification as a resolution fact.
    pub pure_alias_of: Option<CollectionId>,
    pub tag: Fragment,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefineKind {
    Numeric,
    Boolean,
}

impl DefineKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            DefineKind::Numeric => "numeric",
            DefineKind::Boolean => "boolean",
        }
    }
}

/// A resolved `define`: numeric defines resolve to their body expression,
/// boolean defines to their predicate (references inline the body).
#[derive(Debug, Clone, PartialEq)]
pub struct HirDefine {
    pub name: Symbol,
    pub kind: DefineKind,
    pub body: HNode,
    pub span: Span,
}

/// A resolved region statement.
#[derive(Debug, Clone, PartialEq)]
pub enum HirRegionStmt {
    Select(HNode),
    Reject(HNode),
    /// Bare prior-region name: inheritance.
    Inherit {
        region: usize,
        span: Span,
    },
    Trigger(HNode),
    /// Boundary-list bin: `[b0,b1), …, [bn,∞)`; edges are canonical
    /// numeral text (real-valued, divergence 5).
    Bin {
        label: Option<String>,
        var: HNode,
        edges: Vec<String>,
        span: Span,
    },
    /// Boolean bin.
    BinCond {
        label: Option<String>,
        cond: HNode,
        span: Span,
    },
    /// Statement with no membership effect (histo/weight/save/print/
    /// counts/type) or out-of-fragment (`sort`); `tag` says which.
    NonMembership {
        kind: &'static str,
        tag: Fragment,
        span: Span,
    },
}

/// A resolved region.
#[derive(Debug, Clone, PartialEq)]
pub struct HirRegion {
    pub name: Symbol,
    pub stmts: Vec<HirRegionStmt>,
    pub span: Span,
}

/// Binning/fill spec of a `histo` statement (PLAN Phase 9).
#[derive(Debug, Clone, PartialEq)]
pub enum HistoSpec {
    /// `histo h, "title", n, lo, hi, expr` — 1-D uniform binning.
    /// `lo`/`hi` are canonical numeral text (same convention as bin edges).
    Uniform1D {
        nbins: u32,
        lo: String,
        hi: String,
        expr: HNode,
    },
    /// `histo h, "title", e0 e1 … en, expr` — 1-D variable binning.
    /// `edges` are canonical numeral text, strictly increasing (validated
    /// at resolve time; a non-increasing list is `Unsupported` instead).
    Var1D { edges: Vec<String>, expr: HNode },
    /// `histo h, "title", nx, xlo, xhi, ny, ylo, yhi, xexpr, yexpr` — 2-D
    /// uniform binning. Bound texts are canonical numerals (same
    /// convention as [`Self::Uniform1D`]).
    Uniform2D {
        nx: u32,
        xlo: String,
        xhi: String,
        ny: u32,
        ylo: String,
        yhi: String,
        xexpr: HNode,
        yexpr: HNode,
    },
    /// A malformed argument list, or a form not yet accumulable; the reason
    /// is reported when accumulation is attempted.
    Unsupported(String),
}

/// A resolved `histo` statement. Histograms are execution auxiliaries
/// (no membership effect); the region keeps its `NonMembership` marker
/// and the payload lives here.
#[derive(Debug, Clone, PartialEq)]
pub struct HirHisto {
    /// Index into [`Hir::regions`] of the block that declares it (a
    /// selection region or a `histoList` block).
    pub region: usize,
    pub name: String,
    pub title: String,
    pub spec: HistoSpec,
    pub span: Span,
}

/// Value of a `weight` statement.
#[derive(Debug, Clone, PartialEq)]
pub enum HirWeightValue {
    /// Numeric literal (canonical text).
    Num(String),
    /// Non-numeric argument (identifier / function call / table ref);
    /// carries a short description for diagnostics.
    Other(String),
}

/// A resolved `weight` statement (no membership effect; payload only).
#[derive(Debug, Clone, PartialEq)]
pub struct HirWeight {
    /// Index into [`Hir::regions`] of the declaring block.
    pub region: usize,
    pub name: String,
    pub value: HirWeightValue,
    pub span: Span,
}

/// The resolved analysis unit: HIR + symbol/quantity tables + diagnostics.
#[derive(Debug)]
pub struct Hir {
    /// Unit label (file name).
    pub unit: String,
    pub symbols: SymbolTable,
    pub table: QuantityTable,
    /// Names bound to each collection, in binding order (index = `CollectionId.0`).
    pub coll_names: Vec<Vec<Symbol>>,
    /// Interned element predicates (index = `ElemPredId.0`).
    pub elem_preds: Vec<ElemPred>,
    pub objects: Vec<HirObject>,
    pub defines: Vec<HirDefine>,
    pub regions: Vec<HirRegion>,
    /// Region names in declaration order (`RegionPred`/`Inherit` indices
    /// point into this).
    pub region_name_order: Vec<Symbol>,
    /// `true` at index `i` iff `regions[i]` was declared with the
    /// `histoList` keyword (a histogram template block, not a selection
    /// region). May be shorter than `regions` for synthetic regions;
    /// index with `.get(i)`.
    pub histolist_regions: Vec<bool>,
    /// All `histo` statements, in declaration order.
    pub histos: Vec<HirHisto>,
    /// All `weight` statements, in declaration order.
    pub weights: Vec<HirWeight>,
    /// Sema diagnostics (parse diagnostics are the caller's, unless
    /// `analyze_str` merged them).
    pub diags: Vec<Diagnostic>,
}

impl Hir {
    /// Collection bound to `name` (case-insensitive), if any.
    #[must_use]
    pub fn collection_of(&self, name: &str) -> Option<CollectionId> {
        let sym = self.symbols.lookup(name)?;
        self.objects
            .iter()
            .find(|o| o.name == sym)
            .map(|o| o.coll)
            .or_else(|| {
                self.coll_names
                    .iter()
                    .position(|names| names.contains(&sym))
                    .map(|i| CollectionId(u32::try_from(i).expect("collection id overflow")))
            })
    }

    /// Define named `name` (case-insensitive), if any.
    #[must_use]
    pub fn define(&self, name: &str) -> Option<&HirDefine> {
        let sym = self.symbols.lookup(name)?;
        self.defines.iter().find(|d| d.name == sym)
    }

    /// Region named `name` (case-insensitive), if any.
    #[must_use]
    pub fn region(&self, name: &str) -> Option<&HirRegion> {
        let sym = self.symbols.lookup(name)?;
        self.regions.iter().find(|r| r.name == sym)
    }

    #[must_use]
    pub fn elem_pred(&self, id: ElemPredId) -> &ElemPred {
        &self.elem_preds[id.0 as usize]
    }
}
