#include "adl2/analysis/analysis.hpp"

#include "adl2/axioms/axioms.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/solver/solver.hpp"

#include <map>
#include <memory>
#include <set>
#include <sstream>
#include <utility>
#include <vector>

namespace adl2::analysis {
namespace {

using adl2::formula::QFormula;
using adl2::sema::Hir;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::solver::AssertName;
using adl2::solver::QSort;
using adl2::solver::SatResult;
using adl2::solver::Solver;
using adl2::solver::SubprocessSolver;

struct RegionCtx {
  IntervalMap intervals;
  std::vector<std::pair<AssertName, adl2::formula::Over>> overs;
  std::vector<adl2::formula::Under> unders;
};

RegionCtx build_ctx(const RegionEnc& r) {
  RegionCtx ctx;
  for (const auto& s : r.stmts) {
    auto o = s.over();
    ctx.intervals.add_over(s.name, o);
    ctx.overs.emplace_back(s.name, std::move(o));
    ctx.unders.push_back(s.under());
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

void qformula_quantities(const QFormula& f, std::set<QuantityId>& out) {
  switch (f.kind) {
    case QFormula::Kind::True:
    case QFormula::Kind::False:
      return;
    case QFormula::Kind::Atom:
      for (const auto& t : f.atom.terms()) out.insert(t.second);
      return;
    case QFormula::Kind::And:
    case QFormula::Kind::Or:
      for (const auto& p : f.items) qformula_quantities(p, out);
      return;
  }
}

void declare_all(Solver& s, const Hir& hir, const UnitEnc& unit,
                 const adl2::axioms::AxiomSet& axioms) {
  std::set<QuantityId> all_q;
  for (const auto& r : unit.regions) all_q.insert(r.quantities.begin(), r.quantities.end());
  for (const auto& inst : axioms.instances) qformula_quantities(inst.formula, all_q);
  for (auto q : all_q) {
    QSort sort = hir.table.quantity(q).kind == QuantityKind::Size ? QSort::Int : QSort::Real;
    s.declare(q, sort);
  }
}

void assert_axioms(Solver& s, const adl2::axioms::AxiomSet& axioms) {
  for (std::size_t i = 0; i < axioms.instances.size(); ++i) {
    s.assert_formula(axioms.instances[i].formula,
                     AssertName::make("AX" + std::to_string(i)));
  }
}

void assert_overs(Solver& s, const RegionCtx& ctx) {
  for (const auto& p : ctx.overs) {
    s.assert_formula(p.second.qformula(), p.first);
  }
}

void assert_unders(Solver& s, const RegionCtx& ctx) {
  for (const auto& u : ctx.unders) {
    s.assert_formula(u.qformula(), std::nullopt);
  }
}

bool subset_unsat(Solver& s, const RegionCtx& outer, const RegionCtx& inner,
                  std::chrono::milliseconds timeout) {
  // UNSAT(Ax ∧ outer⁺ ∧ ¬inner⁻). Axioms live on the base frame.
  s.push();
  assert_overs(s, outer);
  for (const auto& u : inner.unders) {
    s.assert_formula(u.qformula().qnot(), std::nullopt);
  }
  SatResult r = s.check(timeout);
  s.pop();
  return r.is_unsat();
}

std::unique_ptr<SubprocessSolver> make_solver(SolverChoice choice, std::string& label) {
  if (choice == SolverChoice::NoSolver) {
    label = "none";
    return nullptr;
  }
  if (!adl2::solver::subprocess_available("z3")) {
    label = "none";
    return nullptr;
  }
  label = "smtlib-subprocess";
  return std::make_unique<SubprocessSolver>(SubprocessSolver::z3());
}

void note_failure(Report& report, const SatResult& r) {
  if (!r.is_process_failure() && !r.is_solver_error()) return;
  if (!report.solver_failures) report.solver_failures = SolverFailures{};
  if (r.is_process_failure()) report.solver_failures->spawn++;
  else report.solver_failures->errors++;
  if (report.solver_failures->first_reason.empty() && r.is_unknown()) {
    report.solver_failures->first_reason = r.reason;
  }
}

PairReport interval_or_solver_pair(const Hir& hir, const RegionEnc& ra, const RegionEnc& rb,
                                   const RegionCtx& ca, const RegionCtx& cb, Solver* solver,
                                   std::chrono::milliseconds timeout, Report& report) {
  PairReport pr;
  pr.a = ra.name;
  pr.b = rb.name;
  pr.exact = ra.exact() && rb.exact();
  pr.shared_dimensions = shared_dims(hir, ra, rb);
  pr.kind = VerdictKind::PossiblyOverlapping;

  auto presence = [&](QuantityId q) { return presence_of(hir, q); };

  if (auto d = ca.intervals.disjoint_with(cb.intervals, presence)) {
    pr.kind = VerdictKind::ProvenDisjoint;
    std::ostringstream os;
    os << "intervals cannot intersect on " << adl2::axioms::quantity_label(hir, d->q)
       << ": " << ra.name << " requires " << d->a.human() << ", " << rb.name
       << " requires " << d->b.human();
    pr.reason = os.str();
    pr.proof_path = ProofPath::Interval;
    return pr;
  }
  if (auto empty_a = ca.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + ra.name + " provably selects no events (" + empty_a->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    return pr;
  }
  if (auto empty_b = cb.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + rb.name + " provably selects no events (" + empty_b->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    return pr;
  }

  if (!solver) {
    pr.kind = VerdictKind::PossiblyOverlapping;
    pr.reason = "no solver available: interval heuristics only, verdict capped at POSSIBLY";
    return pr;
  }

  // Canonical solver order by region name (Rust metamorphic swap symmetry).
  const bool a_first = ra.name <= rb.name;
  const RegionCtx& c1 = a_first ? ca : cb;
  const RegionCtx& c2 = a_first ? cb : ca;

  // Disjointness: UNSAT(Ax ∧ A⁺ ∧ B⁺). Axioms are on the base frame.
  solver->push();
  assert_overs(*solver, c1);
  assert_overs(*solver, c2);
  SatResult disjoint = solver->check(timeout);
  note_failure(report, disjoint);
  if (disjoint.is_unsat()) {
    auto core = solver->unsat_core();
    solver->pop();
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.proof_path = ProofPath::SolverCore;
    pr.reason = core && !core->empty() ? "solver unsat core" : "solver unsat";
    if (core) {
      for (const auto& n : *core) {
        CoreItem item;
        item.id = n.value;
        item.text = n.value;
        pr.core.push_back(std::move(item));
      }
    }
    return pr;
  }
  solver->pop();

  bool one_in_two = subset_unsat(*solver, c1, c2, timeout);
  bool two_in_one = subset_unsat(*solver, c2, c1, timeout);
  if (a_first) {
    pr.subset_a_in_b = one_in_two;
    pr.subset_b_in_a = two_in_one;
  } else {
    pr.subset_a_in_b = two_in_one;
    pr.subset_b_in_a = one_in_two;
  }

  // Overlap: SAT(Ax ∧ A⁻ ∧ B⁻). No witness realization in P4 → candidate,
  // never PROVEN OVERLAPPING (the SAT-side net is the interpreter).
  solver->push();
  assert_unders(*solver, c1);
  assert_unders(*solver, c2);
  SatResult overlap = solver->check(timeout);
  note_failure(report, overlap);
  solver->pop();
  if (overlap.is_sat()) {
    if (pr.shared_dimensions.empty()) {
      pr.kind = VerdictKind::PossiblyOverlapping;
      pr.reason =
          "under-approximations intersect but the regions share no dimension; "
          "capped at POSSIBLY";
    } else {
      pr.kind = VerdictKind::CandidateOverlapping;
      pr.reason =
          "under-approximations are SAT; witness realization/re-validation is "
          "not ported, so this is a candidate, not PROVEN OVERLAPPING";
      pr.witness_validated = false;
    }
    return pr;
  }
  if (overlap.is_unknown()) {
    pr.kind = VerdictKind::Unknown;
    pr.reason = overlap.reason.empty() ? "solver unknown on overlap query" : overlap.reason;
    return pr;
  }

  pr.kind = VerdictKind::PossiblyOverlapping;
  pr.reason = disjoint.is_unknown()
                  ? (disjoint.reason.empty() ? "solver unknown on disjointness" : disjoint.reason)
                  : "solver SAT/UNSAT split did not prove disjointness or overlap";
  return pr;
}

}  // namespace

Report analyze_hir(Hir& hir, const std::string& src, const adl2::sema::ExtDecls& ext,
                   const AnalysisOptions& opts) {
  retag_opaque_externals(hir);
  UnitEnc unit = encode_unit(hir, src);

  std::string solver_label;
  std::unique_ptr<SubprocessSolver> owned = make_solver(opts.solver, solver_label);
  Solver* solver = owned.get();

  adl2::axioms::AxiomSet axioms;
  if (solver) {
    std::set<QuantityId> qs;
    for (const auto& r : unit.regions) qs.insert(r.quantities.begin(), r.quantities.end());
    axioms = adl2::axioms::emit_axioms(hir, ext, qs);
    declare_all(*solver, hir, unit, axioms);
    assert_axioms(*solver, axioms);
  }

  std::vector<RegionCtx> ctxs;
  ctxs.reserve(unit.regions.size());
  for (const auto& r : unit.regions) ctxs.push_back(build_ctx(r));

  Report report;
  report.schema_version = SCHEMA_VERSION;
  report.unit = hir.unit;
  report.solver = solver_label;
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
    } else if (solver) {
      solver->push();
      assert_overs(*solver, ctxs[i]);
      SatResult er = solver->check(opts.timeout);
      note_failure(report, er);
      solver->pop();
      if (er.is_unsat()) {
        rr.empty = EmptyStatus::Proven;
        rr.empty_proof = ProofPath::SolverCore;
      } else {
        rr.empty = EmptyStatus::NotProven;
      }
    } else {
      rr.empty = EmptyStatus::Unknown;
    }
    report.regions.push_back(std::move(rr));
  }

  for (std::size_t i = 0; i < unit.regions.size(); ++i) {
    for (std::size_t j = i + 1; j < unit.regions.size(); ++j) {
      report.pairwise.push_back(interval_or_solver_pair(
          hir, unit.regions[i], unit.regions[j], ctxs[i], ctxs[j], solver, opts.timeout,
          report));
    }
  }

  if (report.solver_failures) {
    report.solver_degraded =
        std::to_string(report.solver_failures->spawn + report.solver_failures->errors) +
        " solver check(s) failed via `" + report.solver + "`";
  }

  if (!axioms.instances.empty()) {
    std::map<std::string, AxiomUse> used;
    for (const auto& inst : axioms.instances) {
      std::string id = adl2::axioms::axiom_id_str(inst.id);
      auto& u = used[id];
      if (u.id.empty()) {
        u.id = id;
        u.statement = inst.description;
      }
      u.instances++;
    }
    for (auto& kv : used) report.axioms_used.push_back(std::move(kv.second));
  }

  return report;
}

int module_anchor() { return 4; }

}  // namespace adl2::analysis
