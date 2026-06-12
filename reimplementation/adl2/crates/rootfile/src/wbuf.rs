//! Big-endian byte buffer with the ROOT framing primitives.
//!
//! Everything in a ROOT file is big-endian. Versioned objects are framed
//! `u32 (nbytes + 2 | kByteCountMask)` followed by an `i16` version, where
//! `nbytes` counts the body bytes after the version short (uproot's
//! `numbytes_version`).

/// `kByteCountMask` — set on every byte-count word.
pub const K_BYTE_COUNT_MASK: u32 = 0x4000_0000;

#[derive(Debug, Default)]
pub struct WBuf(pub Vec<u8>);

impl WBuf {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    pub fn bytes(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }

    /// Pascal string: 1-byte length, or `0xFF` + u32 length when len >= 255
    /// (uproot serialization.py `string`). TString uses the same encoding.
    pub fn pstring(&mut self, s: &str) {
        let b = s.as_bytes();
        if b.len() < 255 {
            self.u8(b.len() as u8);
        } else {
            self.u8(0xFF);
            self.u32(b.len() as u32);
        }
        self.bytes(b);
    }

    /// Serialized length of a pascal string.
    pub fn pstring_len(s: &str) -> usize {
        let n = s.len();
        if n < 255 { 1 + n } else { 5 + n }
    }

    /// Versioned-object frame: placeholder byte count, version short, body,
    /// then back-patch the byte count to `(len(version + body)) | mask`.
    pub fn frame(&mut self, version: i16, body: impl FnOnce(&mut Self)) {
        let at = self.0.len();
        self.u32(0);
        self.i16(version);
        body(self);
        let n = (self.0.len() - at - 4) as u32;
        self.0[at..at + 4].copy_from_slice(&(n | K_BYTE_COUNT_MASK).to_be_bytes());
    }

    /// TArrayD: raw `i32 fN` + fN big-endian f64s — no byte count, no version.
    pub fn tarrayd(&mut self, vals: &[f64]) {
        self.i32(vals.len() as i32);
        for &v in vals {
            self.f64(v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pstring_short_and_long() {
        let mut w = WBuf::new();
        w.pstring("abc");
        assert_eq!(w.0, b"\x03abc");
        assert_eq!(WBuf::pstring_len("abc"), 4);

        let long = "x".repeat(300);
        let mut w = WBuf::new();
        w.pstring(&long);
        assert_eq!(w.0[0], 0xFF);
        assert_eq!(&w.0[1..5], &300u32.to_be_bytes());
        assert_eq!(w.0.len(), 305);
        assert_eq!(WBuf::pstring_len(&long), 305);

        // 254 is the last single-byte length; 255 switches encodings.
        assert_eq!(WBuf::pstring_len(&"y".repeat(254)), 255);
        assert_eq!(WBuf::pstring_len(&"y".repeat(255)), 260);
    }

    #[test]
    fn frame_byte_count_includes_version_short() {
        let mut w = WBuf::new();
        w.frame(3, |w| w.u32(0xDEAD_BEEF));
        // body = 4, + version short 2 => byte count 6.
        assert_eq!(&w.0[0..4], &(6u32 | K_BYTE_COUNT_MASK).to_be_bytes());
        assert_eq!(&w.0[4..6], &3i16.to_be_bytes());
        assert_eq!(w.0.len(), 10);
    }

    #[test]
    fn tarrayd_layout() {
        let mut w = WBuf::new();
        w.tarrayd(&[1.5, -2.0]);
        assert_eq!(&w.0[0..4], &2i32.to_be_bytes());
        assert_eq!(&w.0[4..12], &1.5f64.to_be_bytes());
        assert_eq!(&w.0[12..20], &(-2.0f64).to_be_bytes());
    }
}
