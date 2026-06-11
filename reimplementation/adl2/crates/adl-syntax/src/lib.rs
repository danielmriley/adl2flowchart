//! `adl-syntax` — hand-written lexer, recursive-descent parser, spanned AST,
//! multi-error diagnostics and the canonical `--dump-ast` text form for the
//! ADL2 checked fragment (SPEC_LANGUAGE §2–3, SPEC_ARCHITECTURE §3).
//!
//! Entry points:
//! - [`lex`] — tokenize (NEWLINE tokens included; greedy productions only).
//! - [`parse`] — full file parse with statement-level error recovery.
//! - [`dump_ast`] — canonical, deterministic AST dump (snapshot-tested).
//! - [`render_diagnostics`] / [`has_errors`] — reporting helpers.

pub mod ast;
pub mod diag;
pub mod dump;
pub mod lexer;
pub mod parser;
pub mod span;
pub mod token;

pub use diag::{Diagnostic, Severity, has_errors};
pub use parser::{ParseResult, parse};
pub use span::{LineMap, Span};

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-syntax";

/// Tokenize `src`; returns tokens (ending in `Eof`) plus lexical diagnostics.
#[must_use]
pub fn lex(src: &str) -> lexer::LexOutput {
    lexer::lex(src)
}

/// Canonical AST dump (see `dump` module).
#[must_use]
pub fn dump_ast(src: &str, file: &ast::File) -> String {
    dump::dump_ast(src, file)
}

/// Render diagnostics as rustc-like text.
#[must_use]
pub fn render_diagnostics(src: &str, file_name: &str, diags: &[Diagnostic]) -> String {
    diag::render(src, file_name, diags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-syntax");
    }
}
