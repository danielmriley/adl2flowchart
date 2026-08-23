#pragma once

/// Internal TTree loader: Delphes + NanoAOD fixtures (no CERN ROOT).
/// Not a general ROOT reader — enough of TFile/TKey/TTree/TBranch/TBasket
/// to flatten the profile-mapped leaves.

#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <vector>

namespace adl2::ingest::detail {

struct BranchRec {
  std::string name;
  std::string type_name;   // oxyroot-style: "float[]", "int32_t", "bool[]", ...
  std::string leaf_class;  // TLeafF / TLeafI / ...
  bool is_unsigned = false;
  std::vector<std::int64_t> seeks;            // on-disk TBasket TKeys
  std::vector<std::vector<std::uint8_t>> embedded;  // uncompressed basket payloads
};

struct Tree {
  std::int64_t entries = 0;
  std::vector<BranchRec> branches;
  std::map<std::string, std::size_t> index;

  const BranchRec* find(const std::string& name) const {
    auto it = index.find(name);
    if (it == index.end()) return nullptr;
    return &branches[it->second];
  }
};

struct LoadError {
  enum Kind { Open, Tree } kind = Open;
  std::string message;
};

std::optional<Tree> load_tree(const std::string& path, const std::string& tree_name, LoadError& err);

/// Flatten one branch's baskets into host-endian values.
bool flatten_f64(const std::vector<std::uint8_t>& file, const BranchRec& br, std::vector<double>& out,
                 std::string& err);
bool flatten_i64(const std::vector<std::uint8_t>& file, const BranchRec& br, std::vector<std::int64_t>& out,
                 std::string& err);

/// File bytes kept so on-disk baskets can be read by seek.
struct LoadedFile {
  std::vector<std::uint8_t> bytes;
  Tree tree;
};

std::optional<LoadedFile> load_root(const std::string& path, const std::string& tree_name, LoadError& err);

}  // namespace adl2::ingest::detail
