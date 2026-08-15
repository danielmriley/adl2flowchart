#include "adl2/rdgen/ebnf.hpp"

#include <cctype>
#include <sstream>

namespace adl2::rdgen {
namespace {

struct Parser {
  std::string_view src;
  std::size_t i = 0;
  int line = 1;
  int col = 1;
  std::string error;
  int error_line = 0;
  int error_col = 0;

  char peek() const { return i < src.size() ? src[i] : '\0'; }

  void bump() {
    if (i >= src.size()) return;
    if (src[i] == '\n') {
      ++line;
      col = 1;
    } else {
      ++col;
    }
    ++i;
  }

  void fail(const char* msg) {
    if (!error.empty()) return;
    error = msg;
    error_line = line;
    error_col = col;
  }

  void skip() {
    for (;;) {
      while (i < src.size() &&
             std::isspace(static_cast<unsigned char>(src[i]))) {
        bump();
      }
      if (i + 1 < src.size() && src[i] == '(' && src[i + 1] == '*') {
        bump();
        bump();
        while (i + 1 < src.size() && !(src[i] == '*' && src[i + 1] == ')')) {
          bump();
        }
        if (i + 1 >= src.size()) {
          fail("unterminated (* comment *)");
          return;
        }
        bump();
        bump();
        continue;
      }
      break;
    }
  }

  bool take(char c) {
    skip();
    if (peek() != c) return false;
    bump();
    return true;
  }

  bool at_ident() {
    skip();
    const unsigned char c = static_cast<unsigned char>(peek());
    return std::isalpha(c) != 0;
  }

  bool at_string() {
    skip();
    return peek() == '"';
  }

  bool at_term() {
    skip();
    const char c = peek();
    return at_ident() || at_string() || c == '(' || c == '{' || c == '[';
  }

  std::string ident() {
    skip();
    if (!at_ident()) {
      fail("expected identifier");
      return {};
    }
    std::string out;
    while (i < src.size()) {
      const unsigned char c = static_cast<unsigned char>(src[i]);
      if (std::isalnum(c) || c == '-') {
        out.push_back(src[i]);
        bump();
      } else {
        break;
      }
    }
    return out;
  }

  std::string string_lit() {
    skip();
    if (peek() != '"') {
      fail("expected string literal");
      return {};
    }
    bump();
    std::string out;
    while (i < src.size() && src[i] != '"') {
      if (src[i] == '\n') {
        fail("unterminated string literal");
        return {};
      }
      out.push_back(src[i]);
      bump();
    }
    if (peek() != '"') {
      fail("unterminated string literal");
      return {};
    }
    bump();
    return out;
  }

  std::vector<Seq> expression();

  Term term() {
    Term t;
    if (take('{')) {
      t.kind = TermKind::Repeat;
      t.group = expression();
      if (!take('}')) fail("expected '}' after repetition");
      return t;
    }
    if (take('[')) {
      t.kind = TermKind::Optional;
      t.group = expression();
      if (!take(']')) fail("expected ']' after optional");
      return t;
    }
    if (take('(')) {
      t.kind = TermKind::Group;
      t.group = expression();
      if (!take(')')) fail("expected ')' after group");
      return t;
    }
    if (at_string()) {
      t.kind = TermKind::Literal;
      t.text = string_lit();
      return t;
    }
    t.kind = TermKind::Name;
    t.text = ident();
    return t;
  }

