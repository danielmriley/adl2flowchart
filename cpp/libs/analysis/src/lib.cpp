#include "adl2/analysis/analysis.hpp"

#include "adl2/axioms/axioms.hpp"
#include "adl2/sema/quantity.hpp"

#include <sstream>
#include <utility>
#include <vector>

namespace adl2::analysis {
namespace {

using adl2::sema::Hir;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;

struct RegionCtx {
  IntervalMap intervals;
};

RegionCtx build_ctx(const RegionEnc& r) {
  RegionCtx ctx;
  for (const auto& s : r.stmts) {
    auto o = s.over();
    ctx.intervals.add_over(s.name, o);
  }
  return ctx;
}

Presence presence_of(const Hir& hir, QuantityId q) {
  if (!hir.table.may_be_absent(q)) return Presence::total();
  auto pid = hir.table.quantity_id(Quantity::present(q));
  if (!pid) return Presence::unpinned();
  return Presence::of_indicator(*pid);
}

std::vector<std::string> shared_dims(const Hir& hir, const RegionEnc& ra,
                                     const RegionEnc& rb) {
  std::vector<std::string> out;
  for (auto q : ra.quantities) {
    if (rb.quantities.count(q) == 0) continue;
    if (hir.table.quantity(q).kind == QuantityKind::Present) continue;
    out.push_back(adl2::axioms::quantity_label(hir, q));
  }
  return out;
}

}  // namespace

Report analyze_hir(Hir& hir, const std::string& src, const adl2::sema::ExtDecls& ext,
                   const AnalysisOptions& opts) {
  (void)ext;
  (void)opts;
  // Interval-only commit: no solver, certify, reconcile, refute, or sample.
  retag_opaque_externals(hir);
  UnitEnc unit = encode_unit(hir, src);

  std::vector<RegionCtx> ctxs;
  ctxs.reserve(unit.regions.size());
  for (const auto& r : unit.regions) ctxs.push_back(build_ctx(r));

  Report report;
  report.schema_version = SCHEMA_VERSION;
  report.unit = hir.unit;
  report.solver = "none";
  report.certification = false;

  for (std::size_t i = 0; i < unit.regions.size(); ++i) {
    const RegionEnc& r = unit.regions[i];
    RegionReport rr;
    rr.name = r.name;
    rr.leaves_encoded = r.leaves_encoded;
    rr.leaves_total = r.leaves_total;
    rr.exact = r.exact();
    rr.or_clauses = r.or_clauses;
    rr.dual_hedges = r.dual_hedges;
    for (const auto& d : r.dropped) {
      DroppedLeaf leaf;
      leaf.line = d.first;
      leaf.reason = d.second;
      rr.dropped.push_back(std::move(leaf));
    }
    if (auto empty = ctxs[i].intervals.self_empty()) {
      rr.empty = EmptyStatus::Proven;
      rr.empty_proof = ProofPath::Interval;
    } else {
      // No solver in this commit: interval miss is UNKNOWN, not a SAT claim.
      rr.empty = EmptyStatus::Unknown;
    }
    report.regions.push_back(std::move(rr));
  }

  auto presence = [&](QuantityId q) { return presence_of(hir, q); };

  for (std::size_t i = 0; i < unit.regions.size(); ++i) {
    for (std::size_t j = i + 1; j < unit.regions.size(); ++j) {
      const RegionEnc& ra = unit.regions[i];
      const RegionEnc& rb = unit.regions[j];
      const RegionCtx& ca = ctxs[i];
      const RegionCtx& cb = ctxs[j];

      PairReport pr;
      pr.a = ra.name;
      pr.b = rb.name;
      pr.exact = ra.exact() && rb.exact();
      pr.shared_dimensions = shared_dims(hir, ra, rb);
      pr.kind = VerdictKind::PossiblyOverlapping;

      if (auto d = ca.intervals.disjoint_with(cb.intervals, presence)) {
        pr.kind = VerdictKind::ProvenDisjoint;
        std::ostringstream os;
        os << "intervals cannot intersect on " << adl2::axioms::quantity_label(hir, d->q)
           << ": " << ra.name << " requires " << d->a.human() << ", " << rb.name
           << " requires " << d->b.human();
        pr.reason = os.str();
        pr.proof_path = ProofPath::Interval;
      } else if (auto empty_a = ca.intervals.self_empty()) {
        pr.kind = VerdictKind::ProvenDisjoint;
        pr.reason = "region " + ra.name + " provably selects no events (" +
                    empty_a->human() + "), so the pair cannot intersect";
        pr.proof_path = ProofPath::Interval;
      } else if (auto empty_b = cb.intervals.self_empty()) {
        pr.kind = VerdictKind::ProvenDisjoint;
        pr.reason = "region " + rb.name + " provably selects no events (" +
                    empty_b->human() + "), so the pair cannot intersect";
        pr.proof_path = ProofPath::Interval;
      } else {
        pr.kind = VerdictKind::PossiblyOverlapping;
        pr.reason =
            "no solver available: interval heuristics only, verdict capped at POSSIBLY";
      }
      report.pairwise.push_back(std::move(pr));
    }
  }
  return report;
}

int module_anchor() { return 4; }

}  // namespace adl2::analysis
