//! Minimal pure-Rust writer for ROOT files containing TH1D histograms.
//!
//! Implements v1 of `reimplementation/SPEC_ROOT_WRITER.md`: small-format
//! TFile header, TKey v4 records, a single root TDirectory with flat
//! (region-prefixed) histogram names, uncompressed records only
//! (`fObjlen == fNbytes - fKeylen`), a vendored uproot StreamerInfo blob for
//! the TH1D class set, and a terminal free-list segment.
//!
//! ```no_run
//! use rootfile::{FlowBin, H1Spec, RootFile};
//!
//! RootFile::create()
//!     .add_th1d("SR1_h_met", &H1Spec {
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
//! CI oracle is uproot read-back plus a byte-diff of the TH1D payload
//! against an uproot-written reference (see `tests/uproot_oracle.rs`).

mod datime;
mod file;
mod th1d;
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

/// One TH1D, in `histos.json` terms.
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

/// Errors from building or writing a ROOT file.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Histogram rejected before serialization (empty/duplicate name,
    /// zero bins, array length mismatch, non-finite or inverted edges).
    BadHisto { name: String, reason: String },
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
#[derive(Debug, Default)]
#[must_use = "RootFile does nothing until finish() or to_bytes()"]
pub struct RootFile {
    histos: Vec<th1d::Th1d>,
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

    /// Pin the header and root-directory UUIDs. Defaults to pseudo-random
    /// values; pin them for byte-stable output and tests.
    pub fn with_uuids(mut self, header: [u8; 16], dir: [u8; 16]) -> Self {
        self.uuids = Some((header, dir));
        self
    }

    /// Add one TH1D under `name` (`name;1` in the root directory).
    ///
    /// # Errors
    /// [`Error::BadHisto`] on empty or duplicate names, `nbins == 0`,
    /// `sumw`/`sumw2` lengths differing from `nbins`, or non-finite /
    /// inverted axis edges.
    pub fn add_th1d(mut self, name: &str, spec: &H1Spec<'_>) -> Result<Self, Error> {
        let bad = |reason: String| Error::BadHisto {
            name: name.to_owned(),
            reason,
        };
        if name.is_empty() {
            return Err(bad("empty name".into()));
        }
        if self.histos.iter().any(|h| h.name == name) {
            return Err(bad("duplicate name in root directory".into()));
        }
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
        // The TKey's fKeylen is an i16 field; refuse names/titles that
        // would silently truncate it.
        if file::keylen("TH1D", name, spec.title) > u16::MAX as usize {
            return Err(bad("name + title too long for a TKey".into()));
        }

        let mut contents = Vec::with_capacity(n + 2);
        contents.push(spec.under.w);
        contents.extend_from_slice(spec.sumw);
        contents.push(spec.over.w);
        let mut sumw2 = Vec::with_capacity(n + 2);
        sumw2.push(spec.under.w2);
        sumw2.extend_from_slice(spec.sumw2);
        sumw2.push(spec.over.w2);

        self.histos.push(th1d::Th1d {
            name: name.to_owned(),
            title: spec.title.to_owned(),
            nbins: spec.nbins,
            lo: spec.lo,
            hi: spec.hi,
            contents,
            sumw2,
            entries: spec.entries,
            tsumw: spec.tsumw,
            tsumw2: spec.tsumw2,
            tsumwx: spec.tsumwx,
            tsumwx2: spec.tsumwx2,
        });
        Ok(self)
    }

    /// Build the complete file image without touching the filesystem.
    /// `file_name` is recorded in the TFile name record (ROOT convention:
    /// the file's own base name).
    pub fn to_bytes(&self, file_name: &str) -> Result<Vec<u8>, Error> {
        let datime = self.datime.unwrap_or_else(now_datime);
        let (uh, ud) = self
            .uuids
            .unwrap_or_else(|| (pseudo_uuid(0), pseudo_uuid(1)));
        file::build(file_name, &self.histos, datime, &uh, &ud)
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
    fn flow_bins_land_in_tarrayd_slots() {
        let mut s = spec();
        s.under = FlowBin { w: 7.0, w2: 49.0 };
        s.over = FlowBin { w: 9.0, w2: 81.0 };
        let f = RootFile::create()
            .add_th1d("h", &s)
            .unwrap()
            .with_datime(pack_datime(2026, 6, 12, 0, 0, 0))
            .with_uuids([0; 16], [0; 16]);
        let parsed = reader::parse(&f.to_bytes("t.root").unwrap()).unwrap();
        let h = &parsed.histos[0];
        assert_eq!(h.contents, vec![7.0, 1.0, 2.0, 9.0]);
        assert_eq!(h.sumw2, vec![49.0, 1.0, 4.0, 81.0]);
    }
}
