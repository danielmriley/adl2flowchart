//! `smash2 dot` — Graphviz DOT from the resolved HIR. The flowchart
//! (default) shows object lineage and region statements/inheritance; `--ast`
//! shows the resolved expression trees. DOT goes to stdout (machine-clean);
//! diagnostics to stderr. Exit 1 if the file does not resolve.

use crate::cmd::{CliError, read_file, unit_name};
use adl_sema::{ExtDecls, analyze_str};
use adl_syntax::diag::{has_errors, render};
use adl_viz::{ast_dot, flowchart_dot};
use std::path::Path;
use std::process::ExitCode;

pub fn run(file: &Path, ast: bool, verbose: bool) -> Result<ExitCode, CliError> {
    let src = read_file(file)?;
    let name = unit_name(file);
    let ext = ExtDecls::legacy();
    let hir = analyze_str(&src, &name, &ext);

    // Diagnostics (warnings included) always to stderr.
    if !hir.diags.is_empty() {
        eprint!("{}", render(&src, &name, &hir.diags));
    }
    if has_errors(&hir.diags) {
        eprintln!("{name}: cannot render DOT — resolve errors above");
        return Ok(ExitCode::from(1));
    }

    let dot = if ast {
        ast_dot(&hir)
    } else {
        flowchart_dot(&hir)
    };
    print!("{dot}");

    if verbose {
        let kind = if ast { "AST" } else { "flowchart" };
        eprintln!(
            "{name}: {kind} DOT emitted ({} regions, {} objects)",
            hir.regions.len(),
            hir.objects.len()
        );
    }
    Ok(ExitCode::SUCCESS)
}
