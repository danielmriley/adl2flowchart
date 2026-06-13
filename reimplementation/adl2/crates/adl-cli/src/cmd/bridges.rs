//! Bridge renderers for `histos.json` (PLAN Phase 9 + SPEC_EVENT_PIPELINE §3).
//!
//! `histos.json` is the single source of truth (`HistoSet::to_json`). These
//! renderers turn the same in-memory accumulator into the formats a
//! collaborator actually opens — without our toolchain growing a ROOT or
//! plotting dependency:
//!
//! - [`make_histos_c`] — a self-contained ROOT macro (`root -l -b -q
//!   make_histos.C`) that builds one `TH1D`/`TH2D` per histogram with
//!   `Sumw2`, full bin contents/errors (including flow bins),
//!   `SetEntries`, and the fill-time moments via `PutStats`.
//! - [`to_root_py`] — an uproot 5 + `numpy` script (`python3 to_root.py`)
//!   writing the byte-equivalent histograms through
//!   `uproot.writing.identify`.
//! - [`csv_files`] — one CSV per histogram (1-D: `bin_lo,bin_hi,content,
//!   error`; 2-D: `x_lo,x_hi,y_lo,y_hi,content,error`).
//! - [`svg_files`] — one hand-rolled quick-look SVG per histogram (step
//!   plot for 1-D forms, grayscale heatmap for 2-D; no plotting
//!   dependency).
//!
//! ROOT bin/stats conventions (SPEC_ROOT_WRITER §2, §4):
//! - 1-D: `SetBinContent(0, underflow)`, `SetBinContent(i, sumw[i-1])` for
//!   i ∈ 1..=N, `SetBinContent(N+1, overflow)`; `SetBinError(i,
//!   sqrt(sumw2))` over the same index range. 2-D: global-bin indexing
//!   (`gbin = bx + (nx+2)·by`), flow cells included.
//! - `SetEntries(entries)` is the raw fill count (ROOT `fEntries`).
//! - `PutStats` writes the in-range fill-time moments (4 for 1-D, 7 for
//!   2-D) so `GetMean`/`GetStdDev` and merged `hadd` stats stay exact.
//!
//! Layout: per-region TDirectories by default (`SR/hmet` — rootfile v2,
//! SPEC_EVENT_PIPELINE §3); the `flat` argument switches every renderer to
//! the v1 flat names (`SR_hmet`, `--flat-names`). CSV/SVG file stems are
//! always the flat name (one directory of quick-look files). Names are
//! stable across runs so `hadd` merges by name in both layouts.
//!
//! Every renderer is a pure function of the [`HistoSet`] and is
//! byte-deterministic; nothing here reads the clock or the environment.

use adl_interp::{Hist2D, HistAcc, HistoSet};
use std::fmt::Write as _;

/// The flat, region-prefixed ROOT object name (`SR_hmet`). The region path
/// separator `/` (none in v1, but future-proof) collapses to `_`.
///
/// Shared by every bridge renderer AND the native `rootfile` writer so all
/// four outputs (`out.root`, `make_histos.C`, `to_root.py`, CSV/SVG stems)
/// agree on object names and stay `hadd`-mergeable.
pub(crate) fn root_name(region: &str, name: &str) -> String {
    format!("{}_{name}", dir_name(region))
}

/// The directory component for a region in the TDirectory layout (any
/// future `/` in a region path collapses — directories are created per
/// region, not per path segment).
pub(crate) fn dir_name(region: &str) -> String {
    region.replace('/', "_")
}

/// Object path as written/read in a ROOT file under the chosen layout.
fn object_path(region: &str, name: &str, flat: bool) -> String {
    if flat {
        root_name(region, name)
    } else {
        format!("{}/{name}", dir_name(region))
    }
}