  Seq sequence() {
    Seq s;
    while (at_term()) {
      s.terms.push_back(term());
      if (!error.empty()) break;
    }
    return s;
  }
};

std::vector<Seq> Parser::expression() {
  std::vector<Seq> alts;
  alts.push_back(sequence());
  while (take('|')) alts.push_back(sequence());
  return alts;
}

std::string format_term(const Term& t) {
  switch (t.kind) {
    case TermKind::Literal:
      return "\"" + t.text + "\"";
    case TermKind::Name:
      return t.text;
    case TermKind::Group:
    case TermKind::Optional:
    case TermKind::Repeat: {
      std::string inner;
      for (std::size_t i = 0; i < t.group.size(); ++i) {
        if (i) inner += " | ";
        inner += format_seq(t.group[i]);
      }
      if (t.kind == TermKind::Group) return "(" + inner + ")";
      if (t.kind == TermKind::Optional) return "[" + inner + "]";
      return "{" + inner + "}";
    }
  }
  return "?";
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

bool all_names(const std::vector<Seq>& alts) {
  if (alts.empty()) return false;
  for (const auto& alt : alts) {
    if (alt.terms.size() != 1 || alt.terms[0].kind != TermKind::Name) {
      return false;
    }
  }
  return true;
}

}  // namespace

const Production* Grammar::find(std::string_view name) const {
  for (const auto& p : prods) {
    if (p.name == name) return &p;
  }
  return nullptr;
}

Grammar parse_ebnf(std::string_view src) {
  Parser p;
  p.src = src;
  Grammar g;
  p.skip();
  while (p.i < src.size() && p.error.empty()) {
    p.skip();
    if (p.i >= src.size()) break;
    Production prod;
    prod.line = p.line;
    prod.name = p.ident();
    if (!p.error.empty()) break;
    if (!p.take('=')) {
      p.fail("expected '=' after production name");
      break;
    }
    prod.alts = p.expression();
    if (!p.error.empty()) break;
    if (!p.take(';')) {
      p.fail("expected ';' after production");
      break;
    }
    g.prods.push_back(std::move(prod));
    p.skip();
  }
  if (!p.error.empty()) {
    g.error = p.error;
    g.error_line = p.error_line;
    g.error_col = p.error_col;
  }
  return g;
}

std::string hyphen_to_underscore(std::string_view name) {
  std::string out(name);
  for (char& c : out) {
    if (c == '-') c = '_';
  }
  return out;
}

std::string parse_method_name(std::string_view ebnf_name) {
  return "parse_" + hyphen_to_underscore(ebnf_name);
}

std::string format_seq(const Seq& seq) {
  std::string out;
  for (std::size_t i = 0; i < seq.terms.size(); ++i) {
    if (i) out += " ";
    out += format_term(seq.terms[i]);
  }
  return out;
}

std::string format_production(const Production& p) {
  std::ostringstream os;
  os << p.name << " = ";
  for (std::size_t i = 0; i < p.alts.size(); ++i) {
    if (i) os << " | ";
    os << format_seq(p.alts[i]);
  }
  os << " ;";
  return os.str();
}

const char* shape_name(Shape s) {
  switch (s) {
    case Shape::Alias:
      return "Alias";
    case Shape::LeftAssoc:
      return "LeftAssoc";
    case Shape::PrefixUnary:
      return "PrefixUnary";
    case Shape::OptionalSuffix:
      return "OptionalSuffix";
    case Shape::Choice:
      return "Choice";
    case Shape::KeywordSeq:
      return "KeywordSeq";
    case Shape::TokenClass:
      return "TokenClass";
    case Shape::Empty:
      return "Empty";
    case Shape::Other:
      return "Other";
  }
  return "Other";
}

ShapeInfo classify(const Production& p) {
  ShapeInfo info;
  if (p.alts.size() == 1 && p.alts[0].terms.empty()) {
    info.shape = Shape::Empty;
    return info;
  }
  if (p.alts.size() == 1 && p.alts[0].terms.size() == 1 &&
      p.alts[0].terms[0].kind == TermKind::Name) {
    info.shape = Shape::Alias;
    info.next = p.alts[0].terms[0].text;
    return info;
  }
  if (all_lits(p.alts, info.ops)) {
    info.shape = Shape::TokenClass;
    return info;
  }
  if (all_names(p.alts)) {
    info.shape = Shape::Choice;
    return info;
  }

  // A = B { (op|op) B }
  if (p.alts.size() == 1 && p.alts[0].terms.size() == 2) {
    const Term& head = p.alts[0].terms[0];
    const Term& rep = p.alts[0].terms[1];
    if (head.kind == TermKind::Name && rep.kind == TermKind::Repeat &&
        rep.group.size() == 1 && rep.group[0].terms.size() == 2) {
      const Term& ops = rep.group[0].terms[0];
      const Term& again = rep.group[0].terms[1];
      if (ops.kind == TermKind::Group && again.kind == TermKind::Name &&
          again.text == head.text && all_lits(ops.group, info.ops)) {
        info.shape = Shape::LeftAssoc;
        info.next = head.text;
        return info;
      }
    }
  }

  // A = (op|op) A | B
  if (p.alts.size() == 2 && p.alts[0].terms.size() == 2 &&
      p.alts[1].terms.size() == 1 && p.alts[1].terms[0].kind == TermKind::Name) {
    const Term& ops = p.alts[0].terms[0];
    const Term& self = p.alts[0].terms[1];
    if (ops.kind == TermKind::Group && self.kind == TermKind::Name &&
        self.text == p.name && all_lits(ops.group, info.ops)) {
      info.shape = Shape::PrefixUnary;
      info.next = p.alts[1].terms[0].text;
      return info;
    }
    // unary = "-" unary | postfix  (single literal, not a group)
    if (ops.kind == TermKind::Literal && self.kind == TermKind::Name &&
        self.text == p.name) {
      info.shape = Shape::PrefixUnary;
      info.ops = {ops.text};
      info.next = p.alts[1].terms[0].text;
      return info;
    }
  }

  // A = B [ ... ]
  if (p.alts.size() == 1 && p.alts[0].terms.size() == 2 &&
      p.alts[0].terms[0].kind == TermKind::Name &&
      p.alts[0].terms[1].kind == TermKind::Optional) {
    info.shape = Shape::OptionalSuffix;
    info.next = p.alts[0].terms[0].text;
    return info;
  }

  if (!p.alts.empty() && !p.alts[0].terms.empty() &&
      p.alts[0].terms[0].kind == TermKind::Literal) {
    info.shape = Shape::KeywordSeq;
    return info;
  }
  // ("define"|"def") ident … — group of lits first
  if (!p.alts.empty() && !p.alts[0].terms.empty() &&
      p.alts[0].terms[0].kind == TermKind::Group) {
    std::vector<std::string> dummy;
    if (all_lits(p.alts[0].terms[0].group, dummy)) {
      info.shape = Shape::KeywordSeq;
      info.ops = dummy;
      return info;
    }
  }

  info.shape = Shape::Other;
  return info;
}

}  // namespace adl2::rdgen
