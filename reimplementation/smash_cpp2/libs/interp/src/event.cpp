#include "adl2/interp/event.hpp"

#include "adl2/sema/intern.hpp"

#include <cctype>
#include <cerrno>
#include <cmath>
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

class Parser {
 public:
  explicit Parser(std::string t) : text(std::move(t)) {}

  Json parse_value() {
    skip();
    if (i >= text.size()) throw std::runtime_error("unexpected end of JSON");
    char c = text[i];
    if (c == 'n') return parse_lit("null", Json{});
    if (c == 't') {
      Json j;
      j.kind = Json::Kind::Bool;
      j.b = true;
      return parse_lit("true", j);
    }
    if (c == 'f') {
      Json j;
      j.kind = Json::Kind::Bool;
      j.b = false;
      return parse_lit("false", j);
    }
    if (c == '"') return parse_string();
    if (c == '[') return parse_array();
    if (c == '{') return parse_object();
    if (c == '-' || std::isdigit(static_cast<unsigned char>(c))) return parse_number();
    throw std::runtime_error(std::string("unexpected JSON char: ") + c);
  }

 private:
  std::string text;
  std::size_t i{0};

  void skip() {
    while (i < text.size() && std::isspace(static_cast<unsigned char>(text[i]))) ++i;
  }

  Json parse_lit(const char* lit, Json j) {
    std::size_t n = std::char_traits<char>::length(lit);
    if (text.compare(i, n, lit) != 0) throw std::runtime_error(std::string("expected ") + lit);
    i += n;
    return j;
  }

  Json parse_string() {
    if (text[i] != '"') throw std::runtime_error("expected string");
    ++i;
    std::string out;
    while (i < text.size()) {
      char c = text[i++];
      if (c == '"') {
        Json j;
        j.kind = Json::Kind::Str;
        j.s = std::move(out);
        return j;
      }
      if (c == '\\' && i < text.size()) {
        char e = text[i++];
        switch (e) {
          case '"':
          case '\\':
          case '/':
            out.push_back(e);
            break;
          case 'b':
            out.push_back('\b');
            break;
          case 'f':
            out.push_back('\f');
            break;
          case 'n':
            out.push_back('\n');
            break;
          case 'r':
            out.push_back('\r');
            break;
          case 't':
            out.push_back('\t');
            break;
          default:
            out.push_back(e);
            break;
        }
      } else {
        out.push_back(c);
      }
    }
    throw std::runtime_error("unterminated string");
  }

  Json parse_number() {
    std::size_t start = i;
    if (i < text.size() && text[i] == '-') ++i;
    while (i < text.size() && std::isdigit(static_cast<unsigned char>(text[i]))) ++i;
    if (i < text.size() && text[i] == '.') {
      ++i;
      while (i < text.size() && std::isdigit(static_cast<unsigned char>(text[i]))) ++i;
    }
    if (i < text.size() && (text[i] == 'e' || text[i] == 'E')) {
      ++i;
      if (i < text.size() && (text[i] == '+' || text[i] == '-')) ++i;
      while (i < text.size() && std::isdigit(static_cast<unsigned char>(text[i]))) ++i;
    }
    Json j;
    j.kind = Json::Kind::Num;
    // strtod, not stod: libstdc++ stod throws out_of_range on ERANGE, and
    // glibc sets ERANGE for subnormal underflow (e.g. 5e-324 = next_up(0)),
    // which serde_json accepts. Underflow to 0/denormal is a valid JSON
    // number; only overflow to ±inf is rejected.
    errno = 0;
    char* end = nullptr;
    j.n = std::strtod(text.c_str() + start, &end);
    if (end != text.c_str() + i) {
      throw std::runtime_error("invalid JSON number");
    }
    if (!std::isfinite(j.n)) {
      throw std::runtime_error("non-finite JSON number");
    }
    return j;
  }

  Json parse_array() {
    ++i;
    Json j;
    j.kind = Json::Kind::Arr;
    skip();
    if (i < text.size() && text[i] == ']') {
      ++i;
      return j;
    }
    while (true) {
      j.arr.push_back(parse_value());
      skip();
      if (i >= text.size()) throw std::runtime_error("unterminated array");
      if (text[i] == ',') {
        ++i;
        continue;
      }
      if (text[i] == ']') {
        ++i;
        return j;
      }
      throw std::runtime_error("expected , or ]");
    }
  }

  Json parse_object() {
    ++i;
    Json j;
    j.kind = Json::Kind::Obj;
    skip();
    if (i < text.size() && text[i] == '}') {
      ++i;
      return j;
    }
    while (true) {
      skip();
      Json key = parse_string();
      skip();
      if (i >= text.size() || text[i] != ':') throw std::runtime_error("expected :");
      ++i;
      Json val = parse_value();
      j.obj.emplace_back(std::move(key.s), std::move(val));
      skip();
      if (i >= text.size()) throw std::runtime_error("unterminated object");
      if (text[i] == ',') {
        ++i;
        continue;
      }
      if (text[i] == '}') {
        ++i;
        return j;
      }
      throw std::runtime_error("expected , or }");
    }
  }
};

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
    j = p.parse_value();
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
    std::size_t a = 0;
    while (a < row.size() && std::isspace(static_cast<unsigned char>(row[a]))) ++a;
    if (a == row.size()) continue;
    EventError e;
    auto ev = parse_event(row.substr(a), ext, e, line);
    if (!ev) {
      err = std::move(e);
      return false;
    }
    out.push_back(std::move(*ev));
  }
  return true;
}

}  // namespace adl2::interp
