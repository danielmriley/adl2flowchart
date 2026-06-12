//! TFile container layout (SPEC_ROOT_WRITER.md §1): small-format header,
//! name record + root directory header, object records, vendored
//! StreamerInfo record, keys list, terminal free-segments record.
//!
//! Write-once model: every record size is computed up front, so the file is
//! emitted in one linear pass with all pointers already known.

use crate::Error;
use crate::th1d::Th1d;
use crate::wbuf::WBuf;

/// Pre-serialized StreamerInfo TList data for the TH1D class set, copied
/// verbatim from an uproot 5.7.4 reference file (see fixtures/PROVENANCE.md).
/// uproot itself vendors these blobs with hardcoded checksums; we vendor its
/// assembled record. The env-gated oracle test regenerates the reference and
/// asserts this fixture is still byte-identical.
pub(crate) const STREAMERINFO_TH1D: &[u8] = include_bytes!("../fixtures/streamerinfo_th1d.bin");

/// `kStartBigFile`: the terminal free segment ends here, and files must not
/// reach it in small format.
const K_START_BIG_FILE: u32 = 2_000_000_000;

const HEADER_VERSION: i32 = 62400;
const F_BEGIN: u32 = 100;
/// uproot writes `ZLIB(0).code` (= 100) for uncompressed files; copied.
const F_COMPRESS: i32 = 100;
const KEY_VERSION: i16 = 4;
/// Fixed part of a small-format TKey: nbytes(4) version(2) objlen(4)
/// datime(4) keylen(2) cycle(2) seekkey(4) seekpdir(4).
const KEY_FIXED: usize = 26;
const SI_CLASS: &str = "TList";
const SI_NAME: &str = "StreamerInfo";
const SI_TITLE: &str = "Doubly linked list";

pub(crate) fn keylen(class: &str, name: &str, title: &str) -> usize {
    KEY_FIXED + WBuf::pstring_len(class) + WBuf::pstring_len(name) + WBuf::pstring_len(title)
}

#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the on-disk TKey field list"
)]
fn key(
    w: &mut WBuf,
    nbytes: u32,
    objlen: u32,
    datime: u32,
    klen: u16,
    seekkey: u32,
    seekpdir: u32,
    class: &str,
    name: &str,
    title: &str,
) {
    w.u32(nbytes);
    w.i16(KEY_VERSION);
    w.u32(objlen);
    w.u32(datime);
    w.u16(klen);
    w.i16(1); // fCycle
    w.u32(seekkey);
    w.u32(seekpdir);
    w.pstring(class);
    w.pstring(name);
    w.pstring(title);
}

