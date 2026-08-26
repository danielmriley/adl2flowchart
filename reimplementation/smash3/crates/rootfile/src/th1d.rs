//! TH1D object serialization (SPEC_ROOT_WRITER.md §2, SPEC_EVENT_PIPELINE §3).
//!
//! Pinned class versions, matching what uproot 5.7.4 emits today:
//! TH1D=3, TH1=8, TNamed=1, TObject=1, TAttLine=2, TAttFill=2, TAttMarker=2,
//! TAxis=10, TAttAxis=4, TList=5, TObjString=1 (THashList streams as its
//! bare TList base). Decoded byte-for-byte against the vendored uproot
//! reference payloads (`fixtures/reference_th1d_payload.bin`,
//! `fixtures/reference_th1d_var_payload.bin`,
//! `fixtures/reference_th1d_labeled_payload.bin`).
//!
//! The x axis supports the two SPEC_EVENT_PIPELINE §3 extensions:
//! variable bin edges (TAxis `fXbins` TArrayD, `fXmin`/`fXmax` = first/last
//! edge) and bin labels (`fLabels` becomes a real THashList of TObjStrings,
//! each with `fUniqueID` = its 1-based bin number — `TAxis::SetBinLabel`
//! semantics, also what uproot writes for categorical axes).

use crate::wbuf::WBuf;

/// kNotDeleted | kIsOnHeap — the base fBits uproot writes on every TObject.
pub(crate) const FBITS: u32 = 0x0300_0000;
/// kNotDeleted alone: what uproot writes on TObjStrings inside a label list.
const K_NOT_DELETED: u32 = 0x0200_0000;
/// kMustCleanup, OR-ed onto the TH1's own TNamed base (uproot adds it to
/// direct bases of the streamed object, not to member sub-objects).
const K_MUST_CLEANUP: u32 = 0x8;
/// uproot's `| (1 << 16)` quirk on the fFunctions TList.
const FFUNCTIONS_QUIRK: u32 = 1 << 16;

/// One x/y/z axis as streamed inside a TH1/TH2.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct AxisDef {
    pub nbins: i32,
    pub lo: f64,
    pub hi: f64,
    /// Variable bin edges (`fXbins`); empty = uniform bins.
    pub edges: Vec<f64>,
    /// Bin labels (`fLabels` THashList); `None` = null pointer.
    pub labels: Option<Vec<String>>,
}

impl AxisDef {
    pub fn uniform(nbins: i32, lo: f64, hi: f64) -> Self {
        Self {
            nbins,
            lo,
            hi,
            ..Self::default()
        }
    }

    /// The dummy 1-bin `[0, 1)` axis ROOT uses for unused dimensions.
    pub fn dummy() -> Self {
        Self::uniform(1, 0.0, 1.0)
    }
}

/// The TH1 v8 members shared by TH1D and (via its TH1 base) TH2D.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Th1Common {
    pub name: String,
    pub title: String,
    pub xaxis: AxisDef,
    pub yaxis: AxisDef,
    pub zaxis: AxisDef,
    /// Flow-inclusive cell count (`fNcells`).
    pub ncells: i32,
    pub sumw2: Vec<f64>,
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
}

impl Th1Common {
    /// Emit the framed TH1 v8 base stream.
    pub fn th1(&self, w: &mut WBuf) {
        w.frame(8, |w| {
            tnamed(w, &self.name, &self.title, FBITS | K_MUST_CLEANUP);
            // TAttLine v2: color 602, style 1, width 1.
            w.frame(2, |w| {
                w.i16(602);
                w.i16(1);
                w.i16(1);
            });
            // TAttFill v2: color 0, style 1001.
            w.frame(2, |w| {
                w.i16(0);
                w.i16(1001);
            });
            // TAttMarker v2: color 1, style 1, size 1.0.
            w.frame(2, |w| {
                w.i16(1);
                w.i16(1);
                w.f32(1.0);
            });
            w.i32(self.ncells); // fNcells
            taxis(w, "xaxis", &self.xaxis);
            taxis(w, "yaxis", &self.yaxis);
            taxis(w, "zaxis", &self.zaxis);
            w.i16(0); // fBarOffset
            w.i16(1000); // fBarWidth
            w.f64(self.entries);
            w.f64(self.tsumw);
            w.f64(self.tsumw2);
            w.f64(self.tsumwx);
            w.f64(self.tsumwx2);
            w.f64(-1111.0); // fMaximum
            w.f64(-1111.0); // fMinimum
            w.f64(0.0); // fNormFactor
            w.tarrayd(&[]); // fContour
            w.tarrayd(&self.sumw2); // fSumw2
            w.pstring(""); // fOption (TString)
            // fFunctions: empty TList v5, embedded with byte-count framing
            // (uproot writes no class tag here).
            w.frame(5, |w| {
                tobject(w, 0, FBITS | FFUNCTIONS_QUIRK);
                w.pstring(""); // fName
                w.i32(0); // fSize
            });
            w.i32(0); // fBufferSize
            w.u8(0); // speed bump
            // fBuffer: fBufferSize == 0 doubles.
            w.i32(0); // fBinStatErrOpt
            w.i32(2); // fStatOverflows
        });
    }
}