/// A safe filename stem for the per-histogram CSV/SVG files: the flat ROOT
/// name with any character outside `[A-Za-z0-9._-]` replaced by `_`.
fn file_stem(region: &str, name: &str) -> String {
    root_name(region, name)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// A finite `f64` rendered as shortest round-trip text (serde_json/ryu),
/// shared by every numeric emitter so the bridges agree bit-for-bit with
/// `histos.json`.
fn num(v: f64) -> String {
    serde_json::to_string(&v).expect("finite f64 serializes")
}

/// Per-bin error: ROOT's `sqrt(sumw2)` (we always fill weighted, so the
/// error is the square root of the sum of squared weights).
fn bin_error(sumw2: f64) -> f64 {
    sumw2.sqrt()
}

/// 1-D bin edges, whatever the form: uniform forms synthesize them, the
/// variable form carries them.
fn edges_of(nbins: u32, lo: f64, hi: f64) -> Vec<f64> {
    let width = (hi - lo) / f64::from(nbins);
    (0..=nbins)
        .map(|i| {
            if i == nbins {
                hi
            } else {
                lo + width * f64::from(i)
            }
        })
        .collect()
}

// --- make_histos.C (ROOT macro) ------------------------------------------

/// Escape a string for a C/C++ double-quoted literal in the ROOT macro.
/// Titles are collaborator-authored ADL text, so backslashes, quotes, and
/// control characters all get escaped rather than trusted.
fn c_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// Render `make_histos.C`: a self-contained ROOT macro writing one
/// histogram object per fill point into `histos.root`, laid out per
/// `flat` (module docs).
#[must_use]
pub fn make_histos_c(set: &HistoSet, flat: bool) -> String {
    let mut s = String::new();
    s.push_str(
        "// Generated by smash2 from histos.json — do not edit.\n\
         // Run:  root -l -b -q make_histos.C\n\
         // Produces histos.root with one TH1D/TH2D per histogram\n\
         // (Sumw2 errors and fill-time stats intact).\n\
         #include \"TFile.h\"\n\
         #include \"TH1D.h\"\n\
         #include \"TH2D.h\"\n\n\
         void make_histos() {\n\
         \x20 TFile* f = TFile::Open(\"histos.root\", \"RECREATE\");\n\n",
    );

    if !flat {
        let mut seen: Vec<&str> = Vec::new();
        for fill in &set.histos {
            if !seen.contains(&fill.region.as_str()) {
                seen.push(&fill.region);
                let _ = writeln!(s, "  f->mkdir(\"{}\");", c_escape(&dir_name(&fill.region)));
            }
        }
        if !seen.is_empty() {
            s.push('\n');
        }
    }

    for fill in &set.histos {
        let rname = if flat {
            root_name(&fill.region, &fill.name)
        } else {
            fill.name.clone()
        };
        let _ = writeln!(s, "  // {} / {}", fill.region, fill.name);
        s.push_str("  {\n");
        if !flat {
            let _ = writeln!(s, "    f->cd(\"{}\");", c_escape(&dir_name(&fill.region)));
        }
        match &fill.hist {
            HistAcc::H1(h) => {
                let _ = writeln!(
                    s,
                    "    TH1D* h = new TH1D(\"{}\", \"{}\", {}, {}, {});",
                    c_escape(&rname),
                    c_escape(&fill.title),
                    h.nbins,
                    num(h.lo),
                    num(h.hi),
                );
                s.push_str("    h->Sumw2();\n");
                emit_c_h1_bins(
                    &mut s,
                    h.nbins,
                    &h.sumw,
                    &h.sumw2,
                    (h.underflow_w, h.underflow_w2),
                    (h.overflow_w, h.overflow_w2),
                );
                let _ = writeln!(s, "    h->SetEntries({});", h.entries);
                emit_c_stats(&mut s, &[h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2]);
            }
            HistAcc::H1Var(h) => {
                let edge_list = h
                    .edges
                    .iter()
                    .map(|&e| num(e))
                    .collect::<Vec<_>>()
                    .join(", ");
                let _ = writeln!(s, "    Double_t edges[] = {{{edge_list}}};");
                let nbins = h.sumw.len();
                let _ = writeln!(
                    s,
                    "    TH1D* h = new TH1D(\"{}\", \"{}\", {nbins}, edges);",
                    c_escape(&rname),
                    c_escape(&fill.title),
                );
                s.push_str("    h->Sumw2();\n");
                #[allow(clippy::cast_possible_truncation)]
                emit_c_h1_bins(
                    &mut s,
                    nbins as u32,
                    &h.sumw,
                    &h.sumw2,
                    (h.underflow_w, h.underflow_w2),
                    (h.overflow_w, h.overflow_w2),
                );
                let _ = writeln!(s, "    h->SetEntries({});", h.entries);
                emit_c_stats(&mut s, &[h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2]);
            }
            HistAcc::H2(h) => {
                let _ = writeln!(
                    s,
                    "    TH2D* h = new TH2D(\"{}\", \"{}\", {}, {}, {}, {}, {}, {});",
                    c_escape(&rname),
                    c_escape(&fill.title),
                    h.nx,
                    num(h.xlo),
                    num(h.xhi),
                    h.ny,
                    num(h.ylo),
                    num(h.yhi),
                );
                s.push_str("    h->Sumw2();\n");
                // Global-bin indexing covers the flow cells too.
                for (gbin, (&w, &w2)) in h.sumw.iter().zip(&h.sumw2).enumerate() {
                    if w != 0.0 || w2 != 0.0 {
                        emit_c_bin(&mut s, gbin as i64, w, w2);
                    }
                }
                let _ = writeln!(s, "    h->SetEntries({});", h.entries);
                emit_c_stats(
                    &mut s,
                    &[
                        h.tsumw, h.tsumw2, h.tsumwx, h.tsumwx2, h.tsumwy, h.tsumwy2, h.tsumwxy,
                    ],
                );
            }
        }
        s.push_str("    h->Write();\n");
        if !flat {
            s.push_str("    f->cd();\n");
        }
        s.push_str("  }\n\n");
    }

    s.push_str("  f->Write();\n  f->Close();\n}\n");
    s
}

fn emit_c_h1_bins(
    s: &mut String,
    nbins: u32,
    sumw: &[f64],
    sumw2: &[f64],
    under: (f64, f64),
    over: (f64, f64),
) {
    // Underflow (bin 0), in-range bins 1..=N, overflow (bin N+1).
    emit_c_bin(s, 0, under.0, under.1);
    for i in 0..nbins as usize {
        #[allow(clippy::cast_possible_truncation)]
        emit_c_bin(s, (i + 1) as i64, sumw[i], sumw2[i]);
    }
    emit_c_bin(s, i64::from(nbins) + 1, over.0, over.1);
}

fn emit_c_stats(s: &mut String, stats: &[f64]) {
    let list = stats.iter().map(|&v| num(v)).collect::<Vec<_>>().join(", ");
    let _ = writeln!(s, "    Double_t stats[{}] = {{{list}}};", stats.len());
    s.push_str("    h->PutStats(stats);\n");
}

fn emit_c_bin(s: &mut String, bin: i64, w: f64, w2: f64) {
    let _ = writeln!(s, "    h->SetBinContent({bin}, {});", num(w));
    let _ = writeln!(s, "    h->SetBinError({bin}, {});", num(bin_error(w2)));
}

// --- to_root.py (uproot 5) -----------------------------------------------

/// Escape a string for a Python single-quoted literal.
fn py_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

/// A Python list literal of finite floats, shortest round-trip text.
fn py_float_list(vs: &[f64]) -> String {
    let mut out = String::from("[");
    for (i, &v) in vs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&num(v));
    }
    out.push(']');
    out
}

