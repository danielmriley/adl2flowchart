//! `smash2` — the ADL2 command-line tool (SPEC_ARCHITECTURE §1/§9).
//!
//! Subcommands:
//! - `check`  — parse + resolve; report diagnostics. Exit 1 on errors.
//! - `verify` — full pairwise/region/bin analysis; grouped human report
//!   (default), full per-pair proof chains (`--explain`), or `--json`;
//!   `--fail-on=...` gates CI on physics findings; `--no-solver` caps
//!   verdicts at POSSIBLY. (The legacy `smash -r`.)
//! - `run`    — evaluate regions over a JSONL event file: per-region
//!   pass/fail and bin assignment.
//! - `dot`    — Graphviz DOT: flowchart (default) or AST (`--ast`), from
//!   the resolved HIR.
//! - `objects` — aligned object-attribute summary (one row per declared
//!   collection: base chain, element cuts, fragment, derived size facts).
//!
//! Output discipline: machine-clean stdout (the report / DOT / results),
//! diagnostics and progress to stderr; `--verbose` adds detail to stderr.

mod cmd;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "smash2",
    version,
    about = "ADL2 analysis toolchain: check, verify, run, dot, objects",
    propagate_version = true
)]
struct Cli {
    /// Extra detail on stderr (timing, solver backend, per-event lines).
    #[arg(long, short, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse and resolve; report diagnostics (exit 1 on errors).
    Check {
        /// One or more ADL files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Print the canonical AST dump for each file to stdout.
        #[arg(long)]
        dump_ast: bool,
    },
    /// Full analysis: pairwise verdicts, vacuity, bins (legacy `smash -r`).
    Verify {
        /// The ADL file to analyze.
        file: PathBuf,
        /// Emit the versioned JSON report instead of the human report.
        #[arg(long)]
        json: bool,
        /// Full per-pair detail: complete unsat cores, witness values,
        /// per-axiom statements (the proof chains behind the default
        /// report's findings).
        #[arg(long, conflicts_with = "json")]
        explain: bool,
        /// Disable the solver: interval fast path only, verdicts capped at
        /// POSSIBLY (same degradation as the legacy no-solver mode).
        #[arg(long)]
        no_solver: bool,
        /// Gate the exit code on physics findings (comma-separated):
        /// `overlap`, `gap`, `empty`, `non-exact`.
        #[arg(long, value_name = "KINDS")]
        fail_on: Option<String>,
    },
    /// Evaluate regions over JSONL events: per-region pass/fail + bins.
    Run {
        /// The ADL file.
        file: PathBuf,
        /// The JSONL event file (one event per line).
        events: PathBuf,
        /// Emit per-event results as JSON instead of the text table.
        #[arg(long)]
        json: bool,
        /// Accumulate `histo` statements and write `histos.json` plus the
        /// ROOT bridges (`make_histos.C`, `to_root.py`) into this directory
        /// (created if missing).
        #[arg(long, value_name = "DIR")]
        histos: Option<PathBuf>,
        /// Also emit one CSV per histogram (`bin_lo,bin_hi,content,error`)
        /// next to `histos.json` (requires `--histos`).
        #[arg(long, requires = "histos")]
        csv: bool,
        /// Also emit one hand-rolled step-plot SVG per histogram next to
        /// `histos.json` (requires `--histos`).
        #[arg(long, requires = "histos")]
        svg: bool,
        /// Skip writing the native `out.root` (still writes `histos.json`
        /// and the `make_histos.C`/`to_root.py` bridges; requires `--histos`).
        #[arg(long, requires = "histos")]
        no_root: bool,
    },
    /// Graphviz DOT from the resolved HIR (flowchart by default).
    Dot {
        /// The ADL file.
        file: PathBuf,
        /// Emit the AST graph instead of the flowchart.
        #[arg(long)]
        ast: bool,
    },
    /// Object-attribute summary: one aligned row per declared collection.
    Objects {
        /// The ADL file.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let verbose = cli.verbose;
    let result = match cli.command {
        Command::Check { files, dump_ast } => cmd::check::run(&files, verbose, dump_ast),
        Command::Verify {
            file,
            json,
            explain,
            no_solver,
            fail_on,
        } => cmd::verify::run(&file, json, explain, no_solver, fail_on.as_deref(), verbose),
        Command::Run {
            file,
            events,
            json,
            histos,
            csv,
            svg,
            no_root,
        } => cmd::run::run(
            &file,
            &events,
            json,
            cmd::run::HistoOpts {
                dir: histos.as_deref(),
                csv,
                svg,
                no_root,
            },
            verbose,
        ),
        Command::Dot { file, ast } => cmd::dot::run(&file, ast, verbose),
        Command::Objects { file } => cmd::objects::run(&file, verbose),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("smash2: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn cli_definition_is_valid() {
        super::Cli::command().debug_assert();
    }
}
