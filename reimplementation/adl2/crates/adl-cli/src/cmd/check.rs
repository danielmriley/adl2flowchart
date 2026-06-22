//! `smash2 check` — parse + resolve, report diagnostics. In the default
//! (human) mode stdout stays empty on success and diagnostics go to stderr;
//! with `--json` the diagnostics are emitted as a single JSON array on
//! stdout (the machine output, for editors / CI). Exit 1 if any file has
//! error-severity diagnostics, else 0.

use crate::cmd::{CliError, read_file, unit_name};
use adl_sema::{ExtDecls, analyze_str};
use adl_syntax::Severity;
use adl_syntax::diag::{has_errors, render};
use adl_syntax::span::LineMap;
use std::path::PathBuf;
use std::process::ExitCode;

pub fn run(
    files: &[PathBuf],
    verbose: bool,
    dump_ast: bool,
    json: bool,
) -> Result<ExitCode, CliError> {
    if json {
        return run_json(files);
    }
    let ext = ExtDecls::legacy();
    let mut any_errors = false;

    for path in files {
        let src = read_file(path)?;
        let name = unit_name(path);
        if dump_ast {
            let parsed = adl_syntax::parse(&src);
            print!("{}", adl_syntax::dump_ast(&src, &parsed.file));
        }
        // analyze_str merges parse diagnostics in front of sema's, so one
        // resolve pass surfaces both lexical/grammar and resolution issues.
        let hir = analyze_str(&src, &name, &ext);

        if !hir.diags.is_empty() {
            eprint!("{}", render(&src, &name, &hir.diags));
        }
        if has_errors(&hir.diags) {
            any_errors = true;
            eprintln!("{name}: FAILED");
        } else if verbose {
            let warnings = hir
                .diags
                .iter()
                .filter(|d| d.severity == Severity::Warning)
                .count();
            eprintln!(
                "{name}: ok ({} regions, {} objects, {warnings} warnings)",
                hir.regions.len(),
                hir.objects.len()
            );
        }
    }

    Ok(if any_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

/// `--json`: emit a single JSON array of diagnostics across all files to
/// stdout. Stable schema per element:
/// `{file, severity, line, col, start, end, message, label, help}`
/// (1-based `line`/`col`; byte `start`/`end`; `label`/`help` are `null`
/// when absent). Deterministic order: files as given, diagnostics in
/// resolve order. Exit 1 iff any error-severity diagnostic was emitted.
fn run_json(files: &[PathBuf]) -> Result<ExitCode, CliError> {
    let ext = ExtDecls::legacy();
    let mut out: Vec<serde_json::Value> = Vec::new();
    let mut any_errors = false;

    for path in files {
        let src = read_file(path)?;
        let name = unit_name(path);
        let hir = analyze_str(&src, &name, &ext);
        let map = LineMap::new(&src);
        for d in &hir.diags {
            let (line, col) = map.line_col(d.span.start);
            if d.severity == Severity::Error {
                any_errors = true;
            }
            out.push(serde_json::json!({
                "file": name,
                "severity": d.severity.as_str(),
                "line": line,
                "col": col,
                "start": d.span.start,
                "end": d.span.end,
                "message": d.message,
                "label": d.label,
                "help": d.help,
            }));
        }
    }

    println!("{}", serde_json::Value::Array(out));

    Ok(if any_errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
