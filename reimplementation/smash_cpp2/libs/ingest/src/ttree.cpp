#include "ttree.hpp"

#include <algorithm>
#include <cstdlib>
#include <cstring>
#include <fstream>
#include <stdexcept>
#include <unordered_map>
#include <zlib.h>

namespace adl2::ingest::detail {
namespace {

constexpr std::uint32_t kByteCountMask = 0x40000000u;
constexpr std::uint32_t kNewClassTag = 0xFFFFFFFFu;
constexpr std::uint32_t kClassMask = 0x80000000u;
constexpr std::uint32_t kMapOffset = 2;

struct ParseError : std::runtime_error {
  using std::runtime_error::runtime_error;
};

struct Cursor {
  const std::uint8_t* p = nullptr;
  std::size_t n = 0;
  std::size_t i = 0;

  void need(std::size_t k) const {
    // Written so neither `i + k` nor `n - i` can wrap: the cursor may sit
    // past `n` after a frame-relative `i = end` assignment.
    if (i > n || k > n - i) throw ParseError("short ROOT read");
  }
  std::uint8_t u8() {
    need(1);
    return p[i++];
  }
  std::uint16_t u16() {
    need(2);
    std::uint16_t v = (static_cast<std::uint16_t>(p[i]) << 8) | p[i + 1];
    i += 2;
    return v;
  }
  std::int16_t i16() { return static_cast<std::int16_t>(u16()); }
  std::uint32_t u32() {
    need(4);
    std::uint32_t v = (static_cast<std::uint32_t>(p[i]) << 24) | (static_cast<std::uint32_t>(p[i + 1]) << 16) |
                      (static_cast<std::uint32_t>(p[i + 2]) << 8) | p[i + 3];
    i += 4;
    return v;
  }
  std::int32_t i32() { return static_cast<std::int32_t>(u32()); }
  std::uint64_t u64() {
    need(8);
    std::uint64_t v = 0;
    for (int k = 0; k < 8; ++k) v = (v << 8) | p[i++];
    return v;
  }
  std::int64_t i64() { return static_cast<std::int64_t>(u64()); }

