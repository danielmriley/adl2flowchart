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

use crate::cmd::parallel::{self, FormatEvent};
use crate::cmd::{CliError, bridges};
use crate::cmd::{read_file, unit_name};
use adl_interp::{
    BinOutcome, CutflowSet, Event, Hist1D, HistoSet, InputIdentity, Interp, Provenance,
    RegionResult, Sha256,
};
use adl_sema::{ExtDecls, analyze_str};
use adl_syntax::diag::{has_errors, render};
use serde_json::{Value, json};
use std::fmt::Write as FmtWrite;
use std::io::{self, BufReader, Cursor, Read, Write};
use std::path::Path;
use std::process::ExitCode;

/// The §6 provenance `tool` string: `smash2 <crate version>`. No git hash
/// (no build-time capture wired) — deterministic per build, which the
/// determinism guarantee needs.
fn tool_id() -> String {
    format!("smash2 {}", env!("CARGO_PKG_VERSION"))
}

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

/// Where the streamed events come from, paired with their §6 content hash.
/// The JSONL path streams the file line-by-line (bounded memory); the
/// profile path materializes the ingest's JSONL in memory (the ingest
/// crate's existing behavior) and hashes the *original* ROOT bytes.
enum EventSource {
    /// Stream the JSONL file directly; hash it in a separate O(1)-memory
    /// pass (input order, so deterministic).
    File,
    /// Profile-ingested JSONL already in memory, plus the ROOT file's hash.
    Ingested { jsonl: String, sha: String },
}