/// One TH1D, already in TArrayD layout (`contents`/`sumw2` have
/// `nbins + 2` cells: `[0]` underflow, `[nbins + 1]` overflow).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Th1d {
    pub name: String,
    pub title: String,
    pub nbins: u32,
    pub lo: f64,
    pub hi: f64,
    /// Variable bin edges (`nbins + 1` values); empty = uniform bins.
    pub edges: Vec<f64>,
    /// X-axis bin labels (`nbins` strings); `None` = unlabeled.
    pub labels: Option<Vec<String>>,
    pub contents: Vec<f64>,
    pub sumw2: Vec<f64>,
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
}

impl Th1d {
    /// Serialize the full TH1D record payload (the bytes after the TKey).
    pub fn payload(&self) -> Vec<u8> {
        let common = Th1Common {
            name: self.name.clone(),
            title: self.title.clone(),
            xaxis: AxisDef {
                nbins: self.nbins as i32,
                lo: self.lo,
                hi: self.hi,
                edges: self.edges.clone(),
                labels: self.labels.clone(),
            },
            yaxis: AxisDef::dummy(),
            zaxis: AxisDef::dummy(),
            ncells: self.contents.len() as i32,
            sumw2: self.sumw2.clone(),
            entries: self.entries,
            tsumw: self.tsumw,
            tsumw2: self.tsumw2,
            tsumwx: self.tsumwx,
            tsumwx2: self.tsumwx2,
        };
        let mut w = WBuf::new();
        w.frame(3, |w| {
            common.th1(w);
            w.tarrayd(&self.contents);
        });
        w.0
    }
}

/// TObject base bytes: version short 1 (no byte count), fUniqueID, fBits.
pub(crate) fn tobject(w: &mut WBuf, unique_id: u32, fbits: u32) {
    w.u16(1);
    w.u32(unique_id);
    w.u32(fbits);
}

/// TNamed v1, framed: TObject base + name + title.
pub(crate) fn tnamed(w: &mut WBuf, name: &str, title: &str, fbits: u32) {
    w.frame(1, |w| {
        tobject(w, 0, fbits);
        w.pstring(name);
        w.pstring(title);
    });
}

/// TAxis v10, framed: TNamed + TAttAxis v4 + axis members. `fXbins` is the
/// (possibly empty) variable-edge TArrayD; `fLabels` is either the null
/// pointer or an object-any THashList of TObjStrings (uid = bin number).
fn taxis(w: &mut WBuf, name: &str, axis: &AxisDef) {
    w.frame(10, |w| {
        tnamed(w, name, "", FBITS);
        // TAttAxis v4.
        w.frame(4, |w| {
            w.i32(510); // fNdivisions
            w.i16(1); // fAxisColor
            w.i16(1); // fLabelColor
            w.i16(42); // fLabelFont
            w.f32(0.005); // fLabelOffset
            w.f32(0.035); // fLabelSize
            w.f32(0.03); // fTickLength
            w.f32(1.0); // fTitleOffset
            w.f32(0.035); // fTitleSize
            w.i16(1); // fTitleColor
            w.i16(42); // fTitleFont
        });
        w.i32(axis.nbins);
        w.f64(axis.lo);
        w.f64(axis.hi);
        w.tarrayd(&axis.edges); // fXbins (empty: uniform bins)
        w.i32(0); // fFirst
        w.i32(0); // fLast
        w.u16(0); // fBits2
        w.u8(0); // fTimeDisplay
        w.pstring(""); // fTimeFormat (TString)
        match &axis.labels {
            None => w.u32(0), // fLabels (THashList*): null
            Some(labels) => flabels(w, labels),
        }
        w.u32(0); // fModLabs (TList*): null
    });
}

