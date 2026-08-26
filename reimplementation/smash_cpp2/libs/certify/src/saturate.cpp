#include "saturate.hpp"

#include <utility>

namespace adl2::certify {
namespace {

adl2::formula::LinAtom retagged(const adl2::formula::LinAtom& atom,
                                adl2::formula::Rel rel) {
  return adl2::formula::LinAtom::make(atom.terms(), rel, atom.constant());
}

}  // namespace

Saturated saturate(const std::vector<adl2::formula::QFormula>& conj) {
  Saturated out;
  // Push reversed so pop_back walks left-to-right, matching Rust's Vec::pop.
  std::vector<adl2::formula::QFormula> stack;
  stack.reserve(conj.size());
  for (auto it = conj.rbegin(); it != conj.rend(); ++it) {
    stack.push_back(*it);
  }

  while (!stack.empty()) {
    adl2::formula::QFormula f = std::move(stack.back());
    stack.pop_back();
    switch (f.kind) {
      case adl2::formula::QFormula::Kind::True:
        break;
      case adl2::formula::QFormula::Kind::False:
        out.has_false = true;
        break;
      case adl2::formula::QFormula::Kind::And:
        for (auto it = f.items.rbegin(); it != f.items.rend(); ++it) {
          stack.push_back(*it);
        }
        break;
      case adl2::formula::QFormula::Kind::Or:
        out.items.push_back(std::move(f));
        break;
      case adl2::formula::QFormula::Kind::Atom:
        switch (f.atom.rel()) {
          case adl2::formula::Rel::Eq:
            // x = k  ⇔  x ≤ k ∧ x ≥ k
            out.items.push_back(adl2::formula::QFormula::of_atom(
                retagged(f.atom, adl2::formula::Rel::Le)));
            out.items.push_back(adl2::formula::QFormula::of_atom(
                retagged(f.atom, adl2::formula::Rel::Ge)));
            break;
          case adl2::formula::Rel::Ne:
            // x ≠ k  ⇔  x < k ∨ x > k
            out.items.push_back(adl2::formula::QFormula::of_or({
                adl2::formula::QFormula::of_atom(
                    retagged(f.atom, adl2::formula::Rel::Lt)),
                adl2::formula::QFormula::of_atom(
                    retagged(f.atom, adl2::formula::Rel::Gt)),
            }));
            break;
          default:
            out.items.push_back(std::move(f));
            break;
        }
        break;
    }
  }
  return out;
}

std::optional<std::size_t> leftmost_or_index(
    const std::vector<adl2::formula::QFormula>& items) {
  for (std::size_t i = 0; i < items.size(); ++i) {
    if (items[i].kind == adl2::formula::QFormula::Kind::Or) return i;
  }
  return std::nullopt;
}

const std::vector<adl2::formula::QFormula>& disjuncts(
    const adl2::formula::QFormula& item) {
  static const std::vector<adl2::formula::QFormula> kEmpty;
  if (item.kind == adl2::formula::QFormula::Kind::Or) return item.items;
  return kEmpty;
}

std::vector<Constraint> collect_constraints(
    const std::vector<adl2::formula::QFormula>& items) {
  std::vector<Constraint> out;
  out.reserve(items.size());
  for (const auto& f : items) {
    if (f.kind == adl2::formula::QFormula::Kind::Atom) {
      out.push_back(canonicalize(f.atom));
    }
  }
  return out;
}

std::vector<adl2::formula::QFormula> build_child(
    const std::vector<adl2::formula::QFormula>& items, std::size_t or_index,
    const adl2::formula::QFormula& chosen) {
  std::vector<adl2::formula::QFormula> child;
  child.reserve(items.size());
  for (std::size_t i = 0; i < items.size(); ++i) {
    if (i != or_index) child.push_back(items[i]);
  }
  child.push_back(chosen);
  return child;
}

}  // namespace adl2::certify
