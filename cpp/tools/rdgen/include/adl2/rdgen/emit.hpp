#pragma once

#include "adl2/rdgen/check.hpp"
#include "adl2/rdgen/ebnf.hpp"

#include <string>

namespace adl2::rdgen {

/// Emit mechanical expression-ladder method bodies (`role = generate`).
/// Returns false and writes `error` if a generate production is not an
/// emit shape or an operator literal is unknown.
bool emit_expr_ladder(const Grammar& g, const MethodMap& map, std::string& out,
                      std::string& error);

}  // namespace adl2::rdgen