pub fn run(
    file: &Path,
    events: &Path,
    profile: Option<&str>,
    json_out: bool,
    histos: HistoOpts<'_>,
    jobs: usize,
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
    // streaming loader — the native path and the file path share every
    // event-model validation. The §6 input identity hashes the *original*
    // input bytes (the ROOT file under a profile, the JSONL otherwise),
    // never the materialized intermediate.
    let mut profile_id: Option<String> = None;
    let mut decides: Vec<(String, String)> = Vec::new();
    let source = if let Some(pname) = profile {
        let Some(prof) = adl_ingest::by_name(pname) else {
            return Err(CliError::Usage(format!(
                "unknown profile `{pname}` (known: {})",
                adl_ingest::KNOWN_PROFILES.join(", ")
            )));
        };
        if verbose {
            super::ingest::print_profile_choices(&prof);
        }
        let raw = std::fs::read(events).map_err(|source| CliError::Io {
            path: events.display().to_string(),
            source,
        })?;
        let sha = Provenance::input_sha256(&raw);
        profile_id = Some(prof.id());
        decides = prof.decides();
        let jsonl = match adl_ingest::read_root(events, &prof) {
            Ok(ingested) => {
                super::ingest::print_diags(&ingested.diags, &ingested.profile_id, verbose);
                ingested.jsonl()
            }
            Err(e) => {
                eprintln!("{}: {e}", events.display());
                return Ok(ExitCode::from(1));
            }
        };
        EventSource::Ingested { jsonl, sha }
    } else {
        EventSource::File
    };

    // The §6 input content hash. For the JSONL path it is computed in a
    // separate streaming pass (O(1) memory, input order ⇒ deterministic);
    // the profile path already hashed the original ROOT bytes above.
    let input_sha = match &source {
        EventSource::Ingested { sha, .. } => sha.clone(),
        EventSource::File => match hash_file_streaming(events) {
            Ok(sha) => sha,
            Err(e) => {
                eprintln!("{}: {e}", events.display());
                return Ok(ExitCode::from(1));
            }
        },
    };

    let interp = Interp::new(&hir, &ext);
    // Per-event stdout line. `--json`: one object per event (input order);
    // text: one `event N: region -> ...` line per region. The streaming
    // fold writes these in ascending chunk order, i.e. input order.
    let format: Box<FormatEvent<'_>> = if json_out {
        Box::new(|ord: usize, _ev: &Event, results: &[RegionResult]| {
            let regions: Vec<Value> = results.iter().map(region_json).collect();
            json!({ "event": ord, "regions": regions }).to_string()
        })
    } else {
        Box::new(|ord: usize, _ev: &Event, results: &[RegionResult]| {
            let mut s = String::new();
            for (i, r) in results.iter().enumerate() {
                if i > 0 {
                    s.push('\n');
                }
                let _ = FmtWrite::write_fmt(
                    &mut s,
                    format_args!("event {ord}: {} -> {}", r.name, region_text(r)),
                );
            }
            s
        })
    };

    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    let make_histos = || HistoSet::new(&hir);
    let make_cutflow = || CutflowSet::new(&hir, &src);

    let run_result = match &source {
        EventSource::File => {
            let file = match std::fs::File::open(events) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("{}: {e}", events.display());
                    return Ok(ExitCode::from(1));
                }
            };
            parallel::run_streaming(
                BufReader::new(file),
                &interp,
                make_histos,
                make_cutflow,
                &format,
                jobs,
                &mut out,
            )
        }
        EventSource::Ingested { jsonl, .. } => parallel::run_streaming(
            Cursor::new(jsonl.as_bytes()),
            &interp,
            make_histos,
            make_cutflow,
            &format,
            jobs,
            &mut out,
        ),
    };

    let parallel::RunOutput {
        histos: histo_set,
        cutflow,
        pass_counts,
        n_events,
    } = match run_result {
        Ok(o) => o,
        Err(e) => {
            // Flush whatever per-event output already streamed before the
            // bad line, then report the malformed input.
            let _ = out.flush();
            eprintln!("{}: {e}", events.display());
            return Ok(ExitCode::from(1));
        }
    };

    // The single §6 provenance object, embedded byte-identically in
    // histos.json, cutflow.json, out.root (TNamed), and the `--json` lines.
    let provenance = Provenance {
        tool: tool_id(),
        adl_file: name.clone(),
        adl_sha256: Provenance::adl_sha256(src.as_bytes()),
        input: Some(InputIdentity {
            file: unit_name(events),
            sha256: input_sha,
            events: n_events,
            profile: profile_id,
        }),
        seed: None,
        decides,
    };

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
        let _ = writeln!(out, "{}", histo_set.to_json_with(false, Some(&provenance)));
    }
    // Cutflow emission (SPEC_EVENT_PIPELINE §2): the per-region table on
    // stdout in text mode, one `{"cutflow": ...}` line under `--json`;
    // files with no evaluable region emit neither.
    if !cutflow.is_empty() {
        if json_out {
            let _ = writeln!(
                out,
                "{{\"cutflow\":{}}}",
                cutflow.to_json_with(false, Some(&provenance))
            );
        } else {
            if n_events > 0 {
                let _ = writeln!(out);
            }
            let _ = write!(out, "{}", cutflow.text_table());
        }
    }
    let _ = out.flush();
    // `--csv`/`--svg`/`--no-root` require `--histos` (enforced by clap), so
    // they only ever fire with a directory present.
    if let Some(dir) = histos.dir {
        write_histo_outputs(dir, &histo_set, &cutflow, &provenance, histos, verbose)?;
    }

    if verbose && !json_out {
        eprintln!("--- {n_events} events, {} regions ---", pass_counts.len());
        for (region, count) in &pass_counts {
            eprintln!("{region}: {count}/{n_events} passed");
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
    provenance: &Provenance,
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

    emit("histos.json", &set.to_json_with(true, Some(provenance)))?;
    if !cutflow.is_empty() {
        emit(
            "cutflow.json",
            &cutflow.to_json_with(true, Some(provenance)),
        )?;
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
            provenance,
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
    provenance: &Provenance,
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

    // The §6 provenance carrier: a single TNamed in the root directory,
    // title = the compact canonical JSON (the same bytes embedded in
    // histos.json / cutflow.json). `rfile.Get("smash2_provenance")
    // ->GetTitle()` parses as JSON.
    let prov_json = provenance.to_json(false);
    add(&mut root, "smash2_provenance", &|r| {
        r.add_tnamed_at(&[], "smash2_provenance", &prov_json)
    });

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

/// The §6 input content hash for the JSONL path, computed in one streaming
/// pass through a fixed 64 KiB buffer — O(1) memory even on a 1M-event
/// file. Bytes are hashed in file order, so the digest is deterministic and
/// matches a one-shot `input_sha256` of the same file.
fn hash_file_streaming(path: &Path) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize_hex())
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
