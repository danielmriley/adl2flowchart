#include "adl2/certify/bundle.hpp"

#include "adl2/formula/lin.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"

#include <cctype>
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <map>
#include <optional>
#include <set>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace adl2::certify {
namespace {

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::sema::QuantityId;

// ---------------------------------------------------------------------------
// serde_json pretty writer (2-space indent, `: ` after keys, no trailing nl)
// ---------------------------------------------------------------------------

std::string json_escape(const std::string& s) {
  std::string out;
  out.reserve(s.size() + 8);
  for (unsigned char c : s) {
    switch (c) {
      case '"': out += "\\\""; break;
      case '\\': out += "\\\\"; break;
      case '\n': out += "\\n"; break;
      case '\r': out += "\\r"; break;
      case '\t': out += "\\t"; break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", c);
          out += buf;
        } else {
          out += static_cast<char>(c);
        }
    }
  }
  return out;
}

struct Jw {
  std::ostringstream os;
  int indent = 0;
  bool needs_comma = false;

  void pad() {
    os << '\n';
    for (int i = 0; i < indent; ++i) os << "  ";
  }
  void comma() {
    if (needs_comma) os << ',';
    needs_comma = false;
  }
  void begin_obj() {
    os << '{';
    indent++;
    needs_comma = false;
  }
  void end_obj() {
    indent--;
    if (needs_comma) pad();
    os << '}';
    needs_comma = true;
  }
  void begin_arr() {
    os << '[';
    indent++;
    needs_comma = false;
  }
  void end_arr() {
    indent--;
    if (needs_comma) pad();
    os << ']';
    needs_comma = true;
  }
  void key(const char* k) {
    comma();
    pad();
    os << '"' << k << "\": ";
  }
  void key_s(const std::string& k) {
    comma();
    pad();
    os << '"' << json_escape(k) << "\": ";
  }
  void str(const std::string& s) {
    os << '"' << json_escape(s) << '"';
    needs_comma = true;
  }
  void raw(const char* v) {
    os << v;
    needs_comma = true;
  }
  void boolean(bool v) { raw(v ? "true" : "false"); }
  void u64(std::uint64_t v) {
    os << v;
    needs_comma = true;
  }
};

void write_formula(Jw& j, const BundleFormula& f);
void write_assert(Jw& j, const BundleAssert& a);
void write_cert_node(Jw& j, const CertNode& n);

void write_qrat(Jw& j, const QRat& q) { j.str(q.to_repr()); }

void write_rel(Jw& j, Rel r) { j.str(adl2::formula::rel_str(r)); }

void write_formula(Jw& j, const BundleFormula& f) {
  j.begin_obj();
  j.key("op");
  switch (f.op) {
    case BundleFormula::Op::True:
      j.str("true");
      break;
    case BundleFormula::Op::False:
      j.str("false");
      break;
    case BundleFormula::Op::Atom:
      j.str("atom");
      j.key("terms");
      j.begin_arr();
      for (const auto& t : f.terms) {
        j.comma();
        j.pad();
        j.begin_obj();
        j.key("coeff");
        write_qrat(j, t.coeff);
        j.key("q");
        j.u64(t.q);
        j.end_obj();
      }
      j.end_arr();
      j.key("rel");
      write_rel(j, f.rel);
      j.key("k");
      write_qrat(j, f.k);
      break;
    case BundleFormula::Op::And:
      j.str("and");
      j.key("args");
      j.begin_arr();
      for (const auto& a : f.args) {
        j.comma();
        j.pad();
        write_formula(j, a);
      }
      j.end_arr();
      break;
    case BundleFormula::Op::Or:
      j.str("or");
      j.key("args");
      j.begin_arr();
      for (const auto& a : f.args) {
        j.comma();
        j.pad();
        write_formula(j, a);
      }
      j.end_arr();
      break;
  }
  j.end_obj();
}

