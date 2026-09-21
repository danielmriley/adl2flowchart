//! TFile container layout (SPEC_ROOT_WRITER.md §1, §3): small-format
//! header, name record + root directory header, object and subdirectory
//! records, vendored StreamerInfo record, one keys-list record per
//! directory, terminal free-segments record.
//!
//! Write-once model: every record size is computed up front, so the file
//! is emitted in one linear pass with all pointers already known.
//!
//! Record order: name record, then every directory's contents in
//! pre-order (a directory's objects in insertion order, then its
//! subdirectories — each subdirectory record immediately followed by its
//! own contents), then the StreamerInfo record, then the keys-list
//! records (same pre-order), then the free list. Readers follow pointers,
//! so this layout choice is invisible to ROOT/uproot; it differs from
//! uproot's incremental allocation order (documented in
//! tests/uproot_oracle.rs).
//!
//! Subdirectory conventions copied from uproot `_cascade.py`
//! (`SubDirectory`/`DirectoryHeader`): the subdirectory record's TKey has
//! class `TDirectory`, title = name, and its data is exactly the 60-byte
//! directory header; `fNbytesName` = that TKey's keylen; both the
//! subdirectory key and its keys-list key carry `fSeekPdir` = the parent
//! directory's location. All directories reuse the file's (pinnable)
//! directory UUID — uproot draws a fresh UUID per directory, but ROOT
//! treats them as informational and a single pinned value keeps output
//! byte-stable.

use crate::Error;
use crate::th1d::{FBITS, Th1d, tnamed};
use crate::th2d::Th2d;
use crate::wbuf::{K_BYTE_COUNT_MASK, WBuf};

/// Pre-serialized StreamerInfo TList data for the TH1D class set, copied
/// verbatim from an uproot 5.7.4 reference file (see fixtures/PROVENANCE.md).
/// uproot itself vendors these blobs with hardcoded checksums; we vendor its
/// assembled record. The env-gated oracle test regenerates the reference and
/// asserts this fixture is still byte-identical.
pub(crate) const STREAMERINFO_TH1D: &[u8] = include_bytes!("../fixtures/streamerinfo_th1d.bin");
/// Single-class streamer chunks (object-any TStreamerInfo + the trailing
/// TList option byte), copied verbatim from uproot 5.7.4's vendored
/// `class_rawstreamers` tuples (tools/extract_rawstreamers.py). Appended
/// to the TH1D set when a file actually contains the corresponding class,
/// mirroring uproot's dedup-by-first-write order.
pub(crate) const RAWSTREAMER_TH2_V5: &[u8] = include_bytes!("../fixtures/rawstreamer_th2_v5.bin");
pub(crate) const RAWSTREAMER_TH2D_V4: &[u8] = include_bytes!("../fixtures/rawstreamer_th2d_v4.bin");
pub(crate) const RAWSTREAMER_TOBJSTRING_V1: &[u8] =
    include_bytes!("../fixtures/rawstreamer_tobjstring_v1.bin");
/// Entry count of [`STREAMERINFO_TH1D`] (TObject … TH1D — see
/// fixtures/PROVENANCE.md).
const STREAMERINFO_TH1D_ENTRIES: u32 = 14;
/// Byte length of the TList header in a StreamerInfo record
/// (`>IHHIIBI`: byte count, TList v5, TObject v1, fUniqueID, fBits,
/// empty fName, entry count).
const SI_HEADER_LEN: usize = 21;

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
/// Small-format directory header: version(2) + 7 × u32 + TUUID(2+16) +
/// 12 bytes padding.
const DIR_HEADER_LEN: usize = 60;
const SI_CLASS: &str = "TList";
const SI_NAME: &str = "StreamerInfo";
const SI_TITLE: &str = "Doubly linked list";

/// One object awaiting serialization, in its directory.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ObjPayload {
    H1(Th1d),
    H2(Th2d),
    /// A bare TNamed key (provenance carrier, SPEC_EVENT_PIPELINE §6).
    Named {
        name: String,
        title: String,
    },
}