/// Render `to_root.py`: an uproot 5 + numpy script writing the
/// byte-equivalent histograms. Builds each one through
/// `uproot.writing.identify.to_TH1x`/`to_TH2x` so `fEntries`, the
/// fill-time moments, variable edges (TAxis `fXbins`) and the directory
/// layout are set exactly (the high-level `hist` path does not preserve
/// raw entry counts or fill-time moments).
#[must_use]
pub fn to_root_py(set: &HistoSet, flat: bool) -> String {
    let mut s = String::new();
    s.push_str(
        "#!/usr/bin/env python3\n\
         \"\"\"Generated by smash2 from histos.json — do not edit.\n\n\
         Run:  python3 to_root.py   (requires uproot>=5 and numpy)\n\
         Produces histos.root with one TH1D/TH2D per histogram,\n\
         byte-equivalent to make_histos.C.\n\
         \"\"\"\n\
         import numpy as np\n\
         import uproot\n\
         from uproot.writing.identify import to_TAxis, to_TH1x, to_TH2x\n\n\
         def _axis(name, nbins, lo, hi, edges=None):\n\
         \x20   kw = {}\n\
         \x20   if edges is not None:\n\
         \x20       kw[\"fXbins\"] = np.array(edges, dtype=\">f8\")\n\
         \x20   return to_TAxis(fName=name, fTitle=\"\", fNbins=nbins, fXmin=lo, fXmax=hi, **kw)\n\n\
         def _th1(name, title, nbins, lo, hi, contents, errors2,\n\
         \x20        entries, tsumw, tsumw2, tsumwx, tsumwx2, edges=None):\n\
         \x20   # contents/errors2 length is nbins+2 (underflow .. overflow).\n\
         \x20   data = np.array(contents, dtype=\">f8\")\n\
         \x20   fSumw2 = np.array(errors2, dtype=\">f8\")\n\
         \x20   return to_TH1x(\n\
         \x20       fName=name, fTitle=title, data=data, fEntries=entries,\n\
         \x20       fTsumw=tsumw, fTsumw2=tsumw2, fTsumwx=tsumwx, fTsumwx2=tsumwx2,\n\
         \x20       fSumw2=fSumw2, fXaxis=_axis(\"xaxis\", nbins, lo, hi, edges))\n\n\
         def _th2(name, title, nx, xlo, xhi, ny, ylo, yhi, contents, errors2,\n\
         \x20        entries, tsumw, tsumw2, tsumwx, tsumwx2, tsumwy, tsumwy2, tsumwxy):\n\
         \x20   # contents/errors2: (nx+2)*(ny+2) cells, ROOT global-bin order.\n\
         \x20   data = np.array(contents, dtype=\">f8\")\n\
         \x20   fSumw2 = np.array(errors2, dtype=\">f8\")\n\
         \x20   return to_TH2x(\n\
         \x20       fName=name, fTitle=title, data=data, fEntries=entries,\n\
         \x20       fTsumw=tsumw, fTsumw2=tsumw2, fTsumwx=tsumwx, fTsumwx2=tsumwx2,\n\
         \x20       fTsumwy=tsumwy, fTsumwy2=tsumwy2, fTsumwxy=tsumwxy,\n\
         \x20       fSumw2=fSumw2, fXaxis=_axis(\"xaxis\", nx, xlo, xhi),\n\
         \x20       fYaxis=_axis(\"yaxis\", ny, ylo, yhi))\n\n\
         def main():\n\
         \x20   with uproot.recreate(\"histos.root\") as f:\n",
    );

    for fill in &set.histos {
        let path = object_path(&fill.region, &fill.name, flat);
        let oname = if flat {
            root_name(&fill.region, &fill.name)
        } else {
            fill.name.clone()
        };
        let _ = writeln!(s, "        # {} / {}", fill.region, fill.name);
        match &fill.hist {
            HistAcc::H1(h) => {
                let contents = flow_1d(&h.sumw, h.underflow_w, h.overflow_w);
                let errors2 = flow_1d(&h.sumw2, h.underflow_w2, h.overflow_w2);
                let _ = writeln!(
                    s,
                    "        f[{path:?}] = _th1(\n            name='{name}', title='{title}',\n\
                     \x20           nbins={nbins}, lo={lo}, hi={hi},\n\
                     \x20           contents={contents},\n\
                     \x20           errors2={errors2},\n\
                     \x20           entries={entries}, tsumw={tsumw}, tsumw2={tsumw2},\n\
                     \x20           tsumwx={tsumwx}, tsumwx2={tsumwx2})",
                    name = py_escape(&oname),
                    title = py_escape(&fill.title),
                    nbins = h.nbins,
                    lo = num(h.lo),
                    hi = num(h.hi),
                    contents = py_float_list(&contents),
                    errors2 = py_float_list(&errors2),
                    entries = h.entries,
                    tsumw = num(h.tsumw),
                    tsumw2 = num(h.tsumw2),
                    tsumwx = num(h.tsumwx),
                    tsumwx2 = num(h.tsumwx2),
                );
            }
            HistAcc::H1Var(h) => {
                let contents = flow_1d(&h.sumw, h.underflow_w, h.overflow_w);
                let errors2 = flow_1d(&h.sumw2, h.underflow_w2, h.overflow_w2);
                let n = h.sumw.len();
                let _ = writeln!(
                    s,
                    "        f[{path:?}] = _th1(\n            name='{name}', title='{title}',\n\
                     \x20           nbins={n}, lo={lo}, hi={hi},\n\
                     \x20           contents={contents},\n\
                     \x20           errors2={errors2},\n\
                     \x20           entries={entries}, tsumw={tsumw}, tsumw2={tsumw2},\n\
                     \x20           tsumwx={tsumwx}, tsumwx2={tsumwx2},\n\
                     \x20           edges={edges})",
                    name = py_escape(&oname),
                    title = py_escape(&fill.title),
                    lo = num(h.edges[0]),
                    hi = num(h.edges[n]),
                    contents = py_float_list(&contents),
                    errors2 = py_float_list(&errors2),
                    entries = h.entries,
                    tsumw = num(h.tsumw),
                    tsumw2 = num(h.tsumw2),
                    tsumwx = num(h.tsumwx),
                    tsumwx2 = num(h.tsumwx2),
                    edges = py_float_list(&h.edges),
                );
            }
            HistAcc::H2(h) => {
                let _ = writeln!(
                    s,
                    "        f[{path:?}] = _th2(\n            name='{name}', title='{title}',\n\
                     \x20           nx={nx}, xlo={xlo}, xhi={xhi}, ny={ny}, ylo={ylo}, yhi={yhi},\n\
                     \x20           contents={contents},\n\
                     \x20           errors2={errors2},\n\
                     \x20           entries={entries}, tsumw={tsumw}, tsumw2={tsumw2},\n\
                     \x20           tsumwx={tsumwx}, tsumwx2={tsumwx2},\n\
                     \x20           tsumwy={tsumwy}, tsumwy2={tsumwy2}, tsumwxy={tsumwxy})",
                    name = py_escape(&oname),
                    title = py_escape(&fill.title),
                    nx = h.nx,
                    xlo = num(h.xlo),
                    xhi = num(h.xhi),
                    ny = h.ny,
                    ylo = num(h.ylo),
                    yhi = num(h.yhi),
                    contents = py_float_list(&h.sumw),
                    errors2 = py_float_list(&h.sumw2),
                    entries = h.entries,
                    tsumw = num(h.tsumw),
                    tsumw2 = num(h.tsumw2),
                    tsumwx = num(h.tsumwx),
                    tsumwx2 = num(h.tsumwx2),
                    tsumwy = num(h.tsumwy),
                    tsumwy2 = num(h.tsumwy2),
                    tsumwxy = num(h.tsumwxy),
                );
            }
        }
    }

    s.push_str("\nif __name__ == \"__main__\":\n    main()\n");
    s
}

