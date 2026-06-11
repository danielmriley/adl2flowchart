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
        let text = map.line_text(src, d.span.start);
        let _ = writeln!(out, "{}: {}", d.severity.as_str(), d.message);
        let _ = writeln!(out, "  --> {file_name}:{line}:{col}");
        let gutter = format!("{line}");
        let pad = " ".repeat(gutter.len());
        let _ = writeln!(out, "{pad} |");
        let _ = writeln!(out, "{gutter} | {text}");
        let width = (d.span.end.saturating_sub(d.span.start)).max(1) as usize;
        // Clamp the caret run to the visible line.
        let width = width.min(text.len().saturating_sub(col as usize - 1).max(1));
        let carets = "^".repeat(width);
        let label = d.label.as_deref().unwrap_or("");
        let _ = writeln!(
            out,
            "{pad} | {}{carets} {label}",
            " ".repeat(col as usize - 1)
        );
        if let Some(help) = &d.help {
            let _ = writeln!(out, "{pad} = help: {help}");
        }
    }
    out
}
