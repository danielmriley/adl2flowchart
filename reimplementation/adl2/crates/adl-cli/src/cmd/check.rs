//! `smash2 check` — parse + resolve, report diagnostics. Stdout stays
//! empty on success (machine-clean); all diagnostics go to stderr. Exit 1
//! if any file has error-severity diagnostics, else 0.

use crate::cmd::{CliError, read_file, unit_name};
use adl_sema::{ExtDecls, analyze_str};
use adl_syntax::Severity;
use adl_syntax::diag::{has_errors, render};
use std::path::PathBuf;
use std::process::ExitCode;

pub fn run(files: &[PathBuf], verbose: bool) -> Result<ExitCode, CliError> {
    let ext = ExtDecls::legacy();
    let mut any_errors = false;

    for path in files {
        let src = read_file(path)?;
        let name = unit_name(path);
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
