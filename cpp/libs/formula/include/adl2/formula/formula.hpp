#pragma once

/// Polarity-aware formula IR (SPEC_ARCHITECTURE §5, Rust `adl-formula`).
/// Soundness direction is a type: only Formula::over / Formula::under
/// construct Over / Under wrappers around Unknown/Dual-free QFormula.

#include "adl2/formula/lin.hpp"
#include "adl2/sema/diag.hpp"
#include "adl2/sema/quantity.hpp"

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace adl2::formula {

struct DiagId {
  std::uint32_t id = 0;
  std::string to_string() const { return "D" + std::to_string(id); }
  bool operator==(DiagId o) const { return id == o.id; }
};

struct FormulaDiag {
  adl2::sema::Span span;
  std::string reason;
};

class DiagTable {
 public:
  DiagId push(adl2::sema::Span span, std::string reason) {
    DiagId id{static_cast<std::uint32_t>(entries_.size())};
    FormulaDiag d;
    d.span = span;
    d.reason = std::move(reason);
    entries_.push_back(std::move(d));
    return id;
  }
  const FormulaDiag* get(DiagId id) const {
    if (id.id >= entries_.size()) return nullptr;
    return &entries_[id.id];
  }
  const std::vector<FormulaDiag>& entries() const { return entries_; }
  std::size_t size() const { return entries_.size(); }
  bool empty() const { return entries_.empty(); }

 private:
  std::vector<FormulaDiag> entries_;
};

struct QFormula {
  enum class Kind { True, False, Atom, And, Or };
  Kind kind = Kind::True;
  LinAtom atom;
  std::vector<QFormula> items;

  static QFormula ttrue() {
    QFormula f;
    f.kind = Kind::True;
    return f;
  }
  static QFormula ffalse() {
    QFormula f;
    f.kind = Kind::False;
    return f;
  }
  static QFormula of_atom(LinAtom a) {
    QFormula f;
    f.kind = Kind::Atom;
    f.atom = std::move(a);
    return f;
  }
  static QFormula of_and(std::vector<QFormula> v) {
    QFormula f;
    f.kind = Kind::And;
    f.items = std::move(v);
    return f;
  }
  static QFormula of_or(std::vector<QFormula> v) {
    QFormula f;
    f.kind = Kind::Or;
    f.items = std::move(v);
    return f;
  }

  QFormula qnot() const;
  bool operator==(const QFormula& o) const;
  bool operator!=(const QFormula& o) const { return !(*this == o); }
};

/// Over-approximation R⁺ ⊇ R. The only constructor is Formula::over.
class Over {
 public:
  const QFormula& qformula() const { return q_; }
  QFormula into_qformula() && { return std::move(q_); }

 private:
  friend struct Formula;
  explicit Over(QFormula q) : q_(std::move(q)) {}
  QFormula q_;
};

/// Under-approximation R⁻ ⊆ R. The only constructor is Formula::under.
class Under {
 public:
  const QFormula& qformula() const { return q_; }
  QFormula into_qformula() && { return std::move(q_); }

 private:
  friend struct Formula;
  explicit Under(QFormula q) : q_(std::move(q)) {}
  QFormula q_;
};

struct Formula {
  enum class Kind { True, False, Atom, And, Or, Unknown, Dual };
  Kind kind = Kind::True;
  LinAtom atom;
  std::vector<Formula> items;
  DiagId diag;
  std::unique_ptr<Formula> plus;
  std::unique_ptr<Formula> minus;

  Formula() = default;
  Formula(const Formula& o);
  Formula& operator=(const Formula& o);
  Formula(Formula&&) noexcept = default;
  Formula& operator=(Formula&&) noexcept = default;

  static Formula ttrue() {
    Formula f;
    f.kind = Kind::True;
    return f;
  }
  static Formula ffalse() {
    Formula f;
    f.kind = Kind::False;
    return f;
  }
  static Formula of_atom(LinAtom a) {
    Formula f;
    f.kind = Kind::Atom;
    f.atom = std::move(a);
    return f;
  }
  static Formula of_and(std::vector<Formula> v) {
    Formula f;
    f.kind = Kind::And;
    f.items = std::move(v);
    return f;
  }
  static Formula of_or(std::vector<Formula> v) {
    Formula f;
    f.kind = Kind::Or;
    f.items = std::move(v);
    return f;
  }
  static Formula unknown(DiagId d) {
    Formula f;
    f.kind = Kind::Unknown;
    f.diag = d;
    return f;
  }
  static Formula dual(Formula p, Formula m, DiagId why);

  /// Exact NNF negation. Dual swaps branches. Involutive.
  Formula fnot() const;
  Over over() const;
  Under under() const;
  bool is_exact() const;
  /// Reading aid: drop presence bookkeeping literals. Never a proof input.
  Formula without_presence(const adl2::sema::QuantityTable& table) const;

  bool operator==(const Formula& o) const;
  bool operator!=(const Formula& o) const { return !(*this == o); }

 private:
  enum class Polarity { Over, Under };
  QFormula project(Polarity p) const;
};

/// n-ary And with constant folding (drops True, collapses False, flattens).
Formula fand(std::vector<Formula> parts);
/// n-ary Or with constant folding (dual of fand).
Formula forr(std::vector<Formula> parts);

int module_anchor();

}  // namespace adl2::formula
