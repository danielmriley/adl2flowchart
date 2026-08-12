#pragma once

/// External standard-library declarations (legacy ext_objs / ext_lib /
/// property_vars / object_aliases). Public API does not include parser headers.

#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

namespace adl2::sema {

/// Canonical identity key of the MET base-collection family.
inline constexpr const char* MET_FAMILY_KEY = "met";

class ExtDecls {
 public:
  static ExtDecls from_sources(const std::string& ext_objs,
                               const std::string& ext_lib,
                               const std::string& property_vars,
                               const std::string& aliases);
  /// Legacy standard library, embedded at build time from `legacy_parser/adl/`.
  static ExtDecls legacy();

  const std::string* base_collection(const std::string& name) const;
  bool is_met_family(const std::string& name) const;
  bool is_tag_property(const std::string& canon_key) const;
  bool is_event_scalar(const std::string& name) const;
  bool is_function(const std::string& name) const;
  /// (identity_key, display). Unknown properties canonicalize to lowercase.
  std::pair<std::string, std::string> prop_canon(const std::string& name) const;
  bool is_property(const std::string& name) const;

 private:
  void load_aliases(const std::string& text);
  void load_ext_objs(const std::string& text);
  void load_ext_lib(const std::string& text);
  void load_property_vars(const std::string& text);

  std::unordered_map<std::string, std::string> base_canon_;
  std::unordered_set<std::string> functions_;
  struct PropEntry {
    std::string canon_key;
    std::string display;
  };
  std::unordered_map<std::string, PropEntry> props_;
};

}  // namespace adl2::sema
