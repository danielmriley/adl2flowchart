#pragma once

/// SMT-LIB2 reply parsers used by the subprocess backend (Rust
/// `parse_value_list` / `parse_symbol_list`). Public so unit tests can
/// cover s-expression model/core parsing with no z3 binary.

#include "adl2/sema/rat.hpp"

#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::solver {

/// Parse `((q0 v0) (q1 v1) …)` from a `(get-value …)` reply.
std::optional<std::vector<std::pair<std::string, adl2::sema::Rat>>>
parse_value_list(const std::string& reply);

/// Parse `(n1 n7 …)` from a `(get-unsat-core)` reply.
std::optional<std::vector<std::string>> parse_symbol_list(const std::string& reply);

}  // namespace adl2::solver
