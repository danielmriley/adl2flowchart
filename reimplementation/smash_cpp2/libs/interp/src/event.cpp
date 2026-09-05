#include "adl2/interp/event.hpp"

#include "adl2/sema/intern.hpp"

#include <algorithm>
#include <cctype>
#include <cerrno>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <sstream>
#include <stdexcept>
#include <utility>

namespace adl2::interp {
namespace {

using adl2::sema::ExtDecls;
using adl2::sema::Rat;
using adl2::sema::SymbolTable;

struct Json {
  enum class Kind { Null, Bool, Num, Str, Arr, Obj };
  Kind kind{Kind::Null};
  bool b{false};
  double n{0.0};
  std::string s;
  std::vector<Json> arr;
  std::vector<std::pair<std::string, Json>> obj;

  const Json* get(const std::string& key) const {
    if (kind != Kind::Obj) return nullptr;
    for (const auto& kv : obj) {
      if (kv.first == key) return &kv.second;
    }
    return nullptr;
  }
};

// A strict JSON document parser that mirrors serde_json's `from_str` into a
// `Value` (the oracle's loader): the same grammar rejections, the same error
// codes, and the same `<code> at line L column C` positions. Positions follow
// serde_json's two conventions — `err` reports the cursor after the offending
// byte was consumed, `peek_err` reports the byte the cursor is looking at
// (1-based, capped at the input length). Objects are `BTreeMap`-like: a
// repeated key keeps the LAST value, and entries iterate in byte order.
class Parser {
 public:
  explicit Parser(const std::string& t) : text(t), n(t.size()) {}

  Json parse_document() {
    Json v = parse_value();
    skip_ws();
    if (i < n) peek_err("trailing characters");
    return v;
  }

 private:
  static constexpr int kRecursionLimit = 128;

  const std::string& text;
  std::size_t n;
  std::size_t i{0};
  int remaining_depth{kRecursionLimit};

  [[noreturn]] void fail(const char* code, std::size_t at) const {
    std::size_t start_of_line = 0;
    std::size_t line = 1;
    for (std::size_t k = 0; k < at; ++k) {
      if (text[k] == '\n') {
        ++line;
        start_of_line = k + 1;
      }
    }
    throw std::runtime_error(std::string(code) + " at line " + std::to_string(line) + " column " +
                             std::to_string(at - start_of_line));
  }
  [[noreturn]] void err(const char* code) const { fail(code, i); }
  [[noreturn]] void peek_err(const char* code) const { fail(code, std::min(n, i + 1)); }

  static bool is_digit(char c) { return c >= '0' && c <= '9'; }

  void skip_ws() {
    while (i < n && (text[i] == ' ' || text[i] == '\n' || text[i] == '\t' || text[i] == '\r')) ++i;
  }

  Json parse_value() {
    skip_ws();
    if (i >= n) peek_err("EOF while parsing a value");
    switch (text[i]) {
      case 'n':
        ++i;
        parse_ident("ull");
        return Json{};
      case 't': {
        ++i;
        parse_ident("rue");
        Json j;
        j.kind = Json::Kind::Bool;
        j.b = true;
        return j;
      }
      case 'f': {
        ++i;
        parse_ident("alse");
        Json j;
        j.kind = Json::Kind::Bool;
        j.b = false;
        return j;
      }
      case '-':
        ++i;
        return parse_number(false);
      case '"': {
        ++i;
        Json j;
        j.kind = Json::Kind::Str;
        j.s = parse_string_body();
        return j;
      }
      case '[': {
        enter();
        ++i;
        Json j = parse_array_body();
        ++remaining_depth;
        return j;
      }
      case '{': {
        enter();
        ++i;
        Json j = parse_object_body();
        ++remaining_depth;
        return j;
      }
      default:
        if (is_digit(text[i])) return parse_number(true);
        peek_err("expected value");
    }
  }

  // serde_json's check_recursion!: the 128th nested container is refused,
  // reported at its opening bracket.
  void enter() {
    if (--remaining_depth == 0) peek_err("recursion limit exceeded");
  }

