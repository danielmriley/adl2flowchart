//! Verification-grade reader: re-parses files this crate writes and checks
//! the structural invariants (magic, header pointers, TKey arithmetic,
//! byte-count framing, directory tree wiring, TH1D/TH2D/TNamed member
//! round-trip).
//!
//! This is *not* a general ROOT reader — it understands exactly the subset
//! `rootfile` emits (v1 flat layout + the v2 SPEC_EVENT_PIPELINE §3
//! additions) and is deliberately strict about it. The authoritative
//! external oracle is uproot (tests/uproot_oracle.rs).

/// Error from [`parse`]: a structural invariant did not hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyError(pub String);

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rootfile verify: {}", self.0)
    }
}

impl std::error::Error for VerifyError {}

macro_rules! bail {
    ($($t:tt)*) => { return Err(VerifyError(format!($($t)*))) };
}

/// Parsed TFile header (small format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub version: i32,
    pub begin: u32,
    pub end: u32,
    pub seek_free: u32,
    pub nbytes_free: u32,
    pub nfree: u32,
    pub nbytes_name: u32,
    pub units: u8,
    pub compress: i32,
    pub seek_info: u32,
    pub nbytes_info: u32,
}

/// Parsed TKey (small format).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub offset: u32,
    pub nbytes: u32,
    pub objlen: u32,
    pub datime: u32,
    pub keylen: u16,
    pub cycle: i16,
    pub seek_key: u32,
    pub seek_pdir: u32,
    pub class: String,
    pub name: String,
    pub title: String,
}

/// One parsed axis.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisData {
    pub nbins: i32,
    pub lo: f64,
    pub hi: f64,
    /// `fXbins` variable edges; empty for uniform bins.
    pub edges: Vec<f64>,
    /// `fLabels` bin labels; `None` for the null pointer.
    pub labels: Option<Vec<String>>,
}

/// TH1D members recovered from a record payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Th1dData {
    /// Directory path of the record (empty = root directory).
    pub path: Vec<String>,
    pub name: String,
    pub title: String,
    pub nbins: i32,
    pub lo: f64,
    pub hi: f64,
    /// Variable bin edges (TAxis `fXbins`); empty for uniform bins.
    pub edges: Vec<f64>,
    /// X-axis bin labels; `None` for an unlabeled axis.
    pub labels: Option<Vec<String>>,
    pub contents: Vec<f64>,
    pub sumw2: Vec<f64>,
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
}

/// TH2D members recovered from a record payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Th2dData {
    pub path: Vec<String>,
    pub name: String,
    pub title: String,
    pub nx: i32,
    pub xlo: f64,
    pub xhi: f64,
    pub ny: i32,
    pub ylo: f64,
    pub yhi: f64,
    /// Flow-inclusive `(nx+2)·(ny+2)` cells, ROOT global-bin order.
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

/// Whole-file parse result.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFile {
    pub header: Header,
    /// Every record key between fBEGIN and fEND, in file order.
    pub keys: Vec<Key>,
    /// TH1D objects, in directory-walk (= file) order.
    pub histos: Vec<Th1dData>,
    /// TH2D objects, in directory-walk (= file) order.
    pub th2s: Vec<Th2dData>,
    /// TNamed objects: (path, name, title), in directory-walk order.
    pub named: Vec<(Vec<String>, String, String)>,
    /// Directory paths (excluding the root), in directory-walk order.
    pub dirs: Vec<Vec<String>>,
    /// Names listed in the root directory's keys-list record.
    pub keys_list: Vec<String>,
    /// (first, last) free segments.
    pub free: Vec<(u32, u32)>,
}

