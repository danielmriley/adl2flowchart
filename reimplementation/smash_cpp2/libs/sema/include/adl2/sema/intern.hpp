#pragma once

/// Case-insensitive symbol interning (PHASE0). Identity is the ASCII-lowercase
/// fold; the first-seen spelling is kept for dumps/diagnostics.

#include <cstdint>
#include <string>
#include <unordered_map>
#include <vector>

namespace adl2::sema {

struct Symbol {
  std::uint32_t id = 0;

  bool operator==(Symbol o) const { return id == o.id; }
  bool operator!=(Symbol o) const { return id != o.id; }
  bool operator<(Symbol o) const { return id < o.id; }
};

class SymbolTable {
 public:
  Symbol intern(const std::string& name) {
    std::string key = ascii_lower(name);
    auto it = by_key_.find(key);
    if (it != by_key_.end()) return it->second;
    Symbol sym{static_cast<std::uint32_t>(display_.size())};
    by_key_.emplace(key, sym);
    display_.push_back(name);
    keys_.push_back(std::move(key));
    return sym;
  }

  /// First-seen spelling.
  const std::string& display(Symbol s) const { return display_[s.id]; }

  /// Lowercase identity key.
  const std::string& key(Symbol s) const { return keys_[s.id]; }

  /// Look up without interning.
  bool lookup(const std::string& name, Symbol& out) const {
    auto it = by_key_.find(ascii_lower(name));
    if (it == by_key_.end()) return false;
    out = it->second;
    return true;
  }

  static std::string ascii_lower(const std::string& s) {
    std::string out = s;
    for (char& c : out) {
      if (c >= 'A' && c <= 'Z') c = static_cast<char>(c - 'A' + 'a');
    }
    return out;
  }

 private:
  std::unordered_map<std::string, Symbol> by_key_;
  std::vector<std::string> display_;
  std::vector<std::string> keys_;
};

}  // namespace adl2::sema