  void parse_ident(const char* rest) {
    for (; *rest; ++rest) {
      if (i >= n) err("EOF while parsing a value");
      char c = text[i++];
      if (c != *rest) err("expected ident");
    }
  }

  Json parse_number(bool positive) {
    const std::size_t start = positive ? i : i - 1;
    if (i >= n) err("EOF while parsing a value");
    char c = text[i++];
    if (c == '0') {
      if (i < n && is_digit(text[i])) peek_err("invalid number");
    } else if (c >= '1' && c <= '9') {
      while (i < n && is_digit(text[i])) ++i;
    } else {
      err("invalid number");
    }
    if (i < n && text[i] == '.') {
      ++i;
      const std::size_t digits_at = i;
      while (i < n && is_digit(text[i])) ++i;
      if (i == digits_at) {
        if (i < n) peek_err("invalid number");
        peek_err("EOF while parsing a value");
      }
    }
    if (i < n && (text[i] == 'e' || text[i] == 'E')) {
      ++i;
      if (i < n && (text[i] == '+' || text[i] == '-')) ++i;
      if (i >= n) err("EOF while parsing a value");
      char d = text[i++];
      if (!is_digit(d)) err("invalid number");
      while (i < n && is_digit(text[i])) ++i;
    }
    // strtod on exactly the scanned token (never the tail of the line, so
    // hex/inf/nan spellings cannot leak in). strtod, not stod: libstdc++ stod
    // throws out_of_range on ERANGE, and glibc sets ERANGE for subnormal
    // underflow (e.g. 5e-324), which serde_json accepts. Only overflow to
    // ±inf is rejected, as serde_json's `number out of range`.
    std::string tok(text, start, i - start);
    errno = 0;
    char* end = nullptr;
    double v = std::strtod(tok.c_str(), &end);
    if (end != tok.c_str() + tok.size()) err("invalid number");
    if (!std::isfinite(v)) err("number out of range");
    Json j;
    j.kind = Json::Kind::Num;
    j.n = v;
    return j;
  }

  static void push_utf8(std::string& out, std::uint32_t cp) {
    if (cp < 0x80) {
      out.push_back(static_cast<char>(cp));
    } else if (cp < 0x800) {
      out.push_back(static_cast<char>(0xC0 | (cp >> 6)));
      out.push_back(static_cast<char>(0x80 | (cp & 0x3F)));
    } else if (cp < 0x10000) {
      out.push_back(static_cast<char>(0xE0 | (cp >> 12)));
      out.push_back(static_cast<char>(0x80 | ((cp >> 6) & 0x3F)));
      out.push_back(static_cast<char>(0x80 | (cp & 0x3F)));
    } else {
      out.push_back(static_cast<char>(0xF0 | (cp >> 18)));
      out.push_back(static_cast<char>(0x80 | ((cp >> 12) & 0x3F)));
      out.push_back(static_cast<char>(0x80 | ((cp >> 6) & 0x3F)));
      out.push_back(static_cast<char>(0x80 | (cp & 0x3F)));
    }
  }

  std::uint32_t hex4() {
    if (n - i < 4) {
      i = n;
      err("EOF while parsing a string");
    }
    std::uint32_t v = 0;
    bool ok = true;
    for (int k = 0; k < 4; ++k) {
      char c = text[i + static_cast<std::size_t>(k)];
      std::uint32_t d;
      if (c >= '0' && c <= '9') d = static_cast<std::uint32_t>(c - '0');
      else if (c >= 'a' && c <= 'f') d = static_cast<std::uint32_t>(c - 'a' + 10);
      else if (c >= 'A' && c <= 'F') d = static_cast<std::uint32_t>(c - 'A' + 10);
      else {
        d = 0;
        ok = false;
      }
      v = (v << 4) | d;
    }
    i += 4;
    if (!ok) err("invalid escape");
    return v;
  }

