#pragma once

/// Name resolution: source → HIR + QuantityTable (SPEC_ARCHITECTURE §4).
/// Public entry is `analyze_str` so downstream modules never see parser headers.

#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"

#include <string>

namespace adl2::sema {

/// Parse `src` and resolve it. Parse diagnostics are merged in front of
/// sema diagnostics (Rust `analyze_str`).
Hir analyze_str(const std::string& src, const std::string& unit,
                const ExtDecls& ext);

}  // namespace adl2::sema
