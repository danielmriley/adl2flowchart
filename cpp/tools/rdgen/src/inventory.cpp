#include "adl2/rdgen/inventory.hpp"

#include <algorithm>
#include <cctype>
#include <sstream>
#include <unordered_map>
#include <unordered_set>
#include <utility>

namespace adl2::rdgen {
namespace {

struct Pin {
  const char* lit;
  LitClass cls;
  const char* note;
};

/// Closed pin of known spellings. New EBNF quotes not listed here stay
/// Unclassified and fail the inventory. Sibling inherit is identity's job.
///
/// Multi-role words share one pin (primary class + note). Do not treat
/// a second role as an error.
const Pin kPins[] = {
    // section words
    {"info", LitClass::SectionKw, ""},
    {"table", LitClass::SectionKw, ""},
    {"countsformat", LitClass::SectionKw, ""},
    {"define", LitClass::SectionKw,
     "roles: section-kw, region-stmt-kw (object-define / at_stmt_keyword)"},
    {"def", LitClass::SectionKw,
     "roles: section-kw, region-stmt-kw (object-define / at_stmt_keyword)"},
    {"object", LitClass::SectionKw, ""},
    {"obj", LitClass::SectionKw, ""},
    {"composite", LitClass::SectionKw, ""},
    {"trigger", LitClass::SectionKw,
     "roles: section-kw (object-block), region-stmt-kw, weight-stmt target"},
    {"region", LitClass::SectionKw, ""},
    {"algo", LitClass::SectionKw, ""},
    {"histoList", LitClass::SectionKw, ""},

    // region-stmt words
    {"select", LitClass::RegionStmtKw, ""},
    {"cut", LitClass::RegionStmtKw, ""},
    {"cmd", LitClass::RegionStmtKw, ""},
    {"command", LitClass::RegionStmtKw, ""},
    {"reject", LitClass::RegionStmtKw, ""},
    {"bin", LitClass::RegionStmtKw, ""},
    {"weight", LitClass::RegionStmtKw, ""},
    {"histo", LitClass::RegionStmtKw, ""},
    {"save", LitClass::RegionStmtKw, ""},
    {"counts", LitClass::RegionStmtKw, ""},
    {"print", LitClass::RegionStmtKw, ""},
    {"sort", LitClass::RegionStmtKw,
     "roles: region-stmt-kw; hook consumed to end of statement"},

    // object-stmt words
    {"take", LitClass::ObjectStmtKw, ""},
    {"using", LitClass::ObjectStmtKw, ""},

    // expr-op words vs symbolic aliases (aliases.txt: ||→or, &&→and, !→not)
    {"and", LitClass::ExprOpWord, ""},
    {"or", LitClass::ExprOpWord, ""},
    {"not", LitClass::ExprOpWord, ""},

    // primaries — all/none are lexer extras (not in frozen EBNF)
    {"all", LitClass::PrimaryKw, "lexer extra; not in EBNF"},
    {"none", LitClass::PrimaryKw, "lexer extra; not in EBNF"},
    {"true", LitClass::PrimaryKw, "roles: primary-kw, table-block errors"},
    {"false", LitClass::PrimaryKw, "roles: primary-kw, table-block errors"},

    // contextual: EBNF quotes it; MUST NOT be a keyword class
    {"bins", LitClass::ContextualIdent,
     "contextual; lexed as Ident, not a keyword"},

    // other keywords
    {"tabletype", LitClass::OtherKw, ""},
    {"nvars", LitClass::OtherKw, ""},
    {"errors", LitClass::OtherKw, ""},
    {"process", LitClass::OtherKw, ""},
    {"union", LitClass::OtherKw, ""},

    // symbolic ops / punctuation (including two-char lexer tokens)
    {"||", LitClass::SymbolicOp, "alias of or"},
    {"&&", LitClass::SymbolicOp, "alias of and"},
    {"!", LitClass::SymbolicOp, "alias of not"},
    {"+", LitClass::SymbolicOp, ""},
    {"-", LitClass::SymbolicOp, ""},
    {"*", LitClass::SymbolicOp, ""},
    {"/", LitClass::SymbolicOp, ""},
    {"^", LitClass::SymbolicOp, ""},
    {"=", LitClass::SymbolicOp, ""},
    {":", LitClass::SymbolicOp, ""},
    {"?", LitClass::SymbolicOp, ""},
    {".", LitClass::SymbolicOp, ""},
    {",", LitClass::SymbolicOp, ""},
    {"_", LitClass::SymbolicOp, ""},
    {"(", LitClass::SymbolicOp, ""},
    {")", LitClass::SymbolicOp, ""},
    {"[", LitClass::SymbolicOp, ""},
    {"]", LitClass::SymbolicOp, ""},
    {"{", LitClass::SymbolicOp, ""},
    {"}", LitClass::SymbolicOp, ""},
    {"|", LitClass::SymbolicOp, ""},
    {">", LitClass::SymbolicOp, ""},
    {"<", LitClass::SymbolicOp, ""},
    {">=", LitClass::SymbolicOp, ""},
    {"<=", LitClass::SymbolicOp, ""},
    {"==", LitClass::SymbolicOp, ""},
    {"!=", LitClass::SymbolicOp, ""},
    {"~=", LitClass::SymbolicOp, ""},
    {"[]", LitClass::SymbolicOp, ""},
    {"][", LitClass::SymbolicOp, ""},
    {"+-", LitClass::SymbolicOp, ""},

    // lexer-only extra (not in EBNF)
    {"->", LitClass::Extra, "lexer Arrow; not in EBNF"},
};

/// Extra roles beyond the primary pin. One row per spelling.
const std::pair<const char*, LitClass> kExtraRoles[] = {
    {"define", LitClass::RegionStmtKw},
    {"def", LitClass::RegionStmtKw},
    {"trigger", LitClass::RegionStmtKw},
    {"sort", LitClass::RegionStmtKw},  // hook + stmt-keyword
};

const Pin* find_pin(const std::string& lit) {
  for (const auto& p : kPins) {
    if (p.lit == lit) return &p;
  }
  return nullptr;
}

bool is_word(const std::string& s) {
  return !s.empty() && std::isalpha(static_cast<unsigned char>(s[0])) != 0;
}

/// Production → class for an *unpinned* word. Used so a new sibling
/// (`xor` in or-expr, `sel` in cut-stmt) gets a token-class without
/// inheriting TokKind / BinOp (identity owns meaning). Unknown
/// productions and unknown punctuation stay Unclassified.
LitClass class_from_prod(const std::string& prod) {
  if (prod == "info-block" || prod == "define" || prod == "object-block" ||
      prod == "region-block") {
    return LitClass::SectionKw;
  }
  if (prod == "cut-stmt" || prod == "reject-stmt" || prod == "bin-stmt" ||
      prod == "trigger-stmt" || prod == "histo-stmt" || prod == "weight-stmt" ||
      prod == "print-stmt" || prod == "save-stmt" || prod == "counts-stmt" ||
      prod == "sort-stmt") {
    return LitClass::RegionStmtKw;
  }
  if (prod == "take-stmt") return LitClass::ObjectStmtKw;
  if (prod == "or-expr" || prod == "and-expr" || prod == "not-expr") {
    return LitClass::ExprOpWord;
  }
  if (prod == "take-source") return LitClass::OtherKw;
  return LitClass::Unclassified;
}

LitClass infer_word_class(const std::vector<std::string>& sites,
                          std::vector<LitClass>& roles) {
  roles.clear();
  for (const auto& prod : sites) {
    const LitClass c = class_from_prod(prod);
    if (c == LitClass::Unclassified) continue;
    if (std::find(roles.begin(), roles.end(), c) == roles.end()) {
      roles.push_back(c);
    }
  }
  if (roles.empty()) return LitClass::Unclassified;
  return roles.front();
}

void walk_term(const Term& t, const std::string& prod,
               std::vector<std::string>& order,
               std::unordered_set<std::string>& seen,
               std::unordered_map<std::string, std::vector<std::string>>& sites) {
  if (t.kind == TermKind::Literal) {
    if (seen.insert(t.text).second) order.push_back(t.text);
    auto& dest = sites[t.text];
    if (std::find(dest.begin(), dest.end(), prod) == dest.end()) {
      dest.push_back(prod);
    }
    return;
  }
  if (t.kind == TermKind::Group || t.kind == TermKind::Optional ||
      t.kind == TermKind::Repeat) {
    for (const auto& alt : t.group) {
      for (const auto& inner : alt.terms) {
        walk_term(inner, prod, order, seen, sites);
      }
    }
  }
}

std::string join_sites(const std::vector<std::string>& sites) {
  std::ostringstream os;
  for (std::size_t i = 0; i < sites.size(); ++i) {
    if (i) os << ", ";
    os << sites[i];
  }
  return os.str();
}

LitClassRow make_row(const std::string& lit, const Pin* pin) {
  LitClassRow row;
  row.lit = lit;
  if (!pin) {
    row.cls = LitClass::Unclassified;
    row.roles.push_back(LitClass::Unclassified);
    return row;
  }
  row.cls = pin->cls;
  row.note = pin->note;
  row.roles.push_back(pin->cls);
  for (const auto& extra : kExtraRoles) {
    if (extra.first != lit) continue;
    if (std::find(row.roles.begin(), row.roles.end(), extra.second) ==
        row.roles.end()) {
      row.roles.push_back(extra.second);
    }
  }
  return row;
}

void append_site_note(LitClassRow& row,
                      const std::vector<std::string>& sites) {
  if (sites.size() < 2) return;
  const std::string extra = "seen in: " + join_sites(sites);
  if (row.note.empty()) {
    row.note = extra;
  } else if (row.note.find("seen in:") == std::string::npos) {
    row.note += "; ";
    row.note += extra;
  }
}

}  // namespace

const char* lit_class_name(LitClass c) {
  switch (c) {
    case LitClass::SectionKw:
      return "section-kw";
    case LitClass::RegionStmtKw:
      return "region-stmt-kw";
    case LitClass::ObjectStmtKw:
      return "object-stmt-kw";
    case LitClass::ExprOpWord:
      return "expr-op-word";
    case LitClass::PrimaryKw:
      return "primary-kw";
    case LitClass::OtherKw:
      return "other-kw";
    case LitClass::ContextualIdent:
      return "contextual-ident";
    case LitClass::SymbolicOp:
      return "symbolic-op";
    case LitClass::Extra:
      return "extra";
    case LitClass::Unclassified:
      return "unclassified";
  }
  return "unclassified";
}

bool is_keyword_class(LitClass c) {
  switch (c) {
    case LitClass::SectionKw:
    case LitClass::RegionStmtKw:
    case LitClass::ObjectStmtKw:
    case LitClass::ExprOpWord:
    case LitClass::PrimaryKw:
    case LitClass::OtherKw:
      return true;
    case LitClass::ContextualIdent:
    case LitClass::SymbolicOp:
    case LitClass::Extra:
    case LitClass::Unclassified:
      return false;
  }
  return false;
}

const LitClassRow* find_lit(const Inventory& inv, std::string_view lit) {
  for (const auto& row : inv.rows) {
    if (row.lit == lit) return &row;
  }
  return nullptr;
}

std::string format_inventory(const Inventory& inv) {
  std::ostringstream os;
  for (const auto& row : inv.rows) {
    os << row.lit << '\t' << lit_class_name(row.cls);
    if (!row.note.empty()) os << '\t' << row.note;
    os << '\n';
  }
  return os.str();
}

bool build_inventory(const Grammar& g, Inventory& out, std::string& error) {
  out = Inventory{};
  error.clear();

  std::vector<std::string> order;
  std::unordered_set<std::string> seen;
  std::unordered_map<std::string, std::vector<std::string>> sites;
  for (const auto& p : g.prods) {
    for (const auto& alt : p.alts) {
      for (const auto& t : alt.terms) {
        walk_term(t, p.name, order, seen, sites);
      }
    }
  }

  // Lexer extras that the frozen EBNF does not quote. path-token has no
  // literal — do not invent a path-token lexer rule; skip it.
  static const char* kLexerExtras[] = {"all", "none", "->"};
  for (const char* extra : kLexerExtras) {
    if (seen.insert(extra).second) order.push_back(extra);
  }

  for (const auto& lit : order) {
    const Pin* pin = find_pin(lit);
    LitClassRow row = make_row(lit, pin);
    auto it = sites.find(lit);
    if (!pin && it != sites.end() && is_word(lit)) {
      std::vector<LitClass> inferred;
      const LitClass cls = infer_word_class(it->second, inferred);
      if (cls != LitClass::Unclassified) {
        row.cls = cls;
        row.roles = std::move(inferred);
        row.note = "unpinned; classified from " + join_sites(it->second);
      }
    }
    if (it != sites.end()) append_site_note(row, it->second);
    if (row.cls == LitClass::Unclassified && sites.count(lit)) {
      out.errors.push_back("unclassified EBNF literal \"" + lit + "\"");
    }
    out.rows.push_back(std::move(row));
  }

  if (const LitClassRow* bins = find_lit(out, "bins")) {
    if (is_keyword_class(bins->cls)) {
      out.errors.push_back(
          "contextual ident \"bins\" must not be a keyword class (got " +
          std::string(lit_class_name(bins->cls)) + ")");
    }
  }

  if (!out.errors.empty()) {
    if (out.errors.size() == 1) {
      error = out.errors.front();
    } else {
      error = std::to_string(out.errors.size()) + " inventory errors";
    }
    return false;
  }
  return true;
}

}  // namespace adl2::rdgen
