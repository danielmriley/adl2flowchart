#pragma once

/// `adl2_ingest` — converter profiles + native ROOT TTree → canonical JSONL
/// (Rust `adl-ingest`, SPEC_EVENT_PIPELINE §1).
///
/// Experiment names live only in the profile table. The interpreter never
/// sees them: `read_root` emits the same JSONL `read_jsonl` already loads.
/// Does not link CERN ROOT. Basket decompress uses zlib (`ZL` blocks only).

#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::ingest {

enum class LeafKind { F32, I32, Bool, TagBit };

struct LeafSpec {
  std::string leaf;
  std::string prop;
  LeafKind kind = LeafKind::F32;
  std::uint32_t tag_bit = 0;
};

struct CollectionSpec {
  std::string branch;
  std::string key;
  std::vector<LeafSpec> leaves;
  std::vector<std::pair<std::string, double>> constants;
};

struct MetSpec {
  std::string branch;
  std::string pt_leaf;
  std::string phi_leaf;
  std::vector<std::string> known_dropped_leaves;
};

struct ScalarSpec {
  std::string branch;
  std::string leaf;
  std::string key;
};

struct WeightSpec {
  std::string branch;
  std::string leaf;
};

enum class CounterStyle { SizeSuffix, NPrefix };

struct Naming {
  std::string leaf_sep;
  CounterStyle counter = CounterStyle::SizeSuffix;
  bool flat_event_vars = false;
};

struct Profile {
  Naming naming;
  std::string name;
  std::uint32_t version = 1;
  std::string tree;
  std::vector<CollectionSpec> collections;
  std::optional<MetSpec> met;
  std::vector<ScalarSpec> scalars;
  std::optional<WeightSpec> weight;
  std::optional<std::pair<std::string, std::string>> lhe_weights;
  std::vector<std::string> known_drop_branches;

  std::string id() const { return name + "/" + std::to_string(version); }
  std::string leaf_branch(const std::string& branch, const std::string& leaf) const;
  std::string counter_branch(const std::string& branch) const;
  std::optional<std::string> counter_prefix(const std::string& name) const;
  std::vector<std::pair<std::string, std::string>> decides() const;
};

Profile delphes();
Profile nanoaod();
std::optional<Profile> by_name(const std::string& name);

/// CLI names `by_name` accepts (error messages).
inline const char* const KNOWN_PROFILES[] = {"delphes", "nanoaod"};
inline constexpr int KNOWN_PROFILES_N = 2;
inline std::string known_profiles_csv() { return "delphes, nanoaod"; }

enum class IngestErrorKind {
  Open,
  Tree,
  MissingBranch,
  TypeMismatch,
  EntryCount,
  NegativeCount,
  LengthMismatch,
  NonFinite,
  NotPtDescending
};

struct IngestError {
  IngestErrorKind kind = IngestErrorKind::Open;
  std::string path;
  std::string message;
  std::string name;
  std::string needed_for;
  std::string branch;
  std::string expected;
  std::string got;
  std::string collection;
  std::size_t expected_n = 0;
  std::size_t got_n = 0;
  std::size_t entry = 0;
  std::size_t index = 0;
  std::string to_string() const;

  static IngestError open(std::string path, std::string message);
  static IngestError tree(std::string name, std::string message);
  static IngestError missing_branch(std::string name, std::string needed_for);
  static IngestError type_mismatch(std::string branch, std::string expected, std::string got);
  static IngestError entry_count(std::string branch, std::size_t expected, std::size_t got);
  static IngestError negative_count(std::string branch, std::size_t entry);
  static IngestError length_mismatch(std::string branch, std::size_t expected, std::size_t got);
  static IngestError non_finite(std::string branch, std::size_t entry);
  static IngestError not_pt_descending(std::string collection, std::size_t entry, std::size_t index);
};

enum class IngestDiagKind {
  UnmappedLeaves,
  TagBitsIgnored,
  MultiElement,
  EmptyElement,
  LheWeightsDropped,
  UnknownBranch,
  AbsentCollection
};

struct IngestDiag {
  IngestDiagKind kind = IngestDiagKind::UnknownBranch;
  std::string branch;
  std::vector<std::string> leaves;
  std::uint32_t bit = 0;
  std::uint64_t values = 0;
  std::uint64_t events = 0;
  std::uint64_t count = 0;

  bool verbose_only() const { return kind == IngestDiagKind::AbsentCollection; }
  std::optional<std::string> verbose_detail() const;
  std::string to_string() const;
};

struct Ingested {
  std::string profile_id;
  std::size_t entries = 0;
  std::vector<std::string> lines;
  std::vector<IngestDiag> diags;
  std::string jsonl() const;
};

/// Read `path` under `profile`. On failure, `err` is set and nullopt returned.
std::optional<Ingested> read_root(const std::string& path, const Profile& profile, IngestError& err);

/// Independent uproot oracle script generated from the same profile table.
std::string to_jsonl_py(const Profile& profile);

}  // namespace adl2::ingest
