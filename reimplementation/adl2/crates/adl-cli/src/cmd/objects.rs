//! `smash2 objects` — the object-attribute summary: one aligned row per
//! declared collection (base, filtered, union, combination) in declaration
//! order, with its base chain, element cuts, fragment status and derived
//! size facts. The modern successor of the legacy `printObjectAttributes`,
//! built from the resolved HIR's Collection identity model.
//!
//! The table goes to stdout (machine-clean); diagnostics to stderr. Color
//! is used only when stdout is a tty and `NO_COLOR` is unset, so piped or
//! redirected output (and the determinism tests) take the plain path.
//! Exit 1 if the file does not resolve.

use crate::cmd::{CliError, read_file, unit_name};
use adl_sema::{ExtDecls, analyze_str, object_table};
use adl_syntax::diag::{has_errors, render};
use std::path::Path;
use std::process::ExitCode;

pub fn run(file: &Path, verbose: bool) -> Result<ExitCode, CliError> {
    let src = read_file(file)?;
    let name = unit_name(file);
    let ext = ExtDecls::legacy();
    let hir = analyze_str(&src, &name, &ext);

    if !hir.diags.is_empty() {
        eprint!("{}", render(&src, &name, &hir.diags));
    }
    if has_errors(&hir.diags) {
        eprintln!("{name}: cannot summarize objects — resolve errors above");
        return Ok(ExitCode::from(1));
    }

    use std::io::IsTerminal as _;
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    print!("{}", object_table(&hir, color));

    if verbose {
        eprintln!("{name}: {} collections", hir.table.collections().len());
    }
    Ok(ExitCode::SUCCESS)
}
