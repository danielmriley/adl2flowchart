#include "adl2/analysis/witness.hpp"

namespace adl2::analysis {

Validation validate_witness(const adl2::sema::Hir&, const adl2::sema::ExtDecls&,
                            const adl2::interp::Interp&, const adl2::solver::Model&,
                            const std::set<adl2::sema::QuantityId>&, std::size_t,
                            std::size_t) {
  // Filled by the witness agent. Stub never promotes to PROVEN OVERLAPPING.
  return Validation::candidate("witness realization not filled");
}

}  // namespace adl2::analysis
