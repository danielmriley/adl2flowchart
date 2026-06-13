//! Minimal pure-Rust writer for ROOT files containing histograms.
//!
//! Implements `reimplementation/SPEC_ROOT_WRITER.md` v1 plus the
//! SPEC_EVENT_PIPELINE §3 additions (rootfile v2): small-format TFile
//! header, TKey v4 records, uncompressed records only
//! (`fObjlen == fNbytes - fKeylen`), vendored uproot StreamerInfo blobs,
//! a terminal free-list segment, and now
//!
//! - **per-directory placement** — every `add_*_at` method takes a
//!   directory path (`&["SR1"]`); intermediate directories are created on
//!   demand and the root path is `&[]` (the v1 flat layout);
//! - **variable-bin TH1D** ([`H1VarSpec`], TAxis `fXbins`);
//! - **labeled TH1D** ([`RootFile::add_labeled_th1d_at`], TAxis `fLabels`
//!   as a real THashList of TObjStrings) and the SPEC_EVENT_PIPELINE §2
//!   cutflow pair built on it ([`RootFile::add_cutflow_at`]);
//! - **TH2D** ([`H2Spec`]);
//! - **TNamed** ([`RootFile::add_tnamed_at`], the §6 provenance carrier).
//!
//! ```no_run
//! use rootfile::{FlowBin, H1Spec, RootFile};
//!
//! RootFile::create()
//!     .add_th1d_at(&["SR1"], "h_met", &H1Spec {
//!         title: "MET [GeV]",
//!         nbins: 4,
//!         lo: 0.0,
//!         hi: 100.0,
//!         sumw: &[2.0, 0.0, 3.25, 4.0],
//!         sumw2: &[4.0, 0.0, 5.0625, 8.0],
//!         under: FlowBin { w: 1.5, w2: 2.25 },
//!         over: FlowBin { w: 0.5, w2: 0.25 },
//!         entries: 11.0,
//!         tsumw: 9.25,
//!         tsumw2: 17.0625,
//!         tsumwx: 300.5,
//!         tsumwx2: 20000.25,
//!     })?
//!     .finish("out/histos.root")?;
//! # Ok::<(), rootfile::Error>(())
//! ```
//!
//! The output opens natively in ROOT (TBrowser/hadd/PyROOT) and uproot; the
//! CI oracle is uproot read-back plus byte-diffs of every object payload
//! against uproot-written references (see `tests/uproot_oracle.rs`).

mod datime;
mod file;
mod th1d;
mod th2d;
mod wbuf;

pub mod reader;

pub use datime::{now_datime, pack_datime};

use std::path::Path;

/// Sum-of-weights / sum-of-weights² for one flow (under/over) bin.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FlowBin {
    pub w: f64,
    pub w2: f64,
}

/// One uniform-bin TH1D, in `histos.json` terms.
///
/// `sumw`/`sumw2` are the in-range bins only (`nbins` entries each); flow
/// bins ride separately in `under`/`over`. `entries` is the raw fill count
/// (ROOT `fEntries` semantics, not Σw). The four moments are fill-time
/// accumulations: Σw, Σw², Σw·x, Σw·x² — never zeros (GetMean/hadd depend
/// on them).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct H1Spec<'a> {
    pub title: &'a str,
    pub nbins: u32,
    pub lo: f64,
    pub hi: f64,
    pub sumw: &'a [f64],
    pub sumw2: &'a [f64],
    pub under: FlowBin,
    pub over: FlowBin,
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
}

/// One variable-bin TH1D (SPEC_EVENT_PIPELINE §3): `edges` holds the
/// `n + 1` strictly increasing bin edges (TAxis `fXbins`; `fXmin`/`fXmax`
/// are the first/last edge); everything else as in [`H1Spec`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct H1VarSpec<'a> {
    pub title: &'a str,
    pub edges: &'a [f64],
    pub sumw: &'a [f64],
    pub sumw2: &'a [f64],
    pub under: FlowBin,
    pub over: FlowBin,
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
}

