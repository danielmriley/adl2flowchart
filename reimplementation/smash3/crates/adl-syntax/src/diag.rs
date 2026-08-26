//! Diagnostics: span + label + help, multi-error per run (SPEC_LANGUAGE §3.2).

use crate::span::{LineMap, Span};
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Note,
    Warning,
    Error,
}

impl Severity {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Note => "note",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

/// One diagnostic message. `label` annotates the span; `help` suggests a fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub span: Span,
    pub message: String,
    pub label: Option<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Error, span, message)
    }

    #[must_use]
    pub fn warning(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, span, message)
    }

    #[must_use]
    pub fn note(span: Span, message: impl Into<String>) -> Self {
        Self::new(Severity::Note, span, message)
    }

    #[must_use]
    pub fn new(severity: Severity, span: Span, message: impl Into<String>) -> Self {
        Self {
            severity,
            span,
            message: message.into(),
            label: None,
            help: None,
        }
    }

    #[must_use]
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// True if any diagnostic is an error.
#[must_use]
pub fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.severity == Severity::Error)
}

/// Render diagnostics in a rustc-like text format, deterministically.
#[must_use]
pub fn render(src: &str, file_name: &str, diags: &[Diagnostic]) -> String {
    let map = LineMap::new(src);
    let mut out = String::new();
    for d in diags {
        let (line, col) = map.line_col(d.span.start);
        let full = map.line_text(src, d.span.start);
        let (text, col) = window(full, col as usize);
        let _ = writeln!(out, "{}: {}", d.severity.as_str(), d.message);
        let _ = writeln!(out, "  --> {file_name}:{line}:{col}");
        let gutter = format!("{line}");
        let pad = " ".repeat(gutter.len());
        let _ = writeln!(out, "{pad} |");
        let _ = writeln!(out, "{gutter} | {text}");
        let width = (d.span.end.saturating_sub(d.span.start)).max(1) as usize;
        // Clamp the caret run to the visible line.
        let width = width.min(text.len().saturating_sub(col - 1).max(1));
        let carets = "^".repeat(width);
        let label = d.label.as_deref().unwrap_or("");
        let _ = writeln!(out, "{pad} | {}{carets} {label}", " ".repeat(col - 1));
        if let Some(help) = &d.help {
            let _ = writeln!(out, "{pad} = help: {help}");
        }
    }
    out
}

/// Longest source line echoed under a diagnostic, in bytes.
const MAX_SNIPPET: usize = 160;

/// Clip a source line to a window around the caret, returning the visible text
/// and the caret's 1-based column within it.
///
/// A hostile file is entitled to be one 10 MB line. Echoing it verbatim — plus a
/// 10 MB run of padding spaces under it — for every diagnostic turns a parse
/// error into megabytes of terminal output. Lines up to `MAX_SNIPPET` are shown
/// exactly as before, so ordinary output is unchanged.
fn window(line: &str, col: usize) -> (String, usize) {
    if line.len() <= MAX_SNIPPET {
        return (line.to_string(), col);
    }
    const LEAD: usize = 32;
    let caret = col - 1; // byte offset of the caret within the line
    let start = floor_boundary(line, caret.saturating_sub(LEAD));
    let end = floor_boundary(line, (start + MAX_SNIPPET).min(line.len()));
    let mut text = String::new();
    if start > 0 {
        text.push_str("...");
    }
    text.push_str(&line[start..end]);
    if end < line.len() {
        text.push_str("...");
    }
    // The caret keeps its offset relative to the window, shifted by the
    // leading ellipsis.
    let lead = if start > 0 { 3 } else { 0 };
    (text, caret - start + lead + 1)
}

/// Round `i` down to the nearest UTF-8 character boundary of `s`.
fn floor_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}