  void parse_unicode_escape(std::string& out) {
    std::uint32_t n1 = hex4();
    if (n1 >= 0xDC00 && n1 <= 0xDFFF) err("lone leading surrogate in hex escape");
    if (n1 < 0xD800 || n1 > 0xDBFF) {
      push_utf8(out, n1);
      return;
    }
    if (i >= n) err("EOF while parsing a string");
    if (text[i] != '\\') {
      ++i;
      err("unexpected end of hex escape");
    }
    ++i;
    if (i >= n) err("EOF while parsing a string");
    if (text[i] != 'u') {
      ++i;
      err("unexpected end of hex escape");
    }
    ++i;
    std::uint32_t n2 = hex4();
    if (n2 < 0xDC00 || n2 > 0xDFFF) err("lone leading surrogate in hex escape");
    push_utf8(out, (((n1 - 0xD800) << 10) | (n2 - 0xDC00)) + 0x10000);
  }

  void parse_escape(std::string& out) {
    if (i >= n) err("EOF while parsing a string");
    char e = text[i++];
    switch (e) {
      case '"': out.push_back('"'); break;
      case '\\': out.push_back('\\'); break;
      case '/': out.push_back('/'); break;
      case 'b': out.push_back('\b'); break;
      case 'f': out.push_back('\f'); break;
      case 'n': out.push_back('\n'); break;
      case 'r': out.push_back('\r'); break;
      case 't': out.push_back('\t'); break;
      case 'u': parse_unicode_escape(out); break;
      default: err("invalid escape");
    }
  }

  // Cursor sits just after the opening quote.
  std::string parse_string_body() {
    std::string out;
    while (true) {
      if (i >= n) err("EOF while parsing a string");
      unsigned char c = static_cast<unsigned char>(text[i]);
      if (c == '"') {
        ++i;
        return out;
      }
      if (c == '\\') {
        ++i;
        parse_escape(out);
        continue;
      }
      if (c < 0x20) {
        ++i;
        err("control character (\\u0000-\\u001F) found while parsing a string");
      }
      out.push_back(static_cast<char>(c));
      ++i;
    }
  }

  // Cursor sits just after `[`.
  Json parse_array_body() {
    Json j;
    j.kind = Json::Kind::Arr;
    bool first = true;
    while (true) {
      skip_ws();
      if (i >= n) peek_err("EOF while parsing a list");
      if (text[i] == ']') {
        ++i;
        return j;
      }
      if (first) {
        first = false;
      } else if (text[i] == ',') {
        ++i;
        skip_ws();
        if (i >= n) peek_err("EOF while parsing a value");
        if (text[i] == ']') peek_err("trailing comma");
      } else {
        peek_err("expected `,` or `]`");
      }
      j.arr.push_back(parse_value());
    }
  }

