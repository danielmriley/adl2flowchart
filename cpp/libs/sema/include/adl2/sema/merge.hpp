#pragma once

/// Merge several resolved units into one Hir with a shared structural
/// quantity-identity space (Rust `adl_sema::merge_hirs`). Callers must pass
/// error-free units. Merged regions are named `<unit>::<region>`.

#include "adl2/sema/hir.hpp"

#include <vector>

namespace adl2::sema {

Hir merge_hirs(const std::vector<const Hir*>& units);

}  // namespace adl2::sema