/// Build the complete file image.
pub(crate) fn build(
    file_name: &str,
    histos: &[Th1d],
    datime: u32,
    uuid_header: &[u8; 16],
    uuid_dir: &[u8; 16],
) -> Result<Vec<u8>, Error> {
    // ---- sizes and offsets (single source of truth for the layout) ----
    let name_keylen = keylen("TFile", file_name, "");
    if name_keylen > u16::MAX as usize {
        return Err(Error::BadPath {
            path: file_name.to_owned(),
        });
    }
    let name_strings = WBuf::pstring_len(file_name) + WBuf::pstring_len("");
    let nbytes_name = name_keylen + name_strings;
    let name_objlen = name_strings + 60; // strings + directory header
    let name_nbytes = name_keylen + name_objlen;

    let mut off = F_BEGIN as usize + name_nbytes;
    let mut hrecs = Vec::with_capacity(histos.len());
    for h in histos {
        let payload = h.payload();
        let klen = keylen("TH1D", &h.name, &h.title);
        let nbytes = klen + payload.len();
        hrecs.push((off, klen, payload));
        off += nbytes;
    }

    let si_off = off;
    let si_keylen = keylen(SI_CLASS, SI_NAME, SI_TITLE);
    let si_nbytes = si_keylen + STREAMERINFO_TH1D.len();
    off += si_nbytes;

    let keys_off = off;
    let keys_keylen = keylen("TFile", file_name, "");
    let keys_objlen = 4 + hrecs.iter().map(|(_, klen, _)| *klen).sum::<usize>();
    let keys_nbytes = keys_keylen + keys_objlen;
    off += keys_nbytes;

    let free_off = off;
    let free_keylen = keylen("TFile", file_name, "");
    let free_nbytes = free_keylen + 10; // one (u16, u32, u32) segment
    let fend = free_off + free_nbytes;

    if fend >= K_START_BIG_FILE as usize {
        return Err(Error::TooLarge { bytes: fend });
    }

    // ---- emit ----
    let mut w = WBuf(Vec::with_capacity(fend));

    // Header (100 bytes).
    w.bytes(b"root");
    w.i32(HEADER_VERSION);
    w.u32(F_BEGIN);
    w.u32(fend as u32);
    w.u32(free_off as u32);
    w.u32(free_nbytes as u32);
    w.u32(1); // nfree: single terminal segment
    w.u32(nbytes_name as u32);
    w.u8(4); // fUnits
    w.i32(F_COMPRESS);
    w.u32(si_off as u32);
    w.u32(si_nbytes as u32);
    w.bytes(&[0x00, 0x01]);
    w.bytes(uuid_header);
    w.bytes(&[0u8; 100 - 63]); // reserved through fBEGIN

    // Name record at fBEGIN: TKey + name/title strings + directory header.
    key(
        &mut w,
        name_nbytes as u32,
        name_objlen as u32,
        datime,
        name_keylen as u16,
        F_BEGIN,
        0,
        "TFile",
        file_name,
        "",
    );
    w.pstring(file_name);
    w.pstring("");
    // Directory header, class_version 5.
    w.i16(5);
    w.u32(datime); // fDatimeC
    w.u32(datime); // fDatimeM
    w.u32(keys_nbytes as u32); // fNbytesKeys
    w.u32(nbytes_name as u32); // fNbytesName
    w.u32(F_BEGIN); // fSeekDir
    w.u32(0); // fSeekParent
    w.u32(keys_off as u32); // fSeekKeys
    w.bytes(&[0x00, 0x01]);
    w.bytes(uuid_dir);
    w.bytes(&[0u8; 12]); // small-format padding (3 x i32 0)

    // Histogram records.
    for ((rec_off, klen, payload), h) in hrecs.iter().zip(histos) {
        key(
            &mut w,
            (klen + payload.len()) as u32,
            payload.len() as u32,
            datime,
            *klen as u16,
            *rec_off as u32,
            F_BEGIN,
            "TH1D",
            &h.name,
            &h.title,
        );
        w.bytes(payload);
    }

    // StreamerInfo record (vendored, uncompressed).
    key(
        &mut w,
        si_nbytes as u32,
        STREAMERINFO_TH1D.len() as u32,
        datime,
        si_keylen as u16,
        si_off as u32,
        F_BEGIN,
        SI_CLASS,
        SI_NAME,
        SI_TITLE,
    );
    w.bytes(STREAMERINFO_TH1D);

    // Keys list: nkeys + the object TKeys exactly as written above.
    key(
        &mut w,
        keys_nbytes as u32,
        keys_objlen as u32,
        datime,
        keys_keylen as u16,
        keys_off as u32,
        F_BEGIN,
        "TFile",
        file_name,
        "",
    );
    w.u32(hrecs.len() as u32);
    for ((rec_off, klen, payload), h) in hrecs.iter().zip(histos) {
        key(
            &mut w,
            (klen + payload.len()) as u32,
            payload.len() as u32,
            datime,
            *klen as u16,
            *rec_off as u32,
            F_BEGIN,
            "TH1D",
            &h.name,
            &h.title,
        );
    }

    // Terminal free-segments record: [fEND, kStartBigFile).
    key(
        &mut w,
        free_nbytes as u32,
        10,
        datime,
        free_keylen as u16,
        free_off as u32,
        F_BEGIN,
        "TFile",
        file_name,
        "",
    );
    w.u16(1); // TFree version
    w.u32(fend as u32);
    w.u32(K_START_BIG_FILE);

    debug_assert_eq!(w.0.len(), fend);
    Ok(w.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keylen_matches_uproot_reference() {
        // reference.root: name-record keylen 48, fNbytesName 64,
        // h_met TH1D keylen 47, StreamerInfo keylen 64.
        assert_eq!(keylen("TFile", "reference.root", ""), 48);
        assert_eq!(
            keylen("TFile", "reference.root", "")
                + WBuf::pstring_len("reference.root")
                + WBuf::pstring_len(""),
            64
        );
        assert_eq!(keylen("TH1D", "h_met", "MET [GeV]"), 47);
        assert_eq!(keylen(SI_CLASS, SI_NAME, SI_TITLE), 64);
    }

    #[test]
    fn key_bytes_match_uproot_reference() {
        // The h_met TKey exactly as uproot wrote it at offset 1616 of
        // reference.root (nbytes 681, objlen 634, datime 0x7d9902ed,
        // keylen 47, cycle 1, seekkey 1616, seekpdir 100).
        let want = {
            let h = "000002a900040000027a7d9902ed002f00010000065000000064\
                     045448314405685f6d6574094d4554205b4765565d";
            (0..h.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap())
                .collect::<Vec<u8>>()
        };
        let mut w = WBuf::new();
        key(
            &mut w,
            681,
            634,
            0x7d99_02ed,
            47,
            1616,
            100,
            "TH1D",
            "h_met",
            "MET [GeV]",
        );
        assert_eq!(w.0, want);
    }
}
