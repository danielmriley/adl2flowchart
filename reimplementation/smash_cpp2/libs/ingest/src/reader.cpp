#include "adl2/ingest/ingest.hpp"

#include "jnum.hpp"
#include "ttree.hpp"

#include <cmath>
#include <map>
#include <set>
#include <sstream>

namespace adl2::ingest {
namespace {

using detail::BranchRec;
using detail::LoadedFile;

IngestError mk_open(std::string path, std::string message) {
  IngestError e;
  e.kind = IngestErrorKind::Open;
  e.path = std::move(path);
  e.message = std::move(message);
  return e;
}
IngestError mk_tree(std::string name, std::string message) {
  IngestError e;
  e.kind = IngestErrorKind::Tree;
  e.name = std::move(name);
  e.message = std::move(message);
  return e;
}

enum class FlatTag { F64, Int };
struct FlatColumn {
  FlatTag tag = FlatTag::F64;
  std::vector<double> f64s;
  std::vector<std::int64_t> ints;
  std::size_t size() const { return tag == FlatTag::F64 ? f64s.size() : ints.size(); }
};

struct LoadedCollection {
  const CollectionSpec* spec = nullptr;
  std::vector<std::size_t> counts;
  std::vector<FlatColumn> columns;
};

std::set<std::string> leaf_names_of(const detail::Tree& tree) {
  std::set<std::string> s;
  for (const auto& b : tree.branches)
    if (!b.name.empty()) s.insert(b.name);
  return s;
}

const BranchRec* require_branch(const LoadedFile& lf, const std::string& name, const std::string& needed,
                                IngestError& err) {
  const BranchRec* b = lf.tree.find(name);
  if (!b) {
    err = IngestError::missing_branch(name, needed);
    return nullptr;
  }
  return b;
}

bool type_ok_counter(const std::string& t) { return t == "int32_t" || t == "uint32_t"; }

bool type_ok_f32(const std::string& t, bool arr) {
  if (arr) return t == "float[]" || t == "double[]";
  return t == "float" || t == "double";
}

bool type_ok_i32(const std::string& t) {
  return t == "int8_t[]" || t == "char[]" || t == "uint8_t[]" || t == "int16_t[]" || t == "uint16_t[]" ||
         t == "int32_t[]" || t == "uint32_t[]" || t == "int8_t" || t == "uint8_t" || t == "int16_t" ||
         t == "uint16_t" || t == "int32_t" || t == "uint32_t";
}

std::optional<std::vector<std::size_t>> read_counter(const LoadedFile& lf, const std::string& name,
                                                     std::size_t entries, IngestError& err) {
  const BranchRec* b = require_branch(lf, name, "collection chunking (counter-authoritative re-chunk)", err);
  if (!b) return std::nullopt;
  if (!type_ok_counter(b->type_name)) {
    err = IngestError::type_mismatch(name, "int32_t|uint32_t", b->type_name);
    return std::nullopt;
  }
  std::vector<std::int64_t> raw;
  std::string msg;
  if (!detail::flatten_i64(lf.bytes, *b, raw, msg)) {
    err = IngestError::tree(name, msg);
    return std::nullopt;
  }
  if (raw.size() != entries) {
    err = IngestError::entry_count(name, entries, raw.size());
    return std::nullopt;
  }
  std::vector<std::size_t> out;
  out.reserve(raw.size());
  for (std::size_t i = 0; i < raw.size(); ++i) {
    if (raw[i] < 0) {
      err = IngestError::negative_count(name, i);
      return std::nullopt;
    }
    out.push_back(static_cast<std::size_t>(raw[i]));
  }
  return out;
}

std::optional<FlatColumn> read_leaf_flat(const LoadedFile& lf, const std::string& branch, LeafKind kind,
                                         std::uint32_t tag_bit, std::size_t total, IngestError& err) {
  (void)tag_bit;
  const BranchRec* b = require_branch(lf, branch, "a profile-mapped property", err);
  if (!b) return std::nullopt;
  const std::string& got = b->type_name;
  FlatColumn col;
  std::string msg;
  if (kind == LeafKind::F32) {
    if (!type_ok_f32(got, true)) {
      err = IngestError::type_mismatch(branch, "float[]|double[]", got);
      return std::nullopt;
    }
    col.tag = FlatTag::F64;
    if (!detail::flatten_f64(lf.bytes, *b, col.f64s, msg)) {
      err = IngestError::tree(branch, msg);
      return std::nullopt;
    }
  } else if (kind == LeafKind::Bool) {
    if (got != "bool[]") {
      err = IngestError::type_mismatch(branch, "bool[]", got);
      return std::nullopt;
    }
    col.tag = FlatTag::Int;
    if (!detail::flatten_i64(lf.bytes, *b, col.ints, msg)) {
      err = IngestError::tree(branch, msg);
      return std::nullopt;
    }
  } else if (kind == LeafKind::TagBit) {
    if (got != "uint32_t[]") {
      err = IngestError::type_mismatch(branch, "uint32_t[]", got);
      return std::nullopt;
    }
    col.tag = FlatTag::Int;
    if (!detail::flatten_i64(lf.bytes, *b, col.ints, msg)) {
      err = IngestError::tree(branch, msg);
      return std::nullopt;
    }
  } else {
    if (!type_ok_i32(got) || got.find('[') == std::string::npos) {
      err = IngestError::type_mismatch(
          branch, "int8_t[]|uint8_t[]|int16_t[]|uint16_t[]|int32_t[]|uint32_t[]", got);
      return std::nullopt;
    }
    col.tag = FlatTag::Int;
    if (!detail::flatten_i64(lf.bytes, *b, col.ints, msg)) {
      err = IngestError::tree(branch, msg);
      return std::nullopt;
    }
  }
  if (col.size() != total) {
    err = IngestError::length_mismatch(branch, total, col.size());
    return std::nullopt;
  }
  return col;
}

std::size_t entry_of(const std::vector<std::size_t>& counts, std::size_t pos) {
  std::size_t acc = 0;
  for (std::size_t entry = 0; entry < counts.size(); ++entry) {
    acc += counts[entry];
    if (pos < acc) return entry;
  }
  return counts.empty() ? 0 : counts.size() - 1;
}

std::optional<LoadedCollection> load_collection(const LoadedFile& lf, const Profile& profile,
                                                const CollectionSpec& spec, std::size_t entries,
                                                const std::set<std::string>& leaf_names,
                                                std::vector<IngestDiag>& diags, IngestError& err) {
  const std::string counter = profile.counter_branch(spec.branch);
  bool present = leaf_names.count(counter) != 0;
  if (!present) {
    for (const auto& l : spec.leaves) {
      if (leaf_names.count(profile.leaf_branch(spec.branch, l.leaf))) {
        present = true;
        break;
      }
    }
  }
  if (!present) return LoadedCollection{};  // empty spec pointer means absent — caller checks

  LoadedCollection lc;
  lc.spec = &spec;
  auto counts = read_counter(lf, counter, entries, err);
  if (!counts) return std::nullopt;
  lc.counts = std::move(*counts);
  std::size_t total = 0;
  for (auto c : lc.counts) total += c;
  for (const auto& leaf : spec.leaves) {
    const std::string branch = profile.leaf_branch(spec.branch, leaf.leaf);
    auto col = read_leaf_flat(lf, branch, leaf.kind, leaf.tag_bit, total, err);
    if (!col) return std::nullopt;
    if (col->tag == FlatTag::F64) {
      for (std::size_t i = 0; i < col->f64s.size(); ++i) {
        if (!std::isfinite(col->f64s[i])) {
          err = IngestError::non_finite(branch, entry_of(lc.counts, i));
          return std::nullopt;
        }
      }
    } else if (leaf.kind == LeafKind::TagBit) {
      const std::int64_t mask = static_cast<std::int64_t>(1) << leaf.tag_bit;
      std::uint64_t ignored = 0;
      for (auto v : col->ints)
        if ((v & ~mask) != 0) ++ignored;
      if (ignored > 0) {
        IngestDiag d;
        d.kind = IngestDiagKind::TagBitsIgnored;
        d.branch = branch;
        d.bit = leaf.tag_bit;
        d.values = ignored;
        diags.push_back(std::move(d));
      }
      for (auto& v : col->ints) v = (v >> leaf.tag_bit) & 1;
    }
    lc.columns.push_back(std::move(*col));
  }
  return lc;
}

}  // namespace

IngestError IngestError::open(std::string path, std::string message) { return mk_open(std::move(path), std::move(message)); }
IngestError IngestError::tree(std::string name, std::string message) { return mk_tree(std::move(name), std::move(message)); }
IngestError IngestError::missing_branch(std::string name, std::string needed_for) {
  IngestError e;
  e.kind = IngestErrorKind::MissingBranch;
  e.name = std::move(name);
  e.needed_for = std::move(needed_for);
  return e;
}
IngestError IngestError::type_mismatch(std::string branch, std::string expected, std::string got) {
  IngestError e;
  e.kind = IngestErrorKind::TypeMismatch;
  e.branch = std::move(branch);
  e.expected = std::move(expected);
  e.got = std::move(got);
  return e;
}
IngestError IngestError::entry_count(std::string branch, std::size_t expected, std::size_t got) {
  IngestError e;
  e.kind = IngestErrorKind::EntryCount;
  e.branch = std::move(branch);
  e.expected_n = expected;
  e.got_n = got;
  return e;
}
IngestError IngestError::negative_count(std::string branch, std::size_t entry) {
  IngestError e;
  e.kind = IngestErrorKind::NegativeCount;
  e.branch = std::move(branch);
  e.entry = entry;
  return e;
}
IngestError IngestError::length_mismatch(std::string branch, std::size_t expected, std::size_t got) {
  IngestError e;
  e.kind = IngestErrorKind::LengthMismatch;
  e.branch = std::move(branch);
  e.expected_n = expected;
  e.got_n = got;
  return e;
}
IngestError IngestError::non_finite(std::string branch, std::size_t entry) {
  IngestError e;
  e.kind = IngestErrorKind::NonFinite;
  e.branch = std::move(branch);
  e.entry = entry;
  return e;
}
IngestError IngestError::not_pt_descending(std::string collection, std::size_t entry, std::size_t index) {
  IngestError e;
  e.kind = IngestErrorKind::NotPtDescending;
  e.collection = std::move(collection);
  e.entry = entry;
  e.index = index;
  return e;
}

std::string IngestError::to_string() const {
  switch (kind) {
    case IngestErrorKind::Open:
      return "cannot open " + path + " as a ROOT file: " + message;
    case IngestErrorKind::Tree:
      return "cannot read tree `" + name + "`: " + message;
    case IngestErrorKind::MissingBranch:
      return "branch `" + name + "` is missing (needed for " + needed_for + ")";
    case IngestErrorKind::TypeMismatch:
      return "branch `" + branch + "`: expected item type `" + expected + "`, file has `" + got + "`";
    case IngestErrorKind::EntryCount:
      return "branch `" + branch + "`: " + std::to_string(got_n) + " values for " +
             std::to_string(expected_n) + " tree entries";
    case IngestErrorKind::NegativeCount:
      return "branch `" + branch + "`: negative count at entry " + std::to_string(entry);
    case IngestErrorKind::LengthMismatch:
      return "branch `" + branch + "`: " + std::to_string(got_n) +
             " values but the collection counter sums to " + std::to_string(expected_n);
    case IngestErrorKind::NonFinite:
      return "branch `" + branch + "`: non-finite value at entry " + std::to_string(entry) +
             " (unrepresentable in canonical JSONL; refusing)";
    case IngestErrorKind::NotPtDescending:
      return "collection `" + collection + "` is not pT-descending at entry " + std::to_string(entry) +
             ", index " + std::to_string(index) + " (events must arrive ordered; re-sort is OFF)";
  }
  return "ingest error";
}

std::optional<std::string> IngestDiag::verbose_detail() const {
  if (kind != IngestDiagKind::UnmappedLeaves) return std::nullopt;
  std::string s = branch + ": unmapped leaves: ";
  for (std::size_t i = 0; i < leaves.size(); ++i) {
    if (i) s += ", ";
    s += leaves[i];
  }
  return s;
}

std::string IngestDiag::to_string() const {
  switch (kind) {
    case IngestDiagKind::UnmappedLeaves:
      return "collection `" + branch + "`: " + std::to_string(leaves.size()) + " unmapped " +
             (leaves.size() == 1 ? "leaf" : "leaves") + " dropped (--verbose lists them)";
    case IngestDiagKind::TagBitsIgnored:
      return "branch `" + branch + "`: " + std::to_string(values) +
             " value(s) with bits other than bit " + std::to_string(bit) +
             " set; those bits (other working points) are ignored";
    case IngestDiagKind::MultiElement:
      return "branch `" + branch + "`: " + std::to_string(events) +
             " event(s) with multiple elements; first taken";
    case IngestDiagKind::EmptyElement:
      return "branch `" + branch + "`: " + std::to_string(events) +
             " event(s) with no element; value omitted";
    case IngestDiagKind::LheWeightsDropped:
      return std::to_string(count) + " LHE weights present (`" + branch + "`), not mapped in v1";
    case IngestDiagKind::UnknownBranch:
      return "unknown branch `" + branch + "` (" + std::to_string(leaves.size()) +
             " leaf/leaves) — not in profile, dropped";
    case IngestDiagKind::AbsentCollection:
      return "mapped collection `" + branch + "` absent from file";
  }
  return "ingest diagnostic";
}

std::string Ingested::jsonl() const {
  std::string out;
  for (const auto& line : lines) {
    out += line;
    out += '\n';
  }
  return out;
}

namespace {

using OptF64 = std::optional<double>;
using OptPair = std::optional<std::pair<double, double>>;

bool load_one_element_impl(const LoadedFile& lf, const Profile& profile, const std::string& branch,
                           const std::string& leaf, std::size_t entries,
                           const std::set<std::string>& leaf_names, std::vector<IngestDiag>& diags,
                           IngestError& err, bool& present, std::vector<OptF64>& out) {
  const std::string full = profile.leaf_branch(branch, leaf);
  if (!leaf_names.count(full)) {
    present = false;
    return true;
  }
  present = true;
  auto counts = read_counter(lf, profile.counter_branch(branch), entries, err);
  if (!counts) return false;
  std::size_t total = 0;
  for (auto c : *counts) total += c;
  auto col = read_leaf_flat(lf, full, LeafKind::F32, 0, total, err);
  if (!col) return false;
  for (std::size_t i = 0; i < col->f64s.size(); ++i) {
    if (!std::isfinite(col->f64s[i])) {
      err = IngestError::non_finite(full, entry_of(*counts, i));
      return false;
    }
  }
  out.assign(entries, std::nullopt);
  std::uint64_t empty = 0, multi = 0;
  std::size_t offset = 0;
  for (std::size_t e = 0; e < entries; ++e) {
    auto c = (*counts)[e];
    if (c == 0) {
      ++empty;
    } else {
      if (c > 1) ++multi;
      out[e] = col->f64s[offset];
    }
    offset += c;
  }
  if (multi > 0) {
    IngestDiag d;
    d.kind = IngestDiagKind::MultiElement;
    d.branch = branch;
    d.events = multi;
    diags.push_back(std::move(d));
  }
  if (empty > 0) {
    IngestDiag d;
    d.kind = IngestDiagKind::EmptyElement;
    d.branch = branch;
    d.events = empty;
    diags.push_back(std::move(d));
  }
  return true;
}

bool load_one_element_pair(const LoadedFile& lf, const Profile& profile, const std::string& branch,
                           const std::string& pt_leaf, const std::string& phi_leaf, std::size_t entries,
                           const std::set<std::string>& leaf_names, std::vector<IngestDiag>& diags,
                           IngestError& err, bool& present, std::vector<OptPair>& out) {
  const std::string pt_full = profile.leaf_branch(branch, pt_leaf);
  const std::string phi_full = profile.leaf_branch(branch, phi_leaf);
  if (!leaf_names.count(pt_full) && !leaf_names.count(phi_full)) {
    present = false;
    return true;
  }
  present = true;
  auto counts = read_counter(lf, profile.counter_branch(branch), entries, err);
  if (!counts) return false;
  std::size_t total = 0;
  for (auto c : *counts) total += c;
  auto load = [&](const std::string& full, std::vector<double>& vals) -> bool {
    auto col = read_leaf_flat(lf, full, LeafKind::F32, 0, total, err);
    if (!col) return false;
    for (std::size_t i = 0; i < col->f64s.size(); ++i) {
      if (!std::isfinite(col->f64s[i])) {
        err = IngestError::non_finite(full, entry_of(*counts, i));
        return false;
      }
    }
    vals = std::move(col->f64s);
    return true;
  };
  std::vector<double> pts, phis;
  if (!load(pt_full, pts) || !load(phi_full, phis)) return false;
  out.assign(entries, std::nullopt);
  std::uint64_t empty = 0, multi = 0;
  std::size_t offset = 0;
  for (std::size_t e = 0; e < entries; ++e) {
    auto c = (*counts)[e];
    if (c == 0) {
      ++empty;
    } else {
      if (c > 1) ++multi;
      out[e] = std::make_pair(pts[offset], phis[offset]);
    }
    offset += c;
  }
  if (multi > 0) {
    IngestDiag d;
    d.kind = IngestDiagKind::MultiElement;
    d.branch = branch;
    d.events = multi;
    diags.push_back(std::move(d));
  }
  if (empty > 0) {
    IngestDiag d;
    d.kind = IngestDiagKind::EmptyElement;
    d.branch = branch;
    d.events = empty;
    diags.push_back(std::move(d));
  }
  return true;
}

bool read_scalar_flat(const LoadedFile& lf, const std::string& full, std::size_t entries,
                      const std::set<std::string>& leaf_names, IngestError& err, bool& present,
                      std::vector<double>& vals) {
  if (!leaf_names.count(full)) {
    present = false;
    return true;
  }
  present = true;
  const BranchRec* b = require_branch(lf, full, "a flat per-event scalar", err);
  if (!b) return false;
  if (!type_ok_f32(b->type_name, false)) {
    err = IngestError::type_mismatch(full, "float|double", b->type_name);
    return false;
  }
  std::string msg;
  if (!detail::flatten_f64(lf.bytes, *b, vals, msg)) {
    err = IngestError::tree(full, msg);
    return false;
  }
  if (vals.size() != entries) {
    err = IngestError::entry_count(full, entries, vals.size());
    return false;
  }
  for (std::size_t i = 0; i < vals.size(); ++i) {
    if (!std::isfinite(vals[i])) {
      err = IngestError::non_finite(full, i);
      return false;
    }
  }
  return true;
}

std::vector<IngestDiag> classify_rest(const Profile& profile, const std::set<std::string>& leaf_names) {
  enum Fam { Mapped, KnownDrop };
  struct Family {
    Fam tag = Mapped;
    std::vector<std::string> mapped_leaves;
  };
  std::map<std::string, Family> families;
  for (const auto& c : profile.collections) {
    Family f;
    for (const auto& l : c.leaves) f.mapped_leaves.push_back(l.leaf);
    families[c.branch] = std::move(f);
  }
  if (profile.met) {
    Family f;
    f.mapped_leaves = {profile.met->pt_leaf, profile.met->phi_leaf};
    for (const auto& d : profile.met->known_dropped_leaves) f.mapped_leaves.push_back(d);
    families[profile.met->branch] = std::move(f);
  }
  for (const auto& s : profile.scalars) {
    Family f;
    f.mapped_leaves = {s.leaf};
    families[s.branch] = std::move(f);
  }
  for (const auto& b : profile.known_drop_branches) {
    families.emplace(b, Family{KnownDrop, {}});
  }

  std::set<std::string> consumed;
  if (profile.naming.flat_event_vars) {
    if (profile.met) {
      consumed.insert(profile.leaf_branch(profile.met->branch, profile.met->pt_leaf));
      consumed.insert(profile.leaf_branch(profile.met->branch, profile.met->phi_leaf));
    }
    for (const auto& s : profile.scalars) consumed.insert(profile.leaf_branch(s.branch, s.leaf));
    if (profile.weight) consumed.insert(profile.leaf_branch(profile.weight->branch, profile.weight->leaf));
  }

  std::map<std::string, std::vector<std::string>> unmapped, unknown;
  for (const auto& name : leaf_names) {
    if (consumed.count(name)) continue;
    if (auto prefix = profile.counter_prefix(name)) {
      if (!families.count(*prefix)) unknown[*prefix];
      continue;
    }
    auto sep = name.find(profile.naming.leaf_sep);
    if (sep == std::string::npos) {
      unknown[name];
      continue;
    }
    std::string prefix = name.substr(0, sep);
    std::string leaf = name.substr(sep + profile.naming.leaf_sep.size());
    if (leaf == "fUniqueID" || leaf == "fBits") continue;
    auto it = families.find(prefix);
    if (it == families.end()) {
      unknown[prefix].push_back(name);
    } else if (it->second.tag == Mapped) {
      bool mapped = false;
      for (const auto& m : it->second.mapped_leaves)
        if (m == leaf) {
          mapped = true;
          break;
        }
      if (!mapped) unmapped[prefix].push_back(name);
    }
  }

  std::vector<IngestDiag> out;
  for (const auto& c : profile.collections) {
    auto it = unmapped.find(c.branch);
    if (it != unmapped.end()) {
      IngestDiag d;
      d.kind = IngestDiagKind::UnmappedLeaves;
      d.branch = c.branch;
      d.leaves = std::move(it->second);
      out.push_back(std::move(d));
      unmapped.erase(it);
    }
  }
  for (auto& kv : unmapped) {
    IngestDiag d;
    d.kind = IngestDiagKind::UnmappedLeaves;
    d.branch = kv.first;
    d.leaves = std::move(kv.second);
    out.push_back(std::move(d));
  }
  for (auto& kv : unknown) {
    IngestDiag d;
    d.kind = IngestDiagKind::UnknownBranch;
    d.branch = kv.first;
    d.leaves = std::move(kv.second);
    out.push_back(std::move(d));
  }
  return out;
}

bool validate_pt(const std::vector<LoadedCollection>& loaded, IngestError& err) {
  for (const auto& lc : loaded) {
    std::size_t pt_idx = lc.spec->leaves.size();
    for (std::size_t i = 0; i < lc.spec->leaves.size(); ++i) {
      if (lc.spec->leaves[i].prop == "pt") {
        pt_idx = i;
        break;
      }
    }
    if (pt_idx >= lc.columns.size() || lc.columns[pt_idx].tag != FlatTag::F64) continue;
    const auto& pts = lc.columns[pt_idx].f64s;
    std::size_t offset = 0;
    for (std::size_t entry = 0; entry < lc.counts.size(); ++entry) {
      auto c = lc.counts[entry];
      for (std::size_t i = 1; i < c; ++i) {
        if (pts[offset + i] > pts[offset + i - 1]) {
          err = IngestError::not_pt_descending(lc.spec->branch, entry, i);
          return false;
        }
      }
      offset += c;
    }
  }
  return true;
}

std::vector<std::string> emit_lines(std::size_t entries, const std::vector<LoadedCollection>& loaded,
                                    const std::vector<OptPair>* met,
                                    const std::vector<std::pair<std::string, std::vector<OptF64>>>& scalars,
                                    const std::vector<OptF64>* weights) {
  std::vector<std::size_t> offsets(loaded.size(), 0);
  std::vector<std::string> lines;
  lines.reserve(entries);
  for (std::size_t entry = 0; entry < entries; ++entry) {
    std::string line = "{";
    bool first = true;
    auto sep = [&] {
      if (!first) line.push_back(',');
      first = false;
    };
    for (std::size_t ci = 0; ci < loaded.size(); ++ci) {
      const auto& lc = loaded[ci];
      sep();
      line.push_back('"');
      line += lc.spec->key;
      line += "\":[";
      const std::size_t count = lc.counts[entry];
      const std::size_t base = offsets[ci];
      for (std::size_t i = 0; i < count; ++i) {
        if (i) line.push_back(',');
        line.push_back('{');
        bool wrote = false;
        for (std::size_t li = 0; li < lc.spec->leaves.size(); ++li) {
          if (wrote) line.push_back(',');
          wrote = true;
          line.push_back('"');
          line += lc.spec->leaves[li].prop;
          line += "\":";
          if (lc.columns[li].tag == FlatTag::F64) line += jnum(lc.columns[li].f64s[base + i]);
          else line += std::to_string(lc.columns[li].ints[base + i]);
        }
        for (const auto& kv : lc.spec->constants) {
          if (wrote) line.push_back(',');
          wrote = true;
          line.push_back('"');
          line += kv.first;
          line += "\":";
          line += jnum(kv.second);
        }
        line.push_back('}');
      }
      offsets[ci] += count;
      line.push_back(']');
    }
    if (met && (*met)[entry]) {
      sep();
      line += "\"MET\":{\"pt\":";
      line += jnum((*met)[entry]->first);
      line += ",\"phi\":";
      line += jnum((*met)[entry]->second);
      line += "}";
    }
    for (const auto& kv : scalars) {
      if (kv.second[entry]) {
        sep();
        line.push_back('"');
        line += kv.first;
        line += "\":";
        line += jnum(*kv.second[entry]);
      }
    }
    if (weights && (*weights)[entry]) {
      sep();
      line += "\"weight\":";
      line += jnum(*(*weights)[entry]);
    }
    line.push_back('}');
    lines.push_back(std::move(line));
  }
  return lines;
}

}  // namespace

std::optional<Ingested> read_root(const std::string& path, const Profile& profile, IngestError& err) {
  detail::LoadError le;
  auto lf = detail::load_root(path, profile.tree, le);
  if (!lf) {
    err = (le.kind == detail::LoadError::Open) ? IngestError::open(path, le.message)
                                               : IngestError::tree(profile.tree, le.message);
    return std::nullopt;
  }
  // Mirrors the oracle's `usize::try_from(tree.entries()).unwrap_or(0)`.
  const std::size_t entries = lf->tree.entries < 0 ? 0 : static_cast<std::size_t>(lf->tree.entries);
  auto names = leaf_names_of(lf->tree);
  std::vector<IngestDiag> diags;
  std::vector<LoadedCollection> loaded;
  for (const auto& spec : profile.collections) {
    auto lc = load_collection(*lf, profile, spec, entries, names, diags, err);
    if (!lc) return std::nullopt;
    if (!lc->spec) {
      IngestDiag d;
      d.kind = IngestDiagKind::AbsentCollection;
      d.branch = spec.branch;
      diags.push_back(std::move(d));
    } else {
      loaded.push_back(std::move(*lc));
    }
  }

  std::vector<OptPair> met_vals;
  const std::vector<OptPair>* met_ptr = nullptr;
  if (profile.met) {
    if (profile.naming.flat_event_vars) {
      bool ppt = false, pphi = false;
      std::vector<double> pt, phi;
      if (!read_scalar_flat(*lf, profile.leaf_branch(profile.met->branch, profile.met->pt_leaf), entries,
                            names, err, ppt, pt))
        return std::nullopt;
      if (!read_scalar_flat(*lf, profile.leaf_branch(profile.met->branch, profile.met->phi_leaf), entries,
                            names, err, pphi, phi))
        return std::nullopt;
      if (ppt && pphi) {
        met_vals.resize(entries);
        for (std::size_t i = 0; i < entries; ++i) met_vals[i] = std::make_pair(pt[i], phi[i]);
        met_ptr = &met_vals;
      }
    } else {
      bool present = false;
      if (!load_one_element_pair(*lf, profile, profile.met->branch, profile.met->pt_leaf,
                                 profile.met->phi_leaf, entries, names, diags, err, present, met_vals))
        return std::nullopt;
      if (present) met_ptr = &met_vals;
    }
  }

  std::vector<std::pair<std::string, std::vector<OptF64>>> scalars;
  for (const auto& s : profile.scalars) {
    if (profile.naming.flat_event_vars) {
      bool present = false;
      std::vector<double> vals;
      if (!read_scalar_flat(*lf, profile.leaf_branch(s.branch, s.leaf), entries, names, err, present, vals))
        return std::nullopt;
      if (present) {
        std::vector<OptF64> o(entries);
        for (std::size_t i = 0; i < entries; ++i) o[i] = vals[i];
        scalars.emplace_back(s.key, std::move(o));
      }
    } else {
      bool present = false;
      std::vector<OptF64> o;
      if (!load_one_element_impl(*lf, profile, s.branch, s.leaf, entries, names, diags, err, present, o))
        return std::nullopt;
      if (present) scalars.emplace_back(s.key, std::move(o));
    }
  }

  std::vector<OptF64> weights;
  const std::vector<OptF64>* wptr = nullptr;
  if (profile.weight) {
    if (profile.naming.flat_event_vars) {
      bool present = false;
      std::vector<double> vals;
      if (!read_scalar_flat(*lf, profile.leaf_branch(profile.weight->branch, profile.weight->leaf), entries,
                            names, err, present, vals))
        return std::nullopt;
      if (present) {
        weights.resize(entries);
        for (std::size_t i = 0; i < entries; ++i) weights[i] = vals[i];
        wptr = &weights;
      }
    } else {
      bool present = false;
      if (!load_one_element_impl(*lf, profile, profile.weight->branch, profile.weight->leaf, entries, names,
                                diags, err, present, weights))
        return std::nullopt;
      if (present) wptr = &weights;
    }
  }

  if (profile.lhe_weights) {
    const std::string full = profile.leaf_branch(profile.lhe_weights->first, profile.lhe_weights->second);
    if (names.count(full)) {
      auto counts = read_counter(*lf, profile.counter_branch(profile.lhe_weights->first), entries, err);
      if (!counts) return std::nullopt;
      std::uint64_t count = 0;
      for (auto c : *counts) count += c;
      if (count > 0) {
        IngestDiag d;
        d.kind = IngestDiagKind::LheWeightsDropped;
        d.branch = full;
        d.count = count;
        diags.push_back(std::move(d));
      }
    }
  }

  auto rest = classify_rest(profile, names);
  diags.insert(diags.end(), rest.begin(), rest.end());

  if (!validate_pt(loaded, err)) return std::nullopt;

  // Every loaded column was length-checked against `entries`; only a tree
  // that contributes no mapped data at all leaves fEntries unconstrained,
  // and then it must not drive the output size on its own.
  if (loaded.empty() && !met_ptr && scalars.empty() && !wptr && entries > lf->bytes.size()) {
    err = IngestError::tree(profile.tree, "fEntries (" + std::to_string(entries) +
                                              ") exceeds the file size with no mapped branch present");
    return std::nullopt;
  }

  Ingested out;
  out.profile_id = profile.id();
  out.entries = entries;
  out.lines = emit_lines(entries, loaded, met_ptr, scalars, wptr);
  out.diags = std::move(diags);
  return out;
}

}  // namespace adl2::ingest
