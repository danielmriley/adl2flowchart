#pragma once

/// Witness extraction + interpreter re-validation (Rust `adl-analysis::witness`).
///
/// SAT-direction proofs are re-validated through Kleene `region3`
/// (`Interp::eval_region_membership_idx`), never two-valued `run`.
/// A failed validation MUST downgrade; the verifier cannot display a
/// witness the interpreter rejects. Opaque quantities stay Candidate.

#include "adl2/interp/interp.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/solver/solver.hpp"

#include <set>
#include <string>

namespace adl2::analysis {

enum class ValidationKind { Validated, Candidate, Rejected };

struct Validation {
  ValidationKind kind = ValidationKind::Rejected;
  std::string payload;  // JSON event if Validated; reason otherwise

  static Validation validated(std::string json) {
    Validation v;
    v.kind = ValidationKind::Validated;
    v.payload = std::move(json);
    return v;
  }
  static Validation candidate(std::string why) {
    Validation v;
    v.kind = ValidationKind::Candidate;
    v.payload = std::move(why);
    return v;
  }
  static Validation rejected(std::string why) {
    Validation v;
    v.kind = ValidationKind::Rejected;
    v.payload = std::move(why);
    return v;
  }
};

/// Realize `model` as an Event and require both regions to accept it.
Validation validate_witness(const adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext,
                            const adl2::interp::Interp& interp,
                            const adl2::solver::Model& model,
                            const std::set<adl2::sema::QuantityId>& mentioned,
                            std::size_t region_a, std::size_t region_b);

}  // namespace adl2::analysis
