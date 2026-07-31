//! `smash2-recheck` — standalone replay of `--combine` certificate bundles.
//!
//! Re-checks each bundle with the trusted exact-rational kernel only: no
//! solver, no search, no smash2 analysis run. A bundle that replays proves
//! its listed formulas are (real-)unsatisfiable together, and re-derives every
//! reconciliation fact it uses from that fact's own embedded premises — the
//! bundle's `note` field states what that does and does not cover, and this
//! tool repeats the part people most often over-read (see [`CAVEAT`]).
//!
//! Bundles written under the superseded schema `smash2-combine/1` are
//! **rejected**, not half-checked: they carry no derivation chain, so
//! "OK" would mean something weaker than it does for a `/2` bundle. Re-emit
//! them with a current smash2.
//!
//! Usage: `smash2-recheck BUNDLE.json... | DIR...`  (a directory contributes
//! its `*.json` files, sorted).
//!
//! # Exit codes
//!
//! | code | meaning |
//! |------|---------|
//! | 0 | at least one bundle was checked and **every** one replayed |
//! | 1 | bundles were checked and at least one failed to replay |
//! | 2 | nothing could be checked: bad usage, an unreadable path, or a
//!       directory that contains no `*.json` bundles |
//!
//! Exit 2 for "no bundles found" is deliberate and must stay non-zero: a
//! release script that runs `smash2-recheck dist/bundles/` to gate a publish
//! has to fail when the bundle directory is empty, not silently pass having
//! verified nothing. Vacuous success is the one outcome a verification tool
//! must never produce.

use adl_certify::CombineBundle;
use adl_certify::bundle::{BUNDLE_SCHEMA, SUPERSEDED_SCHEMAS, supersession_note};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::ExitCode;

/// What a passing replay does NOT establish. Printed with the summary so an
/// `OK` line is never read as an endorsement of the bundle's prose.
const CAVEAT: &str = "replay proves formula-level unsatisfiability and the derivation \
     chain of every reconciliation fact used; quantity labels, assert sources, region \
     names, producer and input identity are unauthenticated description and are NOT \
     checked - see each bundle's `note`";

/// Just enough of a bundle to report a schema mismatch precisely: a `/1`
/// document does not deserialize into the current struct at all, so the schema
/// has to be read before the full parse.
#[derive(Deserialize)]
struct SchemaProbe {
    schema: String,
}

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
                let total = std::fs::read_dir(&p).map(Iterator::count).unwrap_or(0);
                return Err(format!(
                    "no *.json certificate bundles in directory {} ({}); \
                     nothing was verified, so this exits 2 rather than reporting success. \
                     Bundles are produced by `smash2 verify --combine {}`",
                    p.display(),
                    match total {
                        0 => "the directory is empty".to_owned(),
                        n => format!("{n} entr{} present, none ending in .json", {
                            if n == 1 { "y" } else { "ies" }
                        }),
                    },
                    p.display(),
                ));
            }
            out.extend(found);
        } else {
            out.push(p);
        }
    }
    Ok(out)
}

fn check(path: &PathBuf) -> Result<String, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("read error: {e}"))?;
    let probe: SchemaProbe =
        serde_json::from_str(&text).map_err(|e| format!("parse error: {e}"))?;
    if SUPERSEDED_SCHEMAS.contains(&probe.schema.as_str()) {
        return Err(supersession_note(&probe.schema));
    }
    if probe.schema != BUNDLE_SCHEMA {
        // The schema string is attacker-controlled and unbounded; quote only a
        // recognizable prefix rather than echoing megabytes back to the reader.
        let shown: String = probe.schema.chars().take(64).collect();
        let ellipsis = if shown.len() < probe.schema.len() {
            "..."
        } else {
            ""
        };
        return Err(format!(
            "unknown schema {shown:?}{ellipsis} (expected {BUNDLE_SCHEMA:?})"
        ));
    }
    let bundle: CombineBundle =
        serde_json::from_str(&text).map_err(|e| format!("parse error: {e}"))?;
    if !bundle.replay() {
        return Err("certificate does not refute the listed formulas, or a \
                    reconciliation fact is used without a replayable derivation"
            .to_owned());
    }
    let chain = match bundle.derived_facts.len() {
        0 => String::new(),
        n => format!(", {n} derived fact(s) re-derived"),
    };
    Ok(format!("{} vs {}{chain}", bundle.region_a, bundle.region_b))
}

/// Restore the default SIGPIPE disposition, so `smash2-recheck DIR/ | head`
/// terminates on the closed pipe instead of panicking out of `println!`.
/// See `adl-cli`'s `restore_sigpipe` for the full rationale and tradeoff.
#[cfg(unix)]
fn restore_sigpipe() {
    // SAFETY: runs once at the top of `main`, before any thread exists.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> ExitCode {
    restore_sigpipe();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|a| a == "--help" || a == "-h") {
        eprintln!("usage: smash2-recheck BUNDLE.json... | DIR...");
        eprintln!("re-checks smash2 `verify --combine` certificate bundles ({BUNDLE_SCHEMA})");
        eprintln!("with the trusted exact-rational kernel; no solver required.");
        eprintln!();
        eprintln!("exit codes:");
        eprintln!("  0  every bundle checked replayed successfully (at least one was checked)");
        eprintln!("  1  a bundle failed to replay");
        eprintln!("  2  nothing was checked: bad usage, unreadable path, or a directory");
        eprintln!("     with no *.json bundles (fail-closed, so an empty bundle directory");
        eprintln!("     never gates a release as a vacuous success)");
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
        checked += 1;
        match check(f) {
            Ok(what) => println!("OK   {} ({what})", f.display()),
            Err(why) => {
                failed += 1;
                println!("FAIL {} — {why}", f.display());
            }
        }
    }

    println!("{checked} bundle(s) checked, {failed} failed — {CAVEAT}");
    if failed == 0 && checked > 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
