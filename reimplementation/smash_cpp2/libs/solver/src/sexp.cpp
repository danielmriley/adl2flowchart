#include "adl2/solver/sexp.hpp"

#include <cctype>
#include <cstddef>
#include <string>
#include <vector>

namespace adl2::solver {
namespace {

enum class SexpKind { Atom, List };
struct Sexp {
  SexpKind kind = SexpKind::Atom;
  std::string atom;
  std::vector<Sexp> items;
};

std::vector<std::string> tokenize(const std::string& s) {
  std::vector<std::string> toks;
  std::string cur;
  for (char ch : s) {
    if (ch == '(' || ch == ')') {
      if (!cur.empty()) {
        toks.push_back(cur);
        cur.clear();
      }
      toks.emplace_back(1, ch);
    } else if (std::isspace(static_cast<unsigned char>(ch))) {
      if (!cur.empty()) {
        toks.push_back(cur);
        cur.clear();
      }
    } else {
      cur.push_back(ch);
    }
  }
  if (!cur.empty()) toks.push_back(cur);
  return toks;
}

std::optional<Sexp> parse_sexp(const std::vector<std::string>& toks,
                               std::size_t& pos) {
  if (pos >= toks.size()) return std::nullopt;
  if (toks[pos] == "(") {
    ++pos;
    Sexp list;
    list.kind = SexpKind::List;
    while (true) {
      if (pos >= toks.size()) return std::nullopt;
      if (toks[pos] == ")") {
        ++pos;
        return list;
      }
      auto item = parse_sexp(toks, pos);
      if (!item) return std::nullopt;
      list.items.push_back(std::move(*item));
    }
  }
  if (toks[pos] == ")") return std::nullopt;
  Sexp atom;
  atom.kind = SexpKind::Atom;
  atom.atom = toks[pos];
  ++pos;
  return atom;
}

std::optional<std::vector<Sexp>> first_list(const std::string& reply) {
  auto open = reply.find('(');
  if (open == std::string::npos) return std::nullopt;
  auto toks = tokenize(reply.substr(open));
  std::size_t pos = 0;
  auto s = parse_sexp(toks, pos);
  if (!s || s->kind != SexpKind::List) return std::nullopt;
  return std::move(s->items);
}

std::optional<adl2::sema::Rat> sexp_rat(const Sexp& s) {
  if (s.kind == SexpKind::Atom) {
    try {
      std::size_t idx = 0;
      long long n = std::stoll(s.atom, &idx);
      if (idx == s.atom.size()) {
        return adl2::sema::Rat::from_i64(static_cast<std::int64_t>(n));
      }
    } catch (...) {
    }
    try {
      std::size_t idx = 0;
      double v = std::stod(s.atom, &idx);
      if (idx == s.atom.size()) return adl2::sema::Rat::from_decimal_f64(v);
    } catch (...) {
    }
    return std::nullopt;
  }
  const auto& items = s.items;
  if (items.size() == 2 && items[0].kind == SexpKind::Atom &&
      items[0].atom == "-") {
    auto x = sexp_rat(items[1]);
    if (!x) return std::nullopt;
    return -(*x);
  }
  if (items.size() == 3 && items[0].kind == SexpKind::Atom &&
      items[0].atom == "/") {
    auto num = sexp_rat(items[1]);
    auto den = sexp_rat(items[2]);
    if (!num || !den) return std::nullopt;
    return num->checked_div(*den);
  }
  return std::nullopt;
}

}  // namespace

std::optional<std::vector<std::pair<std::string, adl2::sema::Rat>>>
parse_value_list(const std::string& reply) {
  auto items = first_list(reply);
  if (!items) return std::nullopt;
  std::vector<std::pair<std::string, adl2::sema::Rat>> out;
  for (const auto& p : *items) {
    if (p.kind == SexpKind::List && p.items.size() == 2 &&
        p.items[0].kind == SexpKind::Atom) {
      auto v = sexp_rat(p.items[1]);
      if (v) out.emplace_back(p.items[0].atom, *v);
    }
  }
  return out;
}

std::optional<std::vector<std::string>> parse_symbol_list(
    const std::string& reply) {
  auto items = first_list(reply);
  if (!items) return std::nullopt;
  std::vector<std::string> out;
  for (const auto& s : *items) {
    if (s.kind == SexpKind::Atom) out.push_back(s.atom);
  }
  return out;
}

}  // namespace adl2::solver