void write_source(Jw& j, const AssertSource& s) {
  j.begin_obj();
  j.key("kind");
  switch (s.kind) {
    case AssertSource::Kind::Cut:
      j.str("cut");
      j.key("region");
      j.str(s.region);
      j.key("line");
      j.u64(s.line);
      j.key("text");
      j.str(s.text);
      j.key("whole");
      j.boolean(s.whole);
      break;
    case AssertSource::Kind::Axiom:
      j.str("axiom");
      j.key("id");
      j.str(s.id);
      j.key("statement");
      j.str(s.statement);
      j.key("assumption");
      j.str(s.assumption);
      break;
    case AssertSource::Kind::Derived:
      j.str("derived");
      j.key("fact");
      j.str(s.fact);
      break;
    case AssertSource::Kind::Query:
      j.str("query");
      j.key("role");
      j.str(s.role);
      break;
    case AssertSource::Kind::Unattributed:
      j.str("unattributed");
      break;
  }
  j.end_obj();
}

void write_assert(Jw& j, const BundleAssert& a) {
  j.begin_obj();
  j.key("name");
  j.str(a.name);
  j.key("source");
  write_source(j, a.source);
  j.key("formula");
  write_formula(j, a.formula);
  j.end_obj();
}

void write_cert_node(Jw& j, const CertNode& n) {
  if (n.kind == CertNode::Kind::Contradiction) {
    j.str("Contradiction");
    return;
  }
  j.begin_obj();
  if (n.kind == CertNode::Kind::Farkas) {
    j.key("Farkas");
    j.begin_obj();
    j.key("multipliers");
    j.begin_arr();
    for (const auto& m : n.multipliers) {
      j.comma();
      j.pad();
      write_qrat(j, m);
    }
    j.end_arr();
    j.end_obj();
  } else {
    j.key("Split");
    j.begin_obj();
    j.key("branches");
    j.begin_arr();
    for (const auto& b : n.branches) {
      j.comma();
      j.pad();
      write_cert_node(j, b);
    }
    j.end_arr();
    j.end_obj();
  }
  j.end_obj();
}

void write_certificate(Jw& j, const Certificate& c) {
  j.begin_obj();
  j.key("root");
  write_cert_node(j, c.root());
  j.end_obj();
}

void write_derivation(Jw& j, const Derivation& d) {
  j.begin_obj();
  j.key("claim");
  j.str(d.claim);
  j.key("premises");
  j.begin_arr();
  for (const auto& p : d.premises) {
    j.comma();
    j.pad();
    write_assert(j, p);
  }
  j.end_arr();
  j.key("certificate");
  write_certificate(j, d.certificate);
  j.end_obj();
}

void write_fact(Jw& j, const DerivedFact& f) {
  j.begin_obj();
  j.key("name");
  j.str(f.name);
  j.key("axiom");
  j.str(f.axiom);
  j.key("statement");
  j.str(f.statement);
  j.key("formula");
  write_formula(j, f.formula);
  j.key("derivations");
  j.begin_arr();
  for (const auto& d : f.derivations) {
    j.comma();
    j.pad();
    write_derivation(j, d);
  }
  j.end_arr();
  j.end_obj();
}

// ---------------------------------------------------------------------------
// JSON parser (objects, arrays, strings, numbers, bool, null)
// ---------------------------------------------------------------------------

struct Json {
  enum class Kind { Null, Bool, Number, String, Array, Object };
  Kind kind = Kind::Null;
  bool b = false;
  std::string num;
  std::string s;
  std::vector<Json> arr;
  std::map<std::string, Json> obj;

  bool is_null() const { return kind == Kind::Null; }
  bool is_bool() const { return kind == Kind::Bool; }
  bool is_num() const { return kind == Kind::Number; }
  bool is_str() const { return kind == Kind::String; }
  bool is_arr() const { return kind == Kind::Array; }
  bool is_obj() const { return kind == Kind::Object; }

  const Json* get(const char* k) const {
    if (kind != Kind::Object) return nullptr;
    auto it = obj.find(k);
    return it == obj.end() ? nullptr : &it->second;
  }
};

struct Parser {
  const std::string& t;
  std::size_t i = 0;
  bool ok = true;

  explicit Parser(const std::string& text) : t(text) {}

