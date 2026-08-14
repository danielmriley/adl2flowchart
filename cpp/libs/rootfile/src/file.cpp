#include "dir.hpp"

#include "blobs.hpp"
#include "th1d.hpp"
#include "wbuf.hpp"

#include "adl2/rootfile/rootfile.hpp"

#include <algorithm>
#include <cstdint>
#include <limits>
#include <string>
#include <vector>

namespace adl2::rootfile::detail {
namespace {

constexpr std::uint32_t kStartBigFile = 2000000000u;
constexpr std::int32_t kHeaderVersion = 62400;
constexpr std::uint32_t kFBegin = 100;
constexpr std::int32_t kFCompress = 100;
constexpr std::int16_t kKeyVersion = 4;
constexpr std::size_t kKeyFixed = 26;
constexpr std::size_t kDirHeaderLen = 60;
constexpr std::uint32_t kStreamerInfoTh1dEntries = 14;
constexpr std::size_t kSiHeaderLen = 21;
const char* kSiClass = "TList";
const char* kSiName = "StreamerInfo";
const char* kSiTitle = "Doubly linked list";

void write_key(WBuf& w, std::uint32_t nbytes, std::uint32_t objlen, std::uint32_t datime,
               std::uint16_t klen, std::uint32_t seekkey, std::uint32_t seekpdir, const std::string& cls,
               const std::string& name, const std::string& title) {
  w.u32(nbytes);
  w.i16(kKeyVersion);
  w.u32(objlen);
  w.u32(datime);
  w.u16(klen);
  w.i16(1);
  w.u32(seekkey);
  w.u32(seekpdir);
  w.pstring(cls);
  w.pstring(name);
  w.pstring(title);
}

std::vector<std::uint8_t> streamerinfo_data(bool has_th2, bool has_labels) {
  if (!has_th2 && !has_labels) {
    return std::vector<std::uint8_t>(kStreamerInfoTh1d, kStreamerInfoTh1d + kStreamerInfoTh1dLen);
  }
  std::uint32_t entries = kStreamerInfoTh1dEntries;
  std::vector<std::uint8_t> blobs(kStreamerInfoTh1d + kSiHeaderLen,
                                  kStreamerInfoTh1d + kStreamerInfoTh1dLen);
  if (has_th2) {
    blobs.insert(blobs.end(), kRawstreamerTh2V5, kRawstreamerTh2V5 + kRawstreamerTh2V5Len);
    blobs.insert(blobs.end(), kRawstreamerTh2dV4, kRawstreamerTh2dV4 + kRawstreamerTh2dV4Len);
    entries += 2;
  }
  if (has_labels) {
    blobs.insert(blobs.end(), kRawstreamerTobjstringV1,
                 kRawstreamerTobjstringV1 + kRawstreamerTobjstringV1Len);
    entries += 1;
  }
  WBuf w;
  const std::size_t data_bytes = kSiHeaderLen + blobs.size();
  w.u32(static_cast<std::uint32_t>(data_bytes - 4) | kByteCountMask);
  w.u16(5);
  w.u16(1);
  w.u32(0);
  w.u32(0x02000000u);
  w.u8(0);
  w.u32(entries);
  w.raw(blobs);
  return w.bytes;
}

struct ObjRec {
  std::size_t off = 0;
  std::size_t klen = 0;
  std::vector<std::uint8_t> payload;
  const char* cls = "";
  std::string name;
  std::string title;
  std::size_t dir_loc = 0;
};

struct DirRec {
  std::size_t loc = 0;
  std::size_t parent_loc = 0;
  std::size_t klen = 0;
  std::string name;
  std::size_t objs_begin = 0;
  std::size_t objs_end = 0;
  std::vector<std::size_t> child_locs;
  std::size_t keys_off = 0;
  std::size_t keys_klen = 0;
  std::size_t keys_objlen = 0;
};

void layout_dir(const Dir& dir, std::size_t loc, std::size_t parent_loc, std::size_t& off,
                std::vector<ObjRec>& objs, std::vector<DirRec>& dirs) {
  const std::size_t me = dirs.size();
  DirRec rec;
  rec.loc = loc;
  rec.parent_loc = parent_loc;
  rec.klen = (loc == kFBegin) ? 0 : keylen("TDirectory", dir.name, dir.name);
  rec.name = dir.name;
  dirs.push_back(rec);

  const std::size_t first_obj = objs.size();
  for (const auto& o : dir.objects) {
    ObjRec orc;
    orc.payload = o.payload();
    orc.klen = keylen(o.class_name(), o.obj_name(), o.obj_title());
    orc.off = off;
    orc.cls = o.class_name();
    orc.name = o.obj_name();
    orc.title = o.obj_title();
    orc.dir_loc = loc;
    off += orc.klen + orc.payload.size();
    objs.push_back(std::move(orc));
  }
  dirs[me].objs_begin = first_obj;
  dirs[me].objs_end = objs.size();

  for (const auto& sub : dir.subdirs) {
    const std::size_t sub_loc = off;
    dirs[me].child_locs.push_back(sub_loc);
    off += keylen("TDirectory", sub.name, sub.name) + kDirHeaderLen;
    layout_dir(sub, sub_loc, loc, off, objs, dirs);
  }
}

const DirRec* dir_at(const std::vector<DirRec>& dirs, std::size_t loc) {
  for (const auto& d : dirs)
    if (d.loc == loc) return &d;
  return nullptr;
}

void dir_header(WBuf& w, std::uint32_t datime, std::uint32_t nbytes_keys, std::uint32_t nbytes_name,
                std::uint32_t seek_dir, std::uint32_t seek_parent, std::uint32_t seek_keys,
                const std::uint8_t* uuid) {
  w.i16(5);
  w.u32(datime);
  w.u32(datime);
  w.u32(nbytes_keys);
  w.u32(nbytes_name);
  w.u32(seek_dir);
  w.u32(seek_parent);
  w.u32(seek_keys);
  w.bytes.push_back(0x00);
  w.bytes.push_back(0x01);
  w.raw(uuid, 16);
  for (int i = 0; i < 12; ++i) w.u8(0);
}

void obj_key(WBuf& w, const ObjRec& o, std::uint32_t datime) {
  write_key(w, static_cast<std::uint32_t>(o.klen + o.payload.size()),
            static_cast<std::uint32_t>(o.payload.size()), datime, static_cast<std::uint16_t>(o.klen),
            static_cast<std::uint32_t>(o.off), static_cast<std::uint32_t>(o.dir_loc), o.cls, o.name,
            o.title);
}

void emit_object(WBuf& w, const ObjRec& o, std::uint32_t datime) {
  obj_key(w, o, datime);
  w.raw(o.payload);
}

void subdir_key(WBuf& w, const DirRec& d, std::uint32_t datime) {
  write_key(w, static_cast<std::uint32_t>(d.klen + kDirHeaderLen),
            static_cast<std::uint32_t>(kDirHeaderLen), datime, static_cast<std::uint16_t>(d.klen),
            static_cast<std::uint32_t>(d.loc), static_cast<std::uint32_t>(d.parent_loc), "TDirectory",
            d.name, d.name);
}

void emit_subdir(WBuf& w, const DirRec& d, std::uint32_t datime, const std::uint8_t* uuid) {
  subdir_key(w, d, datime);
  dir_header(w, datime, static_cast<std::uint32_t>(d.keys_klen + d.keys_objlen),
             static_cast<std::uint32_t>(d.klen), static_cast<std::uint32_t>(d.loc),
             static_cast<std::uint32_t>(d.parent_loc), static_cast<std::uint32_t>(d.keys_off), uuid);
}

}  // namespace

std::vector<std::uint8_t> ObjPayload::payload() const {
  if (kind == H1) return h1.payload();
  if (kind == H2) return h2.payload();
  WBuf w;
  tnamed(w, named_name, named_title, kFBits);
  return w.bytes;
}

std::size_t keylen(const std::string& cls, const std::string& name, const std::string& title) {
  return kKeyFixed + WBuf::pstring_len(cls) + WBuf::pstring_len(name) + WBuf::pstring_len(title);
}

bool build_file(const std::string& file_name, const Dir& root, std::uint32_t datime,
                const std::uint8_t* uuid_header, const std::uint8_t* uuid_dir,
                std::vector<std::uint8_t>& out, std::string* err) {
  const std::size_t name_keylen = keylen("TFile", file_name, "");
  if (name_keylen > std::numeric_limits<std::uint16_t>::max()) {
    if (err) *err = "not a writable file path: " + file_name;
    return false;
  }
  const std::size_t name_strings = WBuf::pstring_len(file_name) + WBuf::pstring_len("");
  const std::size_t nbytes_name = name_keylen + name_strings;
  const std::size_t name_objlen = name_strings + kDirHeaderLen;
  const std::size_t name_nbytes = name_keylen + name_objlen;

  std::vector<ObjRec> objs;
  std::vector<DirRec> dirs;
  std::size_t off = kFBegin + name_nbytes;
  layout_dir(root, kFBegin, kFBegin, off, objs, dirs);

  const bool has_th2 = root.any_object([](const ObjPayload& o) { return o.kind == ObjPayload::H2; });
  const bool has_labels = root.any_object(
      [](const ObjPayload& o) { return o.kind == ObjPayload::H1 && o.h1.labels.has_value(); });
  const auto si_data = streamerinfo_data(has_th2, has_labels);
  const std::size_t si_off = off;
  const std::size_t si_keylen = keylen(kSiClass, kSiName, kSiTitle);
  const std::size_t si_nbytes = si_keylen + si_data.size();
  off += si_nbytes;

  for (std::size_t i = 0; i < dirs.size(); ++i) {
    const std::size_t klen = (dirs[i].loc == kFBegin) ? keylen("TFile", file_name, "")
                                                      : keylen("TDirectory", dirs[i].name, dirs[i].name);
    std::size_t objlen = 4;
    for (std::size_t j = dirs[i].objs_begin; j < dirs[i].objs_end; ++j) objlen += objs[j].klen;
    for (std::size_t loc : dirs[i].child_locs) {
      const DirRec* c = dir_at(dirs, loc);
      objlen += c ? c->klen : 0;
    }
    dirs[i].keys_off = off;
    dirs[i].keys_klen = klen;
    dirs[i].keys_objlen = objlen;
    off += klen + objlen;
  }

  const std::size_t free_off = off;
  const std::size_t free_keylen = keylen("TFile", file_name, "");
  const std::size_t free_nbytes = free_keylen + 10;
  const std::size_t fend = free_off + free_nbytes;
  if (fend >= kStartBigFile) {
    if (err) *err = "file would be " + std::to_string(fend) + " bytes; small-format cap is 2 GB";
    return false;
  }

  WBuf w;
  w.bytes.reserve(fend);
  w.raw(reinterpret_cast<const std::uint8_t*>("root"), 4);
  w.i32(kHeaderVersion);
  w.u32(kFBegin);
  w.u32(static_cast<std::uint32_t>(fend));
  w.u32(static_cast<std::uint32_t>(free_off));
  w.u32(static_cast<std::uint32_t>(free_nbytes));
  w.u32(1);
  w.u32(static_cast<std::uint32_t>(nbytes_name));
  w.u8(4);
  w.i32(kFCompress);
  w.u32(static_cast<std::uint32_t>(si_off));
  w.u32(static_cast<std::uint32_t>(si_nbytes));
  w.bytes.push_back(0x00);
  w.bytes.push_back(0x01);
  w.raw(uuid_header, 16);
  while (w.bytes.size() < 100) w.u8(0);

  const DirRec& root_rec = dirs[0];
  write_key(w, static_cast<std::uint32_t>(name_nbytes), static_cast<std::uint32_t>(name_objlen), datime,
            static_cast<std::uint16_t>(name_keylen), kFBegin, 0, "TFile", file_name, "");
  w.pstring(file_name);
  w.pstring("");
  dir_header(w, datime, static_cast<std::uint32_t>(root_rec.keys_klen + root_rec.keys_objlen),
             static_cast<std::uint32_t>(nbytes_name), kFBegin, 0,
             static_cast<std::uint32_t>(root_rec.keys_off), uuid_dir);

  std::size_t oi = 0;
  std::size_t di = 1;
  while (oi < objs.size() || di < dirs.size()) {
    const bool have_o = oi < objs.size();
    const bool have_d = di < dirs.size();
    if (have_o && have_d && objs[oi].off < dirs[di].loc) {
      emit_object(w, objs[oi++], datime);
    } else if (have_d && (!have_o || objs[oi].off >= dirs[di].loc)) {
      emit_subdir(w, dirs[di++], datime, uuid_dir);
    } else if (have_o) {
      emit_object(w, objs[oi++], datime);
    } else {
      emit_subdir(w, dirs[di++], datime, uuid_dir);
    }
  }

  write_key(w, static_cast<std::uint32_t>(si_nbytes), static_cast<std::uint32_t>(si_data.size()), datime,
            static_cast<std::uint16_t>(si_keylen), static_cast<std::uint32_t>(si_off), kFBegin, kSiClass,
            kSiName, kSiTitle);
  w.raw(si_data);

  for (const auto& d : dirs) {
    const std::uint32_t nbytes = static_cast<std::uint32_t>(d.keys_klen + d.keys_objlen);
    if (d.loc == kFBegin) {
      write_key(w, nbytes, static_cast<std::uint32_t>(d.keys_objlen), datime,
                static_cast<std::uint16_t>(d.keys_klen), static_cast<std::uint32_t>(d.keys_off), kFBegin,
                "TFile", file_name, "");
    } else {
      write_key(w, nbytes, static_cast<std::uint32_t>(d.keys_objlen), datime,
                static_cast<std::uint16_t>(d.keys_klen), static_cast<std::uint32_t>(d.keys_off),
                static_cast<std::uint32_t>(d.parent_loc), "TDirectory", d.name, d.name);
    }
    const std::uint32_t n_children =
        static_cast<std::uint32_t>((d.objs_end - d.objs_begin) + d.child_locs.size());
    w.u32(n_children);
    std::size_t coi = d.objs_begin;
    std::size_t cdi = 0;
    while (coi < d.objs_end || cdi < d.child_locs.size()) {
      const bool have_o = coi < d.objs_end;
      const DirRec* cd = (cdi < d.child_locs.size()) ? dir_at(dirs, d.child_locs[cdi]) : nullptr;
      const bool have_d = cd != nullptr;
      if (have_o && have_d && objs[coi].off < cd->loc) {
        obj_key(w, objs[coi++], datime);
      } else if (have_d && (!have_o || objs[coi].off >= cd->loc)) {
        subdir_key(w, *cd, datime);
        ++cdi;
      } else if (have_o) {
        obj_key(w, objs[coi++], datime);
      } else {
        subdir_key(w, *cd, datime);
        ++cdi;
      }
    }
  }

  write_key(w, static_cast<std::uint32_t>(free_nbytes), 10, datime,
            static_cast<std::uint16_t>(free_keylen), static_cast<std::uint32_t>(free_off), kFBegin,
            "TFile", file_name, "");
  w.u16(1);
  w.u32(static_cast<std::uint32_t>(fend));
  w.u32(kStartBigFile);

  out = std::move(w.bytes);
  return true;
}

}  // namespace adl2::rootfile::detail
