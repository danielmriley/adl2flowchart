#pragma once

/// `adl2_sema` — name resolution, typed Quantity/Collection identity, define
/// resolution, fragment tagging, and HIR (Rust `adl-sema` / SPEC_ARCHITECTURE §4).
///
/// Syntax is a PRIVATE CMake dependency: these headers do not include parser
/// types. Downstream (formula / interp / viz / analysis) consumes HIR only.

#include "adl2/sema/diag.hpp"
#include "adl2/sema/dump.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/sema/intern.hpp"
#include "adl2/sema/ops.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/resolve.hpp"

namespace adl2::sema {

/// Linkable anchor (kept so existing stub-graph checks still resolve).
int module_anchor();

}  // namespace adl2::sema
