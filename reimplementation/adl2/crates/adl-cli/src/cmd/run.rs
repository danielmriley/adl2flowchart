//! `smash2 run` — evaluate an ADL file's regions over a JSONL event file
//! (SPEC_ARCHITECTURE §8). Per event, per region: pass / fail / eval-error,
//! plus bin assignment for passing events. Default output is a compact text
//! table; `--json` emits one JSON object per event line (JSONL out).
//!
//! Histograms (PLAN Phase 9): `histo` statements accumulate while events
//! stream. `--histos DIR` writes the canonical `histos.json` there; with
//! `--json`, files that declare histograms get one final
//! `{"histograms": [...]}` line after the per-event lines (files without
//! histograms emit exactly the pre-Phase-9 output). Histogram diagnostics
//! (skipped forms, non-numeric weights, skipped fills) go to stderr.
//!
//! Exit 1 if the ADL file does not resolve or the event file is malformed;
//! otherwise 0 (an event failing a region is data, not a tool error).

use crate::cmd::bridges;
use crate::cmd::{CliError, read_file, unit_name};
use adl_interp::{BinOutcome, HistoSet, Interp, RegionResult, read_jsonl};
use adl_sema::{ExtDecls, analyze_str};
use adl_syntax::diag::{has_errors, render};
use serde_json::{Value, json};
use std::path::Path;
use std::process::ExitCode;

