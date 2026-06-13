//! `smash2 run` — evaluate an ADL file's regions over a JSONL event file
//! (SPEC_ARCHITECTURE §8). Per event, per region: pass / fail / eval-error,
//! plus bin assignment for passing events. Default output is a compact text
//! table; `--json` emits one JSON object per event line (JSONL out).
//!
//! Histograms (PLAN Phase 9 + SPEC_EVENT_PIPELINE §3): `histo` statements
//! accumulate while events stream. `--histos DIR` writes the canonical
//! `histos.json` (v2) there, the two ROOT bridges (`make_histos.C`,
//! `to_root.py`), and — via the `rootfile` crate — a native `out.root`
//! with one TH1D/TH2D per histogram inside per-region TDirectories
//! (`SR/hmet`; rootfile v2), plus each region's §2 cutflow pair
//! (`<region>__cutflow_raw`/`__cutflow_wt`, bins labeled with the
//! verbatim step text). `--flat-names` keeps the v1 flat layout
//! (`SR_hmet`) for one release; `--no-root` opts out of just the binary
//! file.
//! With `--json`, files that declare histograms get one final
//! `{"histograms": [...]}` line after the per-event lines (files without
//! histograms emit exactly the pre-Phase-9 output). Histogram diagnostics
//! (skipped forms, non-numeric weights, skipped fills) go to stderr.
//!
//! Cutflows (SPEC_EVENT_PIPELINE §2): every region accumulates per-step
//! raw + weighted survivor counts (weights = input event weight × the
//! positional ADL `weight` product, §4). Text mode appends one fixed-width
//! table per region to stdout; `--json` appends one final
//! `{"cutflow": {...}}` line instead; `--histos DIR` additionally writes
//! the canonical `cutflow.json` next to `histos.json`.
//!
//! Exit 1 if the ADL file does not resolve or the event file is malformed;
//! otherwise 0 (an event failing a region is data, not a tool error).

use crate::cmd::bridges;
use crate::cmd::{CliError, read_file, unit_name};
use adl_interp::{BinOutcome, CutflowSet, Hist1D, HistoSet, Interp, RegionResult, read_jsonl};
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
    /// v1 flat object names instead of per-region TDirectories
    /// (`--flat-names`).
    pub flat_names: bool,
}

