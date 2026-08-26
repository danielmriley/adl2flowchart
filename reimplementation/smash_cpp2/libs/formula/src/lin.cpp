#include "adl2/formula/lin.hpp"

#include <map>

namespace adl2::formula {

LinAtom LinAtom::make(std::vector<Term> terms, Rel rel, adl2::sema::Rat constant) {
  std::map<adl2::sema::QuantityId, adl2::sema::Rat> merged;
  for (auto& t : terms) {
    auto it = merged.find(t.second);
    if (it == merged.end()) merged.emplace(t.second, std::move(t.first));
    else it->second = it->second + t.first;
  }
  LinAtom a;
  a.rel_ = rel;
  a.constant_ = std::move(constant);
  for (auto& kv : merged) {
    if (!kv.second.is_zero()) a.terms_.emplace_back(std::move(kv.second), kv.first);
  }
  return a;
}

LinAtom LinAtom::single(adl2::sema::QuantityId q, Rel rel, adl2::sema::Rat constant) {
  return make({{adl2::sema::Rat::one(), q}}, rel, std::move(constant));
}

LinAtom LinAtom::negated() const {
  LinAtom a = *this;
  a.rel_ = rel_negated(rel_);
  return a;
}

}  // namespace adl2::formula
