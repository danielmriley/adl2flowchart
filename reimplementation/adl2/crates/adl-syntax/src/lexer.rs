//! Hand-written lexer (SPEC_LANGUAGE §2).
//!
//! Key rules implemented here:
//! - identifiers are `[A-Za-z][A-Za-z0-9]*` segments joined by `_` only when
//!   the next character is a letter; `goodJets_1` lexes as `goodJets` `_` `1`
//!   with a note (the underscore-indexing operator);
//! - unsigned numeric literals only (negation is grammar-level unary minus);
//!   scientific notation is a lexical error with a rewrite help;
//! - NEWLINE tokens are emitted; only greedy productions consult them;
//! - adjacent-pair operators: `[]` `][` `+-` `==` `!=` `>=` `<=` `~=` `&&` `||`;
//! - any other character is an error; the lexer skips it and continues.

use crate::diag::Diagnostic;
use crate::span::Span;
use crate::token::{Kw, TokKind, Token};

pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diags: Vec<Diagnostic>,
}

struct Lexer<'s> {
    src: &'s str,
    bytes: &'s [u8],
    pos: usize,
    line: u32,
    tokens: Vec<Token>,
    diags: Vec<Diagnostic>,
    /// `_<digit>` split occurrences: the note is emitted once per file
    /// (idiomatic ADL like `METLV_0` would otherwise drown the output);
    /// further splits are counted and summarized at end of lex.
    underscore_splits: u32,
    last_underscore_span: Span,
}

#[must_use]
pub fn lex(src: &str) -> LexOutput {
    let mut lx = Lexer {
        src,
        bytes: src.as_bytes(),
        pos: 0,
        line: 1,
        tokens: Vec::new(),
        diags: Vec::new(),
        underscore_splits: 0,
        last_underscore_span: Span::new(0, 0),
    };
    lx.run();
    LexOutput {
        tokens: lx.tokens,
        diags: lx.diags,
    }
}