struct Cur<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], VerifyError> {
        if self.pos + n > self.buf.len() {
            bail!("read past end at {} (+{})", self.pos, n);
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8, VerifyError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, VerifyError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn i16(&mut self) -> Result<i16, VerifyError> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> Result<u32, VerifyError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> Result<i32, VerifyError> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f64(&mut self) -> Result<f64, VerifyError> {
        Ok(f64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }

    fn skip(&mut self, n: usize) -> Result<(), VerifyError> {
        self.take(n).map(|_| ())
    }

    fn pstring(&mut self) -> Result<String, VerifyError> {
        let n = self.u8()?;
        let n = if n == 0xFF {
            self.u32()? as usize
        } else {
            n as usize
        };
        let raw = self.take(n)?;
        String::from_utf8(raw.to_vec()).map_err(|_| VerifyError("non-UTF8 string".into()))
    }

    /// NUL-terminated class name (object-any new-class tag form).
    fn cstring(&mut self) -> Result<String, VerifyError> {
        let start = self.pos;
        while self.pos < self.buf.len() && self.buf[self.pos] != 0 {
            self.pos += 1;
        }
        if self.pos >= self.buf.len() {
            bail!("unterminated class name at {start}");
        }
        let s = String::from_utf8(self.buf[start..self.pos].to_vec())
            .map_err(|_| VerifyError("non-UTF8 class name".into()))?;
        self.pos += 1; // NUL
        Ok(s)
    }

    /// Versioned-object frame header; returns (end position, version).
    fn frame(&mut self, what: &str) -> Result<(usize, i16), VerifyError> {
        let bc = self.u32()?;
        if bc & 0x4000_0000 == 0 {
            bail!("{what}: byte-count mask missing ({bc:#010x})");
        }
        let n = (bc & 0x3FFF_FFFF) as usize;
        let end = self.pos + n;
        let version = self.i16()?;
        if end > self.buf.len() {
            bail!("{what}: frame overruns buffer");
        }
        Ok((end, version))
    }

    fn expect_end(&mut self, end: usize, what: &str) -> Result<(), VerifyError> {
        if self.pos != end {
            bail!(
                "{what}: byte count mismatch (at {}, frame ends {end})",
                self.pos
            );
        }
        Ok(())
    }

    fn tarrayd(&mut self) -> Result<Vec<f64>, VerifyError> {
        let n = self.i32()?;
        if n < 0 {
            bail!("TArrayD: negative fN {n}");
        }
        (0..n).map(|_| self.f64()).collect()
    }
}

fn parse_key(cur: &mut Cur<'_>) -> Result<Key, VerifyError> {
    let offset = cur.pos as u32;
    let nbytes = cur.u32()?;
    let version = cur.i16()?;
    if version != 4 {
        bail!("TKey at {offset}: version {version}, expected 4");
    }
    let objlen = cur.u32()?;
    let datime = cur.u32()?;
    let keylen = cur.u16()?;
    let cycle = cur.i16()?;
    let seek_key = cur.u32()?;
    let seek_pdir = cur.u32()?;
    let class = cur.pstring()?;
    let name = cur.pstring()?;
    let title = cur.pstring()?;
    if cur.pos != offset as usize + keylen as usize {
        bail!(
            "TKey {name}: keylen {keylen} != actual {}",
            cur.pos - offset as usize
        );
    }
    Ok(Key {
        offset,
        nbytes,
        objlen,
        datime,
        keylen,
        cycle,
        seek_key,
        seek_pdir,
        class,
        name,
        title,
    })
}

fn parse_tnamed(cur: &mut Cur<'_>) -> Result<(String, String), VerifyError> {
    let (end, v) = cur.frame("TNamed")?;
    if v != 1 {
        bail!("TNamed version {v}");
    }
    cur.skip(10)?; // TObject: version, fUniqueID, fBits
    let name = cur.pstring()?;
    let title = cur.pstring()?;
    cur.expect_end(end, "TNamed")?;
    Ok((name, title))
}

/// `fLabels`: either a null pointer (u32 0) or an object-any THashList of
/// TObjStrings, each entry followed by its empty option byte. TObjString
/// `fUniqueID` must be the 1-based bin number (`TAxis::SetBinLabel`).
fn parse_flabels(cur: &mut Cur<'_>) -> Result<Option<Vec<String>>, VerifyError> {
    let bc = cur.u32()?;
    if bc == 0 {
        return Ok(None);
    }
    if bc & 0x4000_0000 == 0 {
        bail!("fLabels: byte-count mask missing ({bc:#010x})");
    }
    let end = cur.pos + (bc & 0x3FFF_FFFF) as usize;
    if cur.u32()? != 0xFFFF_FFFF {
        bail!("fLabels: expected kNewClassTag");
    }
    let class = cur.cstring()?;
    if class != "THashList" {
        bail!("fLabels: class {class}, expected THashList");
    }
    let (lend, lv) = cur.frame("fLabels TList")?;
    if lv != 5 {
        bail!("fLabels TList version {lv}");
    }
    cur.skip(10)?; // TObject
    cur.pstring()?; // fName
    let n = cur.i32()?;
    if n < 0 {
        bail!("fLabels: negative fSize {n}");
    }
    let mut labels = Vec::with_capacity(n as usize);
    for i in 0..n {
        let sbc = cur.u32()?;
        if sbc & 0x4000_0000 == 0 {
            bail!("fLabels entry {i}: byte-count mask missing");
        }
        if cur.u32()? != 0xFFFF_FFFF {
            bail!("fLabels entry {i}: expected kNewClassTag");
        }
        let sclass = cur.cstring()?;
        if sclass != "TObjString" {
            bail!("fLabels entry {i}: class {sclass}, expected TObjString");
        }
        let (send, sv) = cur.frame("TObjString")?;
        if sv != 1 {
            bail!("TObjString version {sv}");
        }
        cur.skip(2)?; // TObject version
        let uid = cur.u32()?;
        if uid != (i + 1) as u32 {
            bail!("fLabels entry {i}: fUniqueID {uid}, expected {}", i + 1);
        }
        cur.skip(4)?; // fBits
        labels.push(cur.pstring()?);
        cur.expect_end(send, "TObjString")?;
        if cur.u8()? != 0 {
            bail!("fLabels entry {i}: non-empty option string");
        }
    }
    cur.expect_end(lend, "fLabels TList")?;
    cur.expect_end(end, "fLabels")?;
    Ok(Some(labels))
}

fn parse_taxis(cur: &mut Cur<'_>) -> Result<AxisData, VerifyError> {
    let (end, v) = cur.frame("TAxis")?;
    if v != 10 {
        bail!("TAxis version {v}");
    }
    parse_tnamed(cur)?;
    let (aend, av) = cur.frame("TAttAxis")?;
    if av != 4 {
        bail!("TAttAxis version {av}");
    }
    cur.skip(aend - cur.pos)?;
    let nbins = cur.i32()?;
    let lo = cur.f64()?;
    let hi = cur.f64()?;
    let edges = cur.tarrayd()?;
    if !edges.is_empty() && edges.len() != (nbins + 1) as usize {
        bail!("TAxis: fXbins length {} != nbins + 1", edges.len());
    }
    cur.skip(4 + 4 + 2 + 1)?; // fFirst, fLast, fBits2, fTimeDisplay
    cur.pstring()?; // fTimeFormat
    let labels = parse_flabels(cur)?;
    if let Some(l) = &labels
        && l.len() != nbins as usize
    {
        bail!("TAxis: {} labels != nbins {nbins}", l.len());
    }
    if cur.u32()? != 0 {
        bail!("TAxis: fModLabs not null");
    }
    cur.expect_end(end, "TAxis")?;
    Ok(AxisData {
        nbins,
        lo,
        hi,
        edges,
        labels,
    })
}

/// The TH1 v8 base members shared by TH1D and TH2D records.
struct Th1Body {
    name: String,
    title: String,
    ncells: i32,
    xaxis: AxisData,
    yaxis: AxisData,
    entries: f64,
    tsumw: f64,
    tsumw2: f64,
    tsumwx: f64,
    tsumwx2: f64,
    sumw2: Vec<f64>,
}

fn parse_th1_body(cur: &mut Cur<'_>) -> Result<Th1Body, VerifyError> {
    let (h1end, h1v) = cur.frame("TH1")?;
    if h1v != 8 {
        bail!("TH1 version {h1v}");
    }
    let (name, title) = parse_tnamed(cur)?;
    for att in ["TAttLine", "TAttFill", "TAttMarker"] {
        let (aend, _) = cur.frame(att)?;
        cur.skip(aend - cur.pos)?;
    }
    let ncells = cur.i32()?;
    let xaxis = parse_taxis(cur)?;
    let yaxis = parse_taxis(cur)?;
    parse_taxis(cur)?; // z
    cur.skip(4)?; // fBarOffset, fBarWidth
    let entries = cur.f64()?;
    let tsumw = cur.f64()?;
    let tsumw2 = cur.f64()?;
    let tsumwx = cur.f64()?;
    let tsumwx2 = cur.f64()?;
    cur.skip(24)?; // fMaximum, fMinimum, fNormFactor
    let contour = cur.tarrayd()?;
    if !contour.is_empty() {
        bail!("{name}: non-empty fContour");
    }
    let sumw2 = cur.tarrayd()?;
    cur.pstring()?; // fOption
    let (fend, fv) = cur.frame("fFunctions TList")?;
    if fv != 5 {
        bail!("fFunctions TList version {fv}");
    }
    cur.skip(10)?; // TObject
    cur.pstring()?; // fName
    if cur.i32()? != 0 {
        bail!("{name}: fFunctions not empty");
    }
    cur.expect_end(fend, "fFunctions")?;
    if cur.i32()? != 0 {
        bail!("{name}: fBufferSize != 0");
    }
    cur.skip(1)?; // speed bump
    cur.skip(4)?; // fBinStatErrOpt
    if cur.i32()? != 2 {
        bail!("{name}: fStatOverflows != 2");
    }
    cur.expect_end(h1end, "TH1")?;
    Ok(Th1Body {
        name,
        title,
        ncells,
        xaxis,
        yaxis,
        entries,
        tsumw,
        tsumw2,
        tsumwx,
        tsumwx2,
        sumw2,
    })
}

fn parse_th1d(payload: &[u8], path: &[String]) -> Result<Th1dData, VerifyError> {
    let mut cur = Cur {
        buf: payload,
        pos: 0,
    };
    let (end, v) = cur.frame("TH1D")?;
    if v != 3 {
        bail!("TH1D version {v}");
    }
    let b = parse_th1_body(&mut cur)?;
    let contents = cur.tarrayd()?;
    cur.expect_end(end, "TH1D")?;
    if cur.pos != payload.len() {
        bail!(
            "{}: {} trailing payload bytes",
            b.name,
            payload.len() - cur.pos
        );
    }
    if b.ncells != b.xaxis.nbins + 2 {
        bail!(
            "{}: fNcells {} != nbins {} + 2",
            b.name,
            b.ncells,
            b.xaxis.nbins
        );
    }
    if contents.len() != b.ncells as usize || b.sumw2.len() != b.ncells as usize {
        bail!("{}: array lengths != fNcells", b.name);
    }
    Ok(Th1dData {
        path: path.to_vec(),
        name: b.name,
        title: b.title,
        nbins: b.xaxis.nbins,
        lo: b.xaxis.lo,
        hi: b.xaxis.hi,
        edges: b.xaxis.edges,
        labels: b.xaxis.labels,
        contents,
        sumw2: b.sumw2,
        entries: b.entries,
        tsumw: b.tsumw,
        tsumw2: b.tsumw2,
        tsumwx: b.tsumwx,
        tsumwx2: b.tsumwx2,
    })
}

fn parse_th2d(payload: &[u8], path: &[String]) -> Result<Th2dData, VerifyError> {
    let mut cur = Cur {
        buf: payload,
        pos: 0,
    };
    let (end, v) = cur.frame("TH2D")?;
    if v != 4 {
        bail!("TH2D version {v}");
    }
    let (h2end, h2v) = cur.frame("TH2")?;
    if h2v != 5 {
        bail!("TH2 version {h2v}");
    }
    let b = parse_th1_body(&mut cur)?;
    let scalefactor = cur.f64()?;
    if scalefactor != 1.0 {
        bail!("{}: fScalefactor {scalefactor} != 1", b.name);
    }
    let tsumwy = cur.f64()?;
    let tsumwy2 = cur.f64()?;
    let tsumwxy = cur.f64()?;
    cur.expect_end(h2end, "TH2")?;
    let contents = cur.tarrayd()?;
    cur.expect_end(end, "TH2D")?;
    if cur.pos != payload.len() {
        bail!(
            "{}: {} trailing payload bytes",
            b.name,
            payload.len() - cur.pos
        );
    }
    let ncells = (b.xaxis.nbins + 2) * (b.yaxis.nbins + 2);
    if b.ncells != ncells {
        bail!(
            "{}: fNcells {} != (nx+2)*(ny+2) = {ncells}",
            b.name,
            b.ncells
        );
    }
    if contents.len() != ncells as usize || b.sumw2.len() != ncells as usize {
        bail!("{}: array lengths != fNcells", b.name);
    }
    Ok(Th2dData {
        path: path.to_vec(),
        name: b.name,
        title: b.title,
        nx: b.xaxis.nbins,
        xlo: b.xaxis.lo,
        xhi: b.xaxis.hi,
        ny: b.yaxis.nbins,
        ylo: b.yaxis.lo,
        yhi: b.yaxis.hi,
        contents,
        sumw2: b.sumw2,
        entries: b.entries,
        tsumw: b.tsumw,
        tsumw2: b.tsumw2,
        tsumwx: b.tsumwx,
        tsumwx2: b.tsumwx2,
        tsumwy,
        tsumwy2,
        tsumwxy,
    })
}

/// The 60-byte small-format directory header.
#[derive(Debug, Clone, Copy)]
struct DirHeader {
    nbytes_keys: u32,
    nbytes_name: u32,
    seek_dir: u32,
    seek_parent: u32,
    seek_keys: u32,
}

fn parse_dir_header(data: &[u8]) -> Result<DirHeader, VerifyError> {
    let mut cur = Cur { buf: data, pos: 0 };
    let v = cur.i16()?;
    if v != 5 {
        bail!("directory header version {v}, expected 5");
    }
    cur.skip(8)?; // fDatimeC, fDatimeM
    let nbytes_keys = cur.u32()?;
    let nbytes_name = cur.u32()?;
    let seek_dir = cur.u32()?;
    let seek_parent = cur.u32()?;
    let seek_keys = cur.u32()?;
    if cur.take(2)? != [0x00, 0x01] {
        bail!("directory header: bad TUUID version");
    }
    cur.skip(16)?; // UUID
    if cur.take(12)?.iter().any(|&b| b != 0) {
        bail!("directory header: non-zero padding");
    }
    if cur.pos != data.len() {
        bail!("directory header: trailing bytes");
    }
    Ok(DirHeader {
        nbytes_keys,
        nbytes_name,
        seek_dir,
        seek_parent,
        seek_keys,
    })
}

/// One raw record: its key and data bytes.
struct Record<'a> {
    key: Key,
    data: &'a [u8],
    visited: std::cell::Cell<bool>,
}

/// Recursively walk a directory's keys list, decoding objects and
/// descending into subdirectories.
#[expect(
    clippy::too_many_arguments,
    reason = "single-purpose recursive walker over accumulated outputs"
)]
fn walk_dir(
    records: &std::collections::BTreeMap<u32, Record<'_>>,
    dir_loc: u32,
    seek_keys: u32,
    nbytes_keys: u32,
    path: &[String],
    histos: &mut Vec<Th1dData>,
    th2s: &mut Vec<Th2dData>,
    named: &mut Vec<(Vec<String>, String, String)>,
    dirs: &mut Vec<Vec<String>>,
    root_names: Option<&mut Vec<String>>,
) -> Result<(), VerifyError> {
    let Some(keys_rec) = records.get(&seek_keys) else {
        bail!(
            "directory /{}: fSeekKeys {seek_keys} is not a record",
            path.join("/")
        );
    };
    keys_rec.visited.set(true);
    if keys_rec.key.nbytes != nbytes_keys {
        bail!(
            "directory /{}: fNbytesKeys {nbytes_keys} != keys record nbytes {}",
            path.join("/"),
            keys_rec.key.nbytes
        );
    }
    let mut lc = Cur {
        buf: keys_rec.data,
        pos: 0,
    };
    let n = lc.u32()?;
    let mut names = Vec::new();
    for _ in 0..n {
        let child = parse_key(&mut lc)?;
        names.push(child.name.clone());
        let Some(rec) = records.get(&child.seek_key) else {
            bail!(
                "keys list of /{}: {} points nowhere",
                path.join("/"),
                child.name
            );
        };
        rec.visited.set(true);
        if rec.key.seek_pdir != dir_loc {
            bail!(
                "record {}: fSeekPdir {} != owning directory {dir_loc}",
                child.name,
                rec.key.seek_pdir
            );
        }
        match rec.key.class.as_str() {
            "TH1D" => histos.push(parse_th1d(rec.data, path)?),
            "TH2D" => th2s.push(parse_th2d(rec.data, path)?),
            "TNamed" => {
                let mut cur = Cur {
                    buf: rec.data,
                    pos: 0,
                };
                let (nm, title) = parse_tnamed(&mut cur)?;
                if cur.pos != rec.data.len() {
                    bail!("TNamed {nm}: trailing payload bytes");
                }
                if nm != rec.key.name {
                    bail!("TNamed: key name {} != object name {nm}", rec.key.name);
                }
                named.push((path.to_vec(), nm, title));
            }
            "TDirectory" => {
                let hd = parse_dir_header(rec.data)?;
                if hd.seek_dir != rec.key.offset {
                    bail!(
                        "directory {}: fSeekDir {} != record offset",
                        child.name,
                        hd.seek_dir
                    );
                }
                if hd.seek_parent != dir_loc {
                    bail!(
                        "directory {}: fSeekParent {} != parent {dir_loc}",
                        child.name,
                        hd.seek_parent
                    );
                }
                if u32::from(rec.key.keylen) != hd.nbytes_name {
                    bail!(
                        "directory {}: fNbytesName {} != keylen",
                        child.name,
                        hd.nbytes_name
                    );
                }
                let mut sub_path = path.to_vec();
                sub_path.push(child.name.clone());
                dirs.push(sub_path.clone());
                walk_dir(
                    records,
                    rec.key.offset,
                    hd.seek_keys,
                    hd.nbytes_keys,
                    &sub_path,
                    histos,
                    th2s,
                    named,
                    dirs,
                    None,
                )?;
            }
            c => bail!("keys list of /{}: unexpected class {c}", path.join("/")),
        }
    }
    if lc.pos != keys_rec.data.len() {
        bail!("keys list of /{}: trailing bytes", path.join("/"));
    }
    if let Some(out) = root_names {
        *out = names;
    }
    Ok(())
}

