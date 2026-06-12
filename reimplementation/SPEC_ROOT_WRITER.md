# SPEC: Pure-Rust ROOT Histogram File Writer (smash2)

Status: pre-implementation spec, researched 2026-06-12 against primary sources (uproot5
writing code, go-hep/groot, root.cern format docs). Input: canonical `histos.json`
(name, title, region path, uniform edges lo/hi/n, per-bin sumw, per-bin sumw2,
underflow, overflow, n_entries). Output: a `.root` file that ROOT TBrowser / hadd /
PyROOT / uproot open natively, containing TH1D objects with Sumw2 errors.

Feasibility is proven: uproot5 writes these files from scratch in pure Python
([uproot writing/_cascade.py](https://github.com/scikit-hep/uproot5/blob/main/src/uproot/writing/_cascade.py)),
and go-hep's groot writes H1D/H2D with full two-way ROOT compatibility
([groot docs](https://pkg.go.dev/go-hep.org/x/hep/groot),
[rhist H1D.MarshalROOT](https://pkg.go.dev/go-hep.org/x/hep/groot/rhist)). All
multi-byte integers/floats are **big-endian** except the 3-byte sizes inside
compression block headers, which are little-endian.

## 1. File container

Source: [TFile class docs, "The File Format" tables](https://root.cern/doc/master/classTFile.html);
uproot `_cascade.py` `FileHeader`/`Key` classes.

**Header (offset 0, record area starts at fBEGIN = 100; bytes 96–99 reserved, must be 0):**

| field | small (fVersion < 1000000) | large (fVersion ≥ 1000000) |
|---|---|---|
| magic | `"root"` (4B) | same |
| fVersion | i32, uproot pins **62400** | 62400 + 1000000 |
| fBEGIN | i32 = 100 | i64 |
| fEND | i32 (first free byte at EOF) | i64 |
| fSeekFree / fNbytesFree | i32 / i32 (FreeSegments record ptr/len) | i64 / i32 |
| nfree | i32 (count of free segments) | i32 |
| fNbytesName | i32 (len of the fBEGIN name record) | i32 (uproot table shows widening; copy uproot bytes) |
| fUnits | u8 = 4 | u8 = 8 |
| fCompress | i32 (0 ⇒ uproot writes `ZLIB(0).code`) | same |
| fSeekInfo / fNbytesInfo | i32 / i32 (StreamerInfo record) | i64 / i32 |
| fUUID | 2B version (`\x00\x01`) + 16B UUID | same |

v1 decision: always write the **small** header (our files are ≪ 2 GB); switch threshold
is `kStartBigFile` (2 GB) only.

**TKey record** (precedes every object's data; same layout per
[TFile docs](https://root.cern/doc/master/classTFile.html), uproot `Key.serialize`):
`fNbytes` i32 (KeyLen + compressed data len), `fVersion` i16 (**4** small / **1004**
big — big iff seekKey or parent ≥ 2 GB), `fObjlen` i32 (uncompressed data len),
`fDatime` u32, `fKeylen` i16, `fCycle` i16 (start at 1), `fSeekKey` i32/i64 (offset of
this key itself), `fSeekPdir` i32/i64 (offset of owning directory's key/header), then
three Pascal strings: class name, object name, title (1-byte len; len ≥ 255 → `0xFF` +
u32 len, per uproot
[serialization.py](https://github.com/scikit-hep/uproot5/blob/main/src/uproot/serialization.py)).
Uncompressed record ⇔ `fObjlen == fNbytes - fKeylen` — valid and read natively; this is
v1.

**TDatime packing** ([TDatime.cxx](https://root.cern/doc/master/TDatime_8cxx_source.html)):
`(year-1995)<<26 | month<<22 | day<<17 | hour<<12 | min<<6 | sec`.

**File layout we will emit (mirrors uproot):**
1. header (100 B);
2. the "name record" at fBEGIN: TKey(class `TFile`, name = file name, title) whose data
   is file name + title strings followed by the **root directory header** (fNbytesName
   = key len + that data);
3. one record per object (TH1D, subdirectories);
4. StreamerInfo record (key name `StreamerInfo`, class `TList`, cycle 1) → header
   fSeekInfo/fNbytesInfo;
5. each directory's **keys list** record (i32 nkeys, then the serialized TKeys of that
   dir's entries);
6. FreeSegments record (key class `TFile`); segments serialized as `>HII` small /
   `>HQQ` big: (version, first, last); one terminal "infinity" segment
   `[fEND, kStartBigFile)`. Since we are create-only/write-once, the free list is this
   single trailing segment — no real free-space management needed.

**Directory header** (uproot `DirectoryHeader`, class_version **5**, +1000 when big):
fVersion i16, fDatimeC u32, fDatimeM u32, fNbytesKeys i32 (size of keys-list record),
fNbytesName i32, fSeekDir, fSeekParent, fSeekKeys (i32 small / i64 big), then UUID
(`\x00\x01` + 16 B) and, in small form, 12 B padding (3 × i32 0 — copy uproot bytes).

**Compression (v2):** 9-byte block header `"ZL"`, method byte `0x08`, 3-byte LE
compressed size, 3-byte LE uncompressed size, then a raw zlib stream; blocks split at
2^24−1 uncompressed bytes; if compressed ≥ original (or level 0), store uncompressed.
Source: [uproot compression.py](https://github.com/scikit-hep/uproot5/blob/main/src/uproot/compression.py).

## 2. TH1D object serialization

Sources: [uproot models/TH.py](https://github.com/scikit-hep/uproot5/blob/main/src/uproot/models/TH.py)
(`Model_TH1D_v3`, `Model_TH1_v8`, `Model_TAxis_v10` `_serialize`),
[serialization.py](https://github.com/scikit-hep/uproot5/blob/main/src/uproot/serialization.py);
cross-check [groot rhist/h1.go MarshalROOT](https://github.com/go-hep/hep/blob/main/groot/rhist/h1.go).

Every versioned object/base is framed `u32 (nbytes+2 | kByteCountMask=0x4000_0000)` +
`i16 version` (uproot `numbytes_version`). Pinned versions (what uproot emits today):
**TH1D=3, TH1=8, TNamed=1, TObject=1, TAttLine=2, TAttFill=2, TAttMarker=2, TAxis=10,
TAttAxis=4, TList=5, THashList=0, TString=2 (streamer only)**. TArrayD has **no**
byte-count/version header: raw `i32 fN` + fN f64.

TObject base bytes (uproot `Model_TObject._serialize`): `\x00\x01` (version 1, no byte
count) + u32 fUniqueID=0 + u32 fBits (caller-supplied flags: kNotDeleted | kIsOnHeap
etc.; copy uproot's `tobject_flags`, incl. its `kMustCleanup` for bases and the
`|(1<<16)` quirk on fFunctions). No pid short on write.

**TH1D stream** = [TH1D header v3] → TH1 (below) → TArrayD bin contents
(fN = fNcells, includes under/overflow). **TH1 stream (v8)**, in order: bases TNamed
(TObject + fName + fTitle strings), TAttLine v2 (3×i16: color 602, style 1, width 1),
TAttFill v2 (2×i16: color 0, style 1001), TAttMarker v2 (i16 color 1, i16 style 1, f32
size 1.0); then `fNcells` i32; fXaxis, fYaxis, fZaxis (inline TAxis objects, each with
byte-count header); `fBarOffset` i16 =0, `fBarWidth` i16 =1000; f64 ×8: fEntries,
fTsumw, fTsumw2, fTsumwx, fTsumwx2, fMaximum=−1111, fMinimum=−1111, fNormFactor=0;
fContour TArrayD (empty); **fSumw2 TArrayD**; fOption TString (""); fFunctions —
serialized via object-any as an **empty TList** (TObject base + fName "" + i32 fSize 0);
fBufferSize i32 0, optional 1-byte speed-bump, fBuffer (0 f64s); fBinStatErrOpt i32 0,
fStatOverflows i32 2.

**TAxis stream (v10):** bases TNamed (name "xaxis"/"yaxis"/"zaxis", title = axis label)
and TAttAxis v4 (fNdivisions i32 510, fAxisColor 1, fLabelColor 1, fLabelFont 42 — i16s;
f32 fLabelOffset 0.005, fLabelSize 0.035, fTickLength 0.03, fTitleOffset 1.0, fTitleSize
0.035; i16 fTitleColor 1, fTitleFont 42); then i32 fNbins, f64 fXmin, f64 fXmax; fXbins
TArrayD **empty for uniform bins**; fFirst i32 0, fLast i32 0, fBits2 u16 0,
fTimeDisplay bool 0; fTimeFormat TString ""; fLabels (THashList*) and fModLabs (TList*)
as object-any **null pointers = u32 0**. For 1-D, fYaxis/fZaxis are dummy 1-bin axes
(0,1). Defaults per uproot
[identify.to_TAxis / to_TH1x](https://github.com/scikit-hep/uproot5/blob/main/src/uproot/writing/identify.py).

Object-any pointer encoding (for fFunctions/fLabels/fModLabs): null → u32 0; first
occurrence of a class → u32 (nbytes|kByteCountMask), i16 version is replaced by tag
u32 `kNewClassTag = 0xFFFFFFFF` + NUL-terminated class name, then object body;
back-references use kClassMask 0x8000_0000. We only ever need null and first-occurrence
forms.

**StreamerInfo record.** ROOT files are self-describing via TStreamerInfo
([root manual](https://root.cern/manual/root_files/)). uproot does **not** build
streamers at runtime: it embeds byte-exact pre-serialized TStreamerInfo blobs with
**hardcoded fCheckSum** (`_rawstreamer_TH1D_v3 = (None, b"@\x00\x01W\xff\xff\xff\xffTStreamerInfo\x00...", "TH1D", 3)`).
The full set required for TH1D: TObject, TString, TCollection v3, TSeqCollection v0,
TList v5, THashList v0, TAttLine v2, TAttFill v2, TAttMarker v2, TAttAxis v4, TAxis
v10, TNamed v1, TH1 v8, TH1D v3 (uproot `Model_TH1D_v3.class_rawstreamers`). The record
is a TList (header packs `nbytes|mask, version=5, 1, 0, kNotDeleted, 0, nentries`)
written uncompressed. **We do the same: vendor the blob.** Generate one reference file
with uproot in CI tooling, copy the StreamerInfo record's data bytes verbatim into a
Rust `include_bytes!` asset. No checksum algorithm needs implementing.

## 3. Subdirectories vs flat names

A nested TDirectory costs: one TKey+record whose data is name/title strings + a
directory header (§1), one keys-list record, parent links (fSeekParent, child key's
fSeekPdir), and inclusion of the child's TKey in the parent's keys list. Roughly
~150 bytes + bookkeeping per directory; the cascade offset arithmetic is the only real
cost.

- hadd merges recursively and preserves sub-directories ("The files may contain
  sub-directories", objects matched by name+path;
  [hadd docs](https://root.cern/doc/master/hadd_8cxx.html)). Both layouts merge fine.
- TBrowser shows directories as folders (nice per-region browsing); flat
  `SR1_h_met` names all land in one list.

Recommendation: v1 flat (`<region path with '/'→'_'>_<histo name>`), v2 per-region
TDirectories. Both are hadd-safe; names must be stable across runs for merging.

## 4. histos.json → TH1D mapping

| histos.json | TH1D member | notes |
|---|---|---|
| name / "title" | fName / fTitle (TNamed) | key name = fName; reader picks highest cycle (`;1`) |
| n, lo, hi | fXaxis.fNbins/fXmin/fXmax; fXbins empty | uniform bins only |
| — | fNcells = n + 2 | |
| sumw[1..n], underflow, overflow | TArrayD contents: `[0]`=underflow, `[n+1]`=overflow | |
| sumw2[0..n+1] | fSumw2 TArrayD, len fNcells incl. flow bins | if absent, ROOT uses err = √(bin content) ([TH1 docs](https://root.cern/doc/master/classTH1.html)); we always fill weighted ⇒ **always write fSumw2** |
| n_entries | fEntries (f64) | raw Fill-call count, ≠ Σw; SetEntries semantics |
| Σ weights (all fills incl. flow) | fTsumw; fTsumw2 = Σw² | derivable from sumw/sumw2 arrays (in-range per ROOT convention: GetStats excludes flow) |
| **not tracked today** | fTsumwx = Σw·x, fTsumwx2 = Σw·x² | see below |

fTsumwx/fTsumwx2 gap: GetMean/GetStdDev are computed from this stats array — "if no
range has been set, the returned values are the (unbinned) ones calculated at fill
time" ([TH1 docs](https://root.cern/doc/master/classTH1.html)). Zeroing them makes
GetMean()=0 and corrupts merged stats under hadd (stats arrays are summed). Options:
(a) **preferred** — accumulate Σw·x, Σw·x² per histogram in the smash2 fill loop and
add both to histos.json (two f64s per histo, trivial cost); (b) fallback — binned
approximation at write time, exactly what uproot does for hist-library objects:
`fTsumw=fTsumw2=Σ(in-range contents)`, `fTsumwx=Σ(content·center)`,
`fTsumwx2=Σ(content·center²)` (uproot `_root_stats_1d`, identify.py). Equivalent to
ROOT's ResetStats(); mean correct to within binning. Never write zeros.

## 5. Validation strategy

1. **Primary CI oracle: uproot** in a pinned venv. Read back every written file:
   values(flow=True), errors², axis edges, fEntries, member-level `all_members`, and
   `file.streamers` presence. Pure pip, runs everywhere.
2. **Byte-diff**: write the same histogram with uproot (`f["h"] = (np-arrays...)` via
   `to_TH1x`) and with smash2, with pinned fDatime/UUID injected into both; assert
   byte equality of the TH1D data payload (whole-file diff only after normalizing
   datime/UUID/name-record). This is the strongest regression net.
3. **ROOT binary, env-gated** (`if command -v root`): macro asserting
   GetBinContent/GetBinError/GetEntries/GetMean, plus `TFile::Open` with no
   "StreamerInfo" warnings on stderr.
4. **hadd smoke test** (env-gated): `hadd merged.root a.root b.root`; assert summed
   contents, summed fEntries, and sane merged GetMean.
5. **oxyroot is NOT usable as a third oracle**: it reads/writes TTrees and branches
   only, no TH1 support ([oxyroot README](https://github.com/m-dupont/oxyroot)). Drop
   it; go-hep `root-dump`/groot (Go) could be an optional third reader if ever wanted.

## 6. Risks, gotchas, effort

- **Pinned-version drift**: we emit ROOT 6.24-era versions (TH1 v8, TAxis v10, file
  62400). Forward compatibility is ROOT's own guarantee via StreamerInfo — old files
  stay readable forever; risk is only if a future TH1D bumps and we want new members.
  Low. Mitigation: byte-diff CI against pinned uproot catches any drift on our side.
- **Free list**: create-only ⇒ a single terminal free segment record like uproot; no
  deletion/rewrite management ever needed. Confirmed sufficient (uproot does exactly
  this for fresh files).
- **TList vs THashList**: fFunctions is `TList*` — write an empty **TList**, not
  THashList; fLabels is `THashList*` — write null. THashList adds no members of its own
  (v0) but the class tag string differs; mixing them is a classic read failure.
- **Byte-count discipline**: every header's nbytes must equal the bytes that follow it
  (excluding the 6 header bytes, hence the `+2` for the version short in uproot's
  `numbytes_version`). Off-by-one here is the dominant from-scratch bug class; ROOT
  reports "byte count mismatch" with class name, which aids debugging.
- **Keys-list/fNbytesKeys/fSeekKeys circularity**: directory header is written before
  the keys list exists; uproot back-patches. Our write-once model can instead buffer
  the whole file in memory and patch before flush (files are KB–MB scale).
- **Name cycles**: write each key once with fCycle=1 (`h;1`); readers auto-pick highest
  cycle ([root manual](https://root.cern/manual/root_files/)). Never reuse a name in
  one directory.
- **TDatime**: a zero datime is technically readable but looks like 1995; write real
  time, but make it injectable for byte-diff tests.

**Effort estimate** (one engineer, includes tests):
- v1 — flat, uncompressed, TH1D-only + uproot oracle + byte-diff harness: **3–4 days**
  (≈1 day container cascade, 1 day TH1D stream, 1–2 days validation/debug).
- +zlib (flate2): **0.5–1 day**.
- +per-region TDirectories: **1–1.5 days** (offset bookkeeping + hadd test).
- +TH2D: **1–2 days** (TH2 v5 base inserts fScalefactor/fTsumwy/fTsumwy2/fTsumwxy;
  needs its own vendored streamers; verify against uproot `to_TH2x`).

## Open decisions for Daniel

1. Accumulate fTsumwx/fTsumwx2 (+ flow-inclusive Σw, Σw²) in the smash2 fill loop and
   extend histos.json (recommended), or accept binned-approximation stats at write time?
2. v1 directory layout: flat `REGION_histo` names now, TDirectories in v2 — OK?
3. fEntries: confirm histos.json `n_entries` is raw fill count (not Σw) — that is what
   ROOT expects in fEntries.
4. Vendor the StreamerInfo blob from a CI-generated uproot reference file (proposed) vs
   hand-implementing TStreamerInfo/TStreamerElement serialization (+2–3 days, no real
   benefit until we emit classes uproot can't).
5. Compression: ship v1 uncompressed (simplest, bit-stable diffs) and add ZLIB in v2,
   or require zlib from day one? (Histo files are tiny; uncompressed is fine.)
6. Where does the writer live: new crate `crates/rootfile` in the adl2 workspace
   (recommended — reusable, zero deps beyond `flate2` later), or a module inside the
   histogram crate?

## Phased scope

- **v1**: `rootfile` crate; small-format header, single root directory, flat names,
  uncompressed records, TH1D v3 with Sumw2, vendored StreamerInfo, free list with
  terminal segment; uproot read-back + byte-diff CI; env-gated root/hadd smoke tests.
- **v2**: per-region nested TDirectories; ZLIB compression; fTsumwx/fTsumwx2 from fill
  loop once histos.json grows the fields.
- **v3 (optional)**: TH2D; large-file (+1000000/1004) support if ever needed; TObjString
  metadata key (ADL source snapshot) for provenance.
