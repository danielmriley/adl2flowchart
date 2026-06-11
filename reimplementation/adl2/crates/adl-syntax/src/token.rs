//! Tokens produced by the hand-written lexer (SPEC_LANGUAGE §2).

use crate::span::Span;

/// Reserved keywords (case-insensitive, SPEC_LANGUAGE §2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kw {
    Define,
    Def,
    Object,
    Obj,
    Composite,
    Take,
    Using,
    Select,
    Cut,
    Cmd,
    Command,
    Reject,
    Region,
    Algo,
    Bin,
    Histo,
    HistoList,
    Weight,
    Trigger,
    Info,
    Table,
    TableType,
    Nvars,
    Errors,
    Union,
    Process,
    Counts,
    CountsFormat,
    Print,
    Save,
    Sort,
    All,
    None,
    And,
    Or,
    Not,
    True,
    False,
}

impl Kw {
    /// Case-insensitive keyword lookup over the whole word.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Kw> {
        let lower = word.to_ascii_lowercase();
        Some(match lower.as_str() {
            "define" => Kw::Define,
            "def" => Kw::Def,
            "object" => Kw::Object,
            "obj" => Kw::Obj,
            "composite" => Kw::Composite,
            "take" => Kw::Take,
            "using" => Kw::Using,
            "select" => Kw::Select,
            "cut" => Kw::Cut,
            "cmd" => Kw::Cmd,
            "command" => Kw::Command,
            "reject" => Kw::Reject,
            "region" => Kw::Region,
            "algo" => Kw::Algo,
            "bin" => Kw::Bin,
            "histo" => Kw::Histo,
            "histolist" => Kw::HistoList,
            "weight" => Kw::Weight,
            "trigger" => Kw::Trigger,
            "info" => Kw::Info,
            "table" => Kw::Table,
            "tabletype" => Kw::TableType,
            "nvars" => Kw::Nvars,
            "errors" => Kw::Errors,
            "union" => Kw::Union,
            "process" => Kw::Process,
            "counts" => Kw::Counts,
            "countsformat" => Kw::CountsFormat,
            "print" => Kw::Print,
            "save" => Kw::Save,
            "sort" => Kw::Sort,
            "all" => Kw::All,
            "none" => Kw::None,
            "and" => Kw::And,
            "or" => Kw::Or,
            "not" => Kw::Not,
            "true" => Kw::True,
            "false" => Kw::False,
            _ => return Option::None,
        })
    }

    /// Canonical (lowercase) spelling, used in diagnostics and AST dumps.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Kw::Define => "define",
            Kw::Def => "def",
            Kw::Object => "object",
            Kw::Obj => "obj",
            Kw::Composite => "composite",
            Kw::Take => "take",
            Kw::Using => "using",
            Kw::Select => "select",
            Kw::Cut => "cut",
            Kw::Cmd => "cmd",
            Kw::Command => "command",
            Kw::Reject => "reject",
            Kw::Region => "region",
            Kw::Algo => "algo",
            Kw::Bin => "bin",
            Kw::Histo => "histo",
            Kw::HistoList => "histoList",
            Kw::Weight => "weight",
            Kw::Trigger => "trigger",
            Kw::Info => "info",
            Kw::Table => "table",
            Kw::TableType => "tabletype",
            Kw::Nvars => "nvars",
            Kw::Errors => "errors",
            Kw::Union => "union",
            Kw::Process => "process",
            Kw::Counts => "counts",
            Kw::CountsFormat => "countsformat",
            Kw::Print => "print",
            Kw::Save => "save",
            Kw::Sort => "sort",
            Kw::All => "ALL",
            Kw::None => "none",
            Kw::And => "and",
            Kw::Or => "or",
            Kw::Not => "not",
            Kw::True => "true",
            Kw::False => "false",
        }
    }
}

/// Keywords that can begin a statement or section; used for error
/// resynchronization (SPEC_LANGUAGE §3.2) and "did you mean" suggestions.
pub const STMT_KEYWORDS: &[Kw] = &[
    Kw::Define,
    Kw::Def,
    Kw::Object,
    Kw::Obj,
    Kw::Composite,
    Kw::Take,
    Kw::Using,
    Kw::Select,
    Kw::Cut,
    Kw::Cmd,
    Kw::Command,
    Kw::Reject,
    Kw::Region,
    Kw::Algo,
    Kw::Bin,
    Kw::Histo,
    Kw::HistoList,
    Kw::Weight,
    Kw::Trigger,
    Kw::Info,
    Kw::Table,
    Kw::CountsFormat,
    Kw::Process,
    Kw::Counts,
    Kw::Print,
    Kw::Save,
    Kw::Sort,
];

#[derive(Debug, Clone, PartialEq)]
pub enum TokKind {
    Ident(String),
    /// Unsigned integer literal (no signed-literal lexing; SPEC divergence 4).
    Int(u64),
    /// Unsigned real literal; raw text kept for canonical dumps.
    Real(f64),
    Str(String),
    Kw(Kw),
    // comparison / band operators
    Gt,
    Lt,
    Ge,
    Le,
    EqEq,
    Ne,
    TildeEq,
    BandIn,  // `[]`
    BandOut, // `][`
    // arithmetic
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    PlusMinus, // `+-` (counts statements)
    // structure
    Assign, // `=`
    Question,
    Colon,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Pipe,
    PipePipe,
    AmpAmp,
    Bang,
    Comma,
    Dot,
    Underscore,
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokKind,
    pub span: Span,
    /// 1-based source line; lets the parser do same-line checks cheaply.
    pub line: u32,
    /// Raw lexeme (used for canonical number dumps and counts tails).
    pub text: String,
}

impl Token {
    #[must_use]
    pub fn is_kw(&self, kw: Kw) -> bool {
        matches!(self.kind, TokKind::Kw(k) if k == kw)
    }

    #[must_use]
    pub fn is_number(&self) -> bool {
        matches!(self.kind, TokKind::Int(_) | TokKind::Real(_))
    }

    /// Human-readable description for error messages.
    #[must_use]
    pub fn describe(&self) -> String {
        match &self.kind {
            TokKind::Ident(name) => format!("identifier `{name}`"),
            TokKind::Int(_) | TokKind::Real(_) => format!("number `{}`", self.text),
            TokKind::Str(_) => "string literal".to_string(),
            TokKind::Kw(k) => format!("keyword `{}`", k.as_str()),
            TokKind::Newline => "end of line".to_string(),
            TokKind::Eof => "end of file".to_string(),
            _ => format!("`{}`", self.text),
        }
    }
}
