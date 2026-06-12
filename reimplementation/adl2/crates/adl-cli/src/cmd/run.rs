//! `smash2 run` — evaluate an ADL file's regions over a JSONL event file
//! (SPEC_ARCHITECTURE §8). Per event, per region: pass / fail / eval-error,
//! plus bin assignment for passing events. Default output is a compact text
//! table; `--json` emits one JSON object per event line (JSONL out).
//!
//! Histograms (PLAN Phase 9): `histo` statements accumulate while events
//! stream. `--histos DIR` writes the canonical `histos.json` there, the two
//! ROOT bridges (`make_histos.C`, `to_root.py`), and — via the `rootfile`
//! crate — a native `out.root` with one TH1D per histogram under flat,
//! region-prefixed names (`--no-root` opts out of just the binary file).
//! With `--json`, files that declare histograms get one final
//! `{"histograms": [...]}` line after the per-event lines (files without
//! histograms emit exactly the pre-Phase-9 output). Histogram diagnostics
//! (skipped forms, non-numeric weights, skipped fills) go to stderr.
//!
//! Exit 1 if the ADL file does not resolve or the event file is malformed;
//! otherwise 0 (an event failing a region is data, not a tool error).

use crate::cmd::bridges;
use crate::cmd::{CliError, read_file, unit_name};
use adl_interp::{BinOutcome, Hist1D, HistoSet, Interp, RegionResult, read_jsonl};
use adl_sema::{ExtDecls, analyze_str};
use adl_syntax::diag::{has_errors, render};
use serde_json::{Value, json};
use std::path::Path;
use std::process::ExitCode;

/// Histogram output options for `run --histos` (all inert unless `dir` is
/// set; clap gates the flags on `--histos`).
#[derive(Debug, Default, Clone, Copy)]
pub struct HistoOpts<'a> {
    /// Output directory (`--histos DIR`); `None` disables all histogram output.
    pub dir: Option<&'a Path>,
    /// Also emit one CSV per histogram (`--csv`).
    pub csv: bool,
    /// Also emit one step-plot SVG per histogram (`--svg`).
    pub svg: bool,
    /// Skip the native `out.root` (`--no-root`).
    pub no_root: bool,
}

pub fn run(
    file: &Path,
    events: &Path,
    json_out: bool,
    histos: HistoOpts<'_>,
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
    // `--csv`/`--svg`/`--no-root` require `--histos` (enforced by clap), so
    // they only ever fire with a directory present.
    if let Some(dir) = histos.dir {
        write_histo_outputs(dir, &histo_set, histos, verbose)?;
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

/// Write `histos.json`, the native `out.root`, and the bridge renderers next
/// to it. The canonical JSON, the ROOT macro (`make_histos.C`), the uproot
/// script (`to_root.py`), and (unless `--no-root`) a native `out.root` are
/// always emitted; `--csv`/`--svg` add per-histogram files. Each write maps
/// IO failure to [`CliError::Write`].
fn write_histo_outputs(
    dir: &Path,
    set: &HistoSet,
    opts: HistoOpts<'_>,
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
    if !opts.no_root {
        write_root_file(&dir.join("out.root"), set, verbose)?;
    }
    if opts.csv {
        for (name, body) in bridges::csv_files(set) {
            emit(&name, &body)?;
        }
    }
    if opts.svg {
        for (name, body) in bridges::svg_files(set) {
            emit(&name, &body)?;
        }
    }
    Ok(())
}

/// Build and write the native `out.root` via the `rootfile` crate: one TH1D
/// per accumulated histogram, keyed by the flat region-prefixed name
/// (`baseline_hmet`) that the bridges also use, so `out.root` and the
/// generated `.root` files share object names and stay `hadd`-mergeable.
///
/// A histogram the writer rejects (e.g. a name collision after the
/// region-prefix flattening, or a name/title too long for a TKey) is skipped
/// with a stderr diagnostic — the JSON and bridges still carry it, and the
/// other histograms are written. A hard I/O failure on `finish` is fatal
/// ([`CliError::Write`]).
fn write_root_file(path: &Path, set: &HistoSet, verbose: bool) -> Result<(), CliError> {
    // Pin datime + UUIDs so `out.root` is byte-identical across runs (the
    // determinism guarantee the rest of `run --histos` already gives, and
    // what `hadd`/byte-diffs want). The datime is the ratified Phase-9 epoch
    // (2026-06-12 00:00:00 UTC); UUIDs are zeroed (ROOT treats them as
    // informational).
    let mut root = rootfile::RootFile::create()
        .with_datime(rootfile::pack_datime(2026, 6, 12, 0, 0, 0))
        .with_uuids([0; 16], [0; 16]);
    for fill in &set.histos {
        let h = &fill.hist;
        let name = bridges::root_name(&fill.region, &fill.name);
        let spec = h1_spec(&fill.title, h);
        // `add_th1d` consumes the builder; on the (practically unreachable —
        // flat names of a valid HistoSet don't collide) rejection path we
        // restore the pre-add accumulator and skip just this histogram (the
        // JSON/bridges still carry it).
        let snapshot = root.clone();
        root = match root.add_th1d(&name, &spec) {
            Ok(next) => next,
            Err(e) => {
                eprintln!("histogram `{name}`: skipped in out.root — {e}");
                snapshot
            }
        };
    }
    root.finish(path).map_err(|e| CliError::Write {
        path: path.display().to_string(),
        source: match e {
            rootfile::Error::Io(io) => io,
            other => std::io::Error::other(other.to_string()),
        },
    })?;
    if verbose {
        eprintln!("wrote {}", path.display());
    }
    Ok(())
}

/// Map an interpreter [`Hist1D`] to a `rootfile` [`H1Spec`]: in-range bins
/// stay as-is, flow bins move into `under`/`over`, and `entries` (a raw `u64`
/// fill count) becomes the `f64` ROOT `fEntries`. The four fill-time moments
/// pass through unchanged.
fn h1_spec<'a>(title: &'a str, h: &'a Hist1D) -> rootfile::H1Spec<'a> {
    #[allow(clippy::cast_precision_loss)]
    let entries = h.entries as f64;
    rootfile::H1Spec {
        title,
        nbins: h.nbins,
        lo: h.lo,
        hi: h.hi,
        sumw: &h.sumw,
        sumw2: &h.sumw2,
        under: rootfile::FlowBin {
            w: h.underflow_w,
            w2: h.underflow_w2,
        },
        over: rootfile::FlowBin {
            w: h.overflow_w,
            w2: h.overflow_w2,
        },
        entries,
        tsumw: h.tsumw,
        tsumw2: h.tsumw2,
        tsumwx: h.tsumwx,
        tsumwx2: h.tsumwx2,
    }
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
