#pragma once

/// Exact-rational Fourier–Motzkin (Rust `fm.rs`). Untrusted search.

#include "adl2/sema/rat.hpp"

#include "constraint.hpp"

#include <cstddef>
#include <vector>

namespace adl2::certify {

enum class LeafResultKind { Refuted, Feasible, TooBig };

struct LeafResult {
  LeafResultKind kind = LeafResultKind::Feasible;
  std::vector<adl2::sema::Rat> multipliers;  // set iff Refuted
};

LeafResult solve_leaf(const std::vector<Constraint>& cons, std::size_t fill_cap);

}  // namespace adl2::certify