/// 1-D bin contents in ROOT TArrayD order: `[underflow, bins.., overflow]`.
fn flow_1d(bins: &[f64], under: f64, over: f64) -> Vec<f64> {
    let mut v = Vec::with_capacity(bins.len() + 2);
    v.push(under);
    v.extend_from_slice(bins);
    v.push(over);
    v
}

// --- CSV ------------------------------------------------------------------

/// One CSV per histogram. Each is `(filename, contents)`; the filename is
/// `<flat-name>.csv`. Rows are the in-range bins/cells only (flow bins are
/// out of the visible axis and live in the JSON). Header line included.
#[must_use]
pub fn csv_files(set: &HistoSet) -> Vec<(String, String)> {
    set.histos
        .iter()
        .map(|fill| {
            let body = match &fill.hist {
                HistAcc::H1(h) => csv_1d(&edges_of(h.nbins, h.lo, h.hi), &h.sumw, &h.sumw2),
                HistAcc::H1Var(h) => csv_1d(&h.edges, &h.sumw, &h.sumw2),
                HistAcc::H2(h) => {
                    let xedges = edges_of(h.nx, h.xlo, h.xhi);
                    let yedges = edges_of(h.ny, h.ylo, h.yhi);
                    let mut body = String::from("x_lo,x_hi,y_lo,y_hi,content,error\n");
                    for by in 1..=h.ny as usize {
                        for bx in 1..=h.nx as usize {
                            let gbin = bx + (h.nx as usize + 2) * by;
                            let _ = writeln!(
                                body,
                                "{},{},{},{},{},{}",
                                num(xedges[bx - 1]),
                                num(xedges[bx]),
                                num(yedges[by - 1]),
                                num(yedges[by]),
                                num(h.sumw[gbin]),
                                num(bin_error(h.sumw2[gbin])),
                            );
                        }
                    }
                    body
                }
            };
            (format!("{}.csv", file_stem(&fill.region, &fill.name)), body)
        })
        .collect()
}

