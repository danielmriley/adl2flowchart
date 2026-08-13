#pragma once

/// Linear atoms `Σ cᵢ·Quantityᵢ ⋈ k` over interned QuantityIds, with exact
/// rational coefficients (Rust `adl_formula::lin`). Construction is total.

#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"

#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace adl2::formula {

enum class Rel : std::uint8_t { Lt, Le, Gt, Ge, Eq, Ne };

inline Rel rel_negated(Rel r) {
  switch (r) {
    case Rel::Lt: return Rel::Ge;
    case Rel::Le: return Rel::Gt;
    case Rel::Gt: return Rel::Le;
    case Rel::Ge: return Rel::Lt;
    case Rel::Eq: return Rel::Ne;
    case Rel::Ne: return Rel::Eq;
  }
  return r;
}

inline Rel rel_flipped(Rel r) {
  switch (r) {
    case Rel::Lt: return Rel::Gt;
    case Rel::Le: return Rel::Ge;
    case Rel::Gt: return Rel::Lt;
    case Rel::Ge: return Rel::Le;
    case Rel::Eq:
    case Rel::Ne:
      return r;
  }
  return r;
}

inline const char* rel_str(Rel r) {
  switch (r) {
    case Rel::Lt: return "<";
    case Rel::Le: return "<=";
    case Rel::Gt: return ">";
    case Rel::Ge: return ">=";
    case Rel::Eq: return "==";
    case Rel::Ne: return "!=";
  }
  return "?";
}

inline bool rel_eval(Rel r, const adl2::sema::Rat& lhs, const adl2::sema::Rat& rhs) {
  switch (r) {
    case Rel::Lt: return lhs < rhs;
    case Rel::Le: return lhs <= rhs;
    case Rel::Gt: return lhs > rhs;
    case Rel::Ge: return lhs >= rhs;
    case Rel::Eq: return lhs == rhs;
    case Rel::Ne: return lhs != rhs;
  }
  return false;
}

class LinAtom {
 public:
  using Term = std::pair<adl2::sema::Rat, adl2::sema::QuantityId>;

  static LinAtom make(std::vector<Term> terms, Rel rel, adl2::sema::Rat constant);
  static LinAtom single(adl2::sema::QuantityId q, Rel rel, adl2::sema::Rat constant);

  const std::vector<Term>& terms() const { return terms_; }
  Rel rel() const { return rel_; }
  const adl2::sema::Rat& constant() const { return constant_; }
  LinAtom negated() const;

  bool operator==(const LinAtom& o) const {
    return terms_ == o.terms_ && rel_ == o.rel_ && constant_ == o.constant_;
  }
  bool operator!=(const LinAtom& o) const { return !(*this == o); }

 private:
  std::vector<Term> terms_;
  Rel rel_ = Rel::Eq;
  adl2::sema::Rat constant_;
};

}  // namespace adl2::formula
