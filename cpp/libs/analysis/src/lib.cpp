#include "detail.hpp"

#include "adl2/interp/sample.hpp"

#include <algorithm>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>

namespace adl2::analysis {
using adl2::formula::LinAtom;
using adl2::formula::Over;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::formula::Under;
using adl2::interp::Event;
using adl2::interp::EvalError;
using adl2::interp::Interp;
using adl2::interp::NumOutcomeKind;
using adl2::interp::parse_event;
using adl2::sema::AngKind;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::ElemIndexKind;
using adl2::sema::Hir;
using adl2::sema::ParticleKind;
using adl2::sema::ParticleRef;
using adl2::sema::Quantity;
using adl2::sema::QuantityArgKind;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::Rat;
using adl2::solver::AssertName;
using adl2::solver::Model;
using adl2::solver::QSort;
using adl2::solver::SatResult;
using adl2::solver::Solver;
using adl2::solver::SubprocessSolver;

void classify_overlap_non_sat(PairReport& pr, const adl2::solver::SatResult& overlap,
                              const adl2::solver::SatResult& disjoint) {
  if (overlap.is_unknown()) {
    if (disjoint.is_unknown()) {
      pr.kind = VerdictKind::Unknown;
      pr.reason = "solver inconclusive in both directions (" + disjoint.reason + "; " +
                  overlap.reason + ")";
    } else {
      pr.kind = VerdictKind::PossiblyOverlapping;
      pr.reason = "solver inconclusive in the SAT direction (" + overlap.reason + ")";
    }
    return;
  }
  if (overlap.is_unsat()) {
    pr.kind = VerdictKind::PossiblyOverlapping;
    pr.reason =
        "over-approximations may intersect but under-approximations cannot: "
        "an encoding gap blocks both a disjointness and an overlap proof";
    return;
  }
  pr.kind = VerdictKind::PossiblyOverlapping;
  pr.reason = "no solver";
}

Report analyze_hir(Hir& hir, const std::string& src, const adl2::sema::ExtDecls& ext,
                   const AnalysisOptions& opts) {
  retag_opaque_externals(hir);
  UnitEnc unit = encode_unit(hir, src);

  std::set<QuantityId> qs;
  for (const auto& r : unit.regions) qs.insert(r.quantities.begin(), r.quantities.end());
  for (const auto& set : unit.bin_sets) {
    for (const auto& f : set.bins) formula_quantities(f, qs);
  }
  adl2::axioms::AxiomSet axioms;
  if (opts.solver != SolverChoice::NoSolver || opts.reconcile) {
    axioms = adl2::axioms::emit_axioms(hir, ext, qs);
  }

  // Smash2: intern size(C) for every collection with a mentioned element so
  // witness rows can name the derived size without further table mutation.
  {
    std::set<CollectionId> elem_colls;
    for (auto q : qs) {
      const Quantity& qq = hir.table.quantity(q);
      if (qq.kind == QuantityKind::ElemProp && qq.index.kind == ElemIndexKind::FromFront) {
        elem_colls.insert(qq.coll);
      }
    }
    for (auto c : elem_colls) hir.table.intern_quantity(Quantity::size(c));
  }

  std::optional<ReconEnc> recon;
  if (opts.reconcile) recon = build_recon(hir, ext);

  std::string solver_label;
  std::unique_ptr<SubprocessSolver> owned = make_solver(opts.solver, solver_label);
  Solver* solver = owned.get();

  std::set<QuantityId> recon_qs;
  if (recon) recon_qs = recon->quantities();
  if (solver) {
    declare_all(*solver, hir, unit, axioms, recon_qs);
    assert_axioms(*solver, axioms);
  }

  Report report;
  report.schema_version = SCHEMA_VERSION;
  report.unit = hir.unit;
  report.solver = solver_label;
  report.certification = opts.certify;

  CombineAcc acc;
  acc.enabled = opts.combine;
  acc.hir = &hir;
  for (const auto& r : unit.regions) {
    for (const auto& s : r.stmts) {
      CoreItem item;
      item.origin = CoreItem::Origin::Cut;
      item.region = r.name;
      item.line = s.line;
      item.text = s.text;
      acc.origins[s.name] = std::move(item);
    }
  }
  for (std::size_t i = 0; i < axioms.instances.size(); ++i) {
    CoreItem item;
    item.origin = CoreItem::Origin::Axiom;
    item.id = adl2::axioms::axiom_id_str(axioms.instances[i].id);
    item.statement = axioms.instances[i].description;
    acc.origins[AssertName::make("AX" + std::to_string(i))] = std::move(item);
  }

  ReconRun recon_run;
  if (recon) {
    recon_run = apply_reconcile(hir, unit, solver, opts.certify, opts.timeout, report, axioms,
                                std::move(*recon), acc);
  }

  std::vector<RegionCtx> ctxs;
  ctxs.reserve(unit.regions.size());
  for (const auto& r : unit.regions) ctxs.push_back(build_ctx(r));

  Interp interp(hir, ext);

  std::vector<double> cuts = cut_constants(hir);
  std::vector<Event> gate_events;
  if (opts.sample_gate > 0) {
    gate_events = adl2::interp::battery_with_cuts(ext, opts.sample_gate, cuts);
  }
  std::vector<Event> refute_probes;
  if (opts.refute_gate) {
    refute_probes = probe_events(ext, cuts);
  }
  std::size_t gate_refutations = 0;
  std::size_t refute_refutations = 0;

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
      if (opts.certify) {
        PairReport dummy;
        dummy.a = r.name;
        dummy.b = r.name;
        CombineAcc no_bundle;
        certify_interval_pair(dummy, empty->parts(), ctxs[i], ctxs[i], true, false, report,
                              no_bundle);
      }
    } else if (solver) {
      solver->push();
      assert_overs(*solver, ctxs[i].overs);
      SatResult er = solver->check_unsat(opts.timeout);
      note_failure(report, er);
      std::optional<std::vector<AssertName>> core_names;
      if (er.is_unsat()) {
        if (auto core = solver->unsat_core()) core_names = *core;
      }
      solver->pop();
      if (er.is_unsat()) {
        std::vector<std::pair<AssertName, QFormula>> extra;
        for (const auto& p : ctxs[i].overs) extra.emplace_back(p.first, p.second.qformula());
        auto cert = certify_named_formulas(opts.certify, core_names, extra, &axioms,
                                           &recon_run.facts, false);
        rr.empty = (cert.flag == false) ? EmptyStatus::Candidate : EmptyStatus::Proven;
        rr.empty_proof = ProofPath::SolverCore;
      } else if (er.is_sat()) {
        rr.empty = EmptyStatus::NotProven;
      } else {
        rr.empty = EmptyStatus::Unknown;
      }
    } else {
      rr.empty = EmptyStatus::Unknown;
    }
    if ((rr.empty == EmptyStatus::Proven || rr.empty == EmptyStatus::Candidate) &&
        gate_empty(r.idx, r.name, interp, gate_events, report, gate_refutations)) {
      rr.empty = EmptyStatus::NotProven;
      rr.empty_core.clear();
      rr.empty_proof = std::nullopt;
    }
    if ((rr.empty == EmptyStatus::Proven || rr.empty == EmptyStatus::Candidate) &&
        refute_empty(r.idx, r.name, interp, refute_probes, report, refute_refutations)) {
      rr.empty = EmptyStatus::NotProven;
      rr.empty_core.clear();
      rr.empty_proof = std::nullopt;
    }
    report.regions.push_back(std::move(rr));
  }

  for (std::size_t i = 0; i < unit.regions.size(); ++i) {
    for (std::size_t j = i + 1; j < unit.regions.size(); ++j) {
      PairReport pr = interval_or_solver_pair(
          hir, ext, interp, unit.regions[i], unit.regions[j], ctxs[i], ctxs[j], solver,
          opts.timeout, report, opts.certify, opts.demote_uncertified_interval,
          solver ? &axioms : nullptr, &recon_run.facts, acc);
      gate_pair(pr, unit.regions[i].idx, unit.regions[j].idx, interp, gate_events, refute_probes,
                report, gate_refutations, refute_refutations);
      report.pairwise.push_back(std::move(pr));
    }
  }

  for (const auto& set : unit.bin_sets) {
    if (set.region_idx >= ctxs.size() || set.region_idx >= unit.regions.size()) continue;
    report.bin_checks.push_back(bin_check(solver, opts.timeout, opts.certify, report,
                                          solver ? &axioms : nullptr, &recon_run.facts, hir, set,
                                          ctxs[set.region_idx], unit.regions[set.region_idx].name));
  }

  if (!gate_events.empty()) {
    SamplingInfo si;
    si.events = gate_events.size();
    si.refutations = gate_refutations;
    report.sampling = si;
  }
  if (opts.refute_gate) {
    RefuteInfo ri;
    ri.probes = refute_probes.size();
    ri.refutations = refute_refutations;
    report.refute = ri;
  }

  if (report.solver_failures) {
    report.solver_degraded =
        std::to_string(report.solver_failures->spawn + report.solver_failures->errors) +
        " solver check(s) failed via `" + report.solver + "`";
  }

  if (!axioms.instances.empty() || !recon_run.counts.empty()) {
    std::map<adl2::axioms::AxiomId, std::size_t> counts;
    for (const auto& inst : axioms.instances) counts[inst.id]++;
    const auto* ids = adl2::axioms::axiom_id_all();
    for (int i = 0; i < adl2::axioms::AXIOM_COUNT; ++i) {
      std::size_t n = 0;
      auto it = counts.find(ids[i]);
      if (it != counts.end()) n = it->second;
      auto rit = recon_run.counts.find(adl2::axioms::axiom_id_str(ids[i]));
      if (rit != recon_run.counts.end()) n += rit->second;
      if (n == 0) continue;
      AxiomUse u;
      u.id = adl2::axioms::axiom_id_str(ids[i]);
      u.statement = catalog_statement(ids[i]);
      u.assumption = catalog_assumption(ids[i]);
      u.instances = n;
      report.axioms_used.push_back(std::move(u));
    }
  }

  report.reconciliations = std::move(recon_run.ledger);
  report.recon_near_misses = std::move(recon_run.near_misses);

  acc.bundles.erase(
      std::remove_if(acc.bundles.begin(), acc.bundles.end(),
                     [&](const adl2::certify::CombineBundle& b) {
                       for (const auto& p : report.pairwise) {
                         if (p.kind == VerdictKind::ProvenDisjoint && p.a == b.region_a &&
                             p.b == b.region_b) {
                           return false;
                         }
                       }
                       return true;
                     }),
      acc.bundles.end());
  report.combine_bundles = std::move(acc.bundles);

  return report;
}

int module_anchor() { return 4; }

}  // namespace adl2::analysis