fn csv_1d(edges: &[f64], sumw: &[f64], sumw2: &[f64]) -> String {
    let mut body = String::from("bin_lo,bin_hi,content,error\n");
    for i in 0..sumw.len() {
        let _ = writeln!(
            body,
            "{},{},{},{}",
            num(edges[i]),
            num(edges[i + 1]),
            num(sumw[i]),
            num(bin_error(sumw2[i])),
        );
    }
    body
}

// --- SVG (hand-rolled quick-looks) -----------------------------------------

/// Escape text for an SVG text/attribute context.
fn svg_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

/// Format a coordinate with at most 3 decimals, trailing zeros trimmed, so
/// the SVG path data stays compact and byte-stable.
fn coord(v: f64) -> String {
    let mut t = format!("{v:.3}");
    if t.contains('.') {
        while t.ends_with('0') {
            t.pop();
        }
        if t.ends_with('.') {
            t.pop();
        }
    }
    if t == "-0" { "0".to_owned() } else { t }
}

/// One quick-look SVG per histogram. 1-D forms draw a filled step outline
/// over the (possibly variable) bin edges; the 2-D form draws a grayscale
/// heatmap of the in-range cells. Flow bins/cells are noted in the caption
/// but not drawn (they sit off the visible axes).
#[must_use]
pub fn svg_files(set: &HistoSet) -> Vec<(String, String)> {
    set.histos
        .iter()
        .map(|fill| {
            let rname = root_name(&fill.region, &fill.name);
            let body = match &fill.hist {
                HistAcc::H1(h) => svg_step(
                    &fill.title,
                    &rname,
                    &edges_of(h.nbins, h.lo, h.hi),
                    &h.sumw,
                    h.underflow_w,
                    h.overflow_w,
                    h.entries,
                ),
                HistAcc::H1Var(h) => svg_step(
                    &fill.title,
                    &rname,
                    &h.edges,
                    &h.sumw,
                    h.underflow_w,
                    h.overflow_w,
                    h.entries,
                ),
                HistAcc::H2(h) => svg_heatmap(&fill.title, &rname, h),
            };
            (format!("{}.svg", file_stem(&fill.region, &fill.name)), body)
        })
        .collect()
}

