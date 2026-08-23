#include "constraint.hpp"

#include <map>

namespace adl2::certify {

Constraint canonicalize(const adl2::formula::LinAtom& atom) {
  bool flip = false;
  bool strict = false;
  switch (atom.rel()) {
    case adl2::formula::Rel::Le:
      flip = false;
      strict = false;
      break;
    case adl2::formula::Rel::Lt:
      flip = false;
      strict = true;
      break;
    case adl2::formula::Rel::Ge:
      flip = true;
      strict = false;
      break;
    case adl2::formula::Rel::Gt:
      flip = true;
      strict = true;
      break;
    // Unreachable after saturate splits Eq/Ne. Treat as non-strict ≤.
    case adl2::formula::Rel::Eq:
    case adl2::formula::Rel::Ne:
      flip = false;
      strict = false;
      break;
  }

  Constraint out;
  out.strict = strict;
  out.coeffs.reserve(atom.terms().size());
  for (const auto& t : atom.terms()) {
    out.coeffs.emplace_back(t.second, flip ? -t.first : t.first);
  }
  out.b = flip ? -atom.constant() : atom.constant();
  return out;
}

bool farkas_refutes(const std::vector<Constraint>& cons,
                    const std::vector<adl2::sema::Rat>& lambdas) {
  if (lambdas.size() != cons.size()) return false;

  std::map<adl2::sema::QuantityId, adl2::sema::Rat> acc;
  adl2::sema::Rat s = adl2::sema::Rat::zero();
  bool any_strict = false;

  for (std::size_t i = 0; i < cons.size(); ++i) {
    const Constraint& con = cons[i];
    const adl2::sema::Rat& lam = lambdas[i];
    if (lam.is_negative()) return false;
    if (lam.is_zero()) continue;

    for (const auto& qc : con.coeffs) {
      adl2::sema::Rat term = lam * qc.second;
      auto it = acc.find(qc.first);
      if (it == acc.end()) {
        acc.emplace(qc.first, std::move(term));
      } else {
        it->second = it->second + term;
      }
    }
    s = s + (lam * con.b);
    if (con.strict) any_strict = true;
  }

  for (const auto& kv : acc) {
    if (!kv.second.is_zero()) return false;
  }

  if (any_strict) {
    // Derived `0 < S`; contradictory iff S ≤ 0.
    return !s.is_positive();
  }
  // Derived `0 ≤ S`; contradictory iff S < 0.
  return s.is_negative();
}

}  // namespace adl2::certify
