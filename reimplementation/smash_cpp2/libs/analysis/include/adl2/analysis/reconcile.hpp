#pragma once

/// Cross/intra-collection reconciliation encoding (Rust `adl-analysis::reconcile`).
/// Pure encoding: interns helper quantities and produces a Formula per
/// candidate. Emits no solver fact. The engine proves refinement and only
/// then asserts derived size axioms (XSUB/XEQ).

#include "adl2/formula/formula.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/sema/quantity.hpp"

#include <set>
#include <string>
#include <vector>

namespace adl2::analysis {

struct ReconCandidate {
  adl2::sema::QuantityId size_a;
  adl2::sema::QuantityId size_b;
  adl2::formula::Formula phi_a;
  adl2::formula::Formula phi_b;
  adl2::sema::CollectionId coll_a;
  adl2::sema::CollectionId coll_b;
};

struct ReconSkip {
  adl2::sema::CollectionId coll_a;
  adl2::sema::CollectionId coll_b;
  std::string reason;
};

struct ReconNearMiss {
  adl2::sema::CollectionId coll_a;
  adl2::sema::CollectionId coll_b;
  std::string base_a;
  std::string base_b;
};

struct ReconEnc {
  std::vector<ReconCandidate> candidates;
  std::vector<ReconSkip> skipped;
  std::vector<ReconNearMiss> near_misses;

  std::set<adl2::sema::QuantityId> quantities() const;
  bool empty() const { return candidates.empty(); }
};

/// Intern generic-element quantities and lower candidate filter chains.
/// MUST run after `emit_axioms` so helpers receive no base axioms.
ReconEnc build_recon(adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext);

}  // namespace adl2::analysis