  void skip() {
    while (i < t.size() && std::isspace(static_cast<unsigned char>(t[i]))) ++i;
  }
  char peek() {
    skip();
    return i < t.size() ? t[i] : '\0';
  }
  char take() {
    skip();
    if (i >= t.size()) {
      ok = false;
      return '\0';
    }
    return t[i++];
  }
  bool eat(char c) {
    if (peek() != c) {
      ok = false;
      return false;
    }
    ++i;
    return true;
  }
  bool starts(const char* lit) {
    skip();
    std::size_t n = std::strlen(lit);
    return i + n <= t.size() && t.compare(i, n, lit) == 0;
  }

  Json parse() {
    Json v = value();
    skip();
    if (i != t.size()) ok = false;
    if (!ok) return Json{};
    return v;
  }

  Json value() {
    char c = peek();
    if (c == '{') return object();
    if (c == '[') return array();
    if (c == '"') return string();
    if (c == 't' || c == 'f') return boolean();
    if (c == 'n') return nullv();
    if (c == '-' || std::isdigit(static_cast<unsigned char>(c))) return number();
    ok = false;
    return Json{};
  }

  Json object() {
    Json v;
    v.kind = Json::Kind::Object;
    if (!eat('{')) return v;
    skip();
    if (peek() == '}') {
      ++i;
      return v;
    }
    while (ok) {
      Json k = string();
      if (!ok || k.kind != Json::Kind::String) {
        ok = false;
        return v;
      }
      if (!eat(':')) return v;
      Json val = value();
      v.obj.emplace(std::move(k.s), std::move(val));
      skip();
      if (peek() == ',') {
        ++i;
        continue;
      }
      if (peek() == '}') {
        ++i;
        break;
      }
      ok = false;
      break;
    }
    return v;
  }

  Json array() {
    Json v;
    v.kind = Json::Kind::Array;
    if (!eat('[')) return v;
    skip();
    if (peek() == ']') {
      ++i;
      return v;
    }
    while (ok) {
      v.arr.push_back(value());
      skip();
      if (peek() == ',') {
        ++i;
        continue;
      }
      if (peek() == ']') {
        ++i;
        break;
      }
      ok = false;
      break;
    }
    return v;
  }

  Json string() {
    Json v;
    v.kind = Json::Kind::String;
    if (!eat('"')) return v;
    while (i < t.size() && ok) {
      char c = t[i++];
      if (c == '"') return v;
      if (c == '\\') {
        if (i >= t.size()) {
          ok = false;
          return v;
        }
        char e = t[i++];
        switch (e) {
          case '"':
          case '\\':
          case '/':
            v.s.push_back(e);
            break;
          case 'b': v.s.push_back('\b'); break;
          case 'f': v.s.push_back('\f'); break;
          case 'n': v.s.push_back('\n'); break;
          case 'r': v.s.push_back('\r'); break;
          case 't': v.s.push_back('\t'); break;
          case 'u': {
            if (i + 4 > t.size()) {
              ok = false;
              return v;
            }
            unsigned cp = 0;
            for (int k = 0; k < 4; ++k) {
              char h = t[i++];
              cp <<= 4;
              if (h >= '0' && h <= '9') cp |= static_cast<unsigned>(h - '0');
              else if (h >= 'a' && h <= 'f') cp |= static_cast<unsigned>(h - 'a' + 10);
              else if (h >= 'A' && h <= 'F') cp |= static_cast<unsigned>(h - 'A' + 10);
              else {
                ok = false;
                return v;
              }
            }
            if (cp < 0x80) {
              v.s.push_back(static_cast<char>(cp));
            } else if (cp < 0x800) {
              v.s.push_back(static_cast<char>(0xc0 | (cp >> 6)));
              v.s.push_back(static_cast<char>(0x80 | (cp & 0x3f)));
            } else {
              v.s.push_back(static_cast<char>(0xe0 | (cp >> 12)));
              v.s.push_back(static_cast<char>(0x80 | ((cp >> 6) & 0x3f)));
              v.s.push_back(static_cast<char>(0x80 | (cp & 0x3f)));
            }
            break;
          }
          default:
            ok = false;
            return v;
        }
      } else {
        v.s.push_back(c);
      }
    }
    ok = false;
    return v;
  }

