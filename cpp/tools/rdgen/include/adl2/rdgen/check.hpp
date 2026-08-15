#pragma once

#include "adl2/rdgen/ebnf.hpp"

#include <string>
#include <string_view>
#include <vector>

namespace adl2::rdgen {

enum class MapRole {
  Generate,
  GenerateLater,
  Hook,
  Helper,
  Extra,
  Token,
};

struct MapEntry {
  std::string name;
  std::string symbol;
  MapRole role = MapRole::Hook;
  std::string notes;
  int line = 0;
};

struct MethodMap {
  std::vector<MapEntry> entries;
  std::string error;
  int error_line = 0;
};

MethodMap parse_method_map(std::string_view src);

struct CheckIssue {
  std::string message;
};

struct CheckResult {
  std::vector<CheckIssue> errors;
  std::vector<CheckIssue> notes;
  bool ok() const { return errors.empty(); }
};

/// Extract `parse_foo` identifiers from a C++ header (declaration scan).
std::vector<std::string> scan_parse_symbols(std::string_view header);

/// Fail-closed: every EBNF production is mapped; every generate-role
/// production has an emit shape; every parse_* in the header is listed.
CheckResult check_grammar(const Grammar& g, const MethodMap& map,
                          std::string_view parser_hpp);

const char* role_name(MapRole r);
bool parse_role(std::string_view s, MapRole& out);

}  // namespace adl2::rdgen
