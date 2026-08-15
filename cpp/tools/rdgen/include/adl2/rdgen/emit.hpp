#pragma once

#include "adl2/rdgen/check.hpp"
#include "adl2/rdgen/ebnf.hpp"

#include <string>

namespace adl2::rdgen {

/// Emit mechanical parse_* bodies (`role = generate`): expression ladder,
/// ternary, and keyword+condition region statements. AST construction is
/// inferred from EBNF shape + the literal catalog / sibling synonyms.
bool emit_generated(const Grammar& g, const MethodMap& map, std::string& out,
                    std::string& error);

/// Back-compat name used by the first slice.
inline bool emit_expr_ladder(const Grammar& g, const MethodMap& map,
                             std::string& out, std::string& error) {
  return emit_generated(g, map, out, error);
}

}  // namespace adl2::rdgen