  Json boolean() {
    Json v;
    v.kind = Json::Kind::Bool;
    if (starts("true")) {
      i += 4;
      v.b = true;
      return v;
    }
    if (starts("false")) {
      i += 5;
      v.b = false;
      return v;
    }
    ok = false;
    return v;
  }

  Json nullv() {
    Json v;
    if (starts("null")) {
      i += 4;
      v.kind = Json::Kind::Null;
      return v;
    }
    ok = false;
    return v;
  }

  Json number() {
    Json v;
    v.kind = Json::Kind::Number;
    skip();
    std::size_t start = i;
    if (i < t.size() && t[i] == '-') ++i;
    if (i >= t.size() || !std::isdigit(static_cast<unsigned char>(t[i]))) {
      ok = false;
      return v;
    }
    if (t[i] == '0') {
      ++i;
    } else {
      while (i < t.size() && std::isdigit(static_cast<unsigned char>(t[i]))) ++i;
    }
    if (i < t.size() && t[i] == '.') {
      ++i;
      if (i >= t.size() || !std::isdigit(static_cast<unsigned char>(t[i]))) {
        ok = false;
        return v;
      }
      while (i < t.size() && std::isdigit(static_cast<unsigned char>(t[i]))) ++i;
    }
    if (i < t.size() && (t[i] == 'e' || t[i] == 'E')) {
      ++i;
      if (i < t.size() && (t[i] == '+' || t[i] == '-')) ++i;
      if (i >= t.size() || !std::isdigit(static_cast<unsigned char>(t[i]))) {
        ok = false;
        return v;
      }
      while (i < t.size() && std::isdigit(static_cast<unsigned char>(t[i]))) ++i;
    }
    v.num = t.substr(start, i - start);
    return v;
  }
};

std::optional<std::string> as_str(const Json* v) {
  if (!v || !v->is_str()) return std::nullopt;
  return v->s;
}
std::optional<bool> as_bool(const Json* v) {
  if (!v || !v->is_bool()) return std::nullopt;
  return v->b;
}
std::optional<std::uint32_t> as_u32(const Json* v) {
  if (!v || !v->is_num()) return std::nullopt;
  if (v->num.find('.') != std::string::npos || v->num.find('e') != std::string::npos ||
      v->num.find('E') != std::string::npos || v->num.find('-') != std::string::npos) {
    return std::nullopt;
  }
  try {
    unsigned long x = std::stoul(v->num);
    if (x > 0xFFFFFFFFul) return std::nullopt;
    return static_cast<std::uint32_t>(x);
  } catch (...) {
    return std::nullopt;
  }
}

std::optional<QRat> as_qrat(const Json* v) {
  auto s = as_str(v);
  if (!s) return std::nullopt;
  return QRat::from_repr(*s);
}

std::optional<Rel> as_rel(const Json* v) {
  auto s = as_str(v);
  if (!s) return std::nullopt;
  if (*s == "<") return Rel::Lt;
  if (*s == "<=") return Rel::Le;
  if (*s == ">") return Rel::Gt;
  if (*s == ">=") return Rel::Ge;
  if (*s == "==") return Rel::Eq;
  if (*s == "!=") return Rel::Ne;
  return std::nullopt;
}

std::optional<BundleFormula> parse_formula(const Json* v);
std::optional<BundleAssert> parse_assert(const Json* v);
std::optional<Certificate> parse_certificate(const Json* v);

