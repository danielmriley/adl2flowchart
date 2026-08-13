#include "adl2/analysis/analysis.hpp"

#include "adl2/axioms/axioms.hpp"
#include "adl2/certify/certify.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/formula/lin.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/solver/solver.hpp"

#include <cmath>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <sstream>
#include <utility>
#include <vector>

namespace adl2::analysis {
namespace {

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::interp::Interp;
using adl2::sema::ElemIndexKind;
using adl2::sema::Hir;
using adl2::sema::ParticleKind;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::Rat;
using adl2::solver::AssertName;
using adl2::solver::Model;
using adl2::solver::QSort;
using adl2::solver::SatResult;
using adl2::solver::Solver;
using adl2::solver::SubprocessSolver;

constexpr int MAX_WITNESS_ATTEMPTS = 6;

Model snap_model(const Model& model) {
  const double grid = 4194304.0;  // 2^22
  std::map<QuantityId, Rat> snapped;
  for (const auto& kv : model.values()) {
    double f = kv.second.to_f64();
    double s = (std::isfinite(f) && std::fabs(f) < 1e9) ? std::round(f * grid) / grid : f;
    if (auto r = Rat::from_decimal_f64(s)) snapped[kv.first] = *r;
  }
  return Model(std::move(snapped));
}

std::optional<QFormula> blocking_clause(const Model& model,
                                        const std::set<QuantityId>& mentioned) {
  std::vector<QFormula> parts;
  for (auto q : mentioned) {
    if (auto v = model.get(q)) {
      parts.push_back(QFormula::of_atom(LinAtom::single(q, Rel::Ne, *v)));
    }
  }
  if (parts.empty()) return std::nullopt;
  return QFormula::of_or(std::move(parts));
}

bool mentions_back_index(const Hir& hir, const std::set<QuantityId>& quantities) {
  auto is_back = [](const adl2::sema::ElemIndex& i) {
    return i.kind == ElemIndexKind::FromBack;
  };
  for (auto q : quantities) {
    const Quantity& qq = hir.table.quantity(q);
    if (qq.kind == QuantityKind::ElemProp && is_back(qq.index)) return true;
    if (qq.kind == QuantityKind::AngularSep) {
      if (qq.a.kind == ParticleKind::Elem && is_back(qq.a.index)) return true;
      if (qq.b.kind == ParticleKind::Elem && is_back(qq.b.index)) return true;
    }
  }
  return false;
}

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

void file_contradiction(Report& report, std::string msg) {
  Diagnostic d;
  d.class_ = DiagnosticClass::Contradiction;
  d.message = std::move(msg);
  report.diagnostics.push_back(std::move(d));
  report.internal_diagnostics.push_back(report.diagnostics.back().message);
}

std::optional<QFormula> over_of(const std::vector<std::pair<AssertName, adl2::formula::Over>>& overs,
                                const AssertName& n) {
  for (const auto& p : overs) {
    if (p.first == n) return p.second.qformula();
  }
  return std::nullopt;
}

const char* catalog_assumption(adl2::axioms::AxiomId id) {
  const auto* cat = adl2::axioms::catalog();
  int n = adl2::axioms::catalog_size();
  for (int i = 0; i < n; ++i) {
    if (cat[i].id == id) return cat[i].assumption;
  }
  return "none";
}

const char* catalog_statement(adl2::axioms::AxiomId id) {
  const auto* cat = adl2::axioms::catalog();
  int n = adl2::axioms::catalog_size();
  for (int i = 0; i < n; ++i) {
    if (cat[i].id == id) return cat[i].statement;
  }
  return "";
}

/// Interval-path certification. Disagreement is a diagnostic, never a demotion.
void certify_interval_pair(PairReport& pr, const std::vector<RefutingPart>& parts,
                           const RegionCtx& ca, const RegionCtx& cb, bool certify,
                           Report& report) {
  if (!certify) return;
  if (parts.empty()) {
    file_contradiction(report,
                       "INTERVAL CERTIFICATE unavailable for PROVEN DISJOINT " + pr.a +
                           " vs " + pr.b +
                           ": the interval layer reported no refuting atoms. The "
                           "interval layer and the replay kernel disagree — one of them "
                           "is wrong; the verdict is left as it was and no certification "
                           "is claimed.");
    return;
  }
  std::vector<QFormula> whole;
  std::vector<AssertName> whole_names;
  for (const auto& p : parts) {
    const AssertName& name = p.src();
    bool dup = false;
    for (const auto& n : whole_names) {
      if (n == name) {
        dup = true;
        break;
      }
    }
    if (dup) continue;
    auto f = over_of(ca.overs, name);
    if (!f) f = over_of(cb.overs, name);
    if (!f) {
      file_contradiction(report,
                         "INTERVAL CERTIFICATE unavailable for PROVEN DISJOINT " + pr.a +
                             " vs " + pr.b + ": no over-projection recorded for assert " +
                             name.value +
                             ". The interval layer and the replay kernel disagree — one "
                             "of them is wrong; the verdict is left as it was and no "
                             "certification is claimed.");
      return;
    }
    whole_names.push_back(name);
    whole.push_back(*f);
  }
  if (auto cert = adl2::certify::certify_bounds(whole)) {
    pr.certified = true;
    pr.certificate_size = whole.size();
    (void)cert;
    return;
  }
  std::vector<QFormula> lean;
  for (const auto& p : parts) {
    if (p.kind == RefutingPart::Kind::Whole) {
      file_contradiction(report,
                         "INTERVAL CERTIFICATE unavailable for PROVEN DISJOINT " + pr.a +
                             " vs " + pr.b +
                             ": the kernel did not accept the constant-false cut " +
                             p.src().value +
                             ". The interval layer and the replay kernel disagree — one "
                             "of them is wrong; the verdict is left as it was and no "
                             "certification is claimed.");
      return;
    }
    lean.push_back(QFormula::of_atom(p.atom));
  }
  if (auto cert = adl2::certify::certify_bounds(lean)) {
    pr.certified = true;
    pr.certificate_size = lean.size();
    (void)cert;
    return;
  }
  file_contradiction(report,
                     "INTERVAL CERTIFICATE unavailable for PROVEN DISJOINT " + pr.a +
                         " vs " + pr.b +
                         ": the replay kernel did not accept the bound pair the "
                         "interval layer refuted on. The interval layer and the replay "
                         "kernel disagree — one of them is wrong; the verdict is left "
                         "as it was and no certification is claimed.");
}

/// `None` = certify off; `Some(false)` = fail closed (empty/unknown core).
std::pair<std::optional<bool>, std::optional<std::size_t>> certify_named_formulas(
    bool certify, const std::optional<std::vector<AssertName>>& core,
    const std::vector<std::pair<AssertName, QFormula>>& extra,
    const adl2::axioms::AxiomSet* axioms) {
  if (!certify) return {std::nullopt, std::nullopt};
  if (!core || core->empty()) return {false, std::nullopt};
  std::map<AssertName, QFormula> fmap;
  for (const auto& e : extra) fmap[e.first] = e.second;
  if (axioms) {
    for (std::size_t i = 0; i < axioms->instances.size(); ++i) {
      fmap[AssertName::make("AX" + std::to_string(i))] = axioms->instances[i].formula;
    }
  }
  std::vector<QFormula> formulas;
  formulas.reserve(core->size());
  for (const auto& n : *core) {
    auto it = fmap.find(n);
    if (it == fmap.end()) return {false, std::nullopt};
    formulas.push_back(it->second);
  }
  auto r = adl2::certify::certify_unsat(formulas, adl2::certify::Budget::with_defaults());
  if (r.is_certified()) return {true, formulas.size()};
  return {false, std::nullopt};
}

PairReport interval_or_solver_pair(const Hir& hir, const adl2::sema::ExtDecls& ext,
                                   const Interp& interp, const RegionEnc& ra, const RegionEnc& rb,
                                   const RegionCtx& ca, const RegionCtx& cb, Solver* solver,
                                   std::chrono::milliseconds timeout, Report& report, bool certify,
                                   const adl2::axioms::AxiomSet* axioms) {
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
    certify_interval_pair(pr, d->parts, ca, cb, certify, report);
    return pr;
  }
  if (auto empty_a = ca.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + ra.name + " provably selects no events (" + empty_a->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    certify_interval_pair(pr, empty_a->parts(), ca, cb, certify, report);
    return pr;
  }
  if (auto empty_b = cb.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + rb.name + " provably selects no events (" + empty_b->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    certify_interval_pair(pr, empty_b->parts(), ca, cb, certify, report);
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
    pr.proof_path = ProofPath::SolverCore;
    std::optional<std::vector<AssertName>> core_names;
    if (core) core_names = *core;
    std::vector<std::pair<AssertName, QFormula>> extra;
    for (const auto& p : c1.overs) extra.emplace_back(p.first, p.second.qformula());
    for (const auto& p : c2.overs) extra.emplace_back(p.first, p.second.qformula());
    auto cert = certify_named_formulas(certify, core_names, extra, axioms);
    pr.certified = cert.first;
    if (cert.first == true) pr.certificate_size = cert.second;
    if (core) {
      for (const auto& n : *core) {
        CoreItem item;
        item.id = n.value;
        item.text = n.value;
        if (n.value.size() >= 2 && n.value[0] == 'A' && n.value[1] == 'X') {
          item.origin = CoreItem::Origin::Axiom;
        } else {
          item.origin = CoreItem::Origin::Cut;
        }
        pr.core.push_back(std::move(item));
      }
    }
    if (cert.first == false) {
      pr.kind = VerdictKind::CandidateDisjoint;
      pr.reason =
          "solver reported UNSAT but the proof could not be independently "
          "certified (budget, shape, or an integrality-only refutation); "
          "candidate, not a claim — ";
      pr.reason += (core && !core->empty()) ? "solver unsat core" : "solver unsat";
    } else {
      pr.kind = VerdictKind::ProvenDisjoint;
      pr.reason = core && !core->empty() ? "solver unsat core" : "solver unsat";
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

  // Overlap: SAT(Ax ∧ A⁻ ∧ B⁻) + Kleene region3 re-validation.
  solver->push();
  assert_unders(*solver, c1);
  assert_unders(*solver, c2);
  SatResult overlap = solver->check(timeout);
  note_failure(report, overlap);
  if (!overlap.is_sat()) {
    solver->pop();
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
  if (pr.shared_dimensions.empty()) {
    solver->pop();
    pr.kind = VerdictKind::PossiblyOverlapping;
    pr.reason =
        "under-approximations intersect but the regions share no dimension; "
        "capped at POSSIBLY";
    return pr;
  }

  std::set<QuantityId> combined = ra.quantities;
  combined.insert(rb.quantities.begin(), rb.quantities.end());
  if (mentions_back_index(hir, combined)) {
    solver->pop();
    pr.kind = VerdictKind::PossiblyOverlapping;
    pr.reason =
        "under-approximations intersect, but a back-indexed element "
        "(`coll[-k]`) is not realizable by the witness builder; "
        "capped at POSSIBLY";
    return pr;
  }

  std::optional<std::string> last_reject;
  std::optional<Validation> outcome;
  for (int attempt = 0; attempt < MAX_WITNESS_ATTEMPTS; ++attempt) {
    auto model = solver->model();
    if (!model) break;
    Validation v = validate_witness(hir, ext, interp, *model, combined, ra.idx, rb.idx);
    if (v.kind == ValidationKind::Rejected) {
      std::string first_why = v.payload;
      Validation snapped =
          validate_witness(hir, ext, interp, snap_model(*model), combined, ra.idx, rb.idx);
      if (snapped.kind != ValidationKind::Rejected) {
        outcome = std::move(snapped);
        break;
      }
      last_reject = std::move(first_why);
      auto block = blocking_clause(*model, combined);
      if (!block) break;
      solver->assert_formula(*block, std::nullopt);
      SatResult retry = solver->check(timeout);
      note_failure(report, retry);
      if (!retry.is_sat()) break;
      continue;
    }
    outcome = std::move(v);
    break;
  }
  solver->pop();

  if (outcome && outcome->kind == ValidationKind::Validated) {
    pr.kind = VerdictKind::ProvenOverlapping;
    pr.reason = std::string("both region cut sets are satisfiable together (") + OVERLAP_CAVEAT +
                ")";
    pr.witness_validated = true;
    return pr;
  }
  if (outcome && outcome->kind == ValidationKind::Candidate) {
    pr.kind = VerdictKind::CandidateOverlapping;
    pr.reason =
        std::string(
            "a joint model exists but rests on an opaque quantity the "
            "interpreter cannot decide, so the witness is a candidate, not "
            "a proof (") +
        OVERLAP_CAVEAT + "); " + outcome->payload;
    pr.witness_validated = false;
    return pr;
  }

  pr.kind = VerdictKind::PossiblyOverlapping;
  if (last_reject) {
    const std::string& why = *last_reject;
    if (!pr.exact || why.find("no reference interpretation") != std::string::npos ||
        why.find("OPEN-1 unresolved") != std::string::npos ||
        why.find("cannot evaluate") != std::string::npos ||
        why.find("unresolved identifier") != std::string::npos) {
      pr.reason =
          "under-approximations intersect, but no witness could be realized "
          "through the interpreter (the region depends on an opaque quantity); "
          "capped at POSSIBLY (" +
          why + ")";
    } else {
      pr.reason = "overlap model found, but witness re-validation failed; downgraded to POSSIBLY (" +
                  why + ")";
    }
  } else {
    pr.reason = "under-approximations intersect, but no witness could be realized";
  }
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

  Interp interp(hir, ext);

  Report report;
  report.schema_version = SCHEMA_VERSION;
  report.unit = hir.unit;
  report.solver = solver_label;
  report.certification = opts.certify;

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
        certify_interval_pair(dummy, empty->parts(), ctxs[i], ctxs[i], true, report);
      }
    } else if (solver) {
      solver->push();
      assert_overs(*solver, ctxs[i]);
      SatResult er = solver->check(opts.timeout);
      note_failure(report, er);
      std::optional<std::vector<AssertName>> core_names;
      if (er.is_unsat()) {
        if (auto core = solver->unsat_core()) core_names = *core;
      }
      solver->pop();
      if (er.is_unsat()) {
        std::vector<std::pair<AssertName, QFormula>> extra;
        for (const auto& p : ctxs[i].overs) extra.emplace_back(p.first, p.second.qformula());
        auto cert = certify_named_formulas(opts.certify, core_names, extra, &axioms);
        rr.empty = (cert.first == false) ? EmptyStatus::Candidate : EmptyStatus::Proven;
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
          hir, ext, interp, unit.regions[i], unit.regions[j], ctxs[i], ctxs[j], solver,
          opts.timeout, report, opts.certify, solver ? &axioms : nullptr));
    }
  }

  if (report.solver_failures) {
    report.solver_degraded =
        std::to_string(report.solver_failures->spawn + report.solver_failures->errors) +
        " solver check(s) failed via `" + report.solver + "`";
  }

  if (!axioms.instances.empty()) {
    std::map<adl2::axioms::AxiomId, std::size_t> counts;
    for (const auto& inst : axioms.instances) counts[inst.id]++;
    const auto* ids = adl2::axioms::axiom_id_all();
    for (int i = 0; i < adl2::axioms::AXIOM_COUNT; ++i) {
      auto it = counts.find(ids[i]);
      if (it == counts.end()) continue;
      AxiomUse u;
      u.id = adl2::axioms::axiom_id_str(ids[i]);
      u.statement = catalog_statement(ids[i]);
      u.assumption = catalog_assumption(ids[i]);
      u.instances = it->second;
      report.axioms_used.push_back(std::move(u));
    }
  }

  return report;
}

int module_anchor() { return 4; }

}  // namespace adl2::analysis
