//! `smash2` — the ADL2 command-line tool (SPEC_ARCHITECTURE §1/§9).
//!
//! Subcommands:
//! - `check`  — parse + resolve; report diagnostics. Exit 1 on errors.
//! - `verify` — full pairwise/region/bin analysis; grouped human report
//!   (default), full per-pair proof chains (`--explain`), or `--json`;
//!   `--fail-on=...` gates CI on physics findings; `--no-solver` caps
//!   verdicts at POSSIBLY. (The legacy `smash -r`.)
//! - `run`    — evaluate regions over a JSONL event file (or a ROOT file
//!   via `--profile`): per-region pass/fail and bin assignment.
//! - `ingest` — converter-profile ingestion (SPEC_EVENT_PIPELINE §1):
//!   ROOT → canonical JSONL, plus the generated `to_jsonl.py` oracle.
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
    about = "ADL2 analysis toolchain: check, verify, run, dot, objects, ingest",
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
        #[arg(long, conflicts_with = "json")]
        dump_ast: bool,
        /// Emit diagnostics as a JSON array to stdout (machine-readable;
        /// for editors / CI gating) instead of the human text report.
        #[arg(long)]
        json: bool,
    },
    /// Full analysis: pairwise verdicts, vacuity, bins (legacy `smash -r`).
    Verify {
        /// One or more ADL files, or directories (each contributes its
        /// `*.adl` files). Without `--cross` each file is analyzed
        /// independently; with `--cross` they are merged (see below).
        #[arg(required = true)]
        files: Vec<PathBuf>,
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
        /// Merge all inputs (files and/or directories of `*.adl`) into one
        /// shared identity space and analyze region relations ACROSS files
        /// (the cross-analysis overlap matrix); regions are reported as
        /// `<file>::<region>`. Without this, inputs are analyzed independently.
        #[arg(long)]
        cross: bool,
        /// Skip the independent exact-rational certification of disjointness
        /// proofs (on by default: solver-UNSAT pairs whose unsat core cannot
        /// be certified report CANDIDATE DISJOINT, and certified pairs carry
        /// certified: true in --json).
        #[arg(long)]
        no_certify: bool,
        /// Write a portable certificate bundle (JSON, schema smash2-combine/1)
        /// per certified PROVEN DISJOINT pair into this directory. Each bundle
        /// carries the certified formula set and the Farkas certificate, and
        /// re-checks offline with `smash2-recheck` — no solver, no smash2 run.
        #[arg(long, value_name = "DIR", conflicts_with = "no_certify")]
        combine: Option<PathBuf>,
    },
    /// Evaluate regions over JSONL events: per-region pass/fail + bins.
    Run {
        /// The ADL file.
        file: PathBuf,
        /// The JSONL event file (one event per line) — or, with
        /// `--profile`, a ROOT event file ingested natively.
        events: PathBuf,
        /// Ingest `events` as a ROOT file under this converter profile
        /// (e.g. `delphes`) instead of reading JSONL.
        #[arg(long, value_name = "NAME")]
        profile: Option<String>,
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
        /// Use the v1 flat object names (`SR_hmet`) in `out.root` and the
        /// bridges instead of per-region TDirectories (`SR/hmet`); kept
        /// for one release for existing `hadd` pipelines (requires
        /// `--histos`).
        #[arg(long, requires = "histos")]
        flat_names: bool,
        /// Worker threads for the event loop (SPEC_EVENT_PIPELINE §5).
        /// `0` (default) uses all available cores. Outputs are
        /// byte-identical for any value — parallelism never changes results.
        #[arg(long, value_name = "N", default_value_t = 0)]
        jobs: usize,
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
    /// Ingest a ROOT event file under a converter profile: write canonical
    /// JSONL and/or the independent uproot oracle script (`to_jsonl.py`).
    Ingest {
        /// The ROOT event file (required with `-o`).
        input: Option<PathBuf>,
        /// The converter profile (e.g. `delphes`).
        #[arg(long, value_name = "NAME", required = true)]
        profile: String,
        /// Write the canonical JSONL event stream here.
        #[arg(short, long, value_name = "FILE")]
        output: Option<PathBuf>,
        /// Write the generated `to_jsonl.py` oracle script into this
        /// directory (created if missing).
        #[arg(long, value_name = "DIR")]
        emit_script: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let verbose = cli.verbose;
    let result = match cli.command {
        Command::Check {
            files,
            dump_ast,
            json,
        } => cmd::check::run(&files, verbose, dump_ast, json),
        Command::Verify {
            files,
            json,
            explain,
            no_solver,
            fail_on,
            cross,
            no_certify,
            combine,
        } => cmd::verify::run(
            &files,
            json,
            explain,
            no_solver,
            fail_on.as_deref(),
            verbose,
            cross,
            !no_certify,
            combine.as_deref(),
        ),
        Command::Run {
            file,
            events,
            profile,
            json,
            histos,
            csv,
            svg,
            no_root,
            flat_names,
            jobs,
        } => cmd::run::run(
            &file,
            &events,
            profile.as_deref(),
            json,
            cmd::run::HistoOpts {
                dir: histos.as_deref(),
                csv,
                svg,
                no_root,
                flat_names,
            },
            resolve_jobs(jobs),
            verbose,
        ),
        Command::Dot { file, ast } => cmd::dot::run(&file, ast, verbose),
        Command::Objects { file } => cmd::objects::run(&file, verbose),
        Command::Ingest {
            input,
            profile,
            output,
            emit_script,
        } => cmd::ingest::run(
            input.as_deref(),
            &profile,
            output.as_deref(),
            emit_script.as_deref(),
            verbose,
        ),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("smash2: {e}");
            ExitCode::from(2)
        }
    }
}

/// Resolve `--jobs`: `0` means all available cores (falling back to 1 when
/// the platform cannot report parallelism). The result never changes
/// outputs — only throughput (SPEC_EVENT_PIPELINE §5).
fn resolve_jobs(jobs: usize) -> usize {
    if jobs == 0 {
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    } else {
        jobs
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