impl ObjPayload {
    pub fn class(&self) -> &'static str {
        match self {
            ObjPayload::H1(_) => "TH1D",
            ObjPayload::H2(_) => "TH2D",
            ObjPayload::Named { .. } => "TNamed",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            ObjPayload::H1(h) => &h.name,
            ObjPayload::H2(h) => &h.name,
            ObjPayload::Named { name, .. } => name,
        }
    }

    pub fn title(&self) -> &str {
        match self {
            ObjPayload::H1(h) => &h.title,
            ObjPayload::H2(h) => &h.title,
            ObjPayload::Named { title, .. } => title,
        }
    }

    fn payload(&self) -> Vec<u8> {
        match self {
            ObjPayload::H1(h) => h.payload(),
            ObjPayload::H2(h) => h.payload(),
            ObjPayload::Named { name, title } => {
                let mut w = WBuf::new();
                tnamed(&mut w, name, title, FBITS);
                w.0
            }
        }
    }
}

/// One directory: objects plus subdirectories, both in insertion order.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Dir {
    pub name: String,
    pub objects: Vec<ObjPayload>,
    pub subdirs: Vec<Dir>,
}

impl Dir {
    /// True if `name` is already taken by an object or subdirectory here.
    pub fn has_name(&self, name: &str) -> bool {
        self.objects.iter().any(|o| o.name() == name) || self.subdirs.iter().any(|d| d.name == name)
    }

    fn any_object(&self, f: &impl Fn(&ObjPayload) -> bool) -> bool {
        self.objects.iter().any(f) || self.subdirs.iter().any(|d| d.any_object(f))
    }
}

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

/// The StreamerInfo record data for a file containing the given class
/// set: the vendored TH1D-era TList, extended with the TH2/TH2D and/or
/// TObjString chunks when those classes are present. Entry order mirrors
/// uproot's first-write dedup (TH1D set, then TH2 v5, TH2D v4, then
/// TObjString v1); a TH1D-only file reproduces the v1 record bit-for-bit.
fn streamerinfo_data(has_th2: bool, has_labels: bool) -> Vec<u8> {
    if !has_th2 && !has_labels {
        return STREAMERINFO_TH1D.to_vec();
    }
    let mut entries = STREAMERINFO_TH1D_ENTRIES;
    let mut blobs: Vec<u8> = STREAMERINFO_TH1D[SI_HEADER_LEN..].to_vec();
    if has_th2 {
        blobs.extend_from_slice(RAWSTREAMER_TH2_V5);
        blobs.extend_from_slice(RAWSTREAMER_TH2D_V4);
        entries += 2;
    }
    if has_labels {
        blobs.extend_from_slice(RAWSTREAMER_TOBJSTRING_V1);
        entries += 1;
    }
    let mut w = WBuf(Vec::with_capacity(SI_HEADER_LEN + blobs.len()));
    let data_bytes = SI_HEADER_LEN + blobs.len();
    w.u32((data_bytes as u32 - 4) | K_BYTE_COUNT_MASK);
    w.u16(5); // TList version
    w.u16(1); // TObject version
    w.u32(0); // fUniqueID
    w.u32(0x0200_0000); // fBits: kNotDeleted
    w.u8(0); // fName ""
    w.u32(entries);
    w.bytes(&blobs);
    w.0
}

/// One object record, fully sized and located.
struct ObjRec {
    off: usize,
    klen: usize,
    payload: Vec<u8>,
    class: &'static str,
    name: String,
    title: String,
    /// Location of the directory that owns this record.
    dir_loc: usize,
}