pub fn run(
    file: &Path,
    events: &Path,
    profile: Option<&str>,
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

    // With `--profile`, ingest the ROOT file natively into canonical JSONL
    // lines (in memory, SPEC_EVENT_PIPELINE §1.1) and feed them to the one
    // JSONL loader — the native path and the file path share every
    // event-model validation.
    let events_text = if let Some(pname) = profile {
        let Some(prof) = adl_ingest::by_name(pname) else {
            return Err(CliError::Usage(format!(
                "unknown profile `{pname}` (known: {})",
                adl_ingest::KNOWN_PROFILES.join(", ")
            )));
        };
        if verbose {
            super::ingest::print_profile_choices(&prof);
        }
        match adl_ingest::read_root(events, &prof) {
            Ok(ingested) => {
                super::ingest::print_diags(&ingested.diags, &ingested.profile_id, verbose);
                ingested.jsonl()
            }
            Err(e) => {
                eprintln!("{}: {e}", events.display());
                return Ok(ExitCode::from(1));
            }
        }
    } else {
        read_file(events)?
    };
    let evs = match read_jsonl(&events_text, &ext) {
        Ok(evs) => evs,
        Err(e) => {
            eprintln!("{}: {e}", events.display());
            return Ok(ExitCode::from(1));
        }
    };

    let interp = Interp::new(&hir, &ext);
    let mut histo_set = HistoSet::new(&hir);
    let mut cutflow = CutflowSet::new(&hir, &src);
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
        let (results, traces) = interp.run_event_traced(event);
        cutflow.record_event(event, &results, &traces);
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
    for d in cutflow.diagnostics() {
        eprintln!("{name}: {d}");
    }
    if json_out && !hir.histos.is_empty() {
        println!("{}", histo_set.to_json(false));
    }
    // Cutflow emission (SPEC_EVENT_PIPELINE §2): the per-region table on
    // stdout in text mode, one `{"cutflow": ...}` line under `--json`;
    // files with no evaluable region emit neither.
    if !cutflow.is_empty() {
        if json_out {
            println!("{{\"cutflow\":{}}}", cutflow.to_json(false));
        } else {
            if !evs.is_empty() {
                println!();
            }
            print!("{}", cutflow.text_table());
        }
    }
    // `--csv`/`--svg`/`--no-root` require `--histos` (enforced by clap), so
    // they only ever fire with a directory present.
    if let Some(dir) = histos.dir {
        write_histo_outputs(dir, &histo_set, &cutflow, histos, verbose)?;
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

/// Write `histos.json`, `cutflow.json`, the native `out.root`, and the
/// bridge renderers next to them. The canonical JSONs, the ROOT macro
/// (`make_histos.C`), the uproot script (`to_root.py`), and (unless
/// `--no-root`) a native `out.root` are always emitted; `--csv`/`--svg`
/// add per-histogram files. Each write maps IO failure to
/// [`CliError::Write`].
fn write_histo_outputs(
    dir: &Path,
    set: &HistoSet,
    cutflow: &CutflowSet,
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
    if !cutflow.is_empty() {
        emit("cutflow.json", &cutflow.to_json(true))?;
    }
    emit(
        "make_histos.C",
        &bridges::make_histos_c(set, opts.flat_names),
    )?;
    emit("to_root.py", &bridges::to_root_py(set, opts.flat_names))?;
    if !opts.no_root {
        write_root_file(
            &dir.join("out.root"),
            set,
            cutflow,
            opts.flat_names,
            verbose,
        )?;
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

/// Build and write the native `out.root` via the `rootfile` crate: one
/// TH1D/TH2D per accumulated histogram plus each region's cutflow pair
/// (SPEC_EVENT_PIPELINE §2), inside per-region TDirectories (object names
/// `SR/hmet`) — or under the v1 flat names (`SR_hmet`, shared with the
/// bridges) when `flat` is set. Both layouts are `hadd`-mergeable.
///
/// An object the writer rejects (e.g. a user histogram colliding with the
/// reserved `__cutflow_*` namespace, or a name/title too long for a TKey)
/// is skipped with a stderr diagnostic — the JSONs and bridges still carry
/// it, and the other objects are written. A hard I/O failure on `finish`
/// is fatal ([`CliError::Write`]).
fn write_root_file(
    path: &Path,
    set: &HistoSet,
    cutflow: &CutflowSet,
    flat: bool,
    verbose: bool,
) -> Result<(), CliError> {
    // Pin datime + UUIDs so `out.root` is byte-identical across runs (the
    // determinism guarantee the rest of `run --histos` already gives, and
    // what `hadd`/byte-diffs want). The datime is the ratified Phase-9 epoch
    // (2026-06-12 00:00:00 UTC); UUIDs are zeroed (ROOT treats them as
    // informational).
    let mut root = rootfile::RootFile::create()
        .with_datime(rootfile::pack_datime(2026, 6, 12, 0, 0, 0))
        .with_uuids([0; 16], [0; 16]);
    // `add_*` consumes the builder; on a rejection we restore the pre-add
    // accumulator and skip just this object (the JSON/bridges still carry
    // it).
    let add =
        |root: &mut rootfile::RootFile,
         name: &str,
         f: &dyn Fn(rootfile::RootFile) -> Result<rootfile::RootFile, rootfile::Error>| {
            let snapshot = root.clone();
            match f(std::mem::take(root)) {
                Ok(next) => *root = next,
                Err(e) => {
                    eprintln!("`{name}`: skipped in out.root — {e}");
                    *root = snapshot;
                }
            }
        };

    for fill in &set.histos {
        let region_dir = bridges::dir_name(&fill.region);
        let (dir, name): (Vec<&str>, String) = if flat {
            (Vec::new(), bridges::root_name(&fill.region, &fill.name))
        } else {
            (vec![region_dir.as_str()], fill.name.clone())
        };
        match &fill.hist {
            adl_interp::HistAcc::H1(h) => {
                let spec = h1_spec(&fill.title, h);
                add(&mut root, &name, &|r| r.add_th1d_at(&dir, &name, &spec));
            }
            adl_interp::HistAcc::H1Var(h) => {
                #[allow(clippy::cast_precision_loss)]
                let spec = rootfile::H1VarSpec {
                    title: &fill.title,
                    edges: &h.edges,
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
                    entries: h.entries as f64,
                    tsumw: h.tsumw,
                    tsumw2: h.tsumw2,
                    tsumwx: h.tsumwx,
                    tsumwx2: h.tsumwx2,
                };
                add(&mut root, &name, &|r| r.add_th1d_var_at(&dir, &name, &spec));
            }
            adl_interp::HistAcc::H2(h) => {
                #[allow(clippy::cast_precision_loss)]
                let spec = rootfile::H2Spec {
                    title: &fill.title,
                    nx: h.nx,
                    xlo: h.xlo,
                    xhi: h.xhi,
                    ny: h.ny,
                    ylo: h.ylo,
                    yhi: h.yhi,
                    sumw: &h.sumw,
                    sumw2: &h.sumw2,
                    entries: h.entries as f64,
                    tsumw: h.tsumw,
                    tsumw2: h.tsumw2,
                    tsumwx: h.tsumwx,
                    tsumwx2: h.tsumwx2,
                    tsumwy: h.tsumwy,
                    tsumwy2: h.tsumwy2,
                    tsumwxy: h.tsumwxy,
                };
                add(&mut root, &name, &|r| r.add_th2d_at(&dir, &name, &spec));
            }
        }
    }

    // The §2 cutflow pair per region: bin i+1 labeled with the verbatim
    // step text, raw (Poisson) + weighted (Sumw2) companions, fEntries =
    // events processed (the `all` step's raw count).
    for flow in cutflow.regions() {
        let base = bridges::dir_name(&flow.name);
        let dir: Vec<&str> = if flat {
            Vec::new()
        } else {
            vec![base.as_str()]
        };
        let steps: Vec<rootfile::CutflowStep<'_>> = flow
            .steps
            .iter()
            .map(|s| rootfile::CutflowStep {
                label: &s.label,
                raw: s.counts.raw,
                sumw: s.counts.sumw,
                sumw2: s.counts.sumw2,
            })
            .collect();
        let processed = flow.steps[0].counts.raw;
        add(&mut root, &format!("{base}__cutflow"), &|r| {
            r.add_cutflow_at(&dir, &base, &steps, processed)
        });
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
