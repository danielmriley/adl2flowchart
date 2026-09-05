#include "adl2/rootfile/reader.hpp"

#include <cstring>
#include <map>
#include <sstream>

namespace adl2::rootfile {
namespace {

struct Cur {
  const std::uint8_t* buf = nullptr;
  std::size_t len = 0;
  std::size_t pos = 0;

  bool take(std::size_t n, const std::uint8_t** out, std::string* err) {
    if (pos > len || n > len - pos) {
      if (err) *err = "read past end at " + std::to_string(pos);
      return false;
    }
    *out = buf + pos;
    pos += n;
    return true;
  }
  bool u8(std::uint8_t& v, std::string* err) {
    const std::uint8_t* p;
    if (!take(1, &p, err)) return false;
    v = p[0];
    return true;
  }
  bool u16(std::uint16_t& v, std::string* err) {
    const std::uint8_t* p;
    if (!take(2, &p, err)) return false;
    v = static_cast<std::uint16_t>((p[0] << 8) | p[1]);
    return true;
  }
  bool i16(std::int16_t& v, std::string* err) {
    std::uint16_t u;
    if (!u16(u, err)) return false;
    v = static_cast<std::int16_t>(u);
    return true;
  }
  bool u32(std::uint32_t& v, std::string* err) {
    const std::uint8_t* p;
    if (!take(4, &p, err)) return false;
    v = (static_cast<std::uint32_t>(p[0]) << 24) | (static_cast<std::uint32_t>(p[1]) << 16) |
        (static_cast<std::uint32_t>(p[2]) << 8) | static_cast<std::uint32_t>(p[3]);
    return true;
  }
  bool i32(std::int32_t& v, std::string* err) {
    std::uint32_t u;
    if (!u32(u, err)) return false;
    v = static_cast<std::int32_t>(u);
    return true;
  }
  bool f64(double& v, std::string* err) {
    const std::uint8_t* p;
    if (!take(8, &p, err)) return false;
    std::uint64_t u = 0;
    for (int i = 0; i < 8; ++i) u = (u << 8) | p[i];
    std::memcpy(&v, &u, 8);
    return true;
  }
  bool skip(std::size_t n, std::string* err) {
    const std::uint8_t* p;
    return take(n, &p, err);
  }
  // Advance to a frame end recorded earlier; a frame whose end is behind the
  // cursor is malformed, not a rewind.
  bool skip_to(std::size_t end, const char* what, std::string* err) {
    if (end < pos) {
      if (err) *err = std::string(what) + ": frame ends before its own header";
      return false;
    }
    return skip(end - pos, err);
  }
  bool pstring(std::string& s, std::string* err) {
    std::uint8_t n8;
    if (!u8(n8, err)) return false;
    std::uint32_t n = n8;
    if (n8 == 0xFF) {
      if (!u32(n, err)) return false;
    }
    const std::uint8_t* p;
    if (!take(n, &p, err)) return false;
    s.assign(reinterpret_cast<const char*>(p), n);
    return true;
  }
  bool cstring(std::string& s, std::string* err) {
    const std::size_t start = pos;
    while (pos < len && buf[pos] != 0) ++pos;
    if (pos >= len) {
      if (err) *err = "unterminated class name";
      return false;
    }
    s.assign(reinterpret_cast<const char*>(buf + start), pos - start);
    ++pos;
    return true;
  }
  bool frame(const char* what, std::size_t& end, std::int16_t& version, std::string* err) {
    std::uint32_t bc;
    if (!u32(bc, err)) return false;
    if ((bc & 0x40000000u) == 0) {
      if (err) *err = std::string(what) + ": byte-count mask missing";
      return false;
    }
    const std::size_t n = bc & 0x3FFFFFFFu;
    end = pos + n;
    if (!i16(version, err)) return false;
    if (end > len) {
      if (err) *err = std::string(what) + ": frame overruns buffer";
      return false;
    }
    return true;
  }
  bool expect_end(std::size_t end, const char* what, std::string* err) {
    if (pos != end) {
      if (err) *err = std::string(what) + ": byte count mismatch";
      return false;
    }
    return true;
  }
  bool tarrayd(std::vector<double>& vals, std::string* err) {
    std::int32_t n;
    if (!i32(n, err)) return false;
    if (n < 0) {
      if (err) *err = "TArrayD: negative fN";
      return false;
    }
    if (pos > len || static_cast<std::size_t>(n) > (len - pos) / 8) {
      if (err) *err = "TArrayD: fN exceeds buffer";
      return false;
    }
    vals.resize(static_cast<std::size_t>(n));
    for (std::int32_t i = 0; i < n; ++i) {
      if (!f64(vals[static_cast<std::size_t>(i)], err)) return false;
    }
    return true;
  }
};

bool parse_key(Cur& cur, Key& k, std::string* err) {
  k.offset = static_cast<std::uint32_t>(cur.pos);
  std::int16_t version;
  if (!cur.u32(k.nbytes, err) || !cur.i16(version, err)) return false;
  if (version != 4) {
    if (err) *err = "TKey version != 4";
    return false;
  }
  if (!cur.u32(k.objlen, err) || !cur.u32(k.datime, err) || !cur.u16(k.keylen, err) ||
      !cur.i16(k.cycle, err) || !cur.u32(k.seek_key, err) || !cur.u32(k.seek_pdir, err) ||
      !cur.pstring(k.cls, err) || !cur.pstring(k.name, err) || !cur.pstring(k.title, err))
    return false;
  if (cur.pos != k.offset + k.keylen) {
    if (err) *err = "TKey " + k.name + ": keylen mismatch";
    return false;
  }
  return true;
}

bool parse_tnamed(Cur& cur, std::string& name, std::string& title, std::string* err) {
  std::size_t end;
  std::int16_t v;
  if (!cur.frame("TNamed", end, v, err)) return false;
  if (v != 1) {
    if (err) *err = "TNamed version != 1";
    return false;
  }
  if (!cur.skip(10, err) || !cur.pstring(name, err) || !cur.pstring(title, err)) return false;
  return cur.expect_end(end, "TNamed", err);
}

bool parse_flabels(Cur& cur, std::optional<std::vector<std::string>>& labels, std::string* err) {
  std::uint32_t bc;
  if (!cur.u32(bc, err)) return false;
  if (bc == 0) {
    labels = std::nullopt;
    return true;
  }
  if ((bc & 0x40000000u) == 0) {
    if (err) *err = "fLabels: byte-count mask missing";
    return false;
  }
  const std::size_t end = cur.pos + (bc & 0x3FFFFFFFu);
  std::uint32_t tag;
  if (!cur.u32(tag, err) || tag != 0xFFFFFFFFu) {
    if (err) *err = "fLabels: expected kNewClassTag";
    return false;
  }
  std::string cls;
  if (!cur.cstring(cls, err) || cls != "THashList") {
    if (err) *err = "fLabels: expected THashList";
    return false;
  }
  std::size_t lend;
  std::int16_t lv;
  if (!cur.frame("fLabels TList", lend, lv, err) || lv != 5) {
    if (err) *err = "fLabels TList version";
    return false;
  }
  std::string dummy;
  if (!cur.skip(10, err) || !cur.pstring(dummy, err)) return false;
  std::int32_t n;
  if (!cur.i32(n, err) || n < 0) {
    if (err) *err = "fLabels: negative fSize";
    return false;
  }
  if (cur.pos > cur.len || static_cast<std::size_t>(n) > (cur.len - cur.pos) / 8) {
    if (err) *err = "fLabels: fSize exceeds buffer";
    return false;
  }
  std::vector<std::string> out;
  out.reserve(static_cast<std::size_t>(n));
  for (std::int32_t i = 0; i < n; ++i) {
    std::uint32_t sbc, stag;
    if (!cur.u32(sbc, err) || (sbc & 0x40000000u) == 0 || !cur.u32(stag, err) || stag != 0xFFFFFFFFu) {
      if (err) *err = "fLabels entry: expected kNewClassTag";
      return false;
    }
    std::string sclass;
    if (!cur.cstring(sclass, err) || sclass != "TObjString") {
      if (err) *err = "fLabels entry: expected TObjString";
      return false;
    }
    std::size_t send;
    std::int16_t sv;
    if (!cur.frame("TObjString", send, sv, err) || sv != 1) {
      if (err) *err = "TObjString version";
      return false;
    }
    if (!cur.skip(2, err)) return false;
    std::uint32_t uid;
    if (!cur.u32(uid, err) || uid != static_cast<std::uint32_t>(i + 1)) {
      if (err) *err = "fLabels fUniqueID";
      return false;
    }
    std::string lab;
    if (!cur.skip(4, err) || !cur.pstring(lab, err) || !cur.expect_end(send, "TObjString", err))
      return false;
    std::uint8_t opt;
    if (!cur.u8(opt, err) || opt != 0) {
      if (err) *err = "fLabels non-empty option";
      return false;
    }
    out.push_back(std::move(lab));
  }
  if (!cur.expect_end(lend, "fLabels TList", err) || !cur.expect_end(end, "fLabels", err)) return false;
  labels = std::move(out);
  return true;
}

bool parse_taxis(Cur& cur, AxisData& ax, std::string* err) {
  std::size_t end;
  std::int16_t v;
  if (!cur.frame("TAxis", end, v, err) || v != 10) {
    if (err) *err = "TAxis version";
    return false;
  }
  std::string n, t;
  if (!parse_tnamed(cur, n, t, err)) return false;
  std::size_t aend;
  std::int16_t av;
  if (!cur.frame("TAttAxis", aend, av, err) || av != 4) {
    if (err) *err = "TAttAxis version";
    return false;
  }
  if (!cur.skip_to(aend, "TAttAxis", err)) return false;
  if (!cur.i32(ax.nbins, err) || !cur.f64(ax.lo, err) || !cur.f64(ax.hi, err) ||
      !cur.tarrayd(ax.edges, err))
    return false;
  if (!ax.edges.empty() && ax.edges.size() != static_cast<std::size_t>(ax.nbins + 1)) {
    if (err) *err = "TAxis: fXbins length mismatch";
    return false;
  }
  std::string tf;
  if (!cur.skip(4 + 4 + 2 + 1, err) || !cur.pstring(tf, err) || !parse_flabels(cur, ax.labels, err))
    return false;
  if (ax.labels && ax.labels->size() != static_cast<std::size_t>(ax.nbins)) {
    if (err) *err = "TAxis: label count != nbins";
    return false;
  }
  std::uint32_t mod;
  if (!cur.u32(mod, err) || mod != 0) {
    if (err) *err = "TAxis: fModLabs not null";
    return false;
  }
  return cur.expect_end(end, "TAxis", err);
}

struct Th1Body {
  std::string name;
  std::string title;
  std::int32_t ncells = 0;
  AxisData xaxis;
  AxisData yaxis;
  double entries = 0;
  double tsumw = 0;
  double tsumw2 = 0;
  double tsumwx = 0;
  double tsumwx2 = 0;
  std::vector<double> sumw2;
};

bool parse_th1_body(Cur& cur, Th1Body& b, std::string* err) {
  std::size_t h1end;
  std::int16_t h1v;
  if (!cur.frame("TH1", h1end, h1v, err) || h1v != 8) {
    if (err) *err = "TH1 version";
    return false;
  }
  if (!parse_tnamed(cur, b.name, b.title, err)) return false;
  for (const char* att : {"TAttLine", "TAttFill", "TAttMarker"}) {
    std::size_t aend;
    std::int16_t av;
    if (!cur.frame(att, aend, av, err) || !cur.skip_to(aend, att, err)) return false;
  }
  AxisData z;
  if (!cur.i32(b.ncells, err) || !parse_taxis(cur, b.xaxis, err) || !parse_taxis(cur, b.yaxis, err) ||
      !parse_taxis(cur, z, err))
    return false;
  if (!cur.skip(4, err) || !cur.f64(b.entries, err) || !cur.f64(b.tsumw, err) || !cur.f64(b.tsumw2, err) ||
      !cur.f64(b.tsumwx, err) || !cur.f64(b.tsumwx2, err) || !cur.skip(24, err))
    return false;
  std::vector<double> contour;
  if (!cur.tarrayd(contour, err) || !contour.empty()) {
    if (err) *err = b.name + ": non-empty fContour";
    return false;
  }
  std::string opt;
  if (!cur.tarrayd(b.sumw2, err) || !cur.pstring(opt, err)) return false;
  std::size_t fend;
  std::int16_t fv;
  if (!cur.frame("fFunctions TList", fend, fv, err) || fv != 5) {
    if (err) *err = "fFunctions TList version";
    return false;
  }
  std::string fname;
  std::int32_t fsz;
  if (!cur.skip(10, err) || !cur.pstring(fname, err) || !cur.i32(fsz, err) || fsz != 0 ||
      !cur.expect_end(fend, "fFunctions", err))
    return false;
  std::int32_t bufsz, stat;
  if (!cur.i32(bufsz, err) || bufsz != 0 || !cur.skip(1, err) || !cur.skip(4, err) || !cur.i32(stat, err) ||
      stat != 2) {
    if (err) *err = b.name + ": fBufferSize/fStatOverflows";
    return false;
  }
  return cur.expect_end(h1end, "TH1", err);
}

bool parse_th1d(const std::uint8_t* payload, std::size_t n, const std::vector<std::string>& path,
                Th1dData& h, std::string* err) {
  Cur cur{payload, n, 0};
  std::size_t end;
  std::int16_t v;
  if (!cur.frame("TH1D", end, v, err) || v != 3) {
    if (err) *err = "TH1D version";
    return false;
  }
  Th1Body b;
  if (!parse_th1_body(cur, b, err)) return false;
  std::vector<double> contents;
  if (!cur.tarrayd(contents, err) || !cur.expect_end(end, "TH1D", err)) return false;
  if (cur.pos != n) {
    if (err) *err = b.name + ": trailing payload bytes";
    return false;
  }
  if (b.ncells != b.xaxis.nbins + 2) {
    if (err) *err = b.name + ": fNcells != nbins + 2";
    return false;
  }
  if (contents.size() != static_cast<std::size_t>(b.ncells) ||
      b.sumw2.size() != static_cast<std::size_t>(b.ncells)) {
    if (err) *err = b.name + ": array lengths != fNcells";
    return false;
  }
  h.path = path;
  h.name = std::move(b.name);
  h.title = std::move(b.title);
  h.nbins = b.xaxis.nbins;
  h.lo = b.xaxis.lo;
  h.hi = b.xaxis.hi;
  h.edges = std::move(b.xaxis.edges);
  h.labels = std::move(b.xaxis.labels);
  h.contents = std::move(contents);
  h.sumw2 = std::move(b.sumw2);
  h.entries = b.entries;
  h.tsumw = b.tsumw;
  h.tsumw2 = b.tsumw2;
  h.tsumwx = b.tsumwx;
  h.tsumwx2 = b.tsumwx2;
  return true;
}

bool parse_th2d(const std::uint8_t* payload, std::size_t n, const std::vector<std::string>& path,
                Th2dData& h, std::string* err) {
  Cur cur{payload, n, 0};
  std::size_t end;
  std::int16_t v;
  if (!cur.frame("TH2D", end, v, err) || v != 4) {
    if (err) *err = "TH2D version";
    return false;
  }
  std::size_t h2end;
  std::int16_t h2v;
  if (!cur.frame("TH2", h2end, h2v, err) || h2v != 5) {
    if (err) *err = "TH2 version";
    return false;
  }
  Th1Body b;
  if (!parse_th1_body(cur, b, err)) return false;
  double scalefactor, tsumwy, tsumwy2, tsumwxy;
  if (!cur.f64(scalefactor, err) || scalefactor != 1.0 || !cur.f64(tsumwy, err) || !cur.f64(tsumwy2, err) ||
      !cur.f64(tsumwxy, err) || !cur.expect_end(h2end, "TH2", err)) {
    if (err && err->empty()) *err = b.name + ": fScalefactor";
    return false;
  }
  std::vector<double> contents;
  if (!cur.tarrayd(contents, err) || !cur.expect_end(end, "TH2D", err)) return false;
  if (cur.pos != n) {
    if (err) *err = b.name + ": trailing payload bytes";
    return false;
  }
  const std::int32_t ncells = (b.xaxis.nbins + 2) * (b.yaxis.nbins + 2);
  if (b.ncells != ncells) {
    if (err) *err = b.name + ": fNcells mismatch";
    return false;
  }
  h.path = path;
  h.name = std::move(b.name);
  h.title = std::move(b.title);
  h.nx = b.xaxis.nbins;
  h.xlo = b.xaxis.lo;
  h.xhi = b.xaxis.hi;
  h.ny = b.yaxis.nbins;
  h.ylo = b.yaxis.lo;
  h.yhi = b.yaxis.hi;
  h.contents = std::move(contents);
  h.sumw2 = std::move(b.sumw2);
  h.entries = b.entries;
  h.tsumw = b.tsumw;
  h.tsumw2 = b.tsumw2;
  h.tsumwx = b.tsumwx;
  h.tsumwx2 = b.tsumwx2;
  h.tsumwy = tsumwy;
  h.tsumwy2 = tsumwy2;
  h.tsumwxy = tsumwxy;
  return true;
}

struct DirHeader {
  std::uint32_t nbytes_keys = 0;
  std::uint32_t nbytes_name = 0;
  std::uint32_t seek_dir = 0;
  std::uint32_t seek_parent = 0;
  std::uint32_t seek_keys = 0;
};

bool parse_dir_header(const std::uint8_t* data, std::size_t n, DirHeader& hd, std::string* err) {
  Cur cur{data, n, 0};
  std::int16_t v;
  if (!cur.i16(v, err) || v != 5) {
    if (err) *err = "directory header version";
    return false;
  }
  if (!cur.skip(8, err) || !cur.u32(hd.nbytes_keys, err) || !cur.u32(hd.nbytes_name, err) ||
      !cur.u32(hd.seek_dir, err) || !cur.u32(hd.seek_parent, err) || !cur.u32(hd.seek_keys, err))
    return false;
  const std::uint8_t* uuidv;
  if (!cur.take(2, &uuidv, err) || uuidv[0] != 0 || uuidv[1] != 1) {
    if (err) *err = "directory header: bad TUUID version";
    return false;
  }
  if (!cur.skip(16, err)) return false;
  const std::uint8_t* pad;
  if (!cur.take(12, &pad, err)) return false;
  for (int i = 0; i < 12; ++i) {
    if (pad[i] != 0) {
      if (err) *err = "directory header: non-zero padding";
      return false;
    }
  }
  if (cur.pos != n) {
    if (err) *err = "directory header: trailing bytes";
    return false;
  }
  return true;
}

struct Record {
  Key key;
  const std::uint8_t* data = nullptr;
  std::size_t data_len = 0;
  bool visited = false;
};

constexpr std::size_t kMaxDirDepth = 64;

bool walk_dir(std::map<std::uint32_t, Record>& records, std::uint32_t dir_loc, std::uint32_t seek_keys,
              std::uint32_t nbytes_keys, const std::vector<std::string>& path, ParsedFile& out,
              std::vector<std::string>* root_names, std::string* err) {
  if (path.size() > kMaxDirDepth) {
    if (err) *err = "directory nesting too deep";
    return false;
  }
  auto it = records.find(seek_keys);
  if (it == records.end()) {
    if (err) *err = "fSeekKeys is not a record";
    return false;
  }
  if (it->second.visited) {
    if (err) *err = "directory cycle";
    return false;
  }
  it->second.visited = true;
  if (it->second.key.nbytes != nbytes_keys) {
    if (err) *err = "fNbytesKeys mismatch";
    return false;
  }
  Cur lc{it->second.data, it->second.data_len, 0};
  std::uint32_t n;
  if (!lc.u32(n, err)) return false;
  std::vector<std::string> names;
  for (std::uint32_t i = 0; i < n; ++i) {
    Key child;
    if (!parse_key(lc, child, err)) return false;
    names.push_back(child.name);
    auto rit = records.find(child.seek_key);
    if (rit == records.end()) {
      if (err) *err = "keys list points nowhere: " + child.name;
      return false;
    }
    if (rit->second.visited) {
      if (err) *err = "record listed twice: " + child.name;
      return false;
    }
    rit->second.visited = true;
    if (rit->second.key.seek_pdir != dir_loc) {
      if (err) *err = "fSeekPdir mismatch for " + child.name;
      return false;
    }
    const std::string& cls = rit->second.key.cls;
    if (cls == "TH1D") {
      Th1dData h;
      if (!parse_th1d(rit->second.data, rit->second.data_len, path, h, err)) return false;
      out.histos.push_back(std::move(h));
    } else if (cls == "TH2D") {
      Th2dData h;
      if (!parse_th2d(rit->second.data, rit->second.data_len, path, h, err)) return false;
      out.th2s.push_back(std::move(h));
    } else if (cls == "TNamed") {
      Cur nc{rit->second.data, rit->second.data_len, 0};
      std::string nm, title;
      if (!parse_tnamed(nc, nm, title, err) || nc.pos != rit->second.data_len) {
        if (err) *err = "TNamed trailing bytes";
        return false;
      }
      if (nm != rit->second.key.name) {
        if (err) *err = "TNamed key/object name mismatch";
        return false;
      }
      out.named.emplace_back(path, nm, title);
    } else if (cls == "TDirectory") {
      DirHeader hd;
      if (!parse_dir_header(rit->second.data, rit->second.data_len, hd, err)) return false;
      if (hd.seek_dir != rit->second.key.offset || hd.seek_parent != dir_loc ||
          hd.nbytes_name != rit->second.key.keylen) {
        if (err) *err = "directory header pointers";
        return false;
      }
      auto sub = path;
      sub.push_back(child.name);
      out.dirs.push_back(sub);
      if (!walk_dir(records, rit->second.key.offset, hd.seek_keys, hd.nbytes_keys, sub, out, nullptr,
                    err))
        return false;
    } else {
      if (err) *err = "unexpected class " + cls;
      return false;
    }
  }
  if (lc.pos != it->second.data_len) {
    if (err) *err = "keys list trailing bytes";
    return false;
  }
  if (root_names) *root_names = std::move(names);
  return true;
}

}  // namespace

std::optional<ParsedFile> parse(const std::uint8_t* buf, std::size_t n, VerifyError* verr) {
  std::string err;
  auto fail = [&]() -> std::optional<ParsedFile> {
    if (verr) verr->message = err;
    return std::nullopt;
  };
  if (n < 100) {
    err = "file shorter than the 100-byte header";
    return fail();
  }
  if (std::memcmp(buf, "root", 4) != 0) {
    err = "bad magic";
    return fail();
  }
  Cur cur{buf, n, 4};
  ParsedFile out;
  if (!cur.i32(out.header.version, &err) || !cur.u32(out.header.begin, &err) ||
      !cur.u32(out.header.end, &err) || !cur.u32(out.header.seek_free, &err) ||
      !cur.u32(out.header.nbytes_free, &err) || !cur.u32(out.header.nfree, &err) ||
      !cur.u32(out.header.nbytes_name, &err) || !cur.u8(out.header.units, &err) ||
      !cur.i32(out.header.compress, &err) || !cur.u32(out.header.seek_info, &err) ||
      !cur.u32(out.header.nbytes_info, &err))
    return fail();
  if (out.header.begin != 100 || out.header.units != 4) {
    err = "unexpected fBEGIN/fUnits";
    return fail();
  }
  if (out.header.end != n) {
    err = "fEND != file size";
    return fail();
  }

  std::map<std::uint32_t, Record> records;
  std::size_t pos = out.header.begin;
  while (pos < out.header.end) {
    Cur kc{buf, n, pos};
    Key k;
    if (!parse_key(kc, k, &err)) return fail();
    if (k.seek_key != pos) {
      err = "fSeekKey mismatch";
      return fail();
    }
    if (k.keylen > k.nbytes || pos + k.nbytes > n) {
      err = "record overruns buffer";
      return fail();
    }
    if (k.objlen != k.nbytes - k.keylen) {
      err = "not uncompressed";
      return fail();
    }
    if (k.cycle != 1) {
      err = "cycle != 1";
      return fail();
    }
    Record rec;
    rec.key = k;
    rec.data = buf + pos + k.keylen;
    rec.data_len = k.nbytes - k.keylen;
    out.keys.push_back(k);
    records[static_cast<std::uint32_t>(pos)] = rec;
    pos += k.nbytes;
  }
  if (pos != out.header.end) {
    err = "records end mismatch";
    return fail();
  }

  Record& name_rec = records[out.header.begin];
  name_rec.visited = true;
  Cur nc{name_rec.data, name_rec.data_len, 0};
  std::string dummy;
  if (!nc.pstring(dummy, &err) || !nc.pstring(dummy, &err)) return fail();
  DirHeader root_hd;
  if (!parse_dir_header(name_rec.data + nc.pos, name_rec.data_len - nc.pos, root_hd, &err)) return fail();
  if (root_hd.seek_dir != out.header.begin || root_hd.seek_parent != 0 ||
      root_hd.nbytes_name != out.header.nbytes_name) {
    err = "root directory header pointers";
    return fail();
  }

  auto siit = records.find(out.header.seek_info);
  if (siit == records.end()) {
    err = "fSeekInfo points nowhere";
    return fail();
  }
  siit->second.visited = true;
  if (siit->second.key.cls != "TList" || siit->second.key.name != "StreamerInfo" ||
      siit->second.key.nbytes != out.header.nbytes_info) {
    err = "StreamerInfo record";
    return fail();
  }

  auto frit = records.find(out.header.seek_free);
  if (frit == records.end()) {
    err = "fSeekFree points nowhere";
    return fail();
  }
  frit->second.visited = true;
  Cur fc{frit->second.data, frit->second.data_len, 0};
  while (fc.pos < frit->second.data_len) {
    std::uint16_t ver;
    std::uint32_t a, b;
    if (!fc.u16(ver, &err) || ver != 1 || !fc.u32(a, &err) || !fc.u32(b, &err)) return fail();
    out.free.emplace_back(a, b);
  }
  if (out.free.size() != out.header.nfree) {
    err = "nfree mismatch";
    return fail();
  }

  if (!walk_dir(records, out.header.begin, root_hd.seek_keys, root_hd.nbytes_keys, {}, out,
                &out.keys_list, &err))
    return fail();

  for (const auto& kv : records) {
    if (!kv.second.visited) {
      err = "unreachable record " + kv.second.key.name;
      return fail();
    }
  }
  return out;
}

}  // namespace adl2::rootfile