  std::string pstr() {
    std::size_t len = u8();
    if (len == 0xFF) len = u32();
    need(len);
    std::string s(reinterpret_cast<const char*>(p + i), len);
    i += len;
    return s;
  }
  std::string cstring() {
    std::size_t start = i;
    while (i < n && p[i] != 0) ++i;
    std::string s(reinterpret_cast<const char*>(p + start), i - start);
    if (i < n) ++i;
    return s;
  }
  void skip(std::size_t k) {
    need(k);
    i += k;
  }
  std::pair<std::size_t, std::int16_t> frame() {
    std::uint32_t bc = u32();
    if ((bc & kByteCountMask) == 0) {
      i -= 4;
      throw ParseError("expected bytecount frame");
    }
    std::size_t end = i + (bc & 0x3FFFFFFFu);
    std::int16_t ver = i16();
    return {end, ver};
  }
  void skip_frame() {
    auto [end, ver] = frame();
    (void)ver;
    if (end > n) throw ParseError("frame past end");
    i = end;
  }
};

struct Key {
  std::size_t offset = 0;
  std::int32_t nbytes = 0;
  std::int16_t kver = 0;
  std::int32_t objlen = 0;
  std::uint16_t keylen = 0;
  std::int64_t seekkey = 0;
  std::string cls, name, title;
  std::size_t after_strings = 0;
  bool big = false;
};

Key parse_key(const std::uint8_t* p, std::size_t n, std::size_t pos) {
  Cursor r{p, n, pos};
  Key k;
  k.offset = pos;
  k.nbytes = r.i32();
  k.kver = r.i16();
  k.objlen = r.i32();
  r.u32();  // datime
  k.keylen = r.u16();
  r.i16();  // cycle
  k.big = std::abs(k.kver) > 1000;
  k.seekkey = k.big ? r.i64() : r.i32();
  if (k.big) r.i64();
  else r.i32();
  k.cls = r.pstr();
  k.name = r.pstr();
  k.title = r.pstr();
  k.after_strings = r.i;
  if (k.nbytes < 0 || k.keylen > k.nbytes) throw ParseError("bad TKey header (keylen > nbytes)");
  return k;
}

// A key whose record `[offset, offset + nbytes)` must lie inside the buffer
// (as opposed to a KeysList entry, whose nbytes describes a record elsewhere).
Key parse_record_key(const std::uint8_t* p, std::size_t n, std::size_t pos) {
  Key k = parse_key(p, n, pos);
  if (static_cast<std::size_t>(k.nbytes) > n - k.offset) throw ParseError("TKey record past end of file");
  return k;
}

std::size_t payload_len(const Key& k) { return static_cast<std::size_t>(k.nbytes - k.keylen); }

constexpr std::size_t kMaxObjLen = std::size_t{1} << 30;
constexpr std::size_t kMaxExpansion = 64;

std::vector<std::uint8_t> decompress(const std::uint8_t* data, std::size_t n, std::int32_t objlen) {
  if (objlen <= 0) throw ParseError("bad objlen");
  const std::size_t want = static_cast<std::size_t>(objlen);
  if (want == n) {
    return {data, data + n};
  }
  if (want > kMaxObjLen || want > kMaxExpansion * n) throw ParseError("implausible objlen");
  std::vector<std::uint8_t> out;
  out.reserve(want);
  std::size_t i = 0;
  while (i < n && out.size() < want) {
    if (n - i < 9) throw ParseError("truncated compression header");
    char algo[3] = {static_cast<char>(data[i]), static_cast<char>(data[i + 1]), 0};
    std::uint32_t csz = static_cast<std::uint32_t>(data[i + 3]) |
                        (static_cast<std::uint32_t>(data[i + 4]) << 8) |
                        (static_cast<std::uint32_t>(data[i + 5]) << 16);
    std::uint32_t usz = static_cast<std::uint32_t>(data[i + 6]) |
                        (static_cast<std::uint32_t>(data[i + 7]) << 8) |
                        (static_cast<std::uint32_t>(data[i + 8]) << 16);
    i += 9;
    if (csz > n - i) throw ParseError("truncated compressed chunk");
    if (std::strcmp(algo, "ZL") != 0) {
      throw ParseError(std::string("unsupported ROOT compression ") + algo);
    }
    if (usz == 0 || usz > want - out.size()) throw ParseError("compressed chunk exceeds objlen");
    uLongf dest_len = usz;
    std::size_t at = out.size();
    out.resize(at + usz);
    int rc = uncompress(out.data() + at, &dest_len, data + i, csz);
    if (rc != Z_OK || dest_len != usz) throw ParseError("zlib decompress failed");
    i += csz;
  }
  if (out.size() != want) throw ParseError("decompressed size != objlen");
  return out;
}

constexpr int kMaxObjectDepth = 64;

struct Ctx {
  std::size_t origin = 0;
  int depth = 0;
  std::unordered_map<std::uint32_t, std::string> classes;
};

struct DepthGuard {
  int& depth;
  explicit DepthGuard(int& d) : depth(d) {
    if (depth >= kMaxObjectDepth) throw ParseError("object nesting too deep");
    ++depth;
  }
  ~DepthGuard() { --depth; }
  DepthGuard(const DepthGuard&) = delete;
  DepthGuard& operator=(const DepthGuard&) = delete;
};

struct Obj {
  std::string cls;
  bool is_null = false;
  bool skip = false;
  std::string name;
  std::string leaf_class;
  bool is_unsigned = false;
  bool has_count = false;
  std::int32_t len = 1;
  std::int32_t write_basket = 0;
  std::vector<Obj> sub;
  std::vector<Obj> leaves;
  std::vector<std::vector<std::uint8_t>> embedded;
  std::vector<std::int64_t> seeks;
};

void tobject(Cursor& r) {
  r.u16();
  r.u32();
  r.u32();
}

std::pair<std::string, std::string> tnamed(Cursor& r) {
  auto [end, ver] = r.frame();
  (void)ver;
  tobject(r);
  std::string name = r.pstr();
  std::string title = r.pstr();
  r.i = end;
  return {name, title};
}

Obj read_object_any(Cursor& r, Ctx& ctx);

std::vector<Obj> tobjarray(Cursor& r, Ctx& ctx) {
  auto [end, ver] = r.frame();
  (void)ver;
  tobject(r);
  r.pstr();
  std::int32_t n = r.i32();
  r.i32();
  if (n < 0 || n > 1000000) throw ParseError("implausible TObjArray length");
  // Every element costs at least a 4-byte tag word; do not pre-reserve on an
  // attacker-supplied count.
  if (static_cast<std::size_t>(n) > (r.n - std::min(r.i, r.n)) / 4) {
    throw ParseError("TObjArray length exceeds buffer");
  }
  std::vector<Obj> items;
  for (std::int32_t k = 0; k < n; ++k) items.push_back(read_object_any(r, ctx));
  r.i = end;
  return items;
}

Obj read_tleaf(Cursor& r, Ctx& ctx, const std::string& cls) {
  auto [end, ver] = r.frame();
  (void)ver;
  auto [lend, lver] = r.frame();
  (void)lend;
  (void)lver;
  auto [name, title] = tnamed(r);
  (void)title;
  std::int32_t fLen = r.i32();
  r.i32();  // fLenType
  r.i32();  // fOffset
  r.u8();   // fIsRange
  bool uns = r.u8() != 0;
  Obj count = read_object_any(r, ctx);
  r.i = end;
  Obj o;
  o.cls = cls;
  o.leaf_class = cls;
  o.name = name;
  o.is_unsigned = uns;
  o.len = fLen;
  o.has_count = !count.is_null;
  return o;
}

std::vector<std::uint8_t> read_tbasket_embedded(Cursor& r, std::size_t obj_end) {
  const std::size_t start = r.i;
  std::int32_t nbytes = r.i32();
  r.i16();  // kver
  r.i32();  // objlen
  r.u32();  // datime
  std::uint16_t keylen = r.u16();
  r.i16();  // cycle
  if (start + keylen < 19 || start + keylen > r.n) throw ParseError("bad embedded TBasket keylen");
  r.i = start + keylen - 19;
  r.u16();
  r.i32();
  std::int32_t nevbufsize = r.i32();
  std::int32_t nevbuf = r.i32();
  std::int32_t flast = r.i32();
  r.u8();
  if (nevbuf < 0 || nevbufsize < 0) throw ParseError("negative embedded TBasket count");
  if (flast < static_cast<std::int32_t>(keylen)) throw ParseError("embedded TBasket fLast < keylen");
  std::int32_t border = flast - static_cast<std::int32_t>(keylen);
  if (static_cast<std::int64_t>(nevbufsize) * nevbuf + keylen != flast) {
    std::size_t off_n = 8 + static_cast<std::size_t>(nevbuf) * 4;
    r.skip(off_n);
    if (r.i < 4) throw ParseError("embedded offset rewind");
    r.i -= 4;
  }
  r.skip(keylen);
  r.need(static_cast<std::size_t>(border));
  std::vector<std::uint8_t> data(r.p + r.i, r.p + r.i + static_cast<std::size_t>(border));
  r.i += static_cast<std::size_t>(border);
  (void)nbytes;
  (void)obj_end;
  return data;
}

Obj read_tbranch(Cursor& r, Ctx& ctx) {
  auto [end, ver] = r.frame();
  (void)ver;
  auto [name, title] = tnamed(r);
  (void)title;
  r.skip_frame();  // TAttFill
  r.i32();
  r.i32();
  r.i32();  // fEntryOffsetLen
  std::int32_t write_basket = r.i32();
  r.i64();  // fEntryNumber
  r.skip_frame();  // TIOFeatures
  r.i32();  // fOffset
  std::uint32_t max_baskets = r.u32();
  r.i32();  // fSplitLevel
  r.i64();  // fEntries
  r.i64();
  r.i64();
  r.i64();
  if (max_baskets > 100000) throw ParseError("implausible fMaxBaskets");
  auto sub = tobjarray(r, ctx);
  auto leaves = tobjarray(r, ctx);
  auto baskets = tobjarray(r, ctx);
  r.u8();
  for (std::uint32_t k = 0; k < max_baskets; ++k) r.i32();
  r.u8();
  for (std::uint32_t k = 0; k < max_baskets; ++k) r.i64();
  r.u8();
  std::vector<std::int64_t> seeks;
  seeks.reserve(max_baskets);
  for (std::uint32_t k = 0; k < max_baskets; ++k) seeks.push_back(r.i64());
  r.pstr();  // fFileName
  r.i = end;

  Obj o;
  o.cls = "TBranch";
  o.name = name;
  o.write_basket = write_basket;
  o.sub = std::move(sub);
  o.leaves = std::move(leaves);
  if (write_basket > 0) {
    std::size_t n = std::min(static_cast<std::size_t>(write_basket), seeks.size());
    o.seeks.assign(seeks.begin(), seeks.begin() + static_cast<std::ptrdiff_t>(n));
  } else {
    for (auto s : seeks)
      if (s > 0) o.seeks.push_back(s);
  }
  for (auto& b : baskets) {
    if (!b.embedded.empty()) o.embedded.insert(o.embedded.end(), b.embedded.begin(), b.embedded.end());
  }
  return o;
}

Obj read_object_any(Cursor& r, Ctx& ctx) {
  DepthGuard depth(ctx.depth);
  std::uint32_t word = r.u32();
  if (word == 0) {
    Obj n;
    n.is_null = true;
    return n;
  }
  std::size_t end = 0;
  bool has_end = false;
  std::uint32_t tag = 0;
  std::size_t startpos = 0;
  if (word & kByteCountMask) {
    end = r.i + (word & 0x3FFFFFFFu);
    has_end = true;
    startpos = ctx.origin + r.i;
    tag = r.u32();
  } else {
    tag = word;
    startpos = ctx.origin + (r.i - 4);
  }
  std::string cls;
  if (tag == kNewClassTag) {
    cls = r.cstring();
    ctx.classes[static_cast<std::uint32_t>(startpos + kMapOffset)] = cls;
  } else if (tag & kClassMask) {
    std::uint32_t idx = tag & ~kClassMask;
    auto it = ctx.classes.find(idx);
    if (it == ctx.classes.end()) {
      if (has_end) r.i = end;
      Obj s;
      s.skip = true;
      return s;
    }
    cls = it->second;
  } else {
    if (has_end) r.i = end;
    Obj s;
    s.skip = true;
    s.cls = "<ref>";
    return s;
  }

  Obj o;
  o.cls = cls;
  try {
    if (cls == "TBranch") {
      o = read_tbranch(r, ctx);
    } else if (cls == "TBranchElement") {
      o.skip = true;
    } else if (cls.size() >= 5 && cls.compare(0, 5, "TLeaf") == 0) {
      o = read_tleaf(r, ctx, cls);
    } else if (cls == "TBasket") {
      auto data = read_tbasket_embedded(r, has_end ? end : r.n);
      o.embedded.push_back(std::move(data));
    } else {
      o.skip = true;
    }
  } catch (const ParseError&) {
    o.skip = true;
  }
  if (has_end) r.i = end;
  return o;
}

void flatten_obj(const Obj& b, std::vector<BranchRec>& out) {
  if (b.is_null || (b.skip && b.name.empty())) return;
  bool has_sub = false;
  for (const auto& s : b.sub) {
    if (!s.is_null && !(s.skip && s.name.empty())) {
      has_sub = true;
      flatten_obj(s, out);
    }
  }
  if (has_sub) return;
  BranchRec rec;
  rec.name = b.name;
  rec.seeks = b.seeks;
  rec.embedded = b.embedded;
  const Obj* leaf = nullptr;
  for (const auto& l : b.leaves) {
    if (!l.leaf_class.empty()) {
      leaf = &l;
      break;
    }
  }
  if (leaf) {
    rec.leaf_class = leaf->leaf_class;
    rec.is_unsigned = leaf->is_unsigned;
    const bool arr = leaf->has_count || leaf->len > 1;
    const std::string& c = leaf->leaf_class;
    auto put = [&](const char* scalar, const char* array) { rec.type_name = arr ? array : scalar; };
    if (c == "TLeafF") put("float", "float[]");
    else if (c == "TLeafD") put("double", "double[]");
    else if (c == "TLeafO") put("bool", "bool[]");
    else if (c == "TLeafB") put(leaf->is_unsigned ? "uint8_t" : "int8_t", leaf->is_unsigned ? "uint8_t[]" : "int8_t[]");
    else if (c == "TLeafS") put(leaf->is_unsigned ? "uint16_t" : "int16_t", leaf->is_unsigned ? "uint16_t[]" : "int16_t[]");
    else if (c == "TLeafI") put(leaf->is_unsigned ? "uint32_t" : "int32_t", leaf->is_unsigned ? "uint32_t[]" : "int32_t[]");
    else if (c == "TLeafL") put(leaf->is_unsigned ? "uint64_t" : "int64_t", leaf->is_unsigned ? "uint64_t[]" : "int64_t[]");
    else rec.type_name = c;
  }
  out.push_back(std::move(rec));
}

Tree read_ttree(const std::vector<std::uint8_t>& raw, std::size_t keylen) {
  Cursor r{raw.data(), raw.size(), 0};
  Ctx ctx;
  ctx.origin = keylen;
  auto [end, ver] = r.frame();
  (void)end;
  if (ver < 19) throw ParseError("TTree version < 19");
  tnamed(r);
  r.skip_frame();
  r.skip_frame();
  r.skip_frame();
  std::int64_t entries = r.i64();
  r.i64();
  r.i64();
  r.i64();
  r.i64();
  r.u64();  // fWeight as bits; skip 8
  r.i32();
  r.i32();
  r.i32();
  r.i32();
  std::uint32_t ncluster = r.u32();
  r.i64();
  r.i64();
  r.i64();
  r.i64();
  r.i64();
  r.i64();
  r.u8();
  r.skip(8ull * ncluster);
  r.u8();
  r.skip(8ull * ncluster);
  r.skip_frame();  // TIOFeatures
  auto branches = tobjarray(r, ctx);
  Tree t;
  t.entries = entries;
  for (const auto& b : branches) flatten_obj(b, t.branches);
  for (std::size_t i = 0; i < t.branches.size(); ++i) {
    if (!t.branches[i].name.empty()) t.index[t.branches[i].name] = i;
  }
  return t;
}

std::int64_t dir_seek_keys(const std::vector<std::uint8_t>& b) {
  if (b.size() < 16 || std::memcmp(b.data(), "root", 4) != 0) {
    throw ParseError("not a ROOT file");
  }
  std::int32_t fver = 0;
  std::memcpy(&fver, b.data() + 4, 4);
  // file version is big-endian
  fver = static_cast<std::int32_t>(__builtin_bswap32(static_cast<std::uint32_t>(fver)));
  std::int64_t begin = 0;
  if (fver >= 1000000) {
    Cursor c{b.data(), b.size(), 8};
    begin = c.i64();
  } else {
    Cursor c{b.data(), b.size(), 8};
    begin = c.i32();
  }
  if (begin < 0) throw ParseError("negative fBEGIN");
  Key k = parse_record_key(b.data(), b.size(), static_cast<std::size_t>(begin));
  Cursor r{b.data() + k.offset + k.keylen, payload_len(k), 0};
  r.pstr();
  r.pstr();
  std::int16_t dv = r.i16();
  r.u32();
  r.u32();
  r.u32();
  r.u32();
  if (dv > 1000) {
    r.i64();
    r.i64();
    return r.i64();
  }
  r.u32();
  r.u32();
  return r.u32();
}

std::vector<std::uint8_t> basket_at_seek(const std::vector<std::uint8_t>& file, std::int64_t seek) {
  if (seek <= 0) return {};
  Key k = parse_record_key(file.data(), file.size(), static_cast<std::size_t>(seek));
  const std::uint8_t* payload = file.data() + k.offset + k.keylen;
  auto raw = decompress(payload, payload_len(k), k.objlen);
  Cursor extra{file.data(), file.size(), k.after_strings};
  extra.u16();
  extra.i32();
  extra.i32();
  extra.i32();
  std::int32_t flast = extra.i32();
  extra.u8();
  std::int32_t border = flast - k.keylen;
  if (border < 0) border = 0;
  if (static_cast<std::size_t>(border) > raw.size()) border = static_cast<std::int32_t>(raw.size());
  return {raw.begin(), raw.begin() + border};
}

enum class WidthKind { F32, F64, I8, U8, I16, U16, I32, U32, I64, U64 };

WidthKind width_of(const BranchRec& br) {
  const auto& c = br.leaf_class;
  if (c == "TLeafF") return WidthKind::F32;
  if (c == "TLeafD") return WidthKind::F64;
  if (c == "TLeafO") return WidthKind::U8;
  if (c == "TLeafB") return br.is_unsigned ? WidthKind::U8 : WidthKind::I8;
  if (c == "TLeafS") return br.is_unsigned ? WidthKind::U16 : WidthKind::I16;
  if (c == "TLeafI") return br.is_unsigned ? WidthKind::U32 : WidthKind::I32;
  if (c == "TLeafL") return br.is_unsigned ? WidthKind::U64 : WidthKind::I64;
  throw ParseError("unknown TLeaf class " + c);
}

std::size_t width_bytes(WidthKind k) {
  switch (k) {
    case WidthKind::F32:
    case WidthKind::I32:
    case WidthKind::U32: return 4;
    case WidthKind::F64:
    case WidthKind::I64:
    case WidthKind::U64: return 8;
    case WidthKind::I16:
    case WidthKind::U16: return 2;
    default: return 1;
  }
}

void decode_basket(const std::vector<std::uint8_t>& data, WidthKind wk, std::vector<double>* f64s,
                   std::vector<std::int64_t>* i64s) {
  const std::size_t w = width_bytes(wk);
  const std::size_t n = data.size() / w;
  Cursor r{data.data(), data.size(), 0};
  for (std::size_t i = 0; i < n; ++i) {
    switch (wk) {
      case WidthKind::F32: {
        std::uint32_t u = r.u32();
        float v;
        std::memcpy(&v, &u, 4);
        f64s->push_back(static_cast<double>(v));
        break;
      }
      case WidthKind::F64: {
        std::uint64_t u = r.u64();
        double v;
        std::memcpy(&v, &u, 8);
        f64s->push_back(v);
        break;
      }
      case WidthKind::I8: i64s->push_back(static_cast<std::int8_t>(r.u8())); break;
      case WidthKind::U8: i64s->push_back(r.u8()); break;
      case WidthKind::I16: i64s->push_back(r.i16()); break;
      case WidthKind::U16: i64s->push_back(r.u16()); break;
      case WidthKind::I32: i64s->push_back(r.i32()); break;
      case WidthKind::U32: i64s->push_back(r.u32()); break;
      case WidthKind::I64: i64s->push_back(r.i64()); break;
      case WidthKind::U64: i64s->push_back(static_cast<std::int64_t>(r.u64())); break;
    }
  }
}

void collect_payloads(const std::vector<std::uint8_t>& file, const BranchRec& br,
                      std::vector<std::vector<std::uint8_t>>& payloads) {
  bool any_seek = false;
  for (auto s : br.seeks)
    if (s > 0) any_seek = true;
  if (any_seek) {
    for (auto s : br.seeks) {
      if (s <= 0) continue;
      payloads.push_back(basket_at_seek(file, s));
    }
  } else {
    payloads = br.embedded;
  }
}

}  // namespace

bool flatten_f64(const std::vector<std::uint8_t>& file, const BranchRec& br, std::vector<double>& out,
                 std::string& err) {
  try {
    auto wk = width_of(br);
    if (wk != WidthKind::F32 && wk != WidthKind::F64) {
      err = "not a float leaf";
      return false;
    }
    std::vector<std::vector<std::uint8_t>> payloads;
    collect_payloads(file, br, payloads);
    for (const auto& p : payloads) decode_basket(p, wk, &out, nullptr);
    return true;
  } catch (const std::exception& e) {
    err = e.what();
    return false;
  }
}

bool flatten_i64(const std::vector<std::uint8_t>& file, const BranchRec& br, std::vector<std::int64_t>& out,
                 std::string& err) {
  try {
    auto wk = width_of(br);
    if (wk == WidthKind::F32 || wk == WidthKind::F64) {
      err = "not an integer leaf";
      return false;
    }
    std::vector<std::vector<std::uint8_t>> payloads;
    collect_payloads(file, br, payloads);
    for (const auto& p : payloads) decode_basket(p, wk, nullptr, &out);
    return true;
  } catch (const std::exception& e) {
    err = e.what();
    return false;
  }
}

std::optional<LoadedFile> load_root(const std::string& path, const std::string& tree_name, LoadError& err) {
  try {
    std::ifstream in(path, std::ios::binary);
    if (!in) {
      err = {LoadError::Open, "cannot open file"};
      return std::nullopt;
    }
    LoadedFile lf;
    lf.bytes.assign(std::istreambuf_iterator<char>(in), std::istreambuf_iterator<char>());
    if (lf.bytes.size() < 16 || std::memcmp(lf.bytes.data(), "root", 4) != 0) {
      err = {LoadError::Open, "not a ROOT file"};
      return std::nullopt;
    }
    std::int64_t seek_keys = dir_seek_keys(lf.bytes);
    if (seek_keys < 0) throw ParseError("negative fSeekKeys");
    Key kk = parse_record_key(lf.bytes.data(), lf.bytes.size(), static_cast<std::size_t>(seek_keys));
    const std::uint8_t* kd = lf.bytes.data() + kk.offset + kk.keylen;
    std::size_t kd_n = payload_len(kk);
    Cursor kr{kd, kd_n, 0};
    std::int32_t nkeys = kr.i32();
    if (nkeys < 0) throw ParseError("negative key count");
    // A KeysList entry is at least the 18-byte fixed header plus 3 pstrings.
    if (static_cast<std::size_t>(nkeys) > kd_n / 21) throw ParseError("key count exceeds KeysList");
    Key tree_key{};
    bool found = false;
    Key first_tree{};
    bool any_tree = false;
    for (std::int32_t i = 0; i < nkeys; ++i) {
      Key ck = parse_key(kd, kd_n, kr.i);
      if (ck.offset + ck.keylen < ck.after_strings) throw ParseError("KeysList entry keylen too short");
      kr.i = ck.offset + ck.keylen;
      if (ck.seekkey < 0) throw ParseError("negative fSeekKey");
      Key rec = parse_record_key(lf.bytes.data(), lf.bytes.size(), static_cast<std::size_t>(ck.seekkey));
      if (rec.cls == "TTree") {
        if (!any_tree) {
          first_tree = rec;
          any_tree = true;
        }
        if (rec.name == tree_name) {
          tree_key = rec;
          found = true;
          break;
        }
      }
    }
    if (!found) {
      if (any_tree && tree_name.empty()) {
        tree_key = first_tree;
        found = true;
      }
    }
    if (!found) {
      err = {LoadError::Tree, "TTree `" + tree_name + "` not found"};
      return std::nullopt;
    }
    // tree_key came from parse_record_key at its own seekkey, so
    // [offset, offset + nbytes) is inside the file.
    const std::uint8_t* payload = lf.bytes.data() + tree_key.offset + tree_key.keylen;
    auto raw = decompress(payload, payload_len(tree_key), tree_key.objlen);
    lf.tree = read_ttree(raw, tree_key.keylen);
    return lf;
  } catch (const ParseError& e) {
    err = {LoadError::Tree, e.what()};
    return std::nullopt;
  } catch (const std::exception& e) {
    err = {LoadError::Open, e.what()};
    return std::nullopt;
  }
}

std::optional<Tree> load_tree(const std::string& path, const std::string& tree_name, LoadError& err) {
  auto lf = load_root(path, tree_name, err);
  if (!lf) return std::nullopt;
  return lf->tree;
}

}  // namespace adl2::ingest::detail