/// One TH2D (SPEC_EVENT_PIPELINE §3). Unlike the 1-D specs, `sumw`/`sumw2`
/// are **flow-inclusive**: `(nx+2)·(ny+2)` cells in ROOT global-bin order
/// (`gbin = bx + (nx+2)·by`, x fastest) — the same layout the §3
/// accumulator and `histos.json` v2 carry. The seven moments are fill-time
/// accumulations over in-range fills.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct H2Spec<'a> {
    pub title: &'a str,
    pub nx: u32,
    pub xlo: f64,
    pub xhi: f64,
    pub ny: u32,
    pub ylo: f64,
    pub yhi: f64,
    pub sumw: &'a [f64],
    pub sumw2: &'a [f64],
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
    pub tsumwy: f64,
    pub tsumwy2: f64,
    pub tsumwxy: f64,
}

/// One cutflow step (SPEC_EVENT_PIPELINE §2): the verbatim statement text
/// and the survivors' raw count / Σw / Σw².
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutflowStep<'a> {
    pub label: &'a str,
    pub raw: u64,
    pub sumw: f64,
    pub sumw2: f64,
}

/// Errors from building or writing a ROOT file.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Histogram rejected before serialization (empty/duplicate name,
    /// zero bins, array length mismatch, non-finite or inverted edges).
    BadHisto { name: String, reason: String },
    /// Directory path rejected (empty component, `/` inside a component,
    /// or a component colliding with an existing object name).
    BadDir { path: String, reason: String },
    /// `finish` needs a path with a UTF-8 file name (short enough for a
    /// TKey's i16 fKeylen).
    BadPath { path: String },
    /// The small-format layout caps files below `kStartBigFile` (2 GB).
    TooLarge { bytes: usize },
    /// I/O failure writing the output file.
    Io(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::BadHisto { name, reason } => write!(f, "histogram '{name}': {reason}"),
            Error::BadDir { path, reason } => write!(f, "directory '{path}': {reason}"),
            Error::BadPath { path } => write!(f, "not a writable file path: {path}"),
            Error::TooLarge { bytes } => {
                write!(f, "file would be {bytes} bytes; small-format cap is 2 GB")
            }
            Error::Io(e) => write!(f, "i/o error: {e}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

/// In-memory builder for a write-once ROOT file.
#[derive(Debug, Default, Clone)]
#[must_use = "RootFile does nothing until finish() or to_bytes()"]
pub struct RootFile {
    root: file::Dir,
    datime: Option<u32>,
    uuids: Option<([u8; 16], [u8; 16])>,
}

impl RootFile {
    /// Start an empty file.
    pub fn create() -> Self {
        Self::default()
    }

    /// Pin every TDatime field (header keys and directory timestamps) to a
    /// packed value — see [`pack_datime`]. Defaults to the current UTC time.
    /// Intended for byte-stable output and tests.
    pub fn with_datime(mut self, packed: u32) -> Self {
        self.datime = Some(packed);
        self
    }

    /// Pin the header and directory UUIDs (all directories share the
    /// second one). Defaults to pseudo-random values; pin them for
    /// byte-stable output and tests.
    pub fn with_uuids(mut self, header: [u8; 16], dir: [u8; 16]) -> Self {
        self.uuids = Some((header, dir));
        self
    }

    /// Add one uniform-bin TH1D in the root directory (the v1 flat layout).
    ///
    /// # Errors
    /// See [`RootFile::add_th1d_at`].
    pub fn add_th1d(self, name: &str, spec: &H1Spec<'_>) -> Result<Self, Error> {
        self.add_th1d_at(&[], name, spec)
    }

    /// Add one uniform-bin TH1D under the directory `dir` (created on
    /// demand; `&[]` is the root).
    ///
    /// # Errors
    /// [`Error::BadHisto`] on empty or duplicate names, `nbins == 0`,
    /// `sumw`/`sumw2` lengths differing from `nbins`, or non-finite /
    /// inverted axis edges; [`Error::BadDir`] on a bad directory path.
    pub fn add_th1d_at(self, dir: &[&str], name: &str, spec: &H1Spec<'_>) -> Result<Self, Error> {
        self.add_h1(dir, name, spec, None)
    }

    /// Add one labeled TH1D (TAxis `fLabels`): a uniform-bin histogram
    /// whose bins carry text labels — the SPEC_EVENT_PIPELINE §2 cutflow
    /// carrier. `labels.len()` must equal `spec.nbins`.
    ///
    /// # Errors
    /// As [`RootFile::add_th1d_at`], plus a label-count mismatch.
    pub fn add_labeled_th1d_at(
        self,
        dir: &[&str],
        name: &str,
        spec: &H1Spec<'_>,
        labels: &[&str],
    ) -> Result<Self, Error> {
        if labels.len() != spec.nbins as usize {
            return Err(Error::BadHisto {
                name: name.to_owned(),
                reason: format!("{} labels for {} bins", labels.len(), spec.nbins),
            });
        }
        self.add_h1(
            dir,
            name,
            spec,
            Some(labels.iter().map(|&l| l.to_owned()).collect()),
        )
    }

    fn add_h1(
        mut self,
        dir: &[&str],
        name: &str,
        spec: &H1Spec<'_>,
        labels: Option<Vec<String>>,
    ) -> Result<Self, Error> {
        let bad = |reason: String| Error::BadHisto {
            name: name.to_owned(),
            reason,
        };
        if spec.nbins == 0 {
            return Err(bad("nbins must be >= 1".into()));
        }
        let n = spec.nbins as usize;
        if spec.sumw.len() != n || spec.sumw2.len() != n {
            return Err(bad(format!(
                "sumw/sumw2 lengths {}/{} != nbins {n}",
                spec.sumw.len(),
                spec.sumw2.len()
            )));
        }
        if !spec.lo.is_finite() || !spec.hi.is_finite() || spec.lo >= spec.hi {
            return Err(bad(format!("bad axis edges [{}, {})", spec.lo, spec.hi)));
        }
        self.check_key(dir, "TH1D", name, spec.title)?;

        let (contents, sumw2) = flow_arrays(spec.sumw, spec.sumw2, spec.under, spec.over);
        self.dir_mut(dir)?
            .objects
            .push(file::ObjPayload::H1(th1d::Th1d {
                name: name.to_owned(),
                title: spec.title.to_owned(),
                nbins: spec.nbins,
                lo: spec.lo,
                hi: spec.hi,
                edges: Vec::new(),
                labels,
                contents,
                sumw2,
                entries: spec.entries,
                tsumw: spec.tsumw,
                tsumw2: spec.tsumw2,
                tsumwx: spec.tsumwx,
                tsumwx2: spec.tsumwx2,
            }));
        Ok(self)
    }

    /// Add one variable-bin TH1D (TAxis `fXbins`) under `dir`.
    ///
    /// # Errors
    /// [`Error::BadHisto`] on fewer than 2 edges, non-finite or
    /// non-increasing edges, `sumw`/`sumw2` lengths differing from
    /// `edges.len() - 1`, or name problems; [`Error::BadDir`] on a bad
    /// directory path.
    pub fn add_th1d_var_at(
        mut self,
        dir: &[&str],
        name: &str,
        spec: &H1VarSpec<'_>,
    ) -> Result<Self, Error> {
        let bad = |reason: String| Error::BadHisto {
            name: name.to_owned(),
            reason,
        };
        if spec.edges.len() < 2 {
            return Err(bad(format!("{} edges; need at least 2", spec.edges.len())));
        }
        if spec.edges.iter().any(|e| !e.is_finite()) || spec.edges.windows(2).any(|w| w[0] >= w[1])
        {
            return Err(bad("edges must be finite and strictly increasing".into()));
        }
        let n = spec.edges.len() - 1;
        if spec.sumw.len() != n || spec.sumw2.len() != n {
            return Err(bad(format!(
                "sumw/sumw2 lengths {}/{} != nbins {n}",
                spec.sumw.len(),
                spec.sumw2.len()
            )));
        }
        self.check_key(dir, "TH1D", name, spec.title)?;

        let (contents, sumw2) = flow_arrays(spec.sumw, spec.sumw2, spec.under, spec.over);
        self.dir_mut(dir)?
            .objects
            .push(file::ObjPayload::H1(th1d::Th1d {
                name: name.to_owned(),
                title: spec.title.to_owned(),
                nbins: n as u32,
                lo: spec.edges[0],
                hi: spec.edges[n],
                edges: spec.edges.to_vec(),
                labels: None,
                contents,
                sumw2,
                entries: spec.entries,
                tsumw: spec.tsumw,
                tsumw2: spec.tsumw2,
                tsumwx: spec.tsumwx,
                tsumwx2: spec.tsumwx2,
            }));
        Ok(self)
    }

    /// Add one TH2D under `dir`. `spec.sumw`/`spec.sumw2` are
    /// flow-inclusive `(nx+2)·(ny+2)` arrays in ROOT global-bin order.
    ///
    /// # Errors
    /// [`Error::BadHisto`] on zero bins, array length mismatches,
    /// non-finite or inverted axis edges, or name problems;
    /// [`Error::BadDir`] on a bad directory path.
    pub fn add_th2d_at(
        mut self,
        dir: &[&str],
        name: &str,
        spec: &H2Spec<'_>,
    ) -> Result<Self, Error> {
        let bad = |reason: String| Error::BadHisto {
            name: name.to_owned(),
            reason,
        };
        if spec.nx == 0 || spec.ny == 0 {
            return Err(bad("nx and ny must be >= 1".into()));
        }
        let ncells = (spec.nx as usize + 2) * (spec.ny as usize + 2);
        if spec.sumw.len() != ncells || spec.sumw2.len() != ncells {
            return Err(bad(format!(
                "sumw/sumw2 lengths {}/{} != (nx+2)*(ny+2) = {ncells}",
                spec.sumw.len(),
                spec.sumw2.len()
            )));
        }
        for (lo, hi, axis) in [(spec.xlo, spec.xhi, "x"), (spec.ylo, spec.yhi, "y")] {
            if !lo.is_finite() || !hi.is_finite() || lo >= hi {
                return Err(bad(format!("bad {axis} axis edges [{lo}, {hi})")));
            }
        }
        self.check_key(dir, "TH2D", name, spec.title)?;

        self.dir_mut(dir)?
            .objects
            .push(file::ObjPayload::H2(th2d::Th2d {
                name: name.to_owned(),
                title: spec.title.to_owned(),
                nx: spec.nx,
                xlo: spec.xlo,
                xhi: spec.xhi,
                ny: spec.ny,
                ylo: spec.ylo,
                yhi: spec.yhi,
                contents: spec.sumw.to_vec(),
                sumw2: spec.sumw2.to_vec(),
                entries: spec.entries,
                tsumw: spec.tsumw,
                tsumw2: spec.tsumw2,
                tsumwx: spec.tsumwx,
                tsumwx2: spec.tsumwx2,
                tsumwy: spec.tsumwy,
                tsumwy2: spec.tsumwy2,
                tsumwxy: spec.tsumwxy,
            }));
        Ok(self)
    }

    /// Add a bare TNamed key under `dir` — the SPEC_EVENT_PIPELINE §6
    /// provenance carrier (`name = "smash2_provenance"`, `title` = the
    /// canonical JSON string).
    ///
    /// # Errors
    /// [`Error::BadHisto`] on name problems; [`Error::BadDir`] on a bad
    /// directory path.
    pub fn add_tnamed_at(mut self, dir: &[&str], name: &str, title: &str) -> Result<Self, Error> {
        self.check_key(dir, "TNamed", name, title)?;
        self.dir_mut(dir)?.objects.push(file::ObjPayload::Named {
            name: name.to_owned(),
            title: title.to_owned(),
        });
        Ok(self)
    }

    /// Add the SPEC_EVENT_PIPELINE §2 cutflow pair under `dir`:
    /// `<base>__cutflow_raw` (contents = raw counts, fSumw2 = raw counts —
    /// Poisson) and `<base>__cutflow_wt` (contents = Σw, fSumw2 = Σw²),
    /// both with `nsteps` bins on `[0, nsteps)`, bin `i+1` labeled with the
    /// step's verbatim statement text, `fEntries` = `events_processed`, and
    /// stats via the binned approximation of SPEC_ROOT_WRITER §4(b)
    /// (`fTsumw = fTsumw2 = Σ contents`, `fTsumwx = Σ content·center`,
    /// `fTsumwx2 = Σ content·center²` — never zeros).
    ///
    /// # Errors
    /// [`Error::BadHisto`] on an empty step list, non-finite step weights,
    /// or name problems; [`Error::BadDir`] on a bad directory path.
    pub fn add_cutflow_at(
        self,
        dir: &[&str],
        base: &str,
        steps: &[CutflowStep<'_>],
        events_processed: u64,
    ) -> Result<Self, Error> {
        let bad = |reason: String| Error::BadHisto {
            name: base.to_owned(),
            reason,
        };
        if steps.is_empty() {
            return Err(bad("cutflow needs at least one step".into()));
        }
        if steps
            .iter()
            .any(|s| !s.sumw.is_finite() || !s.sumw2.is_finite())
        {
            return Err(bad("non-finite step weight sums".into()));
        }
        let labels: Vec<&str> = steps.iter().map(|s| s.label).collect();
        let nsteps = steps.len() as u32;
        #[allow(clippy::cast_precision_loss)]
        let entries = events_processed as f64;

        #[allow(clippy::cast_precision_loss)]
        let raw: Vec<f64> = steps.iter().map(|s| s.raw as f64).collect();
        let wt: Vec<f64> = steps.iter().map(|s| s.sumw).collect();
        let wt2: Vec<f64> = steps.iter().map(|s| s.sumw2).collect();

        let spec = |sumw: &[f64], sumw2: &[f64]| -> (f64, f64, f64, f64) {
            // Binned stats approximation, SPEC_ROOT_WRITER §4(b).
            let tsumw: f64 = sumw.iter().sum();
            let mut tsumwx = 0.0;
            let mut tsumwx2 = 0.0;
            for (i, &w) in sumw.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let center = i as f64 + 0.5;
                tsumwx += w * center;
                tsumwx2 += w * center * center;
            }
            let _ = sumw2;
            (tsumw, tsumw, tsumwx, tsumwx2)
        };

        let (rw, rw2, rwx, rwx2) = spec(&raw, &raw);
        let with_raw = self.add_labeled_th1d_at(
            dir,
            &format!("{base}__cutflow_raw"),
            &H1Spec {
                title: &format!("{base} cutflow (raw events)"),
                nbins: nsteps,
                lo: 0.0,
                hi: f64::from(nsteps),
                sumw: &raw,
                sumw2: &raw,
                under: FlowBin::default(),
                over: FlowBin::default(),
                entries,
                tsumw: rw,
                tsumw2: rw2,
                tsumwx: rwx,
                tsumwx2: rwx2,
            },
            &labels,
        )?;
        let (ww, ww2, wwx, wwx2) = spec(&wt, &wt2);
        with_raw.add_labeled_th1d_at(
            dir,
            &format!("{base}__cutflow_wt"),
            &H1Spec {
                title: &format!("{base} cutflow (weighted)"),
                nbins: nsteps,
                lo: 0.0,
                hi: f64::from(nsteps),
                sumw: &wt,
                sumw2: &wt2,
                under: FlowBin::default(),
                over: FlowBin::default(),
                entries,
                tsumw: ww,
                tsumw2: ww2,
                tsumwx: wwx,
                tsumwx2: wwx2,
            },
            &labels,
        )
    }

    /// Build the complete file image without touching the filesystem.
    /// `file_name` is recorded in the TFile name record (ROOT convention:
    /// the file's own base name).
    pub fn to_bytes(&self, file_name: &str) -> Result<Vec<u8>, Error> {
        let datime = self.datime.unwrap_or_else(now_datime);
        let (uh, ud) = self
            .uuids
            .unwrap_or_else(|| (pseudo_uuid(0), pseudo_uuid(1)));
        file::build(file_name, &self.root, datime, &uh, &ud)
    }

    /// Serialize and write to `path`; the file name component becomes the
    /// TFile name.
    pub fn finish(self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path.as_ref();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::BadPath {
                path: path.display().to_string(),
            })?;
        let bytes = self.to_bytes(name)?;
        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Shared pre-add validation: well-formed object name, no duplicate
    /// in the target directory, and a TKey whose i16 fKeylen will not
    /// silently truncate.
    fn check_key(
        &mut self,
        dir: &[&str],
        class: &str,
        name: &str,
        title: &str,
    ) -> Result<(), Error> {
        let bad = |reason: String| Error::BadHisto {
            name: name.to_owned(),
            reason,
        };
        if name.is_empty() {
            return Err(bad("empty name".into()));
        }
        if name.contains('/') {
            return Err(bad("object names must not contain '/'".into()));
        }
        if file::keylen(class, name, title) > u16::MAX as usize {
            return Err(bad("name + title too long for a TKey".into()));
        }
        let d = self.dir_mut(dir)?;
        if d.has_name(name) {
            return Err(bad(format!(
                "duplicate name in directory '{}'",
                dir.join("/")
            )));
        }
        Ok(())
    }

    /// Walk (and create) the directory at `path`.
    fn dir_mut(&mut self, path: &[&str]) -> Result<&mut file::Dir, Error> {
        let bad = |path: &[&str], reason: &str| Error::BadDir {
            path: path.join("/"),
            reason: reason.to_owned(),
        };
        let mut cur = &mut self.root;
        for (i, &comp) in path.iter().enumerate() {
            if comp.is_empty() {
                return Err(bad(path, "empty path component"));
            }
            if comp.contains('/') {
                return Err(bad(path, "path components must not contain '/'"));
            }
            if file::keylen("TDirectory", comp, comp) > u16::MAX as usize {
                return Err(bad(path, "directory name too long for a TKey"));
            }
            if cur.objects.iter().any(|o| o.name() == comp) {
                return Err(bad(&path[..=i], "an object with this name already exists"));
            }
            let idx = match cur.subdirs.iter().position(|d| d.name == comp) {
                Some(idx) => idx,
                None => {
                    cur.subdirs.push(file::Dir {
                        name: comp.to_owned(),
                        ..file::Dir::default()
                    });
                    cur.subdirs.len() - 1
                }
            };
            cur = &mut cur.subdirs[idx];
        }
        Ok(cur)
    }
}

/// Assemble TArrayD-layout flow-inclusive arrays from in-range bins plus
/// the two flow cells.
fn flow_arrays(sumw: &[f64], sumw2: &[f64], under: FlowBin, over: FlowBin) -> (Vec<f64>, Vec<f64>) {
    let mut contents = Vec::with_capacity(sumw.len() + 2);
    contents.push(under.w);
    contents.extend_from_slice(sumw);
    contents.push(over.w);
    let mut s2 = Vec::with_capacity(sumw2.len() + 2);
    s2.push(under.w2);
    s2.extend_from_slice(sumw2);
    s2.push(over.w2);
    (contents, s2)
}

/// Dependency-free pseudo-random UUID (v4-shaped). ROOT treats file UUIDs as
/// informational; uniqueness here is best-effort by design.
fn pseudo_uuid(salt: u64) -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 16];
    let state = std::collections::hash_map::RandomState::new();
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut h = state.build_hasher();
        h.write_u64(salt ^ (i as u64) << 32);
        h.write_u128(
            std::time::UNIX_EPOCH
                .elapsed()
                .map(|d| d.as_nanos())
                .unwrap_or(0),
        );
        chunk.copy_from_slice(&h.finish().to_be_bytes());
    }
    out[6] = (out[6] & 0x0F) | 0x40;
    out[8] = (out[8] & 0x3F) | 0x80;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec<'a>() -> H1Spec<'a> {
        H1Spec {
            title: "t",
            nbins: 2,
            lo: 0.0,
            hi: 1.0,
            sumw: &[1.0, 2.0],
            sumw2: &[1.0, 4.0],
            under: FlowBin::default(),
            over: FlowBin::default(),
            entries: 3.0,
            tsumw: 3.0,
            tsumw2: 5.0,
            tsumwx: 1.25,
            tsumwx2: 0.8125,
        }
    }

    fn pinned(f: RootFile) -> RootFile {
        f.with_datime(pack_datime(2026, 6, 12, 0, 0, 0))
            .with_uuids([0; 16], [0; 16])
    }

    #[test]
    fn rejects_bad_specs() {
        let dup = RootFile::create().add_th1d("h", &spec()).unwrap();
        assert!(matches!(
            dup.add_th1d("h", &spec()),
            Err(Error::BadHisto { .. })
        ));
        assert!(matches!(
            RootFile::create().add_th1d("", &spec()),
            Err(Error::BadHisto { .. })
        ));
        let mut s = spec();
        s.nbins = 3;
        assert!(matches!(
            RootFile::create().add_th1d("h", &s),
            Err(Error::BadHisto { .. })
        ));
        let mut s = spec();
        s.hi = s.lo;
        assert!(matches!(
            RootFile::create().add_th1d("h", &s),
            Err(Error::BadHisto { .. })
        ));
        let mut s = spec();
        s.lo = f64::NAN;
        assert!(matches!(
            RootFile::create().add_th1d("h", &s),
            Err(Error::BadHisto { .. })
        ));
        // fKeylen is an i16 field; names/titles that would overflow it are
        // rejected instead of silently truncated.
        let huge = "x".repeat(70_000);
        assert!(matches!(
            RootFile::create().add_th1d(&huge, &spec()),
            Err(Error::BadHisto { .. })
        ));
    }

    #[test]
    fn rejects_bad_var_and_2d_specs_and_dirs() {
        let var = H1VarSpec {
            title: "t",
            edges: &[0.0, 1.0, 1.0], // not strictly increasing
            sumw: &[1.0, 2.0],
            sumw2: &[1.0, 4.0],
            under: FlowBin::default(),
            over: FlowBin::default(),
            entries: 3.0,
            tsumw: 3.0,
            tsumw2: 5.0,
            tsumwx: 1.25,
            tsumwx2: 0.8125,
        };
        assert!(matches!(
            RootFile::create().add_th1d_var_at(&[], "h", &var),
            Err(Error::BadHisto { .. })
        ));
        let h2 = H2Spec {
            title: "t",
            nx: 2,
            xlo: 0.0,
            xhi: 1.0,
            ny: 1,
            ylo: 0.0,
            yhi: 1.0,
            sumw: &[0.0; 11], // needs (2+2)*(1+2) = 12
            sumw2: &[0.0; 11],
            entries: 0.0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            tsumwxy: 0.0,
        };
        assert!(matches!(
            RootFile::create().add_th2d_at(&[], "h2", &h2),
            Err(Error::BadHisto { .. })
        ));
        assert!(matches!(
            RootFile::create().add_th1d_at(&[""], "h", &spec()),
            Err(Error::BadDir { .. })
        ));
        assert!(matches!(
            RootFile::create().add_th1d_at(&["a/b"], "h", &spec()),
            Err(Error::BadDir { .. })
        ));
        // A directory component must not collide with an object name.
        let f = RootFile::create().add_th1d("SR", &spec()).unwrap();
        assert!(matches!(
            f.add_th1d_at(&["SR"], "h", &spec()),
            Err(Error::BadDir { .. })
        ));
        // Labels must match the bin count.
        assert!(matches!(
            RootFile::create().add_labeled_th1d_at(&[], "h", &spec(), &["one"]),
            Err(Error::BadHisto { .. })
        ));
        // Same name allowed in *different* directories, rejected in one.
        let f = RootFile::create()
            .add_th1d_at(&["A"], "h", &spec())
            .unwrap()
            .add_th1d_at(&["B"], "h", &spec())
            .unwrap();
        assert!(matches!(
            f.add_th1d_at(&["A"], "h", &spec()),
            Err(Error::BadHisto { .. })
        ));
    }

    #[test]
    fn flow_bins_land_in_tarrayd_slots() {
        let mut s = spec();
        s.under = FlowBin { w: 7.0, w2: 49.0 };
        s.over = FlowBin { w: 9.0, w2: 81.0 };
        let f = pinned(RootFile::create().add_th1d("h", &s).unwrap());
        let parsed = reader::parse(&f.to_bytes("t.root").unwrap()).unwrap();
        let h = &parsed.histos[0];
        assert_eq!(h.contents, vec![7.0, 1.0, 2.0, 9.0]);
        assert_eq!(h.sumw2, vec![49.0, 1.0, 4.0, 81.0]);
    }

    #[test]
    fn cutflow_pair_carries_spec2_semantics() {
        let steps = [
            CutflowStep {
                label: "all",
                raw: 20,
                sumw: 19.5,
                sumw2: 20.25,
            },
            CutflowStep {
                label: "select MET > 200",
                raw: 12,
                sumw: 11.25,
                sumw2: 11.0,
            },
            CutflowStep {
                label: "reject nbjets == 0",
                raw: 5,
                sumw: 4.75,
                sumw2: 4.5,
            },
        ];
        let f = pinned(
            RootFile::create()
                .add_cutflow_at(&["SR1"], "SR1", &steps, 20)
                .unwrap(),
        );
        let parsed = reader::parse(&f.to_bytes("t.root").unwrap()).unwrap();
        assert_eq!(parsed.histos.len(), 2);
        let raw = &parsed.histos[0];
        assert_eq!(raw.name, "SR1__cutflow_raw");
        assert_eq!(raw.path, vec!["SR1".to_owned()]);
        assert_eq!(raw.contents, vec![0.0, 20.0, 12.0, 5.0, 0.0]);
        assert_eq!(raw.sumw2, raw.contents, "raw errors are Poisson");
        assert_eq!(
            raw.labels.as_deref(),
            Some(
                &[
                    "all".to_owned(),
                    "select MET > 200".to_owned(),
                    "reject nbjets == 0".to_owned()
                ][..]
            )
        );
        assert_eq!(raw.entries, 20.0);
        // Binned stats approximation: never zeros.
        assert_eq!(raw.tsumw, 37.0);
        assert_eq!(raw.tsumw2, 37.0);
        assert_eq!(raw.tsumwx, 0.5 * 20.0 + 1.5 * 12.0 + 2.5 * 5.0);
        let wt = &parsed.histos[1];
        assert_eq!(wt.name, "SR1__cutflow_wt");
        assert_eq!(wt.contents, vec![0.0, 19.5, 11.25, 4.75, 0.0]);
        assert_eq!(wt.sumw2, vec![0.0, 20.25, 11.0, 4.5, 0.0]);
    }

    #[test]
    fn nested_directories_round_trip() {
        let f = pinned(
            RootFile::create()
                .add_th1d("top", &spec())
                .unwrap()
                .add_th1d_at(&["SR1"], "h_met", &spec())
                .unwrap()
                .add_th1d_at(&["SR1", "sub"], "deep", &spec())
                .unwrap()
                .add_th2d_at(
                    &["SR2"],
                    "h2",
                    &H2Spec {
                        title: "2d",
                        nx: 1,
                        xlo: 0.0,
                        xhi: 1.0,
                        ny: 1,
                        ylo: 0.0,
                        yhi: 2.0,
                        sumw: &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
                        sumw2: &[0.0; 9],
                        entries: 36.0,
                        tsumw: 36.0,
                        tsumw2: 36.0,
                        tsumwx: 1.0,
                        tsumwx2: 2.0,
                        tsumwy: 3.0,
                        tsumwy2: 4.0,
                        tsumwxy: 5.0,
                    },
                )
                .unwrap()
                .add_tnamed_at(&[], "smash2_provenance", "{\"tool\":\"smash2\"}")
                .unwrap(),
        );
        let parsed = reader::parse(&f.to_bytes("t.root").unwrap()).unwrap();
        assert_eq!(
            parsed.dirs,
            vec![
                vec!["SR1".to_owned()],
                vec!["SR1".to_owned(), "sub".to_owned()],
                vec!["SR2".to_owned()],
            ]
        );
        let paths: Vec<(Vec<String>, String)> = parsed
            .histos
            .iter()
            .map(|h| (h.path.clone(), h.name.clone()))
            .collect();
        assert_eq!(
            paths,
            vec![
                (vec![], "top".to_owned()),
                (vec!["SR1".to_owned()], "h_met".to_owned()),
                (vec!["SR1".to_owned(), "sub".to_owned()], "deep".to_owned()),
            ]
        );
        assert_eq!(parsed.th2s.len(), 1);
        assert_eq!(parsed.th2s[0].name, "h2");
        assert_eq!(parsed.th2s[0].path, vec!["SR2".to_owned()]);
        assert_eq!(parsed.th2s[0].contents[8], 8.0);
        assert_eq!(parsed.th2s[0].tsumwxy, 5.0);
        assert_eq!(
            parsed.named,
            vec![(
                vec![],
                "smash2_provenance".to_owned(),
                "{\"tool\":\"smash2\"}".to_owned()
            )]
        );
        // Determinism: same builder, same bytes.
        let again = pinned(
            RootFile::create()
                .add_th1d("top", &spec())
                .unwrap()
                .add_th1d_at(&["SR1"], "h_met", &spec())
                .unwrap()
                .add_th1d_at(&["SR1", "sub"], "deep", &spec())
                .unwrap()
                .add_th2d_at(
                    &["SR2"],
                    "h2",
                    &H2Spec {
                        title: "2d",
                        nx: 1,
                        xlo: 0.0,
                        xhi: 1.0,
                        ny: 1,
                        ylo: 0.0,
                        yhi: 2.0,
                        sumw: &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0],
                        sumw2: &[0.0; 9],
                        entries: 36.0,
                        tsumw: 36.0,
                        tsumw2: 36.0,
                        tsumwx: 1.0,
                        tsumwx2: 2.0,
                        tsumwy: 3.0,
                        tsumwy2: 4.0,
                        tsumwxy: 5.0,
                    },
                )
                .unwrap()
                .add_tnamed_at(&[], "smash2_provenance", "{\"tool\":\"smash2\"}")
                .unwrap(),
        );
        assert_eq!(
            f.to_bytes("t.root").unwrap(),
            again.to_bytes("t.root").unwrap()
        );
    }
}