pub fn run(
    file: &Path,
    events: &Path,
    json_out: bool,
    histos_dir: Option<&Path>,
    csv: bool,
    svg: bool,
    verbose: bool,
) -> Result<ExitCode, CliError> {
    let src = read_file(file)?;
    let name = unit_name(file);
    let ext = ExtDecls::legacy();
    let hir = analyze_str(&src, &name, &ext);

    if !hir.diags.is_empty() {
        eprint!("{}", render(&src, &name, &hir.diags));
    }
    if has_errors(&hir.diags) {
        eprintln!("{name}: cannot run — resolve errors above");
        return Ok(ExitCode::from(1));
    }

    let events_text = read_file(events)?;
    let evs = match read_jsonl(&events_text, &ext) {
        Ok(evs) => evs,
        Err(e) => {
            eprintln!("{}: {e}", events.display());
            return Ok(ExitCode::from(1));
        }
    };

    let interp = Interp::new(&hir, &ext);
    let mut histo_set = HistoSet::new(&hir);
    // Pass counts accumulate in first-seen region order for the summary.
    let mut pass_counts: Vec<(String, usize)> = Vec::new();
    let bump = |name: &str, passed: bool, counts: &mut Vec<(String, usize)>| {
        if let Some(c) = counts.iter_mut().find(|(n, _)| n == name) {
            if passed {
                c.1 += 1;
            }
        } else {
            counts.push((name.to_owned(), usize::from(passed)));
        }
    };

    for (i, event) in evs.iter().enumerate() {
        let results = interp.run_event(event);
        histo_set.fill_event(&interp, event, &results);
        if json_out {
            let regions: Vec<Value> = results
                .iter()
                .map(|r| {
                    bump(&r.name, matches!(r.pass, Ok(true)), &mut pass_counts);
                    region_json(r)
                })
                .collect();
            println!("{}", json!({ "event": i, "regions": regions }));
        } else {
            for r in &results {
                bump(&r.name, matches!(r.pass, Ok(true)), &mut pass_counts);
                println!("event {i}: {} -> {}", r.name, region_text(r));
            }
        }
    }

    // Histogram output: diagnostics to stderr; the `histograms` JSON line
    // only when the file declares histograms (no-histo files keep their
    // exact pre-Phase-9 output).
    for d in histo_set.diagnostics() {
        eprintln!("{name}: {d}");
    }
    if json_out && !hir.histos.is_empty() {
        println!("{}", histo_set.to_json(false));
    }
    // `--csv`/`--svg` require `--histos` (enforced by clap), so they only
    // ever fire with a directory present.
    if let Some(dir) = histos_dir {
        write_histo_outputs(dir, &histo_set, csv, svg, verbose)?;
    }

    if verbose && !json_out {
        eprintln!(
            "--- {} events, {} regions ---",
            evs.len(),
            pass_counts.len()
        );
        for (region, count) in &pass_counts {
            eprintln!("{region}: {count}/{} passed", evs.len());
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// Write `histos.json` and the bridge renderers next to it. The ROOT macro
/// (`make_histos.C`) and uproot script (`to_root.py`) are always emitted;
/// `--csv`/`--svg` add per-histogram files. Each write maps IO failure to
/// [`CliError::Write`].
fn write_histo_outputs(
    dir: &Path,
    set: &HistoSet,
    csv: bool,
    svg: bool,
    verbose: bool,
) -> Result<(), CliError> {
    std::fs::create_dir_all(dir).map_err(|source| CliError::Write {
        path: dir.display().to_string(),
        source,
    })?;

    let emit = |rel: &str, contents: &str| -> Result<(), CliError> {
        let path = dir.join(rel);
        std::fs::write(&path, contents).map_err(|source| CliError::Write {
            path: path.display().to_string(),
            source,
        })?;
        if verbose {
            eprintln!("wrote {}", path.display());
        }
        Ok(())
    };

    emit("histos.json", &set.to_json(true))?;
    emit("make_histos.C", &bridges::make_histos_c(set))?;
    emit("to_root.py", &bridges::to_root_py(set))?;
    if csv {
        for (name, body) in bridges::csv_files(set) {
            emit(&name, &body)?;
        }
    }
    if svg {
        for (name, body) in bridges::svg_files(set) {
            emit(&name, &body)?;
        }
    }
    Ok(())
}

fn region_text(r: &RegionResult) -> String {
    match &r.pass {
        Ok(true) => {
            let mut s = "PASS".to_owned();
            for b in &r.bins {
                s.push_str(&bin_text(b));
            }
            s
        }
        Ok(false) => "fail".to_owned(),
        Err(e) => format!("ERROR: {}", e.reason),
    }
}

fn bin_text(b: &BinOutcome) -> String {
    match b {
        BinOutcome::Boundary { label, bin, .. } => {
            let label = label.as_deref().unwrap_or("bin");
            match bin {
                Some(i) => format!(" [{label}={i}]"),
                None => format!(" [{label}=underflow]"),
            }
        }
        BinOutcome::Cond { label, member } => {
            let label = label.as_deref().unwrap_or("bin");
            format!(" [{label}={member}]")
        }
        BinOutcome::Failed { label, reason } => {
            let label = label.as_deref().unwrap_or("bin");
            format!(" [{label}: error {reason}]")
        }
    }
}

fn region_json(r: &RegionResult) -> Value {
    let mut obj = json!({ "name": r.name });
    let map = obj.as_object_mut().expect("json object");
    match &r.pass {
        Ok(true) => {
            map.insert("pass".into(), Value::Bool(true));
            map.insert(
                "bins".into(),
                Value::Array(r.bins.iter().map(bin_json).collect()),
            );
        }
        Ok(false) => {
            map.insert("pass".into(), Value::Bool(false));
        }
        Err(e) => {
            map.insert("pass".into(), Value::Null);
            map.insert("error".into(), Value::String(e.reason.clone()));
        }
    }
    obj
}

fn bin_json(b: &BinOutcome) -> Value {
    match b {
        BinOutcome::Boundary { label, value, bin } => json!({
            "kind": "boundary",
            "label": label,
            "value": value,
            "bin": bin,
        }),
        BinOutcome::Cond { label, member } => json!({
            "kind": "cond",
            "label": label,
            "member": member,
        }),
        BinOutcome::Failed { label, reason } => json!({
            "kind": "error",
            "label": label,
            "reason": reason,
        }),
    }
}
