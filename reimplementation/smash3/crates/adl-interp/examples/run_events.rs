//! Phase-3 preview of `smash2 run`: evaluate an ADL file's regions over
//! JSONL events (the CLI subcommand proper lands in Phase 6 / adl-cli).
//!
//! Usage: `cargo run -p adl-interp --example run_events -- file.adl events.jsonl`

use adl_interp::{BinOutcome, Interp, read_jsonl};
use adl_sema::{ExtDecls, analyze_str};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [adl_path, events_path] = args.as_slice() else {
        eprintln!("usage: run_events <file.adl> <events.jsonl>");
        return ExitCode::FAILURE;
    };
    let src = match std::fs::read_to_string(adl_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {adl_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let events_text = match std::fs::read_to_string(events_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: cannot read {events_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let ext = ExtDecls::legacy();
    let hir = analyze_str(&src, adl_path, &ext);
    if adl_syntax::diag::has_errors(&hir.diags) {
        eprint!("{}", adl_syntax::diag::render(&src, adl_path, &hir.diags));
        return ExitCode::FAILURE;
    }

    let events = match read_jsonl(&events_text, &ext) {
        Ok(evs) => evs,
        Err(e) => {
            eprintln!("error: {events_path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let interp = Interp::new(&hir, &ext);
    let mut pass_counts: Vec<(String, usize)> = Vec::new();
    for (i, event) in events.iter().enumerate() {
        for result in interp.run_event(event) {
            if pass_counts.iter().all(|(n, _)| *n != result.name) {
                pass_counts.push((result.name.clone(), 0));
            }
            let line = match &result.pass {
                Ok(true) => {
                    if let Some(c) = pass_counts.iter_mut().find(|(n, _)| *n == result.name) {
                        c.1 += 1;
                    }
                    format!("PASS{}", render_bins(&result.bins))
                }
                Ok(false) => "fail".to_owned(),
                Err(e) => format!("ERROR: {}", e.reason),
            };
            println!("event {i}: {} -> {line}", result.name);
        }
    }
    println!("---");
    for (name, count) in &pass_counts {
        println!("{name}: {count}/{} events", events.len());
    }
    ExitCode::SUCCESS
}

fn render_bins(bins: &[BinOutcome]) -> String {
    let mut out = String::new();
    for b in bins {
        match b {
            BinOutcome::Boundary { label, bin, .. } => {
                let label = label.as_deref().unwrap_or("bin");
                match bin {
                    Some(i) => out.push_str(&format!(" [{label}={i}]")),
                    None => out.push_str(&format!(" [{label}=underflow]")),
                }
            }
            BinOutcome::Cond { label, member } => {
                let label = label.as_deref().unwrap_or("bin");
                out.push_str(&format!(" [{label}={member}]"));
            }
            BinOutcome::Failed { label, reason } => {
                let label = label.as_deref().unwrap_or("bin");
                out.push_str(&format!(" [{label}: error {reason}]"));
            }
        }
    }
    out
}
