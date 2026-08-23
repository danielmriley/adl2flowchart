#pragma once

/// Region/bin encoding at statement granularity (Rust `adl-analysis::encode`).
/// Reuses `adl2_formula`'s `encode_region` on a synthetic single-statement
/// region so unsat cores can name individual cuts. Does not include parser
/// headers: spans are `adl2::sema::Span`.

#include "adl2/formula/encode.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/solver/assert_name.hpp"

#include <cstdint>
#include <optional>
#include <set>
#include <string>
#include <vector>

namespace adl2::analysis {

/// One encoded membership statement.
struct StmtEnc {
  adl2::solver::AssertName name;
  adl2::sema::Span span;
  std::uint32_t line = 0;
  std::string text;
  adl2::formula::Formula formula;
  adl2::formula::DiagTable diags;
  /// Filled at encode time so `build_ctx` / certify do not re-walk Dual.
  std::optional<adl2::formula::Over> cached_over;
  std::optional<adl2::formula::Under> cached_under;

  adl2::formula::Over over() const {
    return cached_over ? *cached_over : formula.over();
  }
  adl2::formula::Under under() const {
    return cached_under ? *cached_under : formula.under();
  }
};

/// One region, encoded at statement granularity.
struct RegionEnc {
  std::size_t idx = 0;
  std::string name;
  std::vector<StmtEnc> stmts;
  std::set<adl2::sema::QuantityId> quantities;
  std::size_t leaves_total = 0;
  std::size_t leaves_encoded = 0;
  std::size_t or_clauses = 0;
  std::size_t dual_hedges = 0;
  std::vector<std::pair<std::uint32_t, std::string>> dropped;

  bool exact() const {
    for (const auto& s : stmts) {
      if (!s.formula.is_exact()) return false;
    }
    return true;
  }
};

/// One bin set: a boundary-list `bin` statement or the region's boolean bins.
struct BinSetEnc {
  std::size_t region_idx = 0;
  std::string variable;
  std::vector<adl2::formula::Formula> bins;
  std::vector<adl2::formula::Over> overs;
  std::vector<adl2::formula::Under> unders;
};

/// The encoded analysis unit.
struct UnitEnc {
  std::vector<RegionEnc> regions;
  std::vector<BinSetEnc> bin_sets;
};

/// Verifier-side re-tag of undeclared-external-function quantities. Only
/// `HKind::Quantity(ExternalFn)` nodes whose only problem is "not declared
/// in the external library" are touched.
void retag_opaque_externals(adl2::sema::Hir& hir);

/// Every quantity in a formula, both Dual branches included.
void formula_quantities(const adl2::formula::Formula& f,
                        std::set<adl2::sema::QuantityId>& out);

/// Encode every region (statement granularity) and every bin set.
UnitEnc encode_unit(adl2::sema::Hir& hir, const std::string& src);

}  // namespace adl2::analysis