/// Parse a file image written by this crate and check its invariants.
pub fn parse(buf: &[u8]) -> Result<ParsedFile, VerifyError> {
    if buf.len() < 100 {
        bail!("file shorter than the 100-byte header");
    }
    if &buf[0..4] != b"root" {
        bail!("bad magic {:?}", &buf[0..4]);
    }
    let mut cur = Cur { buf, pos: 4 };
    let header = Header {
        version: cur.i32()?,
        begin: cur.u32()?,
        end: cur.u32()?,
        seek_free: cur.u32()?,
        nbytes_free: cur.u32()?,
        nfree: cur.u32()?,
        nbytes_name: cur.u32()?,
        units: cur.u8()?,
        compress: cur.i32()?,
        seek_info: cur.u32()?,
        nbytes_info: cur.u32()?,
    };
    if header.begin != 100 || header.units != 4 {
        bail!(
            "unexpected fBEGIN/fUnits: {}/{}",
            header.begin,
            header.units
        );
    }
    if header.end as usize != buf.len() {
        bail!("fEND {} != file size {}", header.end, buf.len());
    }

    // Pass 1: linear scan of all records, checking TKey arithmetic.
    let mut keys = Vec::new();
    let mut records: std::collections::BTreeMap<u32, Record<'_>> =
        std::collections::BTreeMap::new();
    let mut pos = header.begin as usize;
    while pos < header.end as usize {
        let mut kc = Cur { buf, pos };
        let k = parse_key(&mut kc)?;
        if k.seek_key != pos as u32 {
            bail!("key {} at {pos}: fSeekKey {}", k.name, k.seek_key);
        }
        if u32::from(k.keylen) > k.nbytes || pos + k.nbytes as usize > buf.len() {
            bail!(
                "key {}: record overruns buffer (nbytes {})",
                k.name,
                k.nbytes
            );
        }
        if k.objlen != k.nbytes - u32::from(k.keylen) {
            bail!(
                "key {}: not uncompressed (objlen/nbytes/keylen mismatch)",
                k.name
            );
        }
        if k.cycle != 1 {
            bail!("key {}: cycle {}", k.name, k.cycle);
        }
        let data = &buf[pos + k.keylen as usize..pos + k.nbytes as usize];
        keys.push(k.clone());
        records.insert(
            pos as u32,
            Record {
                key: k,
                data,
                visited: std::cell::Cell::new(false),
            },
        );
        pos += records[&(pos as u32)].key.nbytes as usize;
    }
    if pos != header.end as usize {
        bail!("records end at {pos}, header says {}", header.end);
    }

    // Name record: strings + root directory header.
    let name_rec = &records[&header.begin];
    name_rec.visited.set(true);
    let mut nc = Cur {
        buf: name_rec.data,
        pos: 0,
    };
    nc.pstring()?;
    nc.pstring()?;
    let root_hd = parse_dir_header(&name_rec.data[nc.pos..])?;
    if root_hd.seek_dir != header.begin || root_hd.seek_parent != 0 {
        bail!("root directory header: bad fSeekDir/fSeekParent");
    }
    if root_hd.nbytes_name != header.nbytes_name {
        bail!("root directory header: fNbytesName disagrees with file header");
    }

    // StreamerInfo record.
    let Some(si) = records.get(&header.seek_info) else {
        bail!("fSeekInfo points nowhere");
    };
    si.visited.set(true);
    if si.key.class != "TList" || si.key.name != "StreamerInfo" {
        bail!(
            "StreamerInfo record has class {} / name {}",
            si.key.class,
            si.key.name
        );
    }
    if si.key.nbytes != header.nbytes_info {
        bail!("StreamerInfo record disagrees with header pointers");
    }

    // Free-segments record.
    let Some(fr) = records.get(&header.seek_free) else {
        bail!("fSeekFree points nowhere");
    };
    fr.visited.set(true);
    let mut free = Vec::new();
    {
        let mut fc = Cur {
            buf: fr.data,
            pos: 0,
        };
        while fc.pos < fr.data.len() {
            if fc.u16()? != 1 {
                bail!("free segment version != 1");
            }
            free.push((fc.u32()?, fc.u32()?));
        }
        if free.len() != header.nfree as usize {
            bail!("nfree {} != {} segments", header.nfree, free.len());
        }
    }

    // Pass 2: directory walk from the root.
    let mut histos = Vec::new();
    let mut th2s = Vec::new();
    let mut named = Vec::new();
    let mut dirs = Vec::new();
    let mut keys_list = Vec::new();
    walk_dir(
        &records,
        header.begin,
        root_hd.seek_keys,
        root_hd.nbytes_keys,
        &[],
        &mut histos,
        &mut th2s,
        &mut named,
        &mut dirs,
        Some(&mut keys_list),
    )?;

    // Every record must be reachable.
    for r in records.values() {
        if !r.visited.get() {
            bail!(
                "unreachable record {} ({}) at {}",
                r.key.name,
                r.key.class,
                r.key.offset
            );
        }
    }

    Ok(ParsedFile {
        header,
        keys,
        histos,
        th2s,
        named,
        dirs,
        keys_list,
        free,
    })
}