std::optional<BundleFormula> parse_formula(const Json* v) {
  if (!v || !v->is_obj()) return std::nullopt;
  auto op = as_str(v->get("op"));
  if (!op) return std::nullopt;
  BundleFormula f;
  if (*op == "true") {
    f.op = BundleFormula::Op::True;
    return f;
  }
  if (*op == "false") {
    f.op = BundleFormula::Op::False;
    return f;
  }
  if (*op == "atom") {
    f.op = BundleFormula::Op::Atom;
    const Json* terms = v->get("terms");
    if (!terms || !terms->is_arr()) return std::nullopt;
    for (const auto& t : terms->arr) {
      if (!t.is_obj()) return std::nullopt;
      auto coeff = as_qrat(t.get("coeff"));
      auto q = as_u32(t.get("q"));
      if (!coeff || !q) return std::nullopt;
      BundleTerm bt;
      bt.coeff = *coeff;
      bt.q = *q;
      f.terms.push_back(std::move(bt));
    }
    auto rel = as_rel(v->get("rel"));
    auto k = as_qrat(v->get("k"));
    if (!rel || !k) return std::nullopt;
    f.rel = *rel;
    f.k = *k;
    return f;
  }
  if (*op == "and" || *op == "or") {
    f.op = (*op == "and") ? BundleFormula::Op::And : BundleFormula::Op::Or;
    const Json* args = v->get("args");
    if (!args || !args->is_arr()) return std::nullopt;
    for (const auto& a : args->arr) {
      auto inner = parse_formula(&a);
      if (!inner) return std::nullopt;
      f.args.push_back(std::move(*inner));
    }
    return f;
  }
  return std::nullopt;
}

std::optional<AssertSource> parse_source(const Json* v) {
  if (!v || !v->is_obj()) return std::nullopt;
  auto kind = as_str(v->get("kind"));
  if (!kind) return std::nullopt;
  if (*kind == "cut") {
    auto region = as_str(v->get("region"));
    auto line = as_u32(v->get("line"));
    auto text = as_str(v->get("text"));
    auto whole = as_bool(v->get("whole"));
    if (!region || !line || !text || !whole) return std::nullopt;
    return AssertSource::cut(*region, *line, *text, *whole);
  }
  if (*kind == "axiom") {
    auto id = as_str(v->get("id"));
    auto statement = as_str(v->get("statement"));
    auto assumption = as_str(v->get("assumption"));
    if (!id || !statement || !assumption) return std::nullopt;
    return AssertSource::axiom(*id, *statement, *assumption);
  }
  if (*kind == "derived") {
    auto fact = as_str(v->get("fact"));
    if (!fact) return std::nullopt;
    return AssertSource::derived(*fact);
  }
  if (*kind == "query") {
    auto role = as_str(v->get("role"));
    if (!role) return std::nullopt;
    return AssertSource::query(*role);
  }
  if (*kind == "unattributed") return AssertSource::unattributed();
  return std::nullopt;
}

std::optional<BundleAssert> parse_assert(const Json* v) {
  if (!v || !v->is_obj()) return std::nullopt;
  auto name = as_str(v->get("name"));
  auto src = parse_source(v->get("source"));
  auto formula = parse_formula(v->get("formula"));
  if (!name || !src || !formula) return std::nullopt;
  BundleAssert a;
  a.name = *name;
  a.source = *src;
  a.formula = *formula;
  return a;
}

std::optional<CertNode> parse_cert_node(const Json* v) {
  if (!v) return std::nullopt;
  if (v->is_str()) {
    if (v->s == "Contradiction") return CertNode::contradiction();
    return std::nullopt;
  }
  if (!v->is_obj()) return std::nullopt;
  if (const Json* f = v->get("Farkas")) {
    if (!f->is_obj()) return std::nullopt;
    const Json* ms = f->get("multipliers");
    if (!ms || !ms->is_arr()) return std::nullopt;
    std::vector<QRat> lam;
    for (const auto& m : ms->arr) {
      auto q = as_qrat(&m);
      if (!q) return std::nullopt;
      lam.push_back(*q);
    }
    return CertNode::farkas(std::move(lam));
  }
  if (const Json* s = v->get("Split")) {
    if (!s->is_obj()) return std::nullopt;
    const Json* br = s->get("branches");
    if (!br || !br->is_arr()) return std::nullopt;
    std::vector<CertNode> branches;
    for (const auto& b : br->arr) {
      auto n = parse_cert_node(&b);
      if (!n) return std::nullopt;
      branches.push_back(std::move(*n));
    }
    return CertNode::split(std::move(branches));
  }
  return std::nullopt;
}

std::optional<Certificate> parse_certificate(const Json* v) {
  if (!v || !v->is_obj()) return std::nullopt;
  auto root = parse_cert_node(v->get("root"));
  if (!root) return std::nullopt;
  return Certificate(std::move(*root));
}