const SVG_W: f64 = 640.0;
const SVG_H: f64 = 400.0;
const PAD_L: f64 = 56.0;
const PAD_R: f64 = 16.0;
const PAD_T: f64 = 36.0;
const PAD_B: f64 = 44.0;

fn svg_header(title: &str, caption: &str) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h_}\" \
         viewBox=\"0 0 {w} {h_}\" font-family=\"sans-serif\">\n\
         \x20 <rect width=\"{w}\" height=\"{h_}\" fill=\"white\"/>\n\
         \x20 <text x=\"{tx}\" y=\"20\" font-size=\"15\" text-anchor=\"middle\">{title}</text>\n\
         \x20 <text x=\"{tx}\" y=\"{cap_y}\" font-size=\"11\" text-anchor=\"middle\" \
         fill=\"#555\">{caption}</text>\n",
        w = coord(SVG_W),
        h_ = coord(SVG_H),
        tx = coord(SVG_W / 2.0),
        title = svg_escape(title),
        cap_y = coord(SVG_H - 12.0),
        caption = svg_escape(caption),
    );
    s
}

fn svg_step(
    title: &str,
    rname: &str,
    edges: &[f64],
    sumw: &[f64],
    underflow: f64,
    overflow: f64,
    entries: u64,
) -> String {
    let plot_w = SVG_W - PAD_L - PAD_R;
    let plot_h = SVG_H - PAD_T - PAD_B;
    let (lo, hi) = (edges[0], edges[edges.len() - 1]);
    let ymax = sumw.iter().copied().fold(0.0_f64, f64::max).max(1.0);

    // x maps [lo, hi] across the plot width; y maps [0, ymax] up from the
    // baseline (SVG y grows downward).
    let x_at = |v: f64| PAD_L + (v - lo) / (hi - lo) * plot_w;
    let y_at = |v: f64| PAD_T + plot_h - (v / ymax) * plot_h;

    // Step outline: start at the baseline, walk each bin's top edge, return
    // to the baseline — one closed polygon for a clean filled quick-look.
    let mut d = String::new();
    let _ = write!(d, "M {} {}", coord(x_at(lo)), coord(y_at(0.0)));
    for (i, &w) in sumw.iter().enumerate() {
        let top = y_at(w);
        let _ = write!(d, " L {} {}", coord(x_at(edges[i])), coord(top));
        let _ = write!(d, " L {} {}", coord(x_at(edges[i + 1])), coord(top));
    }
    let _ = write!(d, " L {} {} Z", coord(x_at(hi)), coord(y_at(0.0)));

    let baseline = coord(y_at(0.0));
    let flow_note = if underflow != 0.0 || overflow != 0.0 {
        format!("  underflow={}  overflow={}", num(underflow), num(overflow))
    } else {
        String::new()
    };
    let caption = format!(
        "{rname}  [{}, {}) x{}  entries={entries}{flow_note}",
        num(lo),
        num(hi),
        sumw.len()
    );

    let mut s = svg_header(title, &caption);
    let _ = write!(
        s,
        "\x20 <line x1=\"{px}\" y1=\"{base}\" x2=\"{pxr}\" y2=\"{base}\" stroke=\"#000\"/>\n\
         \x20 <line x1=\"{px}\" y1=\"{ptop}\" x2=\"{px}\" y2=\"{base}\" stroke=\"#000\"/>\n\
         \x20 <text x=\"{px}\" y=\"{ymax_y}\" font-size=\"10\" text-anchor=\"end\" \
         fill=\"#333\">{ymax}</text>\n\
         \x20 <path d=\"{d}\" fill=\"#cfe3f7\" stroke=\"#1f6fc4\" stroke-width=\"1\"/>\n\
         </svg>\n",
        px = coord(PAD_L),
        pxr = coord(SVG_W - PAD_R),
        base = baseline,
        ptop = coord(PAD_T),
        ymax_y = coord(PAD_T + 4.0),
        ymax = num(ymax),
        d = d,
    );
    s
}