impl<'s> Lexer<'s> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn peek_at(&self, off: usize) -> Option<u8> {
        self.bytes.get(self.pos + off).copied()
    }

    fn push(&mut self, kind: TokKind, start: usize) {
        let span = Span::new(start as u32, self.pos as u32);
        let text = self.src[start..self.pos].to_string();
        self.tokens.push(Token {
            kind,
            span,
            line: self.line,
            text,
        });
    }

    fn run(&mut self) {
        while let Some(c) = self.peek() {
            let start = self.pos;
            match c {
                b'\n' => {
                    self.pos += 1;
                    self.push(TokKind::Newline, start);
                    self.line += 1;
                }
                b' ' | b'\t' | b'\r' => self.pos += 1,
                b'#' => {
                    while self.peek().is_some_and(|c| c != b'\n') {
                        self.pos += 1;
                    }
                }
                b'"' => self.lex_string(),
                b'0'..=b'9' => self.lex_number(),
                b'A'..=b'Z' | b'a'..=b'z' => self.lex_word(),
                _ => self.lex_operator(),
            }
        }
        let start = self.pos;
        self.push(TokKind::Eof, start);
        if self.underscore_splits > 1 {
            self.diags.push(Diagnostic::note(
                self.last_underscore_span,
                format!(
                    "({} more underscore-index split{} in this file)",
                    self.underscore_splits - 1,
                    if self.underscore_splits == 2 { "" } else { "s" }
                ),
            ));
        }
    }

    fn lex_string(&mut self) {
        let start = self.pos;
        self.pos += 1; // opening quote
        while let Some(c) = self.peek() {
            if c == b'"' || c == b'\n' {
                break;
            }
            self.pos += 1;
        }
        match self.peek() {
            Some(b'"') => {
                let content = self.src[start + 1..self.pos].to_string();
                self.pos += 1;
                self.push(TokKind::Str(content), start);
            }
            _ => {
                let content = self.src[start + 1..self.pos].to_string();
                self.diags.push(
                    Diagnostic::error(
                        Span::new(start as u32, self.pos as u32),
                        "unterminated string literal",
                    )
                    .with_label("string starts here and never closes")
                    .with_help("add a closing `\"` before the end of the line"),
                );
                self.push(TokKind::Str(content), start);
            }
        }
    }

    fn lex_number(&mut self) {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.pos += 1;
        }
        let mut is_real = false;
        if self.peek() == Some(b'.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_real = true;
            self.pos += 1;
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.pos += 1;
            }
        }
        let text = &self.src[start..self.pos];

        // Scientific notation is a lexical error (SPEC_LANGUAGE §2, corpus-checked unused).
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mut off = 1;
            if matches!(self.peek_at(1), Some(b'+' | b'-')) {
                off = 2;
            }
            if self.peek_at(off).is_some_and(|c| c.is_ascii_digit()) {
                let exp_start = self.pos;
                self.pos += off;
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.pos += 1;
                }
                let full = &self.src[start..self.pos];
                let expanded: f64 = full.parse().unwrap_or(0.0);
                self.diags.push(
                    Diagnostic::error(
                        Span::new(start as u32, self.pos as u32),
                        format!("scientific notation `{full}` is not supported"),
                    )
                    .with_label("exponent starts here")
                    .with_help(format!("write the value out, e.g. `{expanded:.1}`")),
                );
                // Recover with the mantissa value.
                let _ = exp_start;
            }
        }

        if is_real {
            // Mantissa text always parses as f64.
            let value: f64 = text.parse().unwrap_or(0.0);
            let end = start + text.len();
            let span = Span::new(start as u32, end as u32);
            self.tokens.push(Token {
                kind: TokKind::Real(value),
                span,
                line: self.line,
                text: text.to_string(),
            });
        } else {
            let value: u64 = text.parse().unwrap_or(u64::MAX);
            let end = start + text.len();
            let span = Span::new(start as u32, end as u32);
            self.tokens.push(Token {
                kind: TokKind::Int(value),
                span,
                line: self.line,
                text: text.to_string(),
            });
        }
    }

    /// Identifier rule: letter segments, optionally digit-extended, joined by
    /// `_` only when the next char is a letter. `_` before a digit terminates
    /// the identifier (underscore-indexing operator) with a note.
    fn lex_word(&mut self) {
        let start = self.pos;
        self.eat_segment();
        while self.peek() == Some(b'_') && self.peek_at(1).is_some_and(|c| c.is_ascii_alphabetic())
        {
            self.pos += 1; // `_`
            self.eat_segment();
        }
        let word = &self.src[start..self.pos];

        // Visible note when an `_<digit>` split occurs (SPEC_LANGUAGE §2).
        // Once per file: the first occurrence gets the full note + help,
        // the rest are counted and summarized at end of lex (`run`).
        if self.peek() == Some(b'_') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            let span = Span::new(start as u32, self.pos as u32 + 1);
            self.underscore_splits += 1;
            self.last_underscore_span = span;
            if self.underscore_splits == 1 {
                self.diags.push(
                    Diagnostic::note(
                        span,
                        format!("identifier `{word}` ends before `_`: `_<digit>` is the underscore-indexing operator"),
                    )
                    .with_help("write `name[i]` to make the indexing explicit"),
                );
            }
        }

        let kind = match Kw::from_word(word) {
            Some(kw) => TokKind::Kw(kw),
            None => TokKind::Ident(word.to_string()),
        };
        self.push(kind, start);
    }

    fn eat_segment(&mut self) {
        // [A-Za-z][A-Za-z0-9]*
        if self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
            while self.peek().is_some_and(|c| c.is_ascii_alphanumeric()) {
                self.pos += 1;
            }
        }
    }

    fn lex_operator(&mut self) {
        let start = self.pos;
        let c = self.bytes[self.pos];
        let next = self.peek_at(1);
        let (kind, len) = match (c, next) {
            (b'>', Some(b'=')) => (TokKind::Ge, 2),
            (b'>', _) => (TokKind::Gt, 1),
            (b'<', Some(b'=')) => (TokKind::Le, 2),
            (b'<', _) => (TokKind::Lt, 1),
            (b'=', Some(b'=')) => (TokKind::EqEq, 2),
            (b'=', _) => (TokKind::Assign, 1),
            (b'!', Some(b'=')) => (TokKind::Ne, 2),
            (b'!', _) => (TokKind::Bang, 1),
            (b'~', Some(b'=')) => (TokKind::TildeEq, 2),
            (b'+', Some(b'-')) => (TokKind::PlusMinus, 2),
            (b'+', _) => (TokKind::Plus, 1),
            (b'-', _) => (TokKind::Minus, 1),
            (b'*', _) => (TokKind::Star, 1),
            (b'/', _) => (TokKind::Slash, 1),
            (b'^', _) => (TokKind::Caret, 1),
            (b'?', _) => (TokKind::Question, 1),
            (b':', _) => (TokKind::Colon, 1),
            (b'(', _) => (TokKind::LParen, 1),
            (b')', _) => (TokKind::RParen, 1),
            (b'[', Some(b']')) => (TokKind::BandIn, 2),
            (b'[', _) => (TokKind::LBracket, 1),
            (b']', Some(b'[')) => (TokKind::BandOut, 2),
            (b']', _) => (TokKind::RBracket, 1),
            (b'{', _) => (TokKind::LBrace, 1),
            (b'}', _) => (TokKind::RBrace, 1),
            (b'|', Some(b'|')) => (TokKind::PipePipe, 2),
            (b'|', _) => (TokKind::Pipe, 1),
            (b'&', Some(b'&')) => (TokKind::AmpAmp, 2),
            (b',', _) => (TokKind::Comma, 1),
            (b'.', _) => (TokKind::Dot, 1),
            (b'_', _) => (TokKind::Underscore, 1),
            _ => {
                // Skip one (possibly multi-byte) character and report it.
                let ch_len = self.src[self.pos..]
                    .chars()
                    .next()
                    .map_or(1, char::len_utf8);
                self.pos += ch_len;
                let ch = &self.src[start..self.pos];
                self.diags.push(
                    Diagnostic::error(
                        Span::new(start as u32, self.pos as u32),
                        format!("unexpected character `{ch}`"),
                    )
                    .with_label("not part of ADL syntax")
                    .with_help("remove this character; see SPEC_LANGUAGE §2 for the operator set"),
                );
                return;
            }
        };
        self.pos += len;
        self.push(kind, start);
    }
}
