//! `smash2-recheck` — standalone replay of `--combine` certificate bundles.
//!
//! Re-checks each bundle with the trusted exact-rational kernel only: no
//! solver, no search, no smash2 analysis run. A bundle that replays proves
//! its listed formulas are (real-)unsatisfiable together — the bundle's own
//! `note` field states what that does and does not cover.
//!
//! Usage: `smash2-recheck BUNDLE.json... | DIR...`  (a directory contributes
//! its `*.json` files, sorted). Exit 0 iff at least one bundle was checked
//! and every one replayed.

use adl_certify::CombineBundle;
use adl_certify::bundle::BUNDLE_SCHEMA;
use std::path::PathBuf;
use std::process::ExitCode;

fn collect_inputs(args: &[String]) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for a in args {
        let p = PathBuf::from(a);
        if p.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(&p)
                .map_err(|e| format!("cannot read directory {}: {e}", p.display()))?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|q| q.extension().is_some_and(|x| x == "json"))
                .collect();
            found.sort();
            if found.is_empty() {
                return Err(format!("no .json bundles in directory {}", p.display()));
            }
            out.extend(found);
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("usage: smash2-recheck BUNDLE.json... | DIR...");
        eprintln!("re-checks smash2 `verify --combine` certificate bundles ({BUNDLE_SCHEMA})");
        eprintln!("with the trusted exact-rational kernel; no solver required.");
        return ExitCode::from(2);
    }

    let files = match collect_inputs(&args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("smash2-recheck: {e}");
            return ExitCode::from(2);
        }
    };

    let mut checked = 0usize;
    let mut failed = 0usize;
    for f in &files {
        let verdict = std::fs::read_to_string(f)
            .map_err(|e| format!("read error: {e}"))
            .and_then(|s| {
                serde_json::from_str::<CombineBundle>(&s).map_err(|e| format!("parse error: {e}"))
            })
            .map(|b| {
                if b.schema != BUNDLE_SCHEMA {
                    return Err(format!("unknown schema {:?} (expected {BUNDLE_SCHEMA:?})", b.schema));
                }
                if b.replay() {
                    Ok(format!("{} vs {}", b.region_a, b.region_b))
                } else {
                    Err("certificate does not refute the listed formulas".to_owned())
                }
            })
            .and_then(|r| r);
        checked += 1;
        match verdict {
            Ok(pair) => println!("OK   {} ({pair})", f.display()),
            Err(why) => {
                failed += 1;
                println!("FAIL {} — {why}", f.display());
            }
        }
    }

    println!(
        "{checked} bundle(s) checked, {failed} failed — replay proves formula-level \
         unsatisfiability; see each bundle's `note` for scope"
    );
    if failed == 0 && checked > 0 { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