/// Grayscale heatmap of the in-range cells (white = 0, near-black = the
/// hottest cell). Deterministic integer shading.
fn svg_heatmap(title: &str, rname: &str, h: &Hist2D) -> String {
    let plot_w = SVG_W - PAD_L - PAD_R;
    let plot_h = SVG_H - PAD_T - PAD_B;
    let (nx, ny) = (h.nx as usize, h.ny as usize);
    let mut vmax = 0.0_f64;
    let mut flow = 0.0_f64;
    for by in 0..ny + 2 {
        for bx in 0..nx + 2 {
            let v = h.sumw[bx + (nx + 2) * by];
            if bx >= 1 && bx <= nx && by >= 1 && by <= ny {
                vmax = vmax.max(v);
            } else {
                flow += v;
            }
        }
    }
    let vmax = vmax.max(1.0);

    let flow_note = if flow != 0.0 {
        format!("  flow={}", num(flow))
    } else {
        String::new()
    };
    let caption = format!(
        "{rname}  x[{}, {}) x{}  y[{}, {}) x{}  entries={}{flow_note}",
        num(h.xlo),
        num(h.xhi),
        h.nx,
        num(h.ylo),
        num(h.yhi),
        h.ny,
        h.entries
    );

    let mut s = svg_header(title, &caption);
    #[allow(clippy::cast_precision_loss)]
    let (cw, ch) = (plot_w / nx as f64, plot_h / ny as f64);
    for by in 1..=ny {
        for bx in 1..=nx {
            let v = h.sumw[bx + (nx + 2) * by];
            // Shade 255 (white) → 40 (near-black), rounded — deterministic.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let shade = 255 - ((v / vmax) * 215.0).round() as u32;
            #[allow(clippy::cast_precision_loss)]
            let x = PAD_L + cw * (bx - 1) as f64;
            // y axis grows upward: row 1 sits at the bottom.
            #[allow(clippy::cast_precision_loss)]
            let y = PAD_T + plot_h - ch * by as f64;
            let _ = writeln!(
                s,
                "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" \
                 fill=\"#{shade:02x}{shade:02x}{shade:02x}\" stroke=\"#ddd\" \
                 stroke-width=\"0.5\"/>",
                coord(x),
                coord(y),
                coord(cw),
                coord(ch),
            );
        }
    }
    let _ = write!(
        s,
        "\x20 <rect x=\"{px}\" y=\"{ptop}\" width=\"{pw}\" height=\"{ph}\" fill=\"none\" \
         stroke=\"#000\"/>\n</svg>\n",
        px = coord(PAD_L),
        ptop = coord(PAD_T),
        pw = coord(plot_w),
        ph = coord(plot_h),
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_name_is_region_prefixed_and_path_safe() {
        assert_eq!(root_name("SR", "hmet"), "SR_hmet");
        assert_eq!(root_name("a/b", "h"), "a_b_h");
        assert_eq!(object_path("SR", "hmet", true), "SR_hmet");
        assert_eq!(object_path("SR", "hmet", false), "SR/hmet");
        assert_eq!(object_path("a/b", "h", false), "a_b/h");
    }

    #[test]
    fn c_escape_handles_quotes_backslashes_controls() {
        assert_eq!(c_escape("a\"b\\c"), "a\\\"b\\\\c");
        assert_eq!(c_escape("x\ty"), "x\\ty");
        assert_eq!(c_escape("\u{1}"), "\\x01");
    }

    #[test]
    fn coord_trims_and_kills_negative_zero() {
        assert_eq!(coord(1.0), "1");
        assert_eq!(coord(1.500), "1.5");
        assert_eq!(coord(-0.0), "0");
    }

    #[test]
    fn edges_of_is_exact_at_both_ends() {
        let e = edges_of(4, 0.0, 100.0);
        assert_eq!(e, vec![0.0, 25.0, 50.0, 75.0, 100.0]);
        assert_eq!(edges_of(3, -1.0, 1.0).last().copied(), Some(1.0));
    }
}
