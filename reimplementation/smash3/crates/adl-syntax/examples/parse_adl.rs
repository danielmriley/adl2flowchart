//! Phase-1 driver for the corpus gate and manual inspection.
//!
//! Usage: `parse_adl [--dump-ast] [--quiet] <file.adl>...`
//!
//! Parses each file, prints diagnostics to stderr (and the canonical AST
//! dump to stdout with `--dump-ast`). Exit code 1 if any file has errors.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut dump = false;
    let mut quiet = false;
    let mut files = Vec::new();
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dump-ast" => dump = true,
            "--quiet" => quiet = true,
            _ => files.push(arg),
        }
    }
    if files.is_empty() {
        eprintln!("usage: parse_adl [--dump-ast] [--quiet] <file.adl>...");
        return ExitCode::from(2);
    }

    let mut failed = false;
    for path in &files {
        let src = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read {path}: {e}");
                failed = true;
                continue;
            }
        };
        let result = adl_syntax::parse(&src);
        let errors = result
            .diags
            .iter()
            .filter(|d| d.severity == adl_syntax::Severity::Error)
            .count();
        if !quiet || errors > 0 {
            eprint!(
                "{}",
                adl_syntax::render_diagnostics(&src, path, &result.diags)
            );
        }
        if dump {
            print!("{}", adl_syntax::dump_ast(&src, &result.file));
        }
        if errors > 0 {
            eprintln!("{path}: FAILED with {errors} error(s)");
            failed = true;
        } else if !quiet {
            eprintln!("{path}: ok ({} sections)", result.file.sections.len());
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