std::optional<Derivation> parse_derivation(const Json* v) {
  if (!v || !v->is_obj()) return std::nullopt;
  auto claim = as_str(v->get("claim"));
  const Json* premises = v->get("premises");
  auto cert = parse_certificate(v->get("certificate"));
  if (!claim || !premises || !premises->is_arr() || !cert) return std::nullopt;
  Derivation d;
  d.claim = *claim;
  d.certificate = std::move(*cert);
  for (const auto& p : premises->arr) {
    auto a = parse_assert(&p);
    if (!a) return std::nullopt;
    d.premises.push_back(std::move(*a));
  }
  return d;
}

std::optional<DerivedFact> parse_fact(const Json* v) {
  if (!v || !v->is_obj()) return std::nullopt;
  auto name = as_str(v->get("name"));
  auto axiom = as_str(v->get("axiom"));
  auto statement = as_str(v->get("statement"));
  auto formula = parse_formula(v->get("formula"));
  const Json* derivs = v->get("derivations");
  if (!name || !axiom || !statement || !formula || !derivs || !derivs->is_arr()) return std::nullopt;
  DerivedFact f;
  f.name = *name;
  f.axiom = *axiom;
  f.statement = *statement;
  f.formula = *formula;
  for (const auto& d : derivs->arr) {
    auto der = parse_derivation(&d);
    if (!der) return std::nullopt;
    f.derivations.push_back(std::move(*der));
  }
  return f;
}

std::optional<Producer> parse_producer(const Json* v) {
  if (!v || !v->is_obj()) return std::nullopt;
  auto tool = as_str(v->get("tool"));
  auto version = as_str(v->get("version"));
  const Json* hist = v->get("schema_history");
  if (!tool || !version || !hist || !hist->is_arr()) return std::nullopt;
  Producer p;
  p.tool = *tool;
  p.version = *version;
  for (const auto& h : hist->arr) {
    if (!h.is_str()) return std::nullopt;
    p.schema_history.push_back(h.s);
  }
  return p;
}

void collect_ids(const std::vector<BundleAssert>& asserts, const std::vector<DerivedFact>& facts,
                 std::set<std::uint32_t>& out) {
  for (const auto& a : asserts) a.formula.collect_quantities(out);
  for (const auto& f : facts) {
    f.formula.collect_quantities(out);
    for (const auto& d : f.derivations) {
      for (const auto& p : d.premises) p.formula.collect_quantities(out);
    }
  }
}

}  // namespace

std::string supersession_note(const std::string& schema) {
  return "schema \"" + schema + "\" is superseded by \"" + std::string(BUNDLE_SCHEMA) +
         "\"; re-emit it with this smash2 (`verify --combine DIR/`). A \"" + schema +
         "\" bundle carries no derivation chain for its reconciliation facts, so this "
         "checker will not half-verify it.";
}

bool BundleFormula::operator==(const BundleFormula& o) const {
  return op == o.op && terms == o.terms && rel == o.rel && k == o.k && args == o.args;
}

bool AssertSource::operator==(const AssertSource& o) const {
  return kind == o.kind && region == o.region && line == o.line && text == o.text &&
         whole == o.whole && id == o.id && statement == o.statement &&
         assumption == o.assumption && fact == o.fact && role == o.role;
}

BundleFormula BundleFormula::from_qformula(const QFormula& f) {
  switch (f.kind) {
    case QFormula::Kind::True:
      return ttrue();
    case QFormula::Kind::False:
      return ffalse();
    case QFormula::Kind::Atom: {
      BundleFormula b;
      b.op = Op::Atom;
      for (const auto& t : f.atom.terms()) {
        BundleTerm bt;
        bt.coeff.value = t.first;
        bt.q = t.second.id;
        b.terms.push_back(std::move(bt));
      }
      b.rel = f.atom.rel();
      b.k.value = f.atom.constant();
      return b;
    }
    case QFormula::Kind::And: {
      BundleFormula b;
      b.op = Op::And;
      for (const auto& a : f.items) b.args.push_back(from_qformula(a));
      return b;
    }
    case QFormula::Kind::Or: {
      BundleFormula b;
      b.op = Op::Or;
      for (const auto& a : f.items) b.args.push_back(from_qformula(a));
      return b;
    }
  }
  return ttrue();
}