/// One directory's layout bookkeeping (root included, at index 0).
struct DirRec {
    /// Subdirectory record offset; `F_BEGIN` for the root.
    loc: usize,
    /// Owning directory's `loc`; the root points at itself.
    parent_loc: usize,
    /// TKey length of the subdirectory record (unused for the root).
    klen: usize,
    name: String,
    /// Index range of this directory's object records in `objs`.
    objs: std::ops::Range<usize>,
    /// `loc`s of the immediate subdirectories, in insertion order.
    child_locs: Vec<usize>,
    /// Keys-list record: offset, keylen, objlen.
    keys_off: usize,
    keys_klen: usize,
    keys_objlen: usize,
}

/// Build the complete file image.
pub(crate) fn build(
    file_name: &str,
    root: &Dir,
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
    let name_objlen = name_strings + DIR_HEADER_LEN;
    let name_nbytes = name_keylen + name_objlen;

    // Pre-order walk: object records interleaved with subdirectory
    // records, exactly in emission order.
    let mut objs: Vec<ObjRec> = Vec::new();
    let mut dirs: Vec<DirRec> = Vec::new();
    let mut off = F_BEGIN as usize + name_nbytes;
    layout_dir(
        root,
        F_BEGIN as usize,
        F_BEGIN as usize,
        &mut off,
        &mut objs,
        &mut dirs,
    );

    let si_data = streamerinfo_data(
        root.any_object(&|o| matches!(o, ObjPayload::H2(_))),
        root.any_object(&|o| matches!(o, ObjPayload::H1(h) if h.labels.is_some())),
    );
    let si_off = off;
    let si_keylen = keylen(SI_CLASS, SI_NAME, SI_TITLE);
    let si_nbytes = si_keylen + si_data.len();
    off += si_nbytes;

    // Keys-list records, one per directory, same pre-order.
    for i in 0..dirs.len() {
        let klen = if dirs[i].loc == F_BEGIN as usize {
            keylen("TFile", file_name, "")
        } else {
            keylen("TDirectory", &dirs[i].name, &dirs[i].name)
        };
        let mut objlen = 4;
        for o in &objs[dirs[i].objs.clone()] {
            objlen += o.klen;
        }
        for ci in 0..dirs[i].child_locs.len() {
            objlen += dirs_child_klen(&dirs, dirs[i].child_locs[ci]);
        }
        dirs[i].keys_off = off;
        dirs[i].keys_klen = klen;
        dirs[i].keys_objlen = objlen;
        off += klen + objlen;
    }

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
    let root_rec = &dirs[0];
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
    dir_header(
        &mut w,
        datime,
        (root_rec.keys_klen + root_rec.keys_objlen) as u32,
        nbytes_name as u32,
        F_BEGIN,
        0,
        root_rec.keys_off as u32,
        uuid_dir,
    );

    // Object and subdirectory records, pre-order. Directories after the
    // root appear as records between the object runs; reconstruct the
    // interleaving by offset (`objs` and `dirs[1..]` are both
    // offset-sorted).
    let mut emit_objs = objs.iter().peekable();
    let mut emit_dirs = dirs.iter().skip(1).peekable();
    loop {
        let next_obj = emit_objs.peek().map(|o| o.off);
        let next_dir = emit_dirs.peek().map(|d| d.loc);
        match (next_obj, next_dir) {
            (Some(o), Some(d)) if o < d => emit_object(&mut w, emit_objs.next().unwrap(), datime),
            (Some(_), Some(_)) | (None, Some(_)) => {
                emit_subdir(&mut w, emit_dirs.next().unwrap(), datime, uuid_dir);
            }
            (Some(_), None) => emit_object(&mut w, emit_objs.next().unwrap(), datime),
            (None, None) => break,
        }
    }

    // StreamerInfo record (vendored/assembled, uncompressed).
    key(
        &mut w,
        si_nbytes as u32,
        si_data.len() as u32,
        datime,
        si_keylen as u16,
        si_off as u32,
        F_BEGIN,
        SI_CLASS,
        SI_NAME,
        SI_TITLE,
    );
    w.bytes(&si_data);

    // Keys-list records: nkeys + the child TKeys exactly as written above.
    for d in &dirs {
        let nbytes = (d.keys_klen + d.keys_objlen) as u32;
        if d.loc == F_BEGIN as usize {
            key(
                &mut w,
                nbytes,
                d.keys_objlen as u32,
                datime,
                d.keys_klen as u16,
                d.keys_off as u32,
                F_BEGIN,
                "TFile",
                file_name,
                "",
            );
        } else {
            key(
                &mut w,
                nbytes,
                d.keys_objlen as u32,
                datime,
                d.keys_klen as u16,
                d.keys_off as u32,
                d.parent_loc as u32,
                "TDirectory",
                &d.name,
                &d.name,
            );
        }
        let n_children = d.objs.len() + d.child_locs.len();
        w.u32(n_children as u32);
        // Children in record order: objects and subdirectories interleaved
        // by offset.
        let mut child_objs = objs[d.objs.clone()].iter().peekable();
        let mut child_dirs = d
            .child_locs
            .iter()
            .map(|&loc| dir_at(&dirs, loc))
            .peekable();
        loop {
            let next_obj = child_objs.peek().map(|o| o.off);
            let next_dir = child_dirs.peek().map(|c| c.loc);
            match (next_obj, next_dir) {
                (Some(o), Some(dd)) if o < dd => {
                    obj_key(&mut w, child_objs.next().unwrap(), datime);
                }
                (Some(_), Some(_)) | (None, Some(_)) => {
                    subdir_key(&mut w, child_dirs.next().unwrap(), datime);
                }
                (Some(_), None) => obj_key(&mut w, child_objs.next().unwrap(), datime),
                (None, None) => break,
            }
        }
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

/// Recursive pre-order layout: assigns offsets to this directory's object
/// records, then to each subdirectory record and (recursively) its
/// contents. `loc` is this directory's location.
fn layout_dir(
    dir: &Dir,
    loc: usize,
    parent_loc: usize,
    off: &mut usize,
    objs: &mut Vec<ObjRec>,
    dirs: &mut Vec<DirRec>,
) {
    let me = dirs.len();
    dirs.push(DirRec {
        loc,
        parent_loc,
        klen: if loc == F_BEGIN as usize {
            0
        } else {
            keylen("TDirectory", &dir.name, &dir.name)
        },
        name: dir.name.clone(),
        objs: 0..0,
        child_locs: Vec::new(),
        keys_off: 0,
        keys_klen: 0,
        keys_objlen: 0,
    });

    let first_obj = objs.len();
    for o in &dir.objects {
        let payload = o.payload();
        let klen = keylen(o.class(), o.name(), o.title());
        objs.push(ObjRec {
            off: *off,
            klen,
            class: o.class(),
            name: o.name().to_owned(),
            title: o.title().to_owned(),
            payload,
            dir_loc: loc,
        });
        let last = objs.last().expect("just pushed");
        *off += klen + last.payload.len();
    }
    dirs[me].objs = first_obj..objs.len();

    for sub in &dir.subdirs {
        let sub_loc = *off;
        dirs[me].child_locs.push(sub_loc);
        *off += keylen("TDirectory", &sub.name, &sub.name) + DIR_HEADER_LEN;
        layout_dir(sub, sub_loc, loc, off, objs, dirs);
    }
}

fn dir_at(dirs: &[DirRec], loc: usize) -> &DirRec {
    dirs.iter()
        .find(|d| d.loc == loc)
        .expect("child directory laid out")
}

fn dirs_child_klen(dirs: &[DirRec], loc: usize) -> usize {
    dir_at(dirs, loc).klen
}

/// The 60-byte small-format directory header.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the on-disk directory header field list"
)]
fn dir_header(
    w: &mut WBuf,
    datime: u32,
    nbytes_keys: u32,
    nbytes_name: u32,
    seek_dir: u32,
    seek_parent: u32,
    seek_keys: u32,
    uuid: &[u8; 16],
) {
    w.i16(5); // class_version
    w.u32(datime); // fDatimeC
    w.u32(datime); // fDatimeM
    w.u32(nbytes_keys);
    w.u32(nbytes_name);
    w.u32(seek_dir);
    w.u32(seek_parent);
    w.u32(seek_keys);
    w.bytes(&[0x00, 0x01]);
    w.bytes(uuid);
    w.bytes(&[0u8; 12]); // small-format padding (3 x i32 0)
}

