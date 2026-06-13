//! TH2D object serialization (SPEC_EVENT_PIPELINE §3; SPEC_ROOT_WRITER §6
//! "TH2D" bullet).
//!
//! Stream layout, pinned to what uproot 5.7.4 (`to_TH2x`,
//! `Model_TH2D_v4`/`Model_TH2_v5`) emits: TH2D v4 frame → TH2 v5 frame
//! (TH1 v8 base, then f64 `fScalefactor`, `fTsumwy`, `fTsumwy2`,
//! `fTsumwxy`) → TArrayD of `(nx+2)·(ny+2)` cells. Cells are in ROOT
//! global-bin order: `gbin = bx + (nx+2)·by` (x fastest). Decoded
//! byte-for-byte against the vendored uproot reference payload
//! (`fixtures/reference_th2d_payload.bin`).

use crate::th1d::{AxisDef, Th1Common};
use crate::wbuf::WBuf;

/// One TH2D with flow-inclusive `contents`/`sumw2` of `(nx+2)·(ny+2)`
/// cells in ROOT global-bin order. Both axes are uniform (the ADL 2-D
/// histo form is uniform-only; variable 2-D axes are not emitted today).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Th2d {
    pub name: String,
    pub title: String,
    pub nx: u32,
    pub xlo: f64,
    pub xhi: f64,
    pub ny: u32,
    pub ylo: f64,
    pub yhi: f64,
    pub contents: Vec<f64>,
    pub sumw2: Vec<f64>,
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
    pub tsumwy: f64,
    pub tsumwy2: f64,
    pub tsumwxy: f64,
}

impl Th2d {
    /// Serialize the full TH2D record payload (the bytes after the TKey).
    pub fn payload(&self) -> Vec<u8> {
        let common = Th1Common {
            name: self.name.clone(),
            title: self.title.clone(),
            xaxis: AxisDef::uniform(self.nx as i32, self.xlo, self.xhi),
            yaxis: AxisDef::uniform(self.ny as i32, self.ylo, self.yhi),
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
        w.frame(4, |w| {
            // TH2 v5: TH1 v8 base, then the four TH2-level members.
            w.frame(5, |w| {
                common.th1(w);
                w.f64(1.0); // fScalefactor
                w.f64(self.tsumwy);
                w.f64(self.tsumwy2);
                w.f64(self.tsumwxy);
            });
            w.tarrayd(&self.contents);
        });
        w.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pinned 2-D reference; must stay in sync with
    /// tools/make_reference_v2.py.
    pub(crate) fn reference_th2d() -> Th2d {
        // 3 x bins on [0, 300), 2 y bins on [0, 4): 5*4 = 20 cells.
        let contents: Vec<f64> = (0..20).map(|i| f64::from(i) * 0.5).collect();
        let sumw2: Vec<f64> = (0..20).map(|i| f64::from(i) * 0.25).collect();
        Th2d {
            name: "h2_met_njets".into(),
            title: "MET vs njets".into(),
            nx: 3,
            xlo: 0.0,
            xhi: 300.0,
            ny: 2,
            ylo: 0.0,
            yhi: 4.0,
            contents,
            sumw2,
            entries: 95.0,
            tsumw: 47.5,
            tsumw2: 23.75,
            tsumwx: 5125.0,
            tsumwx2: 880625.0,
            tsumwy: 95.5,
            tsumwy2: 250.25,
            tsumwxy: 10250.5,
        }
    }

    /// Gold test: byte-for-byte equality with the TH2D payload uproot
    /// 5.7.4 (`to_TH2x`) wrote for the identical histogram — the §3
    /// "byte-diff vs uproot to_TH2x" gate (vendored fixture; regenerated
    /// and re-checked by the env-gated uproot oracle test).
    #[test]
    fn payload_matches_uproot_reference_bytes() {
        let payload = reference_th2d().payload();
        let want = include_bytes!("../fixtures/reference_th2d_payload.bin");
        assert_eq!(payload.len(), want.len());
        assert_eq!(payload, want);
    }

    #[test]
    fn payload_framing_spot_checks() {
        let p = reference_th2d().payload();
        let bc = u32::from_be_bytes(p[0..4].try_into().unwrap());
        assert_eq!(bc & 0xC000_0000, 0x4000_0000);
        assert_eq!((bc & 0x3FFF_FFFF) as usize, p.len() - 4);
        assert_eq!(i16::from_be_bytes(p[4..6].try_into().unwrap()), 4); // TH2D v4
        assert_eq!(i16::from_be_bytes(p[10..12].try_into().unwrap()), 5); // TH2 v5
        assert_eq!(i16::from_be_bytes(p[16..18].try_into().unwrap()), 8); // TH1 v8
        // Trailing TArrayD: fN == 20 cells, last cell == 9.5.
        let n = p.len();
        let fn_at = n - 4 - 20 * 8;
        assert_eq!(
            i32::from_be_bytes(p[fn_at..fn_at + 4].try_into().unwrap()),
            20
        );
        assert_eq!(f64::from_be_bytes(p[n - 8..].try_into().unwrap()), 9.5);
    }
}