QFormula BundleFormula::to_qformula() const {
  switch (op) {
    case Op::True:
      return QFormula::ttrue();
    case Op::False:
      return QFormula::ffalse();
    case Op::Atom: {
      std::vector<LinAtom::Term> ts;
      ts.reserve(terms.size());
      for (const auto& t : terms) ts.emplace_back(t.coeff.value, QuantityId{t.q});
      return QFormula::of_atom(LinAtom::make(std::move(ts), rel, k.value));
    }
    case Op::And: {
      std::vector<QFormula> v;
      v.reserve(args.size());
      for (const auto& a : args) v.push_back(a.to_qformula());
      return QFormula::of_and(std::move(v));
    }
    case Op::Or: {
      std::vector<QFormula> v;
      v.reserve(args.size());
      for (const auto& a : args) v.push_back(a.to_qformula());
      return QFormula::of_or(std::move(v));
    }
  }
  return QFormula::ttrue();
}

void BundleFormula::collect_quantities(std::set<std::uint32_t>& out) const {
  switch (op) {
    case Op::True:
    case Op::False:
      break;
    case Op::Atom:
      for (const auto& t : terms) out.insert(t.q);
      break;
    case Op::And:
    case Op::Or:
      for (const auto& a : args) a.collect_quantities(out);
      break;
  }
}

std::vector<QFormula> Derivation::formulas() const {
  std::vector<QFormula> out;
  out.reserve(premises.size());
  for (const auto& p : premises) out.push_back(p.formula.to_qformula());
  return out;
}

CombineBundle CombineBundle::make(BundleParts parts,
                                  const std::function<std::string(std::uint32_t)>& label) {
  std::set<std::uint32_t> ids;
  collect_ids(parts.asserts, parts.derived_facts, ids);
  CombineBundle b;
  b.schema = BUNDLE_SCHEMA;
  b.producer = Producer::smash_cpp2();
  b.region_a = std::move(parts.region_a);
  b.region_b = std::move(parts.region_b);
  b.verdict = BUNDLE_VERDICT;
  b.note = SCOPE_NOTE;
  for (auto q : ids) b.quantities[q] = label(q);
  b.asserts = std::move(parts.asserts);
  b.derived_facts = std::move(parts.derived_facts);
  b.certificate = std::move(parts.certificate);
  return b;
}

std::vector<QFormula> CombineBundle::formulas() const {
  std::vector<QFormula> out;
  out.reserve(asserts.size());
  for (const auto& a : asserts) out.push_back(a.formula.to_qformula());
  return out;
}

bool CombineBundle::replay() const {
  if (schema != BUNDLE_SCHEMA) return false;
  if (verdict != BUNDLE_VERDICT) return false;
  if (note != SCOPE_NOTE) return false;

  std::set<std::uint32_t> ids;
  collect_ids(asserts, derived_facts, ids);
  for (auto q : ids) {
    if (quantities.find(q) == quantities.end()) return false;
  }

  std::map<std::string, const DerivedFact*> facts;
  for (const auto& f : derived_facts) {
    if (!facts.emplace(f.name, &f).second) return false;  // duplicate name
  }
  for (const auto& a : asserts) {
    if (a.source.kind == AssertSource::Kind::Derived) {
      auto it = facts.find(a.source.fact);
      if (it == facts.end() || !(it->second->formula == a.formula)) return false;
    } else if (a.name.size() >= 2 && a.name.compare(0, 2, "XR") == 0) {
      return false;
    }
  }

  for (const auto& f : derived_facts) {
    if (!f.replay()) return false;
  }
  return certificate.replay(formulas());
}

