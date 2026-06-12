//! Verification-grade reader: re-parses files this crate writes and checks
//! the structural invariants (magic, header pointers, TKey arithmetic,
//! byte-count framing, TH1D member round-trip).
//!
//! This is *not* a general ROOT reader — it understands exactly the v1
//! subset `rootfile` emits and is deliberately strict about it. The
//! authoritative external oracle is uproot (tests/uproot_oracle.rs).

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

/// TH1D members recovered from a record payload.
#[derive(Debug, Clone, PartialEq)]
pub struct Th1dData {
    pub name: String,
    pub title: String,
    pub nbins: i32,
    pub lo: f64,
    pub hi: f64,
    pub contents: Vec<f64>,
    pub sumw2: Vec<f64>,
    pub entries: f64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
}

/// Whole-file parse result.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedFile {
    pub header: Header,
    /// Every record key between fBEGIN and fEND, in file order.
    pub keys: Vec<Key>,
    /// TH1D objects decoded from their records, in file order.
    pub histos: Vec<Th1dData>,
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

fn parse_taxis(cur: &mut Cur<'_>) -> Result<(i32, f64, f64), VerifyError> {
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
    let xbins = cur.tarrayd()?;
    if !xbins.is_empty() {
        bail!("TAxis: non-empty fXbins");
    }
    cur.skip(4 + 4 + 2 + 1)?; // fFirst, fLast, fBits2, fTimeDisplay
    cur.pstring()?; // fTimeFormat
    if cur.u32()? != 0 || cur.u32()? != 0 {
        bail!("TAxis: fLabels/fModLabs not null");
    }
    cur.expect_end(end, "TAxis")?;
    Ok((nbins, lo, hi))
}

fn parse_th1d(payload: &[u8]) -> Result<Th1dData, VerifyError> {
    let mut cur = Cur {
        buf: payload,
        pos: 0,
    };
    let (end, v) = cur.frame("TH1D")?;
    if v != 3 {
        bail!("TH1D version {v}");
    }
    let (h1end, h1v) = cur.frame("TH1")?;
    if h1v != 8 {
        bail!("TH1 version {h1v}");
    }
    let (name, title) = parse_tnamed(&mut cur)?;
    for att in ["TAttLine", "TAttFill", "TAttMarker"] {
        let (aend, _) = cur.frame(att)?;
        cur.skip(aend - cur.pos)?;
    }
    let ncells = cur.i32()?;
    let (nbins, lo, hi) = parse_taxis(&mut cur)?;
    parse_taxis(&mut cur)?; // y
    parse_taxis(&mut cur)?; // z
    if ncells != nbins + 2 {
        bail!("{name}: fNcells {ncells} != nbins {nbins} + 2");
    }
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
    let contents = cur.tarrayd()?;
    cur.expect_end(end, "TH1D")?;
    if cur.pos != payload.len() {
        bail!("{name}: {} trailing payload bytes", payload.len() - cur.pos);
    }
    if contents.len() != ncells as usize || sumw2.len() != ncells as usize {
        bail!("{name}: array lengths != fNcells");
    }
    Ok(Th1dData {
        name,
        title,
        nbins,
        lo,
        hi,
        contents,
        sumw2,
        entries,
        tsumw,
        tsumw2,
        tsumwx,
        tsumwx2,
    })
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

    let mut keys = Vec::new();
    let mut histos = Vec::new();
    let mut keys_list = Vec::new();
    let mut free = Vec::new();
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
        match (pos as u32 == header.begin, k.class.as_str()) {
            (true, _) => {} // name record + directory header, checked below
            (false, "TH1D") => histos.push(parse_th1d(data)?),
            (false, "TList") => {
                if k.name != "StreamerInfo" {
                    bail!("unexpected TList record {}", k.name);
                }
                if (k.seek_key, k.nbytes) != (header.seek_info, header.nbytes_info) {
                    bail!("StreamerInfo record disagrees with header pointers");
                }
            }
            (false, "TFile") if k.seek_key == header.seek_free => {
                let mut fc = Cur { buf: data, pos: 0 };
                while fc.pos < data.len() {
                    if fc.u16()? != 1 {
                        bail!("free segment version != 1");
                    }
                    free.push((fc.u32()?, fc.u32()?));
                }
                if free.len() != header.nfree as usize {
                    bail!("nfree {} != {} segments", header.nfree, free.len());
                }
            }
            (false, "TFile") => {
                // Keys list.
                let mut lc = Cur { buf: data, pos: 0 };
                let n = lc.u32()?;
                for _ in 0..n {
                    keys_list.push(parse_key(&mut lc)?.name);
                }
                if lc.pos != data.len() {
                    bail!("keys list: trailing bytes");
                }
            }
            (false, c) => bail!("unexpected record class {c}"),
        }
        keys.push(k.clone());
        pos += k.nbytes as usize;
    }
    if pos != header.end as usize {
        bail!("records end at {pos}, header says {}", header.end);
    }
    Ok(ParsedFile {
        header,
        keys,
        histos,
        keys_list,
        free,
    })
}
