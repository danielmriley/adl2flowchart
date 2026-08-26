#pragma once

/// Search-free bound-pair certificate producer (Rust `direct.rs`). Untrusted.

#include "adl2/certify/certify.hpp"
#include "adl2/formula/formula.hpp"

#include <optional>
#include <vector>

namespace adl2::certify {

/// Unchecked construction. Callers must replay before returning.
std::optional<Certificate> construct_bounds(
    const std::vector<adl2::formula::QFormula>& formulas);

}  // namespace adl2::certify