std::string CombineBundle::to_json() const {
  Jw j;
  j.begin_obj();
  j.key("schema");
  j.str(schema);
  j.key("producer");
  {
    j.begin_obj();
    j.key("tool");
    j.str(producer.tool);
    j.key("version");
    j.str(producer.version);
    j.key("schema_history");
    j.begin_arr();
    for (const auto& s : producer.schema_history) {
      j.comma();
      j.pad();
      j.str(s);
    }
    j.end_arr();
    j.end_obj();
  }
  j.key("inputs");
  j.begin_arr();
  for (const auto& in : inputs) {
    j.comma();
    j.pad();
    j.begin_obj();
    j.key("name");
    j.str(in.name);
    j.key("sha256");
    j.str(in.sha256);
    j.end_obj();
  }
  j.end_arr();
  j.key("region_a");
  j.str(region_a);
  j.key("region_b");
  j.str(region_b);
  j.key("verdict");
  j.str(verdict);
  j.key("note");
  j.str(note);
  j.key("quantities");
  j.begin_obj();
  for (const auto& kv : quantities) {
    j.key_s(std::to_string(kv.first));
    j.str(kv.second);
  }
  j.end_obj();
  j.key("asserts");
  j.begin_arr();
  for (const auto& a : asserts) {
    j.comma();
    j.pad();
    write_assert(j, a);
  }
  j.end_arr();
  j.key("derived_facts");
  j.begin_arr();
  for (const auto& f : derived_facts) {
    j.comma();
    j.pad();
    write_fact(j, f);
  }
  j.end_arr();
  j.key("certificate");
  write_certificate(j, certificate);
  j.end_obj();
  return j.os.str();
}

std::optional<CombineBundle> CombineBundle::from_json(const std::string& text) {
  Parser p(text);
  Json root = p.parse();
  if (!p.ok || !root.is_obj()) return std::nullopt;

  CombineBundle b;
  auto schema = as_str(root.get("schema"));
  auto producer = parse_producer(root.get("producer"));
  const Json* inputs = root.get("inputs");
  auto region_a = as_str(root.get("region_a"));
  auto region_b = as_str(root.get("region_b"));
  auto verdict = as_str(root.get("verdict"));
  auto note = as_str(root.get("note"));
  const Json* quantities = root.get("quantities");
  const Json* asserts = root.get("asserts");
  const Json* facts = root.get("derived_facts");
  auto cert = parse_certificate(root.get("certificate"));
  if (!schema || !producer || !inputs || !inputs->is_arr() || !region_a || !region_b || !verdict ||
      !note || !quantities || !quantities->is_obj() || !asserts || !asserts->is_arr() || !facts ||
      !facts->is_arr() || !cert) {
    return std::nullopt;
  }
  b.schema = *schema;
  b.producer = *producer;
  b.region_a = *region_a;
  b.region_b = *region_b;
  b.verdict = *verdict;
  b.note = *note;
  b.certificate = std::move(*cert);
  for (const auto& in : inputs->arr) {
    if (!in.is_obj()) return std::nullopt;
    auto name = as_str(in.get("name"));
    auto sha = as_str(in.get("sha256"));
    if (!name || !sha) return std::nullopt;
    BundleInput bi;
    bi.name = *name;
    bi.sha256 = *sha;
    b.inputs.push_back(std::move(bi));
  }
  for (const auto& kv : quantities->obj) {
    if (!kv.second.is_str()) return std::nullopt;
    try {
      unsigned long q = std::stoul(kv.first);
      if (q > 0xFFFFFFFFul) return std::nullopt;
      b.quantities[static_cast<std::uint32_t>(q)] = kv.second.s;
    } catch (...) {
      return std::nullopt;
    }
  }
  for (const auto& a : asserts->arr) {
    auto ba = parse_assert(&a);
    if (!ba) return std::nullopt;
    b.asserts.push_back(std::move(*ba));
  }
  for (const auto& f : facts->arr) {
    auto df = parse_fact(&f);
    if (!df) return std::nullopt;
    b.derived_facts.push_back(std::move(*df));
  }
  return b;
}

bool CombineBundle::operator==(const CombineBundle& o) const {
  return schema == o.schema && producer == o.producer && inputs == o.inputs &&
         region_a == o.region_a && region_b == o.region_b && verdict == o.verdict && note == o.note &&
         quantities == o.quantities && asserts == o.asserts && derived_facts == o.derived_facts &&
         certificate == o.certificate;
}

}  // namespace adl2::certify
