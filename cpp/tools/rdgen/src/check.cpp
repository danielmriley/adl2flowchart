#include "adl2/rdgen/check.hpp"
#include "adl2/rdgen/inventory.hpp"

#include <cctype>
#include <regex>
#include <sstream>
#include <unordered_map>
#include <unordered_set>

namespace adl2::rdgen {
namespace {

std::string trim(std::string_view s) {
  std::size_t a = 0;
  while (a < s.size() && std::isspace(static_cast<unsigned char>(s[a]))) ++a;
  std::size_t b = s.size();
  while (b > a && std::isspace(static_cast<unsigned char>(s[b - 1]))) --b;
  return std::string(s.substr(a, b - a));
}

std::vector<std::string> split_ws(const std::string& line) {
  std::vector<std::string> out;
  std::istringstream is(line);
  std::string tok;
  while (is >> tok) out.push_back(tok);
  return out;
}

bool header_mentions(std::string_view header, const std::string& symbol) {
  if (symbol.find("TokKind::") == 0) return true;
  if (symbol.find('|') != std::string::npos) return true;
  return header.find(symbol) != std::string_view::npos;
}

bool is_emit_shape(Shape s) {
  return s == Shape::Alias || s == Shape::LeftAssoc || s == Shape::PrefixUnary ||
         s == Shape::OptionalSuffix || s == Shape::KeywordSeq ||
         s == Shape::Choice;
}

}  // namespace

const char* role_name(MapRole r) {
  switch (r) {
    case MapRole::Generate:
      return "generate";
    case MapRole::GenerateLater:
      return "generate-later";
    case MapRole::Hook:
      return "hook";
    case MapRole::Helper:
      return "helper";
    case MapRole::Extra:
      return "extra";
    case MapRole::Token:
      return "token";
  }
  return "?";
}

bool parse_role(std::string_view s, MapRole& out) {
  if (s == "generate") {
    out = MapRole::Generate;
    return true;
  }
  if (s == "generate-later") {
    out = MapRole::GenerateLater;
    return true;
  }
  if (s == "hook") {
    out = MapRole::Hook;
    return true;
  }
  if (s == "helper") {
    out = MapRole::Helper;
    return true;
  }
  if (s == "extra") {
    out = MapRole::Extra;
    return true;
  }
  if (s == "token") {
    out = MapRole::Token;
    return true;
  }
  return false;
}

MethodMap parse_method_map(std::string_view src) {
  MethodMap map;
  int line_no = 0;
  std::size_t i = 0;
  while (i <= src.size()) {
    std::size_t nl = src.find('\n', i);
    if (nl == std::string_view::npos) nl = src.size();
    std::string line = trim(src.substr(i, nl - i));
    ++line_no;
    i = nl + 1;
    if (line.empty() || line[0] == '#') {
      if (nl == src.size()) break;
      continue;
    }
    auto parts = split_ws(line);
    if (parts.size() < 3) {
      map.error = "expected: name symbol role [notes]";
      map.error_line = line_no;
      return map;
    }
    MapEntry e;
    e.name = parts[0];
    e.symbol = parts[1];
    e.line = line_no;
    if (!parse_role(parts[2], e.role)) {
      map.error = "unknown role '" + parts[2] + "'";
      map.error_line = line_no;
      return map;
    }
    if (parts.size() > 3) {
      e.notes = parts[3];
      for (std::size_t k = 4; k < parts.size(); ++k) {
        e.notes += " ";
        e.notes += parts[k];
      }
    }
    map.entries.push_back(std::move(e));
    if (nl == src.size()) break;
  }
  return map;
}

std::vector<std::string> scan_parse_symbols(std::string_view header) {
  static const std::regex re(R"(parse_([a-z_]+)\s*\()");
  std::vector<std::string> out;
  std::unordered_set<std::string> seen;
  auto begin = std::cregex_iterator(header.data(), header.data() + header.size(), re);
  auto end = std::cregex_iterator();
  for (auto it = begin; it != end; ++it) {
    const std::string name = "parse_" + (*it)[1].str();
    if (seen.insert(name).second) out.push_back(name);
  }
  return out;
}

CheckResult check_grammar(const Grammar& g, const MethodMap& map,
                          std::string_view parser_hpp) {
  CheckResult r;
  if (!g.error.empty()) {
    r.errors.push_back({"EBNF parse error: " + g.error});
    return r;
  }
  if (!map.error.empty()) {
    r.errors.push_back({"method_map parse error: " + map.error});
    return r;
  }

  std::unordered_map<std::string, const MapEntry*> by_name;
  std::unordered_set<std::string> mapped_symbols;
  std::unordered_set<std::string> token_names;
  for (const auto& e : map.entries) {
    if (e.role == MapRole::Token) {
      token_names.insert(e.name);
      continue;
    }
    if (e.name != "*") {
      auto [it, ok] = by_name.emplace(e.name, &e);
      if (!ok) {
        r.errors.push_back({"duplicate map entry for '" + e.name + "'"});
      }
    }
    mapped_symbols.insert(e.symbol);
  }

  std::unordered_set<std::string> prod_names;
  for (const auto& p : g.prods) {
    if (!prod_names.insert(p.name).second) {
      r.errors.push_back({"duplicate EBNF production '" + p.name + "'"});
    }
    auto it = by_name.find(p.name);
    if (it == by_name.end()) {
      std::vector<std::string> kws;
      if (keyword_condition_kws(p, kws)) {
        r.notes.push_back({p.name + " → (inferred Cut) [generate, KeywordSeq]"});
        continue;
      }
      r.errors.push_back({"EBNF production '" + p.name +
                          "' has no method_map.txt entry"});
      continue;
    }
    const MapEntry& e = *it->second;
    if (e.role == MapRole::Extra || e.role == MapRole::Token) {
      r.errors.push_back({"production '" + p.name +
                          "' cannot use role " + role_name(e.role)});
    }
    if (!header_mentions(parser_hpp, e.symbol)) {
      r.errors.push_back({"mapped symbol '" + e.symbol + "' for '" + p.name +
                          "' not found in parser.hpp"});
    }
    const ShapeInfo sh = classify(p);
    if (e.role == MapRole::Generate && !is_emit_shape(sh.shape)) {
      r.errors.push_back({"'" + p.name + "' is role=generate but shape is " +
                          std::string(shape_name(sh.shape)) +
                          " (emitter cannot write it)"});
    }
    r.notes.push_back({p.name + " → " + e.symbol + " [" + role_name(e.role) +
                       ", " + shape_name(sh.shape) + "]"});
  }

  for (const auto& e : map.entries) {
    if (e.name == "*" || e.role == MapRole::Token) continue;
    if (!prod_names.count(e.name)) {
      r.errors.push_back({"method_map name '" + e.name +
                          "' is not an EBNF production"});
    }
  }

  // RHS names must be productions or lexer tokens.
  for (const auto& p : g.prods) {
    auto walk_seq = [&](auto&& self, const Seq& seq) -> void {
      for (const auto& t : seq.terms) {
        if (t.kind == TermKind::Name) {
          if (!prod_names.count(t.text) && !token_names.count(t.text)) {
            r.errors.push_back({"'" + p.name + "' references unknown name '" +
                                t.text + "' (not a production or token)"});
          }
        } else if (t.kind == TermKind::Group || t.kind == TermKind::Optional ||
                   t.kind == TermKind::Repeat) {
          for (const auto& alt : t.group) self(self, alt);
        }
      }
    };
    for (const auto& alt : p.alts) walk_seq(walk_seq, alt);
  }

  for (const auto& sym : scan_parse_symbols(parser_hpp)) {
    if (!mapped_symbols.count(sym)) {
      r.errors.push_back({"parser.hpp declares '" + sym +
                          "' but method_map.txt does not list it"});
    }
  }

  if (!header_mentions(parser_hpp, "extend_particle_list")) {
    r.errors.push_back({"parser.hpp is missing extend_particle_list"});
  }

  Inventory inv;
  std::string inv_err;
  if (!build_inventory(g, inv, inv_err) && inv.errors.empty() &&
      !inv_err.empty()) {
    r.errors.push_back({inv_err});
  }
  for (const auto& e : inv.errors) {
    r.errors.push_back({e});
  }

  return r;
}

}  // namespace adl2::rdgen