  // Cursor sits just after `{`.
  Json parse_object_body() {
    Json j;
    j.kind = Json::Kind::Obj;
    bool first = true;
    while (true) {
      skip_ws();
      if (i >= n) peek_err("EOF while parsing an object");
      if (text[i] == '}') {
        ++i;
        break;
      }
      if (first) {
        first = false;
        if (text[i] != '"') peek_err("key must be a string");
      } else if (text[i] == ',') {
        ++i;
        skip_ws();
        if (i >= n) peek_err("EOF while parsing a value");
        if (text[i] == '}') peek_err("trailing comma");
        if (text[i] != '"') peek_err("key must be a string");
      } else {
        peek_err("expected `,` or `}`");
      }
      ++i;
      std::string key = parse_string_body();
      skip_ws();
      if (i >= n) peek_err("EOF while parsing an object");
      if (text[i] != ':') peek_err("expected `:`");
      ++i;
      Json val = parse_value();
      bool replaced = false;
      for (auto& kv : j.obj) {
        if (kv.first == key) {
          kv.second = std::move(val);
          replaced = true;
          break;
        }
      }
      if (!replaced) j.obj.emplace_back(std::move(key), std::move(val));
    }
    std::sort(j.obj.begin(), j.obj.end(),
              [](const std::pair<std::string, Json>& a, const std::pair<std::string, Json>& b) {
                return a.first < b.first;
              });
    return j;
  }
};

// Rust's `BufRead::lines` refuses a line that is not valid UTF-8 before the
// JSON parser ever sees it; mirror that (overlongs, surrogates, > U+10FFFF).
bool valid_utf8(const std::string& s) {
  std::size_t i = 0;
  const std::size_t n = s.size();
  while (i < n) {
    unsigned char c = static_cast<unsigned char>(s[i]);
    if (c < 0x80) {
      ++i;
      continue;
    }
    std::size_t len;
    std::uint32_t cp;
    if (c >= 0xC2 && c <= 0xDF) {
      len = 2;
      cp = c & 0x1F;
    } else if (c >= 0xE0 && c <= 0xEF) {
      len = 3;
      cp = c & 0x0F;
    } else if (c >= 0xF0 && c <= 0xF4) {
      len = 4;
      cp = c & 0x07;
    } else {
      return false;
    }
    if (n - i < len) return false;
    for (std::size_t k = 1; k < len; ++k) {
      unsigned char cc = static_cast<unsigned char>(s[i + k]);
      if ((cc & 0xC0) != 0x80) return false;
      cp = (cp << 6) | (cc & 0x3F);
    }
    if ((len == 3 && cp < 0x800) || (len == 4 && (cp < 0x10000 || cp > 0x10FFFF)) ||
        (cp >= 0xD800 && cp <= 0xDFFF)) {
      return false;
    }
    i += len;
  }
  return true;
}

EventError json_err(std::size_t line, std::string message) {
  EventError e;
  e.kind = EventError::Kind::Json;
  e.line = line;
  e.message = "line " + std::to_string(line) + ": invalid JSON: " + message;
  return e;
}

EventError shape_err(std::size_t line, std::string message) {
  EventError e;
  e.kind = EventError::Kind::Shape;
  e.line = line;
  e.message = "line " + std::to_string(line) + ": " + message;
  return e;
}

EventError domain_err(std::size_t line, const std::string& context, const std::string& property,
                      const Rat& value, const char* requirement) {
  EventError e;
  e.kind = EventError::Kind::AxiomDomain;
  e.line = line;
  std::ostringstream o;
  o << "line " << line << ": " << context << ": property `" << property << "` is "
    << value.to_f64() << "; " << requirement;
  e.message = o.str();
  return e;
}

std::optional<Rat> json_rat(const Json& val) {
  if (val.kind != Json::Kind::Num) return std::nullopt;
  return Rat::from_decimal_f64(val.n);
}

bool check_domain(std::size_t line, const ExtDecls& ext, const std::string& context,
                  const std::string& display, const std::string& canon_key, const Rat& value,
                  EventError& err) {
  auto pt_key = ext.prop_canon("pt").first;
  auto e_key = ext.prop_canon("e").first;
  if ((canon_key == pt_key || canon_key == e_key) && value.is_negative()) {
    err = domain_err(line, context, display, value,
                     "pt/energy are magnitudes and the NNEG axiom asserts they are >= 0 "
                     "(a negative value would make the analyzer's proofs unsound)");
    return false;
  }
  if (ext.is_tag_property(canon_key) && !(value.is_zero() || value == Rat::one())) {
    err = domain_err(line, context, display, value,
                     "the TAG axiom asserts btag/ctag/tautag are in {0, 1} "
                     "(use a differently-named property for a continuous discriminant)");
    return false;
  }
  return true;
}

bool load_triggers(std::size_t line, Event& ev, const Json& val, EventError& err) {
  if (val.kind != Json::Kind::Obj) {
    err = shape_err(line, "`triggers` must be an object of 0/1 flags");
    return false;
  }
  for (const auto& kv : val.obj) {
    auto flag = json_rat(kv.second);
    if (!flag) {
      err = shape_err(line, "trigger flag `" + kv.first + "` must be a number");
      return false;
    }
    if (!(*flag == Rat::zero() || *flag == Rat::one())) {
      EventError e;
      e.kind = EventError::Kind::BadTriggerFlag;
      e.line = line;
      e.message = "line " + std::to_string(line) + ": trigger flag `" + kv.first + "` is " +
                  std::to_string(flag->to_f64()) + "; flags must be 0 or 1";
      err = std::move(e);
      return false;
    }
    std::string lk = SymbolTable::ascii_lower(kv.first);
    if (ev.triggers.find(lk) != ev.triggers.end()) {
      err = shape_err(line, "duplicate trigger flag `" + lk + "` after case folding");
      return false;
    }
    ev.triggers.emplace(std::move(lk), *flag);
  }
  return true;
}

bool load_met(std::size_t line, Event& ev, const std::string& key, const Json& val,
              const ExtDecls& ext, EventError& err) {
  if (!ev.met.empty()) {
    err = shape_err(line, "duplicate MET vector (key `" + key + "`)");
    return false;
  }
  if (val.kind == Json::Kind::Obj) {
    for (const auto& kv : val.obj) {
      auto n = json_rat(kv.second);
      if (!n) {
        err = shape_err(line, "MET component `" + kv.first + "` must be a number");
        return false;
      }
      auto canon = ext.prop_canon(kv.first).first;
      if (!check_domain(line, ext, "MET", kv.first, canon, *n, err)) return false;
      if (ev.met.find(canon) != ev.met.end()) {
        err = shape_err(line, "duplicate MET component `" + kv.first + "` after canonicalization");
        return false;
      }
      ev.met.emplace(std::move(canon), *n);
    }
    return true;
  }
  auto n = json_rat(val);
  if (!n) {
    err = shape_err(line, "MET (key `" + key + "`) must be a number");
    return false;
  }
  auto pt_key = ext.prop_canon("pt").first;
  if (!check_domain(line, ext, "MET", "pt", pt_key, *n, err)) return false;
  ev.met.emplace(std::move(pt_key), *n);
  return true;
}

bool load_collection(std::size_t line, Event& ev, const std::string& key, const Json& items,
                     const ExtDecls& ext, EventError& err) {
  std::string ckey;
  if (const std::string* base = ext.base_collection(key)) {
    ckey = SymbolTable::ascii_lower(*base);
  } else {
    ckey = SymbolTable::ascii_lower(key);
  }
  std::vector<EventObject> objs;
  objs.reserve(items.arr.size());
  for (std::size_t i = 0; i < items.arr.size(); ++i) {
    const Json& item = items.arr[i];
    if (item.kind != Json::Kind::Obj) {
      err = shape_err(line, "collection `" + key + "`: element " + std::to_string(i) +
                                " must be a JSON object");
      return false;
    }
    EventObject obj;
    for (const auto& kv : item.obj) {
      auto n = json_rat(kv.second);
      if (!n) {
        err = shape_err(line, "collection `" + key + "`: element " + std::to_string(i) +
                                  ": property `" + kv.first + "` must be a number");
        return false;
      }
      auto canon = ext.prop_canon(kv.first).first;
      std::string ctx = "collection `" + key + "`: element " + std::to_string(i);
      if (!check_domain(line, ext, ctx, kv.first, canon, *n, err)) return false;
      if (obj.props.find(canon) != obj.props.end()) {
        err = shape_err(line, "collection `" + key + "`: element " + std::to_string(i) +
                                  ": duplicate property `" + kv.first + "` after canonicalization");
        return false;
      }
      obj.props.emplace(std::move(canon), *n);
    }
    objs.push_back(std::move(obj));
  }
  if (ev.collections.find(ckey) != ev.collections.end()) {
    err = shape_err(line, "duplicate collection `" + ckey + "` after canonicalization");
    return false;
  }
  ev.collections.emplace(std::move(ckey), std::move(objs));
  return true;
}

bool validate_pt_descending(std::size_t line, const Event& ev, const ExtDecls& ext,
                            EventError& err) {
  auto pt_key = ext.prop_canon("pt").first;
  for (const auto& kv : ev.collections) {
    const Rat* prev = nullptr;
    for (std::size_t i = 0; i < kv.second.size(); ++i) {
      const Rat* pt = kv.second[i].get(pt_key);
      if (!pt) continue;
      if (prev && *pt > *prev) {
        EventError e;
        e.kind = EventError::Kind::NotPtDescending;
        e.line = line;
        e.message = "line " + std::to_string(line) + ": collection `" + kv.first +
                    "` is not pT-descending at index " + std::to_string(i) +
                    " (events must arrive ordered; re-sort is OFF)";
        err = std::move(e);
        return false;
      }
      prev = pt;
    }
  }
  return true;
}

bool event_from_json(std::size_t line, const Json& value, const ExtDecls& ext, Event& ev,
                     EventError& err) {
  if (value.kind != Json::Kind::Obj) {
    err = shape_err(line, "event record must be a JSON object");
    return false;
  }
  ev = Event{};
  bool saw_weight = false;
  for (const auto& kv : value.obj) {
    std::string lk = SymbolTable::ascii_lower(kv.first);
    if (lk == "triggers") {
      if (!load_triggers(line, ev, kv.second, err)) return false;
    } else if (lk == "weight") {
      if (kv.second.kind != Json::Kind::Num) {
        err = shape_err(line, "`weight` must be a number");
        return false;
      }
      if (saw_weight) {
        err = shape_err(line, "duplicate `weight` key after case folding");
        return false;
      }
      saw_weight = true;
      ev.weight = kv.second.n;
    } else if (ext.is_met_family(kv.first)) {
      if (!load_met(line, ev, kv.first, kv.second, ext, err)) return false;
    } else if (kv.second.kind == Json::Kind::Arr) {
      if (!load_collection(line, ev, kv.first, kv.second, ext, err)) return false;
    } else if (kv.second.kind == Json::Kind::Num) {
      auto n = json_rat(kv.second);
      if (!n) {
        err = shape_err(line, "event scalar `" + lk + "` must be a finite number");
        return false;
      }
      if (ext.is_event_scalar(lk) && n->is_negative()) {
        err = domain_err(line, "event scalar", kv.first, *n,
                         "HT-family scalars are sums of magnitudes and the NNEG "
                         "axiom asserts they are >= 0");
        return false;
      }
      if (ev.scalars.find(lk) != ev.scalars.end()) {
        err = shape_err(line, "duplicate event scalar `" + lk + "` after case folding");
        return false;
      }
      ev.scalars.emplace(std::move(lk), *n);
    } else {
      err = shape_err(line, "key `" + kv.first +
                                "`: expected an object list, a number, or `triggers`");
      return false;
    }
  }
  return validate_pt_descending(line, ev, ext, err);
}

}  // namespace

std::optional<Event> parse_event(const std::string& text, const ExtDecls& ext, EventError& err,
                                 std::size_t line) {
  Json j;
  try {
    Parser p(text);
    j = p.parse_document();
  } catch (const std::exception& ex) {
    err = json_err(line, ex.what());
    return std::nullopt;
  }
  Event ev;
  if (!event_from_json(line, j, ext, ev, err)) return std::nullopt;
  return ev;
}

bool read_jsonl(const std::string& text, const ExtDecls& ext, std::vector<Event>& out,
                EventError& err) {
  out.clear();
  std::istringstream in(text);
  std::string row;
  std::size_t line = 0;
  while (std::getline(in, row)) {
    ++line;
    // Rust `BufRead::lines`: strip one trailing '\r', refuse invalid UTF-8,
    // then `trim().is_empty()` skips blank lines.
    if (!row.empty() && row.back() == '\r') row.pop_back();
    if (!valid_utf8(row)) {
      err = EventError{};
      err.kind = EventError::Kind::Json;
      err.line = line;
      err.message = "read error: stream did not contain valid UTF-8";
      return false;
    }
    bool blank = true;
    for (unsigned char c : row) {
      if (!std::isspace(c)) {
        blank = false;
        break;
      }
    }
    if (blank) continue;
    EventError e;
    // The whole line goes to the parser so error columns count leading
    // whitespace exactly as serde_json does.
    auto ev = parse_event(row, ext, e, line);
    if (!ev) {
      err = std::move(e);
      return false;
    }
    out.push_back(std::move(*ev));
  }
  return true;
}

}  // namespace adl2::interp
