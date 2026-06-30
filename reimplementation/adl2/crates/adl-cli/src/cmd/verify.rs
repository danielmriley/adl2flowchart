//! `smash2 verify` — the legacy `smash -r` equivalent: full analysis with
//! a human report (default), full per-pair proof chains (`--explain`),
//! or versioned JSON (`--json`). `--no-solver` caps verdicts at POSSIBLY;
//! `--fail-on=overlap|gap|empty|non-exact` gates the exit code on physics
//! findings.
//!
//! Multiple files are each analyzed independently and reported in turn (a
//! per-unit header in human mode, a JSON array in `--json`); a single file
//! produces exactly the original byte-for-byte output. Cross-file region
//! relations (the merged-table analysis) are a separate, planned step — see
//! `MULTIFILE_PLAN.md`.
//!
//! The default report uses ANSI color only when stdout is a tty and
//! `NO_COLOR` is unset, so piped/redirected output (and every
//! determinism test) takes the plain path.
//!
//! Exit codes: 1 on parse/sema errors (the analysis did not run); 4 when a
//! selected `--fail-on` finding fired (SPEC_ANALYSIS §6); else 0; for
//! several files, the worst code wins. The report (human or JSON) is the
//! only thing on stdout; everything else is stderr.

use crate::cmd::{CliError, read_file, unit_name};
use adl_analysis::report::FailOn;
use adl_analysis::{AnalysisOptions, SolverChoice, analyze_hir, analyze_source};
use adl_sema::{ExtDecls, analyze_str, merge_hirs, object_table};
use std::io::IsTerminal as _;
use std::path::PathBuf;
use std::process::ExitCode;

/// When the user did NOT ask for `--no-solver` but no backend was found,
/// every verdict silently capped at POSSIBLY — warn loudly on stderr so a
/// physicist never reads an empty result as "found nothing".
fn warn_if_no_solver(name: &str, report: &adl_analysis::Report, no_solver: bool) {
    if !no_solver && report.solver == "none" {
        eprintln!(
            "{name}: WARNING — no SMT solver found, so only the solver-free interval checks ran; \
             overlaps and any disjoint/empty beyond simple interval bounds cap at POSSIBLY. Put a \
             `z3` or `cvc5` binary on PATH (e.g. `apt install z3`), or build with `--features \
             native`. Pass `--no-solver` to acknowledge and silence this."
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run(
    files: &[PathBuf],
    json: bool,
    explain: bool,
    no_solver: bool,
    fail_on: Option<&str>,
    verbose: bool,
    cross: bool,
) -> Result<ExitCode, CliError> {
    let fail_on = match fail_on {
        Some(s) => FailOn::parse(s).map_err(CliError::Usage)?,
        None => FailOn::default(),
    };
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
    if cross {
        return run_cross(files, &ext, &opts, json, explain, verbose);
    }
    let multi = files.len() > 1;
    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();

    let mut worst: u8 = 0;
    let mut json_reports: Vec<String> = Vec::new();

    for (i, file) in files.iter().enumerate() {
        let src = read_file(file)?;
        let name = unit_name(file);

        let report = match analyze_source(&src, &name, &ext, &opts) {
            Ok(r) => r,
            Err(e) => {
                eprint!("{}", e.rendered);
                eprintln!("{name}: analysis did not run (resolve errors above)");
                worst = worst.max(1);
                continue;
            }
        };

        warn_if_no_solver(&name, &report, no_solver);

        if verbose {
            eprintln!(
                "{name}: solver={}; regions={}; pairs={}",
                report.solver,
                report.regions.len(),
                report.pairwise.len()
            );
        }

        if json {
            json_reports.push(report.to_json());
            // Internal diagnostics (witness re-validation failures) are bugs,
            // not user errors; mirror them to the stderr warning channel so a
            // JSON consumer still sees them.
            for d in &report.internal_diagnostics {
                eprintln!("internal: {d}");
            }
        } else {
            // Per-unit header only when analyzing several files, so a single
            // file's output stays byte-identical to the original.
            if multi {
                if i > 0 {
                    println!();
                }
                println!("==== {name} ====");
            }
            if explain {
                print!("{}", report.human());
                // The object-attribute summary is a pure function of the
                // resolved HIR; re-resolve (deterministic, cheap) and append
                // it as an `== objects ==` section.
                let hir = analyze_str(&src, &name, &ext);
                println!();
                print!("{}", object_table(&hir, color));
            } else {
                print!("{}", report.human_default(color));
            }
        }

        let findings = report.findings(&fail_on);
        if !findings.is_empty() {
            eprintln!("{name}: --fail-on fired:");
            for f in &findings {
                eprintln!("  {f}");
            }
        }
        worst = worst.max(u8::try_from(report.exit_code(&fail_on)).unwrap_or(4));
    }

    if json {
        if multi {
            println!("[{}]", json_reports.join(","));
        } else if let Some(j) = json_reports.first() {
            println!("{j}");
        }
    }

    Ok(ExitCode::from(worst))
}

/// `--cross`: merge every unit into one shared identity space and analyze
/// region relations across files (regions reported as `<file>::<region>`).
/// Soundness comes from structural interning in `merge_hirs` — two quantities
/// unify iff structurally identical, so a cross-file PROVEN can only fire on
/// genuinely-shared quantities.
fn run_cross(
    files: &[PathBuf],
    ext: &ExtDecls,
    opts: &AnalysisOptions,
    json: bool,
    explain: bool,
    verbose: bool,
) -> Result<ExitCode, CliError> {
    // Resolve every unit; refuse if any has errors (merge needs clean units).
    let mut hirs = Vec::with_capacity(files.len());
    for file in files {
        let src = read_file(file)?;
        let name = unit_name(file);
        let hir = analyze_str(&src, &name, ext);
        if adl_syntax::diag::has_errors(&hir.diags) {
            eprint!("{}", adl_syntax::diag::render(&src, &name, &hir.diags));
            eprintln!("{name}: analysis did not run (resolve errors above)");
            return Ok(ExitCode::from(1));
        }
        hirs.push(hir);
    }

    let refs: Vec<&adl_sema::Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    // No single source spans a merged unit (the empty `src`), so cut text and
    // bin labels are rendered from the HIR instead of sliced from source;
    // source-LINE numbers in the report are therefore not meaningful here.
    let report = analyze_hir(&mut merged, "", ext, opts);
    warn_if_no_solver("cross", &report, opts.solver == SolverChoice::NoSolver);

    if verbose {
        eprintln!(
            "cross: {} units; regions={}; pairs={}",
            files.len(),
            report.regions.len(),
            report.pairwise.len()
        );
    }

    let color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    if json {
        println!("{}", report.to_json());
        for d in &report.internal_diagnostics {
            eprintln!("internal: {d}");
        }
    } else if explain {
        print!("{}", report.human());
    } else {
        print!("{}", report.human_default(color));
    }

    let findings = report.findings(&opts.fail_on);
    if !findings.is_empty() {
        eprintln!("cross: --fail-on fired:");
        for f in &findings {
            eprintln!("  {f}");
        }
    }
    Ok(ExitCode::from(
        u8::try_from(report.exit_code(&opts.fail_on)).unwrap_or(4),
    ))
}