/// `fLabels` as uproot writes it for a categorical axis: an object-any
/// THashList whose body is its bare TList v5 base (THashList adds no
/// members), holding one object-any TObjString per bin. The TList's
/// TObject carries fBits 0 (uproot passes flags 0 through
/// `_serialize_object_any`); each TObjString carries kNotDeleted and
/// `fUniqueID` = 1-based bin number (`TAxis::SetBinLabel`); each list
/// entry is followed by its empty option string (one 0x00 byte).
fn flabels(w: &mut WBuf, labels: &[String]) {
    w.obj_any("THashList", |w| {
        w.frame(5, |w| {
            tobject(w, 0, 0);
            w.pstring(""); // fName
            w.i32(labels.len() as i32); // fSize
            for (i, label) in labels.iter().enumerate() {
                w.obj_any("TObjString", |w| {
                    w.frame(1, |w| {
                        tobject(w, i as u32 + 1, K_NOT_DELETED);
                        w.pstring(label);
                    });
                });
                w.u8(0); // option ""
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference_th1d() -> Th1d {
        // Must stay in sync with tools/make_reference.py.
        Th1d {
            name: "h_met".into(),
            title: "MET [GeV]".into(),
            nbins: 4,
            lo: 0.0,
            hi: 100.0,
            edges: Vec::new(),
            labels: None,
            contents: vec![1.5, 2.0, 0.0, 3.25, 4.0, 0.5],
            sumw2: vec![2.25, 4.0, 0.0, 5.0625, 8.0, 0.25],
            entries: 11.0,
            tsumw: 9.25,
            tsumw2: 17.0625,
            tsumwx: 300.5,
            tsumwx2: 20000.25,
        }
    }

    /// Gold test: byte-for-byte equality with the TH1D payload uproot 5.7.4
    /// wrote for the identical histogram (vendored fixture; regenerated and
    /// re-checked by the env-gated uproot oracle test).
    #[test]
    fn payload_matches_uproot_reference_bytes() {
        let payload = reference_th1d().payload();
        let want = include_bytes!("../fixtures/reference_th1d_payload.bin");
        assert_eq!(payload.len(), want.len());
        assert_eq!(payload, want);
    }

    /// Same gold test for the variable-bin form (TAxis fXbins; see
    /// tools/make_reference_v2.py for the pinned values).
    #[test]
    fn var_bin_payload_matches_uproot_reference_bytes() {
        let mut h = reference_th1d();
        h.name = "h_var".into();
        h.title = "varbin".into();
        h.edges = vec![0.0, 30.0, 70.0, 150.0, 400.0];
        h.lo = 0.0;
        h.hi = 400.0;
        let want = include_bytes!("../fixtures/reference_th1d_var_payload.bin");
        assert_eq!(h.payload(), want);
    }

    /// Same gold test for the labeled (cutflow-shaped) form (TAxis fLabels
    /// THashList; see tools/make_reference_v2.py for the pinned values).
    #[test]
    fn labeled_payload_matches_uproot_reference_bytes() {
        let h = Th1d {
            name: "h_cutflow".into(),
            title: "cutflow".into(),
            nbins: 3,
            lo: 0.0,
            hi: 3.0,
            edges: Vec::new(),
            labels: Some(vec![
                "all".into(),
                "select MET > 200".into(),
                "reject nbjets == 0".into(),
            ]),
            contents: vec![0.0, 20.0, 12.0, 5.0, 0.0],
            sumw2: vec![0.0, 20.0, 12.0, 5.0, 0.0],
            entries: 20.0,
            tsumw: 37.0,
            tsumw2: 37.0,
            tsumwx: 0.5 * 20.0 + 1.5 * 12.0 + 2.5 * 5.0,
            tsumwx2: 0.25 * 20.0 + 2.25 * 12.0 + 6.25 * 5.0,
        };
        let want = include_bytes!("../fixtures/reference_th1d_labeled_payload.bin");
        assert_eq!(h.payload(), want);
    }

    #[test]
    fn payload_framing_spot_checks() {
        let p = reference_th1d().payload();
        // Outer TH1D v3 frame: byte count covers everything after the u32.
        let bc = u32::from_be_bytes(p[0..4].try_into().unwrap());
        assert_eq!(bc & 0xC000_0000, 0x4000_0000);
        assert_eq!((bc & 0x3FFF_FFFF) as usize, p.len() - 4);
        assert_eq!(i16::from_be_bytes(p[4..6].try_into().unwrap()), 3);
        // TH1 v8 frame immediately follows.
        assert_eq!(i16::from_be_bytes(p[10..12].try_into().unwrap()), 8);
        // Trailing TArrayD: fN == fNcells, last cell == overflow.
        let n = p.len();
        let fn_at = n - 4 - 6 * 8;
        assert_eq!(
            i32::from_be_bytes(p[fn_at..fn_at + 4].try_into().unwrap()),
            6
        );
        assert_eq!(f64::from_be_bytes(p[n - 8..].try_into().unwrap()), 0.5);
    }
}
