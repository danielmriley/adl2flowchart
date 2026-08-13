#pragma once

/// HIR → Formula region encoder (SPEC_ANALYSIS §1, Rust `adl-formula::encode`).

#include "adl2/formula/formula.hpp"
#include "adl2/sema/hir.hpp"

#include <cstdint>
#include <string>
#include <vector>

namespace adl2::formula {

/// OPEN-1 bounded-expansion depth (PHASE0: k = 3).
constexpr std::uint32_t OPEN1_BOUND = 3;
constexpr std::uint32_t MAX_STATIC_SLICE_REDUCE = 1024;
constexpr std::uint32_t COMB2D_BOUND = 2;

struct EncodedRegion {
  std::size_t region = 0;
  std::string name;
  Formula formula;
  DiagTable diags;
  bool is_exact() const { return formula.is_exact(); }
};

EncodedRegion encode_region(adl2::sema::Hir& hir, std::size_t region);
std::vector<EncodedRegion> encode_regions(adl2::sema::Hir& hir);

}  // namespace adl2::formula
