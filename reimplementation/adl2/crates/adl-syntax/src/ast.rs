//! Spanned AST: plain owned enums, `Box`ed children (SPEC_ARCHITECTURE §3).

use crate::span::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StrLit {
    pub value: String,
    pub span: Span,
}

/// A signed numeric literal as written (sign applied by the grammar's
/// `signed-num`, never by the lexer). `raw` preserves the source spelling of
/// the unsigned part for canonical dumps.
#[derive(Debug, Clone, PartialEq)]
pub struct NumLit {
    pub neg: bool,
    pub raw: String,
    pub is_real: bool,
    pub value: f64,
    pub span: Span,
}

impl NumLit {
    /// Canonical text: sign + raw unsigned literal.
    #[must_use]
    pub fn canon(&self) -> String {
        if self.neg {
            format!("-{}", self.raw)
        } else {
            self.raw.clone()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Section {
    Info(InfoBlock),
    Table(TableBlock),
    CountsFormat(CountsFormatBlock),
    Define(Define),
    Object(ObjectBlock),
    Region(RegionBlock),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InfoBlock {
    pub name: Ident,
    pub lines: Vec<InfoLine>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InfoLine {
    pub key: Ident,
    /// Free-form metadata value: the raw rest of the line after the key,
    /// trimmed. Never semantically analyzed, so kept as opaque text
    /// (may contain URLs, arithmetic, punctuation; SPEC_LANGUAGE info-line).
    pub value: String,
    /// Span of the value text; empty (`key.span` collapsed) when there is
    /// no value after the key.
    pub value_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableBlock {
    pub name: Ident,
    pub table_type: Ident,
    pub nvars: u64,
    pub errors: bool,
    pub values: Vec<NumLit>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CountsFormatBlock {
    pub name: Ident,
    pub processes: Vec<ProcessDecl>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcessDecl {
    pub name: Ident,
    pub title: StrLit,
    pub columns: Vec<Ident>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Define {
    /// `define` or `def` as written (canonical lowercase).
    pub keyword: String,
    pub name: Ident,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectKw {
    Object,
    Obj,
    Composite,
    Trigger,
}

impl ObjectKw {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ObjectKw::Object => "object",
            ObjectKw::Obj => "obj",
            ObjectKw::Composite => "composite",
            ObjectKw::Trigger => "trigger",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectBlock {
    pub keyword: ObjectKw,
    pub name: Ident,
    pub stmts: Vec<ObjectStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ObjectStmt {
    Take {
        /// `take`, `using` or `:` (stored as "take"/"using"/":").
        keyword: String,
        source: TakeSource,
        /// Element binder names: `take jets j`, `take leptons l1, l2`.
        binders: Vec<Ident>,
        /// `... alias adilepton` suffix.
        alias: Option<Ident>,
        span: Span,
    },
    Cut {
        /// `select`, `cut`, `cmd`, `command`.
        keyword: String,
        cond: Expr,
        span: Span,
    },
    Reject {
        cond: Expr,
        span: Span,
    },
    /// Derived candidate inside a composite block: `object <name> = <expr>`
    /// (canonical) or `candidate <name> = <expr>` (NPS dialect synonym).
    /// Both forms are equivalent; `keyword` records which was written.
    Derived {
        keyword: String,
        name: Ident,
        body: Expr,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TakeSource {
    Ident(Ident),
    Call { name: Ident, args: Vec<Arg> },
    Union { members: Vec<Ident>, span: Span },
    /// A postfix collection expression as a take source (`take coll[2:]`,
    /// `take coll[:4]`): the source is the sliced/indexed collection.
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKw {
    Region,
    Algo,
    HistoList,
}

impl RegionKw {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RegionKw::Region => "region",
            RegionKw::Algo => "algo",
            RegionKw::HistoList => "histoList",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionBlock {
    pub keyword: RegionKw,
    pub name: Ident,
    pub stmts: Vec<RegionStmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RegionStmt {
    Cut {
        keyword: String,
        cond: Expr,
        span: Span,
    },
    Reject {
        cond: Expr,
        span: Span,
    },
    /// A bare identifier: prior region (inheritance) or boolean define.
    RegionRef(Ident),
    Bin {
        label: Option<StrLit>,
        body: BinBody,
        span: Span,
    },
    Trigger {
        cond: Expr,
        span: Span,
    },
    Histo {
        name: Ident,
        title: StrLit,
        args: Vec<HistoArg>,
        span: Span,
    },
    Weight {
        /// Weight name; `trigger` is allowed as a name here.
        name: Ident,
        value: WeightValue,
        span: Span,
    },
    Save {
        name: Ident,
        format: Ident,
        args: Vec<Arg>,
        span: Span,
    },
    Print {
        args: Vec<Arg>,
        span: Span,
    },
    Counts {
        format: Ident,
        /// Raw tail tokens (numbers, idents, `+`, `-`, `+-`, `,`) to end of line.
        items: Vec<String>,
        span: Span,
    },
    /// `sort ...` — consumed to end of statement; always an Unsupported node.
    Sort {
        raw: String,
        span: Span,
    },
    /// `type search|control` — region metadata tag (corpus extension;
    /// CMS-SUS-21-002; see BUILD_NOTES).
    TypeTag {
        value: Ident,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinBody {
    /// `bin v b0 b1 ... bn` — boundary-list binning (real edges, divergence 5).
    Boundaries { var: Box<Expr>, edges: Vec<NumLit> },
    /// `bin <condition>` — boolean bin.
    Cond(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum HistoArg {
    Num(NumLit),
    /// Space-separated variable-bin edge list (`0.0 10.0 20.0 ...`),
    /// bracketed or bare.
    NumList(Vec<NumLit>),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum WeightValue {
    Num(NumLit),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Expr(Box<Expr>),
    Str(StrLit),
    /// Bare weight-file token (deprecated; SPEC_LANGUAGE §2 strings note).
    Path(StrLit),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Or,
    And,
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

impl BinOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Or => "or",
            BinOp::And => "and",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Pow => "^",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Lt,
    Ge,
    Le,
    Eq,
    Ne,
    /// `~=` — parsed distinctly; OPEN-4 maps it to `!=` downstream.
    ApproxEq,
}

impl CmpOp {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            CmpOp::Gt => ">",
            CmpOp::Lt => "<",
            CmpOp::Ge => ">=",
            CmpOp::Le => "<=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::ApproxEq => "~=",
        }
    }

    /// The relation with operands swapped (`a ⋈ b` ⇔ `b flipped(⋈) a`):
    /// `>`↔`<`, `>=`↔`<=`; `==`/`!=`/`~=` are symmetric.
    #[must_use]
    pub fn flipped(self) -> CmpOp {
        match self {
            CmpOp::Gt => CmpOp::Lt,
            CmpOp::Lt => CmpOp::Gt,
            CmpOp::Ge => CmpOp::Le,
            CmpOp::Le => CmpOp::Ge,
            CmpOp::Eq => CmpOp::Eq,
            CmpOp::Ne => CmpOp::Ne,
            CmpOp::ApproxEq => CmpOp::ApproxEq,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandKind {
    /// `x [] lo hi` — inclusive band.
    In,
    /// `x ][ lo hi` — excluded band.
    Out,
}

/// An element index: `[i]`, `[-i]` (reserved pending OPEN-3), `_i`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexVal {
    pub neg: bool,
    pub value: u64,
}

impl IndexVal {
    #[must_use]
    pub fn canon(&self) -> String {
        if self.neg {
            format!("-{}", self.value)
        } else {
            self.value.to_string()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(NumLit),
    Ident(Ident),
    /// `ALL` — always-true selection marker.
    All(Span),
    /// `NONE` keyword.
    NoneKw(Span),
    True(Span),
    False(Span),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Cmp {
        op: CmpOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Band {
        kind: BandKind,
        expr: Box<Expr>,
        lo: NumLit,
        hi: NumLit,
        span: Span,
    },
    Ternary {
        guard: Box<Expr>,
        then: Box<Expr>,
        els: Option<Box<Expr>>,
        span: Span,
    },
    Call {
        name: Ident,
        args: Vec<Arg>,
        span: Span,
    },
    /// `base.field` — grammar-level dotted access (divergence 3).
    Dot {
        base: Box<Expr>,
        field: Ident,
        span: Span,
    },
    /// `base->field` — member access into a composite candidate.
    Member {
        base: Box<Expr>,
        field: Ident,
        span: Span,
    },
    /// `base[i]` single-element index.
    Index {
        base: Box<Expr>,
        index: IndexVal,
        span: Span,
    },
    /// `base[a:b]`, `base[:b]`, `base[a:]` slice.
    Slice {
        base: Box<Expr>,
        start: Option<IndexVal>,
        end: Option<IndexVal>,
        span: Span,
    },
    /// `base_i` underscore indexing (`goodJets_1`).
    UnderscoreIndex {
        base: Box<Expr>,
        index: IndexVal,
        span: Span,
    },
    /// `base_` trailing underscore: implicit per-element reference
    /// (legacy loop notation; corpus: `{ JET_ }Pt`).
    UnderscoreAll {
        base: Box<Expr>,
        span: Span,
    },
    /// `|x|` absolute value.
    Abs {
        expr: Box<Expr>,
        span: Span,
    },
    /// `{ args } prop` braced property access.
    Braced {
        args: Vec<Arg>,
        prop: Ident,
        span: Span,
    },
    /// Two or more adjacent object refs forming one argument
    /// (divergence 7: `pT(jets[0] jets[1])`, `COMB(a b)`).
    ParticleList {
        items: Vec<Expr>,
        span: Span,
    },
    /// Placeholder produced during error recovery.
    Error(Span),
}

impl Expr {
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Expr::Num(n) => n.span,
            Expr::Ident(i) => i.span,
            Expr::All(s) | Expr::NoneKw(s) | Expr::True(s) | Expr::False(s) | Expr::Error(s) => *s,
            Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Cmp { span, .. }
            | Expr::Band { span, .. }
            | Expr::Ternary { span, .. }
            | Expr::Call { span, .. }
            | Expr::Dot { span, .. }
            | Expr::Member { span, .. }
            | Expr::Index { span, .. }
            | Expr::Slice { span, .. }
            | Expr::UnderscoreIndex { span, .. }
            | Expr::UnderscoreAll { span, .. }
            | Expr::Abs { span, .. }
            | Expr::Braced { span, .. }
            | Expr::ParticleList { span, .. } => *span,
        }
    }

    /// Is this a bare object reference chain (no operators)? Used to decide
    /// whether adjacent postfix expressions may join into a particle-list.
    #[must_use]
    pub fn is_postfix_like(&self) -> bool {
        matches!(
            self,
            Expr::Ident(_)
                | Expr::Dot { .. }
                | Expr::Member { .. }
                | Expr::Index { .. }
                | Expr::Slice { .. }
                | Expr::UnderscoreIndex { .. }
                | Expr::UnderscoreAll { .. }
                | Expr::Braced { .. }
                | Expr::Call { .. }
        )
    }
}
