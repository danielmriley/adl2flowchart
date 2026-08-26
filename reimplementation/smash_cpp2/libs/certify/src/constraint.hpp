#pragma once

/// Canonical upper-bound form and the Farkas refutation primitive.
/// Trusted kernel (Rust `constraint.rs`): replay relies on these.

#include "adl2/formula/lin.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"

#include <vector>

namespace adl2::certify {

struct Constraint {
  std::vector<std::pair<adl2::sema::QuantityId, adl2::sema::Rat>> coeffs;
  adl2::sema::Rat b;
  bool strict = false;
};

Constraint canonicalize(const adl2::formula::LinAtom& atom);

/// Nonnegative λ, same order as `cons`. True iff they witness real-infeasibility.
bool farkas_refutes(const std::vector<Constraint>& cons,
                    const std::vector<adl2::sema::Rat>& lambdas);

}  // namespace adl2::certify
