#pragma once

/// Provenance object for later histos.json / cutflow.json embedding.
/// Tool identity for this binary is `smash_cpp2 0.1.0`. Not emitted by
/// this unit's default `run` text path.

#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::interp {

struct InputIdentity {
  std::string file;
  std::string sha256;
  std::uint64_t events = 0;
  std::optional<std::string> profile;
};

struct Provenance {
  std::string tool;
  std::string adl_file;
  std::string adl_sha256;
  std::optional<InputIdentity> input;
  std::optional<std::uint64_t> seed;
  std::vector<std::pair<std::string, std::string>> decides;

  std::string to_json(bool pretty) const;

  template <typename W>
  void write(W& w) const {
    w.open('{');
    w.key("tool");
    w.str_val(tool);
    w.key("adl");
    w.open('{');
    w.key("file");
    w.str_val(adl_file);
    w.key("sha256");
    w.str_val(adl_sha256);
    w.close('}');
    if (input) {
      w.key("input");
      w.open('{');
      w.key("file");
      w.str_val(input->file);
      w.key("sha256");
      w.str_val(input->sha256);
      w.key("events");
      w.raw(std::to_string(input->events));
      if (input->profile) {
        w.key("profile");
        w.str_val(*input->profile);
      }
      w.close('}');
    }
    if (seed) {
      w.key("seed");
      w.raw(std::to_string(*seed));
    }
    if (!decides.empty()) {
      w.key("decides");
      w.open('{');
      for (const auto& kv : decides) {
        w.key(kv.first);
        w.str_val(kv.second);
      }
      w.close('}');
    }
    w.close('}');
  }
};

}  // namespace adl2::interp
