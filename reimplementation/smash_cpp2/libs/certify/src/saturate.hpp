#pragma once

/// Boolean saturation — shared by search and trusted replay (Rust `saturate.rs`).

#include "adl2/formula/formula.hpp"

#include "constraint.hpp"

#include <cstddef>
#include <optional>
#include <vector>

namespace adl2::certify {

struct Saturated {
  bool has_false = false;
  std::vector<adl2::formula::QFormula> items;
};

Saturated saturate(const std::vector<adl2::formula::QFormula>& conj);

std::optional<std::size_t> leftmost_or_index(
    const std::vector<adl2::formula::QFormula>& items);

const std::vector<adl2::formula::QFormula>& disjuncts(
    const adl2::formula::QFormula& item);

std::vector<Constraint> collect_constraints(
    const std::vector<adl2::formula::QFormula>& items);

std::vector<adl2::formula::QFormula> build_child(
    const std::vector<adl2::formula::QFormula>& items, std::size_t or_index,
    const adl2::formula::QFormula& chosen);

}  // namespace adl2::certify
