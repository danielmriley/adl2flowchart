#include "adl2/rdgen/literals.hpp"

#include <cctype>
#include <sstream>
#include <unordered_map>
#include <unordered_set>
#include <utility>

namespace adl2::rdgen {
namespace {

const LitBind kCatalog[] = {
    // Keywords (lexer word map)
    {"info", "KwInfo", nullptr, nullptr, nullptr, true},
    {"define", "KwDefine", nullptr, nullptr, nullptr, true},
    {"def", "KwDef", nullptr, nullptr, nullptr, true},
    {"object", "KwObject", nullptr, nullptr, nullptr, true},
    {"obj", "KwObj", nullptr, nullptr, nullptr, true},
    {"composite", "KwComposite", nullptr, nullptr, nullptr, true},
    {"trigger", "KwTrigger", nullptr, nullptr, nullptr, true},
    {"take", "KwTake", nullptr, nullptr, nullptr, true},
    {"using", "KwUsing", nullptr, nullptr, nullptr, true},
    {"select", "KwSelect", nullptr, nullptr, nullptr, true},
    {"cut", "KwCut", nullptr, nullptr, nullptr, true},
    {"cmd", "KwCmd", nullptr, nullptr, nullptr, true},
    {"command", "KwCommand", nullptr, nullptr, nullptr, true},
    {"reject", "KwReject", nullptr, nullptr, nullptr, true},
    {"region", "KwRegion", nullptr, nullptr, nullptr, true},
    {"algo", "KwAlgo", nullptr, nullptr, nullptr, true},
    {"histoList", "KwHistoList", nullptr, nullptr, nullptr, true},
    {"bin", "KwBin", nullptr, nullptr, nullptr, true},
    // Contextual: lexed as Ident. Must not inherit KwBin onto new siblings
    // via a "first keyword" rule that skips this entry — see resolve.
    {"bins", nullptr, nullptr, nullptr, nullptr, false},
    {"histo", "KwHisto", nullptr, nullptr, nullptr, true},
    {"weight", "KwWeight", nullptr, nullptr, nullptr, true},
    {"table", "KwTable", nullptr, nullptr, nullptr, true},
    {"tabletype", "KwTabletype", nullptr, nullptr, nullptr, true},
    {"nvars", "KwNvars", nullptr, nullptr, nullptr, true},
    {"errors", "KwErrors", nullptr, nullptr, nullptr, true},
    {"union", "KwUnion", nullptr, nullptr, nullptr, true},
    {"process", "KwProcess", nullptr, nullptr, nullptr, true},
    {"counts", "KwCounts", nullptr, nullptr, nullptr, true},
    {"countsformat", "KwCountsformat", nullptr, nullptr, nullptr, true},
    {"print", "KwPrint", nullptr, nullptr, nullptr, true},
    {"save", "KwSave", nullptr, nullptr, nullptr, true},
    {"sort", "KwSort", nullptr, nullptr, nullptr, true},
    {"all", "KwAll", nullptr, nullptr, nullptr, true},
    {"none", "KwNone", nullptr, nullptr, nullptr, true},
    {"and", "KwAnd", "And", nullptr, nullptr, true},
    {"or", "KwOr", "Or", nullptr, nullptr, true},
    {"not", "KwNot", nullptr, "Not", nullptr, true},
    {"true", "KwTrue", nullptr, nullptr, nullptr, true},
    {"false", "KwFalse", nullptr, nullptr, nullptr, true},

    // Operators / punctuation
    {"||", "OrOr", "Or", nullptr, nullptr, false},
    {"&&", "AndAnd", "And", nullptr, nullptr, false},
    {"!", "Bang", nullptr, "Not", nullptr, false},
    {"+", "Plus", "Add", nullptr, nullptr, false},
    {"-", "Minus", "Sub", "Neg", nullptr, false},
    {"*", "Star", "Mul", nullptr, nullptr, false},
    {"/", "Slash", "Div", nullptr, nullptr, false},
    {"^", "Caret", "Pow", nullptr, nullptr, false},
    {"=", "Assign", nullptr, nullptr, nullptr, false},
    {":", "Colon", nullptr, nullptr, nullptr, false},
    {"?", "Question", nullptr, nullptr, nullptr, false},
    {".", "Dot", nullptr, nullptr, nullptr, false},
    {",", "Comma", nullptr, nullptr, nullptr, false},
    {"_", "Underscore", nullptr, nullptr, nullptr, false},
    {"(", "LParen", nullptr, nullptr, nullptr, false},
    {")", "RParen", nullptr, nullptr, nullptr, false},
    {"[", "LBracket", nullptr, nullptr, nullptr, false},
    {"]", "RBracket", nullptr, nullptr, nullptr, false},
    {"{", "LBrace", nullptr, nullptr, nullptr, false},
    {"}", "RBrace", nullptr, nullptr, nullptr, false},
    {"|", "Pipe", nullptr, nullptr, nullptr, false},
    {">", "Gt", nullptr, nullptr, "Gt", false},
    {"<", "Lt", nullptr, nullptr, "Lt", false},
    {">=", "Ge", nullptr, nullptr, "Ge", false},
    {"<=", "Le", nullptr, nullptr, "Le", false},
    {"==", "EqEq", nullptr, nullptr, "Eq", false},
    {"!=", "Ne", nullptr, nullptr, "Ne", false},
    {"~=", "TildeEq", nullptr, nullptr, "ApproxEq", false},
    {"[]", "BandIncl", nullptr, nullptr, nullptr, false},
    {"][", "BandExcl", nullptr, nullptr, nullptr, false},
    {"+-", "PlusMinus", nullptr, nullptr, nullptr, false},
};

bool is_word(const std::string& s) {
  if (s.empty()) return false;
  return std::isalpha(static_cast<unsigned char>(s[0])) != 0;
}

bool all_lits(const std::vector<Seq>& alts, std::vector<std::string>& ops) {
  ops.clear();
  for (const auto& alt : alts) {
    if (alt.terms.size() != 1 || alt.terms[0].kind != TermKind::Literal) {
      return false;
    }
    ops.push_back(alt.terms[0].text);
  }
  return !ops.empty();
}

void collect_groups(const Term& t, std::vector<std::vector<std::string>>& groups) {
  if (t.kind == TermKind::Group || t.kind == TermKind::Optional ||
      t.kind == TermKind::Repeat) {
    std::vector<std::string> ops;
    if (all_lits(t.group, ops)) groups.push_back(std::move(ops));
    for (const auto& alt : t.group) {
      for (const auto& inner : alt.terms) collect_groups(inner, groups);
    }
  }
}

void collect_groups(const Production& p,
                    std::vector<std::vector<std::string>>& groups) {
  std::vector<std::string> ops;
  if (all_lits(p.alts, ops)) groups.push_back(std::move(ops));
  for (const auto& alt : p.alts) {
    for (const auto& t : alt.terms) collect_groups(t, groups);
  }
}

std::string lower_copy(std::string s) {
  for (char& c : s) {
    c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  }
  return s;
}

}  // namespace

const LitBind* lookup_lit(const std::string& lit) {
  for (const auto& b : kCatalog) {
    if (b.lit == lit) return &b;
  }
  return nullptr;
}

bool resolve_synonyms(const Grammar& g, std::vector<Synonym>& out,
                      std::string& error) {
  out.clear();
  error.clear();
  std::vector<std::vector<std::string>> groups;
  for (const auto& p : g.prods) collect_groups(p, groups);

  std::unordered_set<std::string> seen;
  for (const auto& group : groups) {
    const LitBind* inherit = nullptr;
    for (const auto& lit : group) {
      const LitBind* b = lookup_lit(lit);
      if (b && b->keyword && b->tok) {
        inherit = b;
        break;
      }
    }
    for (const auto& lit : group) {
      if (lookup_lit(lit)) continue;
      if (!is_word(lit)) {
        error = "unknown symbolic literal \"" + lit +
                "\" — new punctuation needs a lexer token; word synonyms "
                "inherit from a sibling keyword";
        return false;
      }
      if (!inherit) {
        error = "unknown keyword \"" + lit +
                "\" has no known sibling to inherit a TokKind from";
        return false;
      }
      if (!seen.insert(lit).second) continue;
      Synonym s;
      s.lit = lower_copy(lit);
      s.tok = inherit->tok;
      if (inherit->bin) s.bin = inherit->bin;
      if (inherit->un) s.un = inherit->un;
      out.push_back(std::move(s));
    }
  }
  return true;
}

bool emit_keyword_synonyms(const Grammar& g, std::string& out,
                           std::string& error) {
  std::vector<Synonym> syns;
  if (!resolve_synonyms(g, syns, error)) return false;
  std::ostringstream os;
  os << "// Generated by adl2_rdgen from grammar.ebnf. Do not edit.\n";
  os << "// Extra lexer keyword entries (sibling synonyms). Included from\n";
  os << "// lexer.cpp inside the static keyword map.\n";
  for (const auto& s : syns) {
    os << "      {\"" << s.lit << "\", TokKind::" << s.tok << "},\n";
  }
  out = os.str();
  return true;
}

}  // namespace adl2::rdgen
