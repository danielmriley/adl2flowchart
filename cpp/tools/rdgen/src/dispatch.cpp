#include "adl2/rdgen/emit.hpp"

#include "adl2/rdgen/literals.hpp"

#include <cctype>
#include <sstream>
#include <unordered_set>
#include <utility>

namespace adl2::rdgen {
namespace {

std::string lower_copy(std::string s) {
  for (char& c : s) {
    c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  }
  return s;
}

bool is_word(const std::string& s) {
  return !s.empty() && std::isalpha(static_cast<unsigned char>(s[0])) != 0;
}

enum class FirstKind { Tok, IdentText, AnyIdent };

struct FirstItem {
  FirstKind kind = FirstKind::Tok;
  std::string tok;
  std::string text;
};

bool same_item(const FirstItem& a, const FirstItem& b) {
  if (a.kind != b.kind) return false;
  if (a.kind == FirstKind::Tok) return a.tok == b.tok;
  if (a.kind == FirstKind::IdentText) return a.text == b.text;
  return true;
}

void add_item(std::vector<FirstItem>& items, FirstItem it) {
  for (const auto& e : items) {
    if (same_item(e, it)) return;
  }
  items.push_back(std::move(it));
}

bool bind_first_lit(const std::string& lit, FirstItem& out, std::string& error) {
  if (const LitBind* b = lookup_lit(lit)) {
    if (b->tok && b->tok[0] != '\0') {
      out = FirstItem{FirstKind::Tok, b->tok, {}};
      return true;
    }
    if (is_word(lit)) {
      out = FirstItem{FirstKind::IdentText, {}, lower_copy(lit)};
      return true;
    }
    error = "literal \"" + lit + "\" has no TokKind and is not a word";
    return false;
  }
  if (const Alias* a = lookup_alias(lit)) {
    return bind_first_lit(a->canonical, out, error);
  }
  if (is_word(lit)) {
    out = FirstItem{FirstKind::IdentText, {}, lower_copy(lit)};
    return true;
  }
  error = "unknown first-set literal \"" + lit + "\"";
  return false;
}

bool term_nullable(const Term& t) {
  return t.kind == TermKind::Optional || t.kind == TermKind::Repeat;
}

bool first_of_seq(const Grammar& g, const Seq& seq, std::vector<FirstItem>& out,
                  std::string& error, std::unordered_set<std::string>& visiting);

bool first_of_prod(const Grammar& g, const Production& p,
                   std::vector<FirstItem>& out, std::string& error,
                   std::unordered_set<std::string>& visiting);

bool first_of_term(const Grammar& g, const Term& t, std::vector<FirstItem>& out,
                   std::string& error, std::unordered_set<std::string>& visiting) {
  if (t.kind == TermKind::Literal) {
    FirstItem it;
    if (!bind_first_lit(t.text, it, error)) return false;
    add_item(out, std::move(it));
    return true;
  }
  if (t.kind == TermKind::Name) {
    if (const Production* p = g.find(t.text)) {
      return first_of_prod(g, *p, out, error, visiting);
    }
    if (t.text == "ident") {
      add_item(out, FirstItem{FirstKind::AnyIdent, {}, {}});
      return true;
    }
    if (t.text == "string") {
      add_item(out, FirstItem{FirstKind::Tok, "String", {}});
      return true;
    }
    if (t.text == "integer") {
      add_item(out, FirstItem{FirstKind::Tok, "Int", {}});
      return true;
    }
    if (t.text == "number") {
      add_item(out, FirstItem{FirstKind::Tok, "Int", {}});
      add_item(out, FirstItem{FirstKind::Tok, "Real", {}});
      return true;
    }
    if (t.text == "EOF") {
      add_item(out, FirstItem{FirstKind::Tok, "Eof", {}});
      return true;
    }
    error = "unknown first-set name '" + t.text + "'";
    return false;
  }
  if (t.kind == TermKind::Group || t.kind == TermKind::Optional ||
      t.kind == TermKind::Repeat) {
    for (const auto& alt : t.group) {
      if (!first_of_seq(g, alt, out, error, visiting)) return false;
    }
    return true;
  }
  return true;
}

bool first_of_seq(const Grammar& g, const Seq& seq, std::vector<FirstItem>& out,
                  std::string& error, std::unordered_set<std::string>& visiting) {
  for (const auto& t : seq.terms) {
    if (!first_of_term(g, t, out, error, visiting)) return false;
    if (!term_nullable(t)) return true;
  }
  return true;
}

bool first_of_prod(const Grammar& g, const Production& p,
                   std::vector<FirstItem>& out, std::string& error,
                   std::unordered_set<std::string>& visiting) {
  if (!visiting.insert(p.name).second) {
    error = "first-set cycle at '" + p.name + "'";
    return false;
  }
  for (const auto& alt : p.alts) {
    if (!first_of_seq(g, alt, out, error, visiting)) {
      visiting.erase(p.name);
      return false;
    }
  }
  visiting.erase(p.name);
  return true;
}

bool first_set(const Grammar& g, const std::string& name,
               std::vector<FirstItem>& out, std::string& error) {
  out.clear();
  const Production* p = g.find(name);
  if (!p) {
    error = "no production '" + name + "'";
    return false;
  }
  std::unordered_set<std::string> visiting;
  return first_of_prod(g, *p, out, error, visiting);
}

const MapEntry* lookup_entry(const MethodMap& map, const std::string& name) {
  for (const auto& e : map.entries) {
    if (e.name == name) return &e;
  }
  return nullptr;
}

void emit_or(std::ostringstream& os, const std::vector<FirstItem>& items) {
  bool first = true;
  for (const auto& it : items) {
    if (it.kind == FirstKind::AnyIdent) continue;
    if (!first) os << " || ";
    first = false;
    if (it.kind == FirstKind::Tok) {
      os << "check(TokKind::" << it.tok << ")";
    } else {
      os << "(check(TokKind::Ident) && iequals(peek().text, \"" << it.text
         << "\"))";
    }
  }
}

bool has_usable_item(const std::vector<FirstItem>& items) {
  for (const auto& it : items) {
    if (it.kind != FirstKind::AnyIdent) return true;
  }
  return false;
}

void add_tok(std::vector<FirstItem>& items, const char* tok) {
  add_item(items, FirstItem{FirstKind::Tok, tok, {}});
}

void emit_pred(std::ostringstream& os, const char* method,
               const std::vector<FirstItem>& items) {
  os << "bool Parser::" << method << "() const {\n";
  os << "  return ";
  emit_or(os, items);
  os << ";\n";
  os << "}\n\n";
}

std::vector<std::string> choice_names(const Production& p) {
  std::vector<std::string> names;
  for (const auto& alt : p.alts) {
    if (alt.terms.size() == 1 && alt.terms[0].kind == TermKind::Name) {
      names.push_back(alt.terms[0].text);
    }
  }
  return names;
}

bool emit_inline_cut(std::ostringstream& os, const std::string& word) {
  os << "  if (check(TokKind::Ident) && iequals(peek().text, \"" << word
     << "\")) {\n";
  os << "    Token kw_tok = advance();\n";
  os << "    RegionStmt st;\n";
  os << "    st.kind = RegionStmt::Kind::Cut;\n";
  os << "    st.keyword = lower_copy(kw_tok.text);\n";
  os << "    st.cond = parse_condition();\n";
  os << "    st.span = kw_tok.span.to(last_span_);\n";
  os << "    return st;\n";
  os << "  }\n";
  return true;
}

}  // namespace

bool emit_dispatch(const Grammar& g, const MethodMap& map, std::string& out,
                   std::string& error) {
  error.clear();
  const Production* section = g.find("section");
  const Production* region_stmt = g.find("region-stmt");
  const Production* cut_stmt = g.find("cut-stmt");
  const Production* reject_stmt = g.find("reject-stmt");
  if (!section || !region_stmt || !cut_stmt || !reject_stmt) {
    error = "dispatch emit needs section, region-stmt, cut-stmt, reject-stmt";
    return false;
  }
  if (classify(*section).shape != Shape::Choice) {
    error = "section must classify as Choice";
    return false;
  }
  if (classify(*region_stmt).shape != Shape::Choice) {
    error = "region-stmt must classify as Choice";
    return false;
  }

  std::vector<FirstItem> section_first;
  std::vector<FirstItem> stmt_first;
  std::vector<FirstItem> cut_first;
  std::vector<FirstItem> reject_first;
  if (!first_set(g, "section", section_first, error)) return false;
  if (!first_set(g, "region-stmt", stmt_first, error)) return false;
  if (!first_set(g, "cut-stmt", cut_first, error)) return false;
  if (!first_set(g, "reject-stmt", reject_first, error)) return false;

  // Recovery extras that are not in the region-stmt Choice (object-define
  // and take/using as a region-ref prefix). Keep them when generating.
  add_tok(stmt_first, "KwTake");
  add_tok(stmt_first, "KwUsing");
  add_tok(stmt_first, "KwDefine");
  add_tok(stmt_first, "KwDef");

  std::vector<FirstItem> stmt_pred;
  for (const auto& it : stmt_first) {
    if (it.kind == FirstKind::AnyIdent) continue;
    if (it.kind == FirstKind::IdentText && it.text == "bins") continue;
    add_item(stmt_pred, it);
  }

  if (!has_usable_item(section_first) || !has_usable_item(stmt_pred) ||
      !has_usable_item(cut_first) || !has_usable_item(reject_first)) {
    error = "empty first-set for a dispatch predicate";
    return false;
  }

  std::ostringstream os;
  os << "// Generated by adl2_rdgen from grammar.ebnf. Do not edit.\n";
  os << "// Choice dispatch + first-set predicates (RDGEN.md slice 2).\n";
  os << "// Included from parser.cpp inside namespace adl2::syntax.\n\n";

  emit_pred(os, "at_section_start", section_first);
  emit_pred(os, "at_stmt_keyword", stmt_pred);
  emit_pred(os, "is_cut_keyword", cut_first);
  emit_pred(os, "is_reject_keyword", reject_first);

  os << "bool Parser::parse_section(Section& out) {\n";
  for (const auto& name : choice_names(*section)) {
    std::vector<FirstItem> fs;
    if (!first_set(g, name, fs, error)) return false;
    if (!has_usable_item(fs)) {
      error = "section alternative '" + name + "' has an empty first-set";
      return false;
    }
    const MapEntry* ent = lookup_entry(map, name);
    if (!ent) {
      error = "section alternative '" + name + "' has no method_map symbol";
      return false;
    }
    os << "  if (";
    emit_or(os, fs);
    os << ") {\n";
    os << "    out = " << ent->symbol << "();\n";
    os << "    return true;\n";
    os << "  }\n";
  }
  os << "  return false;\n";
  os << "}\n\n";

  os << "std::optional<RegionStmt> Parser::parse_region_stmt() {\n";
  std::vector<std::string> inferred;
  for (const auto& name : choice_names(*region_stmt)) {
    if (name == "region-ref") continue;
    if (name == "bin-stmt") {
      os << "  if (check(TokKind::KwBin) ||\n";
      os << "      (is_ident_text(\"bins\") && !next_is_line_end()))\n";
      os << "    return parse_bin_stmt();\n";
      continue;
    }
    const MapEntry* ent = lookup_entry(map, name);
    if (!ent) {
      std::vector<std::string> kws;
      const Production* p = g.find(name);
      if (!p || !keyword_condition_kws(*p, kws)) {
        error = "region-stmt alternative '" + name +
                "' is unmapped and not `keywords condition`";
        return false;
      }
      for (const auto& kw : kws) {
        inferred.push_back(lower_copy(kw));
      }
      continue;
    }
    std::vector<FirstItem> fs;
    if (!first_set(g, name, fs, error)) return false;
    if (!has_usable_item(fs)) {
      error = "region-stmt alternative '" + name + "' has an empty first-set";
      return false;
    }
    os << "  if (";
    emit_or(os, fs);
    os << ")\n";
    os << "    return " << ent->symbol << "();\n";
  }
  // Inferred Ident words before the generic region-ref Ident arm.
  for (const auto& word : inferred) {
    emit_inline_cut(os, word);
  }
  os << "  if (check(TokKind::KwTake) || check(TokKind::KwUsing)) {\n";
  os << "    advance();\n";
  os << "    return parse_region_ref();\n";
  os << "  }\n";
  os << "  if (check(TokKind::Ident)) return parse_region_ref();\n";
  os << "  return std::nullopt;\n";
  os << "}\n";

  out = os.str();
  return true;
}

}  // namespace adl2::rdgen
