//! `smash2 verify` — the legacy `smash -r` equivalent: full analysis with
//! a human report (default), full per-pair proof chains (`--explain`),
//! or versioned JSON (`--json`). `--no-solver` caps verdicts at POSSIBLY;
//! `--fail-on=overlap|gap|empty|non-exact` gates the exit code on physics
//! findings.
//!
//! The default report uses ANSI color only when stdout is a tty and
//! `NO_COLOR` is unset, so piped/redirected output (and every
//! determinism test) takes the plain path.
//!
//! Exit codes: 1 on parse/sema errors (the analysis did not run); 4 when a
//! selected `--fail-on` finding fired (SPEC_ANALYSIS §6); else 0. The report
//! (human or JSON) is the only thing on stdout; everything else is stderr.

use crate::cmd::{CliError, read_file, unit_name};
use adl_analysis::report::FailOn;
use adl_analysis::{AnalysisOptions, SolverChoice, analyze_source};
use adl_sema::{ExtDecls, analyze_str, object_table};
use std::path::Path;
use std::process::ExitCode;

pub fn run(
    file: &Path,
    json: bool,
    explain: bool,
    no_solver: bool,
    fail_on: Option<&str>,
    verbose: bool,
) -> Result<ExitCode, CliError> {
    let fail_on = match fail_on {
        Some(s) => FailOn::parse(s).map_err(CliError::Usage)?,
        None => FailOn::default(),
    };

    let src = read_file(file)?;
    let name = unit_name(file);
    let ext = ExtDecls::legacy();

    let opts = AnalysisOptions {
        solver: if no_solver {
            SolverChoice::NoSolver
        } else {
            SolverChoice::Auto
        },
        fail_on,
        ..AnalysisOptions::default()
    };

    let report = match analyze_source(&src, &name, &ext, &opts) {
        Ok(r) => r,
        Err(e) => {
            // Parse/sema errors: render to stderr, exit 1. Stdout stays empty.
            eprint!("{}", e.rendered);
            eprintln!("{name}: analysis did not run (resolve errors above)");
            return Ok(ExitCode::from(1));
        }
    };

    if verbose {
        eprintln!(
            "{name}: solver={}; regions={}; pairs={}",
            report.solver,
            report.regions.len(),
            report.pairwise.len()
        );
    }

    // The report is the machine output: JSON or human, to stdout.
    if json {
        println!("{}", report.to_json());
    } else if explain {
        use std::io::IsTerminal as _;
        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        print!("{}", report.human());
        // The object-attribute summary is a pure function of the resolved
        // HIR; re-resolve (deterministic, cheap) and append it as an
        // `== objects ==` section. Default and JSON output are unchanged.
        let hir = analyze_str(&src, &name, &ext);
        println!();
        print!("{}", object_table(&hir, color));
    } else {
        use std::io::IsTerminal as _;
        let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        print!("{}", report.human_default(color));
    }

    // Internal diagnostics (witness re-validation failures) are bugs, not
    // user errors. The human report already prints them; in JSON mode they
    // live in the JSON body, but mirror them to stderr so a JSON consumer
    // still sees the warning channel.
    if json {
        for d in &report.internal_diagnostics {
            eprintln!("internal: {d}");
        }
    }

    let findings = report.findings(&fail_on);
    if !findings.is_empty() {
        eprintln!("{name}: --fail-on fired:");
        for f in &findings {
            eprintln!("  {f}");
        }
    }
    Ok(ExitCode::from(
        u8::try_from(report.exit_code(&fail_on)).unwrap_or(4),
    ))
}