fn emit_object(w: &mut WBuf, o: &ObjRec, datime: u32) {
    obj_key(w, o, datime);
    w.bytes(&o.payload);
}

fn obj_key(w: &mut WBuf, o: &ObjRec, datime: u32) {
    key(
        w,
        (o.klen + o.payload.len()) as u32,
        o.payload.len() as u32,
        datime,
        o.klen as u16,
        o.off as u32,
        o.dir_loc as u32,
        o.class,
        &o.name,
        &o.title,
    );
}

fn emit_subdir(w: &mut WBuf, d: &DirRec, datime: u32, uuid: &[u8; 16]) {
    subdir_key(w, d, datime);
    dir_header(
        w,
        datime,
        (d.keys_klen + d.keys_objlen) as u32,
        d.klen as u32,
        d.loc as u32,
        d.parent_loc as u32,
        d.keys_off as u32,
        uuid,
    );
}

fn subdir_key(w: &mut WBuf, d: &DirRec, datime: u32) {
    key(
        w,
        (d.klen + DIR_HEADER_LEN) as u32,
        DIR_HEADER_LEN as u32,
        datime,
        d.klen as u16,
        d.loc as u32,
        d.parent_loc as u32,
        "TDirectory",
        &d.name,
        &d.name,
    );
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

    #[test]
    fn th1d_only_streamerinfo_is_the_vendored_v1_record() {
        assert_eq!(streamerinfo_data(false, false), STREAMERINFO_TH1D);
    }

    /// Gold test: our assembled TH1D+TH2D streamer record equals, byte for
    /// byte, the StreamerInfo record uproot wrote for reference_v2.root
    /// (which contains TH1Ds and a TH2D — uproot does not register a
    /// TObjString streamer for labeled axes, so the no-labels assembly is
    /// the comparable one; we add TObjString deliberately, per
    /// SPEC_EVENT_PIPELINE §2).
    #[test]
    fn assembled_th2_streamerinfo_matches_uproot_record() {
        let vendored_v2: &[u8] = include_bytes!("../fixtures/streamerinfo_v2.bin");
        assert_eq!(streamerinfo_data(true, false), vendored_v2);
    }

    #[test]
    fn extended_streamerinfo_appends_chunks_and_fixes_counts() {
        for (th2, labels, extra_entries, extra_len) in [
            (
                true,
                false,
                2,
                RAWSTREAMER_TH2_V5.len() + RAWSTREAMER_TH2D_V4.len(),
            ),
            (false, true, 1, RAWSTREAMER_TOBJSTRING_V1.len()),
            (
                true,
                true,
                3,
                RAWSTREAMER_TH2_V5.len()
                    + RAWSTREAMER_TH2D_V4.len()
                    + RAWSTREAMER_TOBJSTRING_V1.len(),
            ),
        ] {
            let data = streamerinfo_data(th2, labels);
            assert_eq!(data.len(), STREAMERINFO_TH1D.len() + extra_len);
            // Header byte count covers everything after the count word.
            let bc = u32::from_be_bytes(data[0..4].try_into().unwrap());
            assert_eq!((bc & 0x3FFF_FFFF) as usize, data.len() - 4);
            // Entry count at the end of the 21-byte header.
            let n = u32::from_be_bytes(data[17..21].try_into().unwrap());
            assert_eq!(n, STREAMERINFO_TH1D_ENTRIES + extra_entries);
            // The base set is carried unchanged.
            assert_eq!(
                &data[SI_HEADER_LEN..STREAMERINFO_TH1D.len()],
                &STREAMERINFO_TH1D[SI_HEADER_LEN..]
            );
        }
    }
}
