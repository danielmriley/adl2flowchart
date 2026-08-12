#include "adl2/sema/ext.hpp"

#include "adl2/sema/intern.hpp"

#include <sstream>

namespace adl2::sema {
namespace embedded {
extern const char kExtObjs[];
extern const char kExtLib[];
extern const char kPropertyVars[];
extern const char kAliases[];
}  // namespace embedded

namespace {

const char* kExactNameProps[] = {"btag", "ctag", "tautag"};
const char* kEventScalarKeys[] = {"ht", "st", "fht", "scalarht", "delphes_scalarht"};

std::string strip_comment(const std::string& line) {
  auto hash = line.find('#');
  std::string s = hash == std::string::npos ? line : line.substr(0, hash);
  // trim
  auto b = s.find_first_not_of(" \t\r\n");
  if (b == std::string::npos) return {};
  auto e = s.find_last_not_of(" \t\r\n");
  return s.substr(b, e - b + 1);
}

bool is_exact_name_prop(const std::string& lc) {
  for (const char* p : kExactNameProps) {
    if (lc == p) return true;
  }
  return false;
}

}  // namespace

ExtDecls ExtDecls::from_sources(const std::string& ext_objs,
                                const std::string& ext_lib,
                                const std::string& property_vars,
                                const std::string& aliases) {
  ExtDecls d;
  d.load_aliases(aliases);
  d.load_ext_objs(ext_objs);
  d.load_ext_lib(ext_lib);
  d.load_property_vars(property_vars);
  return d;
}

ExtDecls ExtDecls::legacy() {
  return from_sources(embedded::kExtObjs, embedded::kExtLib,
                      embedded::kPropertyVars, embedded::kAliases);
}

void ExtDecls::load_aliases(const std::string& text) {
  std::istringstream in(text);
  std::string line;
  while (std::getline(in, line)) {
    line = strip_comment(line);
    if (line.empty()) continue;
    std::istringstream toks(line);
    std::string canon;
    if (!(toks >> canon)) continue;
    base_canon_[SymbolTable::ascii_lower(canon)] = canon;
    std::string alias;
    while (toks >> alias) {
      base_canon_[SymbolTable::ascii_lower(alias)] = canon;
    }
  }
}

void ExtDecls::load_ext_objs(const std::string& text) {
  std::istringstream in(text);
  std::string line;
  while (std::getline(in, line)) {
    auto name = strip_comment(line);
    if (name.empty()) continue;
    std::string lc = SymbolTable::ascii_lower(name);
    if (base_canon_.find(lc) == base_canon_.end()) {
      base_canon_[lc] = name;
    }
  }
}

void ExtDecls::load_ext_lib(const std::string& text) {
  std::istringstream in(text);
  std::string line;
  while (std::getline(in, line)) {
    auto name = strip_comment(line);
    if (name.empty()) continue;
    functions_.insert(SymbolTable::ascii_lower(name));
  }
}

void ExtDecls::load_property_vars(const std::string& text) {
  std::istringstream in(text);
  std::string line;
  while (std::getline(in, line)) {
    line = strip_comment(line);
    if (line.empty()) continue;
    auto arrow = line.find("->");
    if (arrow == std::string::npos) continue;
    std::string name = line.substr(0, arrow);
    std::string internal = line.substr(arrow + 2);
    auto trim = [](std::string s) {
      auto b = s.find_first_not_of(" \t");
      if (b == std::string::npos) return std::string{};
      auto e = s.find_last_not_of(" \t");
      return s.substr(b, e - b + 1);
    };
    name = trim(name);
    internal = trim(internal);
    if (name.empty()) continue;
    std::string lc = SymbolTable::ascii_lower(name);
    std::string canon_key;
    if (is_exact_name_prop(lc) || internal.empty() ||
        SymbolTable::ascii_lower(internal) == "blank") {
      canon_key = lc;
    } else {
      canon_key = SymbolTable::ascii_lower(internal);
    }
    std::string display = name;
    for (const auto& kv : props_) {
      if (kv.second.canon_key == canon_key) {
        display = kv.second.display;
        break;
      }
    }
    if (props_.find(lc) == props_.end()) {
      props_[lc] = PropEntry{std::move(canon_key), std::move(display)};
    }
  }
}

const std::string* ExtDecls::base_collection(const std::string& name) const {
  auto it = base_canon_.find(SymbolTable::ascii_lower(name));
  if (it == base_canon_.end()) return nullptr;
  return &it->second;
}

bool ExtDecls::is_met_family(const std::string& name) const {
  const std::string* c = base_collection(name);
  if (!c) return false;
  return SymbolTable::ascii_lower(*c) == MET_FAMILY_KEY;
}

bool ExtDecls::is_tag_property(const std::string& canon_key) const {
  return is_exact_name_prop(canon_key);
}

bool ExtDecls::is_event_scalar(const std::string& name) const {
  std::string lc = SymbolTable::ascii_lower(name);
  for (const char* k : kEventScalarKeys) {
    if (lc == k) return true;
  }
  return false;
}

bool ExtDecls::is_function(const std::string& name) const {
  return functions_.count(SymbolTable::ascii_lower(name)) != 0;
}

std::pair<std::string, std::string> ExtDecls::prop_canon(
    const std::string& name) const {
  std::string lc = SymbolTable::ascii_lower(name);
  auto it = props_.find(lc);
  if (it != props_.end()) {
    return {it->second.canon_key, it->second.display};
  }
  return {lc, name};
}

bool ExtDecls::is_property(const std::string& name) const {
  return props_.count(SymbolTable::ascii_lower(name)) != 0;
}

}  // namespace adl2::sema
