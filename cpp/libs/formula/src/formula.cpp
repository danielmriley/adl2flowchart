#include "adl2/formula/formula.hpp"

namespace adl2::formula {
namespace {

std::unique_ptr<Formula> clone_ptr(const std::unique_ptr<Formula>& p) {
  if (!p) return nullptr;
  return std::make_unique<Formula>(*p);
}

}  // namespace

Formula::Formula(const Formula& o)
    : kind(o.kind),
      atom(o.atom),
      items(o.items),
      diag(o.diag),
      plus(clone_ptr(o.plus)),
      minus(clone_ptr(o.minus)) {}

Formula& Formula::operator=(const Formula& o) {
  if (this == &o) return *this;
  kind = o.kind;
  atom = o.atom;
  items = o.items;
  diag = o.diag;
  plus = clone_ptr(o.plus);
  minus = clone_ptr(o.minus);
  return *this;
}

Formula Formula::dual(Formula p, Formula m, DiagId why) {
  Formula f;
  f.kind = Kind::Dual;
  f.diag = why;
  f.plus = std::make_unique<Formula>(std::move(p));
  f.minus = std::make_unique<Formula>(std::move(m));
  return f;
}

Formula Formula::fnot() const {
  switch (kind) {
    case Kind::True:
      return ffalse();
    case Kind::False:
      return ttrue();
    case Kind::Atom:
      return of_atom(atom.negated());
    case Kind::And: {
      std::vector<Formula> v;
      v.reserve(items.size());
      for (const auto& x : items) v.push_back(x.fnot());
      return of_or(std::move(v));
    }
    case Kind::Or: {
      std::vector<Formula> v;
      v.reserve(items.size());
      for (const auto& x : items) v.push_back(x.fnot());
      return of_and(std::move(v));
    }
    case Kind::Unknown:
      return unknown(diag);
    case Kind::Dual:
      return dual(minus->fnot(), plus->fnot(), diag);
  }
  return unknown(diag);
}

bool Formula::is_exact() const {
  switch (kind) {
    case Kind::True:
    case Kind::False:
    case Kind::Atom:
      return true;
    case Kind::And:
    case Kind::Or:
      for (const auto& x : items) {
        if (!x.is_exact()) return false;
      }
      return true;
    case Kind::Unknown:
    case Kind::Dual:
      return false;
  }
  return false;
}

QFormula Formula::project(Polarity p) const {
  switch (kind) {
    case Kind::True:
      return QFormula::ttrue();
    case Kind::False:
      return QFormula::ffalse();
    case Kind::Atom:
      return QFormula::of_atom(atom);
    case Kind::And: {
      std::vector<QFormula> v;
      v.reserve(items.size());
      for (const auto& x : items) v.push_back(x.project(p));
      return QFormula::of_and(std::move(v));
    }
    case Kind::Or: {
      std::vector<QFormula> v;
      v.reserve(items.size());
      for (const auto& x : items) v.push_back(x.project(p));
      return QFormula::of_or(std::move(v));
    }
    case Kind::Unknown:
      return p == Polarity::Over ? QFormula::ttrue() : QFormula::ffalse();
    case Kind::Dual:
      return p == Polarity::Over ? plus->project(p) : minus->project(p);
  }
  return QFormula::ttrue();
}

Over Formula::over() const { return Over(project(Polarity::Over)); }
Under Formula::under() const { return Under(project(Polarity::Under)); }

namespace {

bool is_presence(const adl2::sema::QuantityTable& table, const Formula& f) {
  if (f.kind != Formula::Kind::Atom) return false;
  if (f.atom.terms().size() != 1) return false;
  return table.quantity(f.atom.terms()[0].second).kind ==
         adl2::sema::QuantityKind::Present;
}

Formula strip(const adl2::sema::QuantityTable& table, const Formula& f) {
  if (f.kind == Formula::Kind::And || f.kind == Formula::Kind::Or) {
    bool is_and = f.kind == Formula::Kind::And;
    std::vector<Formula> kept;
    for (const auto& p : f.items) {
      if (is_presence(table, p)) continue;
      kept.push_back(strip(table, p));
    }
    if (kept.empty()) return is_and ? Formula::ttrue() : Formula::ffalse();
    if (kept.size() == 1) return std::move(kept[0]);
    return is_and ? Formula::of_and(std::move(kept)) : Formula::of_or(std::move(kept));
  }
  if (f.kind == Formula::Kind::Dual) {
    return Formula::dual(strip(table, *f.plus), strip(table, *f.minus), f.diag);
  }
  if (f.kind == Formula::Kind::Atom && is_presence(table, f)) return Formula::ttrue();
  return f;
}

}  // namespace

Formula Formula::without_presence(const adl2::sema::QuantityTable& table) const {
  return strip(table, *this);
}

bool Formula::operator==(const Formula& o) const {
  if (kind != o.kind) return false;
  switch (kind) {
    case Kind::True:
    case Kind::False:
      return true;
    case Kind::Atom:
      return atom == o.atom;
    case Kind::And:
    case Kind::Or:
      return items == o.items;
    case Kind::Unknown:
      return diag == o.diag;
    case Kind::Dual:
      return diag == o.diag && plus && o.plus && minus && o.minus &&
             *plus == *o.plus && *minus == *o.minus;
  }
  return false;
}

QFormula QFormula::qnot() const {
  switch (kind) {
    case Kind::True:
      return ffalse();
    case Kind::False:
      return ttrue();
    case Kind::Atom:
      return of_atom(atom.negated());
    case Kind::And: {
      std::vector<QFormula> v;
      v.reserve(items.size());
      for (const auto& x : items) v.push_back(x.qnot());
      return of_or(std::move(v));
    }
    case Kind::Or: {
      std::vector<QFormula> v;
      v.reserve(items.size());
      for (const auto& x : items) v.push_back(x.qnot());
      return of_and(std::move(v));
    }
  }
  return ttrue();
}

bool QFormula::operator==(const QFormula& o) const {
  if (kind != o.kind) return false;
  switch (kind) {
    case Kind::True:
    case Kind::False:
      return true;
    case Kind::Atom:
      return atom == o.atom;
    case Kind::And:
    case Kind::Or:
      return items == o.items;
  }
  return false;
}

bool QFormula::operator<(const QFormula& o) const {
  if (kind != o.kind) return static_cast<int>(kind) < static_cast<int>(o.kind);
  switch (kind) {
    case Kind::True:
    case Kind::False:
      return false;
    case Kind::Atom:
      return atom < o.atom;
    case Kind::And:
    case Kind::Or:
      return items < o.items;
  }
  return false;
}

Formula fand(std::vector<Formula> parts) {
  std::vector<Formula> out;
  for (auto& p : parts) {
    if (p.kind == Formula::Kind::True) continue;
    if (p.kind == Formula::Kind::False) return Formula::ffalse();
    if (p.kind == Formula::Kind::And) {
      for (auto& x : p.items) out.push_back(std::move(x));
    } else {
      out.push_back(std::move(p));
    }
  }
  if (out.empty()) return Formula::ttrue();
  if (out.size() == 1) return std::move(out[0]);
  return Formula::of_and(std::move(out));
}

Formula forr(std::vector<Formula> parts) {
  std::vector<Formula> out;
  for (auto& p : parts) {
    if (p.kind == Formula::Kind::False) continue;
    if (p.kind == Formula::Kind::True) return Formula::ttrue();
    if (p.kind == Formula::Kind::Or) {
      for (auto& x : p.items) out.push_back(std::move(x));
    } else {
      out.push_back(std::move(p));
    }
  }
  if (out.empty()) return Formula::ffalse();
  if (out.size() == 1) return std::move(out[0]);
  return Formula::of_or(std::move(out));
}

int module_anchor() { return 3; }

}  // namespace adl2::formula
