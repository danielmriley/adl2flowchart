#pragma once

#include "adl2/rdgen/check.hpp"
#include "adl2/rdgen/ebnf.hpp"

#include <string>

namespace adl2::rdgen {

/// Emit mechanical parse_* bodies (`role = generate`): expression ladder,
/// ternary, and keyword+condition region statements. AST construction is
/// inferred from EBNF shape + the literal catalog / alias table. New
/// words keep their own `bin_key`; they do not inherit a sibling BinOp.
/// Choice dispatchers live in `emit_dispatch`.
bool emit_generated(const Grammar& g, const MethodMap& map, std::string& out,
                    std::string& error);

/// Emit `parse_section` / `parse_region_stmt` / first-set predicates
/// (`at_section_start`, `at_stmt_keyword`, `is_cut_keyword`,
/// `is_reject_keyword`). Unmapped `keywords condition` productions are
/// inlined as `Cut` with `keyword = lowercase(token.text)`.
bool emit_dispatch(const Grammar& g, const MethodMap& map, std::string& out,
                   std::string& error);

/// Back-compat name used by the first slice.
inline bool emit_expr_ladder(const Grammar& g, const MethodMap& map,
                             std::string& out, std::string& error) {
  return emit_generated(g, map, out, error);
}

}  // namespace adl2::rdgen
