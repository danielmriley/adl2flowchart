#pragma once

/// Canonical formula dump (P3 oracle form). Byte-for-byte vs smash2
/// `check --dump-formula` once that flag is wired on the Rust side.

#include "adl2/formula/formula.hpp"
#include "adl2/sema/hir.hpp"

#include <string>
#include <vector>

namespace adl2::formula {

struct EncodedRegion;

std::string dump_formula(const Formula& f);
std::string dump_qformula(const QFormula& f);
std::string dump_encoded(const adl2::sema::Hir& hir,
                         const std::vector<EncodedRegion>& regions);

}  // namespace adl2::formula
