#include "detail.hpp"

#include "adl2/interp/sample.hpp"
#include "adl2/sema/dump.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <sstream>
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

std::optional<QuantityId> lookup_size(const Hir& hir, CollectionId coll) {
  return hir.table.quantity_id(Quantity::size(coll));
}

/// Solver-model rows (smash2 `witness_values`). Mentioned quantities plus
/// derived sizes of collections whose elements appear.
std::vector<WitnessValue> witness_values(const Hir& hir, const Model& model,
                                         const std::set<QuantityId>& mentioned) {
  std::vector<WitnessValue> rows;
  std::set<QuantityId> listed;
  for (auto q : mentioned) {
    if (auto v = model.get_f64(q)) {
      if (listed.insert(q).second) {
        WitnessValue w;
        w.quantity = adl2::axioms::quantity_label(hir, q);
        w.value = *v;
        w.derived = false;
        rows.push_back(std::move(w));
      }
    }
  }
  for (auto q : mentioned) {
    const Quantity& qq = hir.table.quantity(q);
    if (qq.kind != QuantityKind::ElemProp) continue;
    auto sq = lookup_size(hir, qq.coll);
    if (!sq || listed.count(*sq)) continue;
    if (auto v = model.get_f64(*sq)) {
      listed.insert(*sq);
      WitnessValue w;
      w.quantity = adl2::axioms::quantity_label(hir, *sq);
      w.value = *v;
      w.derived = true;
      rows.push_back(std::move(w));
    }
  }
  std::sort(rows.begin(), rows.end(),
            [](const WitnessValue& a, const WitnessValue& b) { return a.quantity < b.quantity; });
  return rows;
}

/// Validated overlap rows (smash2 `validated_witness_values` / review F2):
/// read back from the realized event so pT-sort cannot display a pre-sort
/// index next to a "validated" label.
std::vector<WitnessValue> validated_witness_values(const Hir& hir, const adl2::sema::ExtDecls& ext,
                                                   const Interp& interp, const std::string& json,
                                                   const Model& model,
                                                   const std::set<QuantityId>& mentioned) {
  adl2::interp::EventError ee;
  auto event = parse_event(json, ext, ee);
  if (!event) return witness_values(hir, model, mentioned);
  Interp::EventEval ev(interp, *event);
  auto value_of = [&](QuantityId q) -> std::optional<double> {
    EvalError err;
    auto o = ev.quantity(q, err);
    if (o && o->kind == NumOutcomeKind::Value) return o->value;
    return model.get_f64(q);
  };
  std::vector<WitnessValue> rows;
  std::set<QuantityId> listed;
  for (auto q : mentioned) {
    if (auto v = value_of(q)) {
      if (listed.insert(q).second) {
        WitnessValue w;
        w.quantity = adl2::axioms::quantity_label(hir, q);
        w.value = *v;
        w.derived = false;
        rows.push_back(std::move(w));
      }
    }
  }
  for (auto q : mentioned) {
    const Quantity& qq = hir.table.quantity(q);
    if (qq.kind != QuantityKind::ElemProp) continue;
    auto sq = lookup_size(hir, qq.coll);
    if (!sq || listed.count(*sq)) continue;
    if (auto v = value_of(*sq)) {
      listed.insert(*sq);
      WitnessValue w;
      w.quantity = adl2::axioms::quantity_label(hir, *sq);
      w.value = *v;
      w.derived = true;
      rows.push_back(std::move(w));
    }
  }
  return rows;
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
                 const adl2::axioms::AxiomSet& axioms,
                 const std::set<QuantityId>& extra) {
  std::set<QuantityId> all_q;
  for (const auto& r : unit.regions) all_q.insert(r.quantities.begin(), r.quantities.end());
  for (const auto& set : unit.bin_sets) {
    for (const auto& f : set.bins) formula_quantities(f, all_q);
  }
  for (const auto& inst : axioms.instances) qformula_quantities(inst.formula, all_q);
  all_q.insert(extra.begin(), extra.end());
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

std::string join_with(const std::vector<std::string>& parts, const char* sep) {
  std::string out;
  for (std::size_t i = 0; i < parts.size(); ++i) {
    if (i) out += sep;
    out += parts[i];
  }
  return out;
}

void assert_overs(Solver& s, const std::vector<std::pair<AssertName, Over>>& overs) {
  for (const auto& p : overs) {
    s.assert_formula(p.second.qformula(), p.first);
  }
}

void assert_unders(Solver& s, const std::vector<Under>& unders) {
  for (const auto& u : unders) {
    s.assert_formula(u.qformula(), std::nullopt);
  }
}

/// `¬(R⁻)`: the under-projection is the conjunction of statement unders, so
/// its exact negation is the disjunction of their NNF negations (smash2
/// `negated_under`). Asserting each `¬uᵢ` separately would be `∧ ¬uᵢ` —
/// the dual, which makes UNSAT easier and fabricates PROVEN SUBSET.
QFormula negated_under(const std::vector<Under>& sup_unders) {
  std::vector<QFormula> parts;
  parts.reserve(sup_unders.size());
  for (const auto& u : sup_unders) {
    parts.push_back(u.qformula().qnot());
  }
  return QFormula::of_or(std::move(parts));
}

std::string core_item_human(const CoreItem& c) {
  if (c.origin == CoreItem::Origin::Axiom) {
    if (c.statement.empty()) return "axiom " + c.id;
    return "axiom " + c.id + " (" + c.statement + ")";
  }
  if (c.region.empty()) {
    const std::string& t = c.text.empty() ? c.id : c.text;
    return "`" + t + "`";
  }
  return "`" + c.region + " line " + std::to_string(c.line) + ": " + c.text + "`";
}

std::string core_reason(const std::vector<CoreItem>& items) {
  if (items.empty()) return "UNSAT (no core available)";
  std::vector<std::string> cuts;
  std::vector<std::string> axs;
  for (const auto& c : items) {
    if (c.origin == CoreItem::Origin::Axiom) axs.push_back(core_item_human(c));
    else cuts.push_back(core_item_human(c));
  }
  std::string reason = "UNSAT core: " +
                       (cuts.size() == 1 ? cuts[0] + " cannot hold"
                                         : join_with(cuts, " cannot hold together with "));
  if (cuts.empty()) {
    reason = "UNSAT core: " + join_with(axs, ", ");
    return reason;
  }
  if (!axs.empty()) reason += " (using " + join_with(axs, ", ") + ")";
  return reason;
}

/// `UNSAT(Ax ∧ sub⁺ ∧ ¬(sup⁻))` ⇒ sub ⊆ sup. Named QSUB{k} / QSUBNEG
/// so certify can replay the core. When certify is on, `Some(false)` is not a
/// subset claim (smash2 `subset_proof`). Parameter names are the polarity
/// types: handing unders where overs go does not compile (ADR-004).
bool region_subset(Solver& s, const std::vector<std::pair<AssertName, Over>>& sub_overs,
                   const std::vector<Under>& sup_unders, bool certify,
                   std::chrono::milliseconds timeout, Report& report,
                   const adl2::axioms::AxiomSet* axioms,
                   const std::vector<std::pair<AssertName, QFormula>>* recon_facts) {
  s.push();
  std::vector<std::pair<AssertName, QFormula>> extra;
  extra.reserve(sub_overs.size() + 1);
  std::size_t k = 0;
  for (const auto& p : sub_overs) {
    AssertName name = AssertName::make("QSUB" + std::to_string(k++));
    QFormula f = p.second.qformula();
    s.assert_formula(f, name);
    extra.emplace_back(name, std::move(f));
  }
  AssertName neg_name = AssertName::make("QSUBNEG");
  QFormula neg = negated_under(sup_unders);
  s.assert_formula(neg, neg_name);
  extra.emplace_back(neg_name, std::move(neg));
  SatResult r = s.check_unsat(timeout);
  note_failure(report, r);
  std::optional<std::vector<AssertName>> core_names;
  if (r.is_unsat()) {
    if (auto core = s.unsat_core()) core_names = *core;
  }
  s.pop();
  if (!r.is_unsat()) return false;
  auto cert = certify_named_formulas(certify, core_names, extra, axioms, recon_facts, false);
  return cert.flag != false;
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
  // smash3 report label: backend plus the binary it shells out to.
  label = "smtlib-subprocess(z3)";
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

void file_fail_closed(Report& report, std::string msg) {
  Diagnostic d;
  d.class_ = DiagnosticClass::FailClosed;
  d.message = std::move(msg);
  report.diagnostics.push_back(std::move(d));
  report.internal_diagnostics.push_back(report.diagnostics.back().message);
}

PairReport interval_or_solver_pair(const Hir& hir, const adl2::sema::ExtDecls& ext,
                                   const Interp& interp, const RegionEnc& ra, const RegionEnc& rb,
                                   const RegionCtx& ca, const RegionCtx& cb, Solver* solver,
                                   std::chrono::milliseconds timeout, Report& report, bool certify,
                                   bool demote_uncertified_interval,
                                   const adl2::axioms::AxiomSet* axioms,
                                   const std::vector<std::pair<AssertName, QFormula>>* recon_facts,
                                   CombineAcc& acc) {
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
    certify_interval_pair(pr, d->parts, ca, cb, certify, demote_uncertified_interval, report, acc);
    return pr;
  }
  if (auto empty_a = ca.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + ra.name + " provably selects no events (" + empty_a->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    certify_interval_pair(pr, empty_a->parts(), ca, cb, certify, demote_uncertified_interval, report,
                          acc);
    return pr;
  }
  if (auto empty_b = cb.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + rb.name + " provably selects no events (" + empty_b->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    certify_interval_pair(pr, empty_b->parts(), ca, cb, certify, demote_uncertified_interval, report,
                          acc);
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
  assert_overs(*solver, c1.overs);
  assert_overs(*solver, c2.overs);
  SatResult disjoint = solver->check_unsat(timeout);
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
    auto cert = certify_named_formulas(certify, core_names, extra, axioms, recon_facts,
                                      acc.enabled);
    pr.certified = cert.flag;
    if (cert.flag == true && core_names) pr.certificate_size = core_names->size();
    if (core) {
      for (const auto& n : *core) {
        auto it = acc.origins.find(n);
        if (it != acc.origins.end()) {
          pr.core.push_back(it->second);
        } else {
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
    }
    if (cert.payload) push_bundle(acc, pr.a, pr.b, *cert.payload, report);
    if (cert.flag == false) {
      pr.kind = VerdictKind::CandidateDisjoint;
      pr.reason =
          "solver reported UNSAT but the proof could not be independently "
          "certified (budget, shape, or an integrality-only refutation); "
          "candidate, not a claim — ";
      pr.reason += core_reason(pr.core);
    } else {
      pr.kind = VerdictKind::ProvenDisjoint;
      pr.reason = core_reason(pr.core);
    }
    return pr;
  }
  solver->pop();

  bool one_in_two =
      region_subset(*solver, c1.overs, c2.unders, certify, timeout, report, axioms, recon_facts);
  bool two_in_one =
      region_subset(*solver, c2.overs, c1.unders, certify, timeout, report, axioms, recon_facts);
  if (a_first) {
    pr.subset_a_in_b = one_in_two;
    pr.subset_b_in_a = two_in_one;
  } else {
    pr.subset_a_in_b = two_in_one;
    pr.subset_b_in_a = one_in_two;
  }

  // SAT-direction cap (SPEC_ANALYSIS §4 / OPEN-2): oriented twins make a
  // joint model convention-ambiguous, so overlap is POSSIBLY, never PROVEN.
  std::set<QuantityId> combined = ra.quantities;
  combined.insert(rb.quantities.begin(), rb.quantities.end());
  auto twins = adl2::axioms::twin_pairs(hir.table, combined);
  if (!twins.empty()) {
    pr.kind = VerdictKind::PossiblyOverlapping;
    const auto& t = twins.front();
    pr.reason =
        "convention-ambiguous oriented twin pair present (" +
        adl2::axioms::quantity_label(hir, t.first) + " / " +
        adl2::axioms::quantity_label(hir, t.second) +
        "): SAT-direction verdicts capped at POSSIBLY until OPEN-2 is resolved";
    return pr;
  }

  // Overlap: SAT(Ax ∧ A⁻ ∧ B⁻) + Kleene region3 re-validation.
  solver->push();
  assert_unders(*solver, c1.unders);
  assert_unders(*solver, c2.unders);
  SatResult overlap = solver->check(timeout);
  note_failure(report, overlap);
  if (!overlap.is_sat()) {
    solver->pop();
    classify_overlap_non_sat(pr, overlap, disjoint);
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

  if (mentions_back_index(hir, combined)) {
    solver->pop();
    pr.kind = VerdictKind::PossiblyOverlapping;
    pr.reason =
        "under-approximations intersect, but a back-indexed element "
        "(`coll[-k]`) is not realizable by the witness builder; "
        "capped at POSSIBLY";
    return pr;
  }

  std::vector<QFormula> interior;
  interior.reserve(c1.unders.size() + c2.unders.size());
  for (const auto& u : c1.unders) interior.push_back(tightened(hir, u.qformula()));
  for (const auto& u : c2.unders) interior.push_back(tightened(hir, u.qformula()));

  std::optional<std::string> last_reject;
  std::optional<Validation> outcome;
  std::optional<Model> kept;
  for (int attempt = 0; attempt < MAX_WITNESS_ATTEMPTS; ++attempt) {
    auto model = refined_model(*solver, hir, combined, interior, timeout, &report);
    if (!model) break;
    Validation v = validate_witness(hir, ext, interp, *model, combined, ra.idx, rb.idx);
    if (v.kind == ValidationKind::Rejected) {
      std::string first_why = v.payload;
      Model snapped = snap_model(*model);
      Validation sv =
          validate_witness(hir, ext, interp, snapped, combined, ra.idx, rb.idx);
      if (sv.kind != ValidationKind::Rejected) {
        kept = std::move(snapped);
        outcome = std::move(sv);
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
    kept = std::move(*model);
    outcome = std::move(v);
    break;
  }
  solver->pop();

  if (outcome && outcome->kind == ValidationKind::Validated) {
    pr.kind = VerdictKind::ProvenOverlapping;
    pr.reason = std::string("both region cut sets are satisfiable together (") + OVERLAP_CAVEAT +
                ")";
    pr.witness_validated = true;
    if (kept) {
      pr.witness = validated_witness_values(hir, ext, interp, outcome->payload, *kept, combined);
    }
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
    if (kept) pr.witness = witness_values(hir, *kept, combined);
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
      file_fail_closed(report, "WITNESS NOT REALIZED for " + ra.name + " vs " + rb.name +
                                   ": witness validation failed for every model tried "
                                   "within the retry budget, so the overlap is capped at "
                                   "POSSIBLY and no claim is made — " +
                                   why);
    }
  } else {
    pr.reason = "solver returned SAT but no model; capped at POSSIBLY";
  }
  return pr;
}

void collect_nums(const adl2::sema::HNode& node, std::vector<double>& out) {
  if (node.kind == adl2::sema::HNode::Kind::Num) {
    try {
      std::size_t idx = 0;
      double v = std::stod(node.text, &idx);
      if (idx == node.text.size() && std::isfinite(v)) out.push_back(v);
    } catch (...) {
    }
    return;
  }
  for (const adl2::sema::HNode* c : node.children()) collect_nums(*c, out);
}

std::vector<double> cut_constants(const Hir& hir) {
  std::vector<double> vals;
  for (const auto& region : hir.regions) {
    for (const auto& stmt : region.stmts) {
      using SK = adl2::sema::HirRegionStmt::Kind;
      switch (stmt.kind) {
        case SK::Select:
        case SK::Reject:
        case SK::Trigger:
          collect_nums(stmt.node, vals);
          break;
        case SK::Bin:
          collect_nums(stmt.node, vals);
          for (const auto& e : stmt.edges) {
            try {
              std::size_t idx = 0;
              double v = std::stod(e, &idx);
              if (idx == e.size() && std::isfinite(v)) vals.push_back(v);
            } catch (...) {
            }
          }
          break;
        case SK::BinCond:
          collect_nums(stmt.node, vals);
          break;
        case SK::Inherit:
        case SK::NonMembership:
          break;
      }
    }
  }
  for (const auto& def : hir.defines) collect_nums(def.body, vals);
  for (const auto& pred : hir.elem_preds) collect_nums(pred.node, vals);
  auto total_lt = [](double a, double b) {
    auto bits = [](double x) -> std::uint64_t {
      std::uint64_t u = 0;
      std::memcpy(&u, &x, sizeof(u));
      return u;
    };
    auto xform = [&](double x) -> std::int64_t {
      std::int64_t v = static_cast<std::int64_t>(bits(x));
      std::uint64_t mask = (v < 0) ? 0x7FFFFFFFFFFFFFFFULL : 0;
      return v ^ static_cast<std::int64_t>(mask);
    };
    return xform(a) < xform(b);
  };
  std::sort(vals.begin(), vals.end(), total_lt);
  vals.erase(std::unique(vals.begin(), vals.end(),
                         [](double a, double b) {
                           std::uint64_t ua = 0, ub = 0;
                           std::memcpy(&ua, &a, sizeof(ua));
                           std::memcpy(&ub, &b, sizeof(ub));
                           return ua == ub;
                         }),
             vals.end());
  if (vals.size() > adl2::interp::MAX_CUT_CONSTANTS) vals.resize(adl2::interp::MAX_CUT_CONSTANTS);
  return vals;
}

std::optional<bool> memb(adl2::interp::Interp::EventEval& ev, std::size_t idx) {
  EvalError err;
  return ev.region_membership(idx, err);
}

void demote_disjoint(PairReport& report, const char* gate, const char* kind_word) {
  report.kind = VerdictKind::PossiblyOverlapping;
  report.reason = std::string("the ") + gate + " gate refuted a disjointness proof "
                                               "(internal contradiction, reported as a bug); capped "
                                               "at POSSIBLY";
  (void)kind_word;
  report.core.clear();
  report.certified = std::nullopt;
  report.proof_path = std::nullopt;
  report.certificate_size = std::nullopt;
}

void gate_pair(PairReport& report, std::size_t ia, std::size_t ib, const Interp& interp,
               const std::vector<Event>& gate_events, const std::vector<Event>& refute_probes,
               Report& diag, std::size_t& sample_refutations, std::size_t& refute_refutations) {
  if (report.kind == VerdictKind::ProvenDisjoint && !gate_events.empty()) {
    for (const auto& e : gate_events) {
      adl2::interp::Interp::EventEval ev(interp, e);
      if (memb(ev, ia) == true && memb(ev, ib) == true) {
        ++sample_refutations;
        file_contradiction(diag, "SAMPLING GATE refuted PROVEN DISJOINT for " + report.a + " vs " +
                                     report.b +
                                     ": a sampled event passes both regions — an encoder/axiom "
                                     "fact is false on a real event; verdict demoted");
        demote_disjoint(report, "sampling", "disjointness");
        break;
      }
    }
  }
  if (report.kind == VerdictKind::ProvenDisjoint &&
      search_shared_membership(interp, ia, ib, refute_probes)) {
    ++refute_refutations;
    file_contradiction(diag, "REFUTE GATE refuted PROVEN DISJOINT for " + report.a + " vs " +
                                 report.b +
                                 ": an adversarial probe event passes both regions — an "
                                 "encoder/axiom fact is false on a real event; verdict demoted");
    demote_disjoint(report, "refute", "disjointness");
  }
  auto sample_subset = [&](std::size_t sub, std::size_t sup, bool& flag, const char* label) {
    if (!flag || gate_events.empty()) return;
    for (const auto& e : gate_events) {
      adl2::interp::Interp::EventEval ev(interp, e);
      if (memb(ev, sub) == true && memb(ev, sup) != true) {
        ++sample_refutations;
        flag = false;
        file_contradiction(diag, std::string("SAMPLING GATE refuted PROVEN SUBSET (") + label +
                                     ") for " + report.a + " vs " + report.b +
                                     ": a sampled event is In the subset region but not In the "
                                     "superset (Out or Unknown); claim withdrawn");
        break;
      }
    }
  };
  auto refute_subset = [&](std::size_t sub, std::size_t sup, bool& flag, const char* label) {
    if (!flag) return;
    if (search_subset_counterexample(interp, sub, sup, refute_probes)) {
      ++refute_refutations;
      flag = false;
      file_contradiction(diag, std::string("REFUTE GATE refuted PROVEN SUBSET (") + label +
                                   ") for " + report.a + " vs " + report.b +
                                   ": an adversarial probe is In the subset region but not In the "
                                   "superset (Out or Unknown); claim withdrawn");
    }
  };
  bool a_in_b = report.subset_a_in_b;
  bool b_in_a = report.subset_b_in_a;
  sample_subset(ia, ib, a_in_b, "a within b");
  sample_subset(ib, ia, b_in_a, "b within a");
  refute_subset(ia, ib, a_in_b, "a within b");
  refute_subset(ib, ia, b_in_a, "b within a");
  report.subset_a_in_b = a_in_b;
  report.subset_b_in_a = b_in_a;
}

bool gate_empty(std::size_t idx, const std::string& name, const Interp& interp,
                const std::vector<Event>& gate_events, Report& diag, std::size_t& refutations) {
  for (const auto& e : gate_events) {
    Interp::EventEval ev(interp, e);
    if (memb(ev, idx) == true) {
      ++refutations;
      file_contradiction(diag, "SAMPLING GATE refuted REGION EMPTY for " + name +
                                   ": a sampled event is a member — an encoder/axiom fact is "
                                   "false on a real event; claim withdrawn");
      return true;
    }
  }
  return false;
}

bool refute_empty(std::size_t idx, const std::string& name, const Interp& interp,
                  const std::vector<Event>& refute_probes, Report& diag, std::size_t& refutations) {
  if (search_membership(interp, idx, refute_probes)) {
    ++refutations;
    file_contradiction(diag, "REFUTE GATE refuted REGION EMPTY for " + name +
                                 ": an adversarial probe is a member — an encoder/axiom fact is "
                                 "false on a real event; claim withdrawn");
    return true;
  }
  return false;
}

bool rsplit_once(const std::string& s, std::string& left, std::string& right) {
  auto pos = s.rfind("::");
  if (pos == std::string::npos) return false;
  left = s.substr(0, pos);
  right = s.substr(pos + 2);
  return true;
}

std::string coll_label(const Hir& hir, CollectionId c) {
  return adl2::sema::collection_ref(hir, c);
}

std::string size_label(const Hir& hir, QuantityId q) {
  const Quantity& qq = hir.table.quantity(q);
  if (qq.kind == QuantityKind::Size) {
    return "size(" + adl2::sema::collection_ref(hir, qq.coll) + ")";
  }
  return adl2::axioms::quantity_label(hir, q);
}

std::vector<std::string> coll_units(const Hir& hir, const UnitEnc& unit, CollectionId c) {
  auto mentions = [&](QuantityId q) {
    const Quantity& qq = hir.table.quantity(q);
    if (qq.kind == QuantityKind::Size) return qq.coll == c;
    if (qq.kind == QuantityKind::ElemProp) return qq.coll == c;
    return false;
  };
  std::vector<std::string> out;
  for (const auto& r : unit.regions) {
    std::string file, rest;
    if (!rsplit_once(r.name, file, rest)) continue;
    bool seen = false;
    for (const auto& u : out) {
      if (u == file) {
        seen = true;
        break;
      }
    }
    if (seen) continue;
    for (auto q : r.quantities) {
      if (mentions(q)) {
        out.push_back(file);
        break;
      }
    }
  }
  std::sort(out.begin(), out.end());
  return out;
}

std::optional<std::string> base_label(const Hir& hir, CollectionId c) {
  adl2::sema::Symbol base;
  std::vector<adl2::sema::ElemPredId> preds;
  if (!hir.table.filter_chain(c, base, preds)) return std::nullopt;
  return hir.symbols.display(base);
}

std::set<std::pair<QuantityId, QuantityId>> existing_size_le(
    const adl2::axioms::AxiomSet& axioms) {
  std::set<std::pair<QuantityId, QuantityId>> out;
  for (const auto& inst : axioms.instances) {
    if (inst.id != adl2::axioms::AxiomId::Sub) continue;
    if (inst.formula.kind != QFormula::Kind::Atom) continue;
    std::vector<QuantityId> qs;
    for (const auto& t : inst.formula.atom.terms()) qs.push_back(t.second);
    if (qs.size() != 2) continue;
    for (auto pair : {std::pair<QuantityId, QuantityId>{qs[0], qs[1]},
                      std::pair<QuantityId, QuantityId>{qs[1], qs[0]}}) {
      if (inst.formula == adl2::axioms::derived_size_le(pair.first, pair.second)) {
        out.insert(pair);
      }
    }
  }
  return out;
}

bool frame_sat(Solver& s, const adl2::formula::Formula& phi_a,
               const adl2::formula::Formula& phi_b, std::chrono::milliseconds timeout,
               Report& report) {
  s.push();
  s.assert_formula(phi_a.under().qformula(), std::nullopt);
  s.assert_formula(phi_b.under().qformula(), std::nullopt);
  SatResult r = s.check(timeout);
  note_failure(report, r);
  s.pop();
  return r.is_sat();
}

/// Returns (holds, certified_chain). `holds` is true when UNSAT and not
/// `Some(false)` from certify. Chain is present only when certified.
std::pair<bool, std::optional<CertPayload>> subset_proof(
    Solver& s, const adl2::formula::Over& sub_over, const adl2::formula::Under& sup_under,
    bool certify, std::chrono::milliseconds timeout, Report& report,
    const adl2::axioms::AxiomSet* axioms,
    const std::vector<std::pair<AssertName, QFormula>>* recon_facts) {
  s.push();
  AssertName qsub = AssertName::make("QSUB0");
  AssertName qneg = AssertName::make("QSUBNEG");
  QFormula over_f = sub_over.qformula();
  QFormula neg = sup_under.qformula().qnot();
  s.assert_formula(over_f, qsub);
  s.assert_formula(neg, qneg);
  SatResult r = s.check_unsat(timeout);
  note_failure(report, r);
  std::optional<std::vector<AssertName>> core_names;
  if (r.is_unsat()) {
    if (auto core = s.unsat_core()) core_names = *core;
  }
  s.pop();
  if (!r.is_unsat()) return {false, std::nullopt};
  std::vector<std::pair<AssertName, QFormula>> extra;
  extra.emplace_back(qsub, over_f);
  extra.emplace_back(qneg, neg);
  auto cert = certify_named_formulas(certify, core_names, extra, axioms, recon_facts, true);
  bool holds = cert.flag != false;
  return {holds, std::move(cert.payload)};
}

PredImplies prove_pred_implies(Solver& s, const adl2::formula::Formula& phi_a,
                               const adl2::formula::Formula& phi_b, bool certify,
                               std::chrono::milliseconds timeout, Report& report,
                               const adl2::axioms::AxiomSet* axioms,
                               const std::vector<std::pair<AssertName, QFormula>>* recon_facts) {
  PredImplies out;
  if (!frame_sat(s, phi_a, phi_b, timeout, report)) return out;
  auto a = subset_proof(s, phi_a.over(), phi_b.under(), certify, timeout, report, axioms,
                        recon_facts);
  auto b = subset_proof(s, phi_b.over(), phi_a.under(), certify, timeout, report, axioms,
                        recon_facts);
  out.a_in_b = a.first;
  out.a_chain = std::move(a.second);
  out.b_in_a = b.first;
  out.b_chain = std::move(b.second);
  return out;
}

ReconRun apply_reconcile(Hir& hir, const UnitEnc& unit, Solver* solver, bool certify,
                         std::chrono::milliseconds timeout, Report& report,
                         const adl2::axioms::AxiomSet& axioms, ReconEnc recon, CombineAcc& acc) {
  ReconRun run;
  for (const auto& s : recon.skipped) {
    ReconReport row;
    row.a = coll_label(hir, s.coll_a);
    row.b = coll_label(hir, s.coll_b);
    row.outcome = ReconOutcome::Skipped;
    row.note = s.reason;
    row.a_units = coll_units(hir, unit, s.coll_a);
    row.b_units = coll_units(hir, unit, s.coll_b);
    run.ledger.push_back(std::move(row));
  }
  for (const auto& n : recon.near_misses) {
    ReconNearMissReport row;
    row.a = coll_label(hir, n.coll_a);
    row.b = coll_label(hir, n.coll_b);
    row.base_a = n.base_a;
    row.base_b = n.base_b;
    run.near_misses.push_back(std::move(row));
  }
  if (!solver || recon.empty()) return run;
  auto existing = existing_size_le(axioms);
  std::size_t k = 0;
  for (const auto& cand : recon.candidates) {
    PredImplies pi = prove_pred_implies(*solver, cand.phi_a, cand.phi_b, certify, timeout, report,
                                       &axioms, &run.facts);
    std::string label_a = coll_label(hir, cand.coll_a);
    std::string label_b = coll_label(hir, cand.coll_b);
    ReconReport row;
    row.a = label_a;
    row.b = label_b;
    if (pi.a_in_b && pi.b_in_a) {
      row.outcome = ReconOutcome::Equivalent;
    } else if (pi.a_in_b) {
      row.outcome = ReconOutcome::ARefinesB;
    } else if (pi.b_in_a) {
      row.outcome = ReconOutcome::BRefinesA;
    } else {
      row.outcome = ReconOutcome::Unrelated;
    }
    row.base = base_label(hir, cand.coll_a);
    if (!(pi.a_in_b || pi.b_in_a)) row.note = "neither cut set implies the other";
    row.a_units = coll_units(hir, unit, cand.coll_a);
    row.b_units = coll_units(hir, unit, cand.coll_b);
    run.ledger.push_back(std::move(row));

    struct Fact {
      QuantityId sub;
      QuantityId sup;
      adl2::axioms::AxiomId id;
      const CertPayload* chain;
    };
    std::vector<Fact> facts;
    if (pi.a_in_b && pi.b_in_a) {
      facts.push_back({cand.size_a, cand.size_b, adl2::axioms::AxiomId::Xeq,
                       pi.a_chain ? &*pi.a_chain : nullptr});
      facts.push_back({cand.size_b, cand.size_a, adl2::axioms::AxiomId::Xeq,
                       pi.b_chain ? &*pi.b_chain : nullptr});
    } else if (pi.a_in_b) {
      facts.push_back({cand.size_a, cand.size_b, adl2::axioms::AxiomId::Xsub,
                       pi.a_chain ? &*pi.a_chain : nullptr});
    } else if (pi.b_in_a) {
      facts.push_back({cand.size_b, cand.size_a, adl2::axioms::AxiomId::Xsub,
                       pi.b_chain ? &*pi.b_chain : nullptr});
    }
    for (const auto& f : facts) {
      if (f.sub == f.sup || existing.count({f.sub, f.sup})) continue;
      if (certify && !f.chain) {
        file_fail_closed(report, "RECONCILIATION FACT WITHHELD for " + label_a + " / " +
                                     label_b +
                                     ": the subset refutation behind it produced no "
                                     "replayable certificate, so the derived size fact is "
                                     "not asserted.");
        continue;
      }
      QFormula fact = adl2::axioms::derived_size_le(f.sub, f.sup);
      AssertName name = AssertName::make("XR" + std::to_string(k));
      std::string statement = size_label(hir, f.sub) + " <= " + size_label(hir, f.sup);
      solver->assert_formula(fact, name);
      run.facts.emplace_back(name, fact);
      if (f.chain) {
        bool sub_first = f.sub == cand.size_a;
        const std::string& from = sub_first ? label_a : label_b;
        const std::string& to = sub_first ? label_b : label_a;
        std::vector<adl2::certify::BundleAssert> premises;
        premises.reserve(f.chain->asserts.size());
        for (const auto& nf : f.chain->asserts) {
          premises.push_back(adl2::certify::BundleAssert::make(
              nf.first.value, nf.second, adl2::certify::AssertSource::query(query_role(nf.first.value, from, to))));
        }
        adl2::certify::Derivation der;
        der.claim = "every element passing the cuts of " + from + " also passes those of " + to +
                    ", so " + from + " is a subset of " + to +
                    " element-wise: UNSAT(over(" + from + ") AND NOT under(" + to +
                    ")) over one shared generic element";
        der.premises = std::move(premises);
        der.certificate = f.chain->cert;
        acc.recon_chains.insert(
            {name, adl2::certify::DerivedFact::make(name.value, adl2::axioms::axiom_id_str(f.id),
                                                    statement, fact, {std::move(der)})});
      }
      CoreItem origin;
      origin.origin = CoreItem::Origin::Axiom;
      origin.id = adl2::axioms::axiom_id_str(f.id);
      origin.statement = statement;
      acc.origins[name] = std::move(origin);
      run.counts[adl2::axioms::axiom_id_str(f.id)]++;
      ++k;
    }
  }
  return run;
}


/// `UNSAT(Ax ∧ R⁺ ∧ Bᵢ⁺ ∧ Bⱼ⁺)` ⇒ bins i, j disjoint within R.
bool bins_disjoint(Solver* solver, std::chrono::milliseconds timeout, bool certify,
                   Report& report, const adl2::axioms::AxiomSet* axioms,
                   const std::vector<std::pair<AssertName, QFormula>>* recon_facts,
                   const RegionCtx& region_ctx, const Over& bi, const Over& bj,
                   const std::string& region) {
  AssertName bi_name = AssertName::make("QBINI");
  AssertName bj_name = AssertName::make("QBINJ");
  if (solver) {
    solver->push();
    assert_overs(*solver, region_ctx.overs);
    QFormula bi_f = bi.qformula();
    QFormula bj_f = bj.qformula();
    solver->assert_formula(bi_f, bi_name);
    solver->assert_formula(bj_f, bj_name);
    SatResult r = solver->check_unsat(timeout);
    note_failure(report, r);
    std::optional<std::vector<AssertName>> core;
    if (r.is_unsat()) {
      if (auto c = solver->unsat_core()) core = *c;
    }
    solver->pop();
    if (!r.is_unsat()) return false;
    std::vector<std::pair<AssertName, QFormula>> extra;
    for (const auto& p : region_ctx.overs) extra.emplace_back(p.first, p.second.qformula());
    extra.emplace_back(bi_name, std::move(bi_f));
    extra.emplace_back(bj_name, std::move(bj_f));
    auto cert = certify_named_formulas(certify, core, extra, axioms, recon_facts, false);
    return !(cert.flag.has_value() && !*cert.flag);
  }
  IntervalMap a = region_ctx.intervals;
  a.add_over(bi_name, bi);
  IntervalMap b = region_ctx.intervals;
  b.add_over(bj_name, bj);
  std::optional<std::vector<RefutingPart>> parts;
  if (auto e = a.self_empty()) parts = e->parts();
  else if (auto e = b.self_empty()) parts = e->parts();
  else if (auto d = a.disjoint_with(b, [](QuantityId) { return Presence::total(); })) {
    parts = d->parts;
  }
  if (!parts) return false;
  certify_interval_bin(*parts, region_ctx, bi_name, bi, bj_name, bj, certify, report, region);
  return true;
}

/// `UNSAT(Ax ∧ R⁺ ∧ ⋀ᵢ ¬(Bᵢ⁻))` ⇒ the bins cover the region.
std::pair<CoverageStatus, std::vector<WitnessValue>> bin_coverage(
    Solver* solver, std::chrono::milliseconds timeout, bool certify, Report& report,
    const adl2::axioms::AxiomSet* axioms,
    const std::vector<std::pair<AssertName, QFormula>>* recon_facts, const Hir& hir,
    const BinSetEnc& set, const RegionCtx& region_ctx, const std::vector<Under>& unders) {
  if (!solver) return {CoverageStatus::Unknown, {}};
  solver->push();
  assert_overs(*solver, region_ctx.overs);
  std::vector<std::pair<AssertName, QFormula>> extra;
  for (const auto& p : region_ctx.overs) extra.emplace_back(p.first, p.second.qformula());
  for (std::size_t k = 0; k < unders.size(); ++k) {
    AssertName name = AssertName::make("QBINNEG" + std::to_string(k));
    QFormula neg = unders[k].qformula().qnot();
    solver->assert_formula(neg, name);
    extra.emplace_back(name, std::move(neg));
  }
  SatResult result = solver->check(timeout);
  note_failure(report, result);
  std::pair<CoverageStatus, std::vector<WitnessValue>> out{CoverageStatus::Unknown, {}};
  if (result.is_unsat()) {
    std::optional<std::vector<AssertName>> core;
    if (auto c = solver->unsat_core()) core = *c;
    auto cert = certify_named_formulas(certify, core, extra, axioms, recon_facts, false);
    if (cert.flag.has_value() && !*cert.flag) {
      out.first = CoverageStatus::NotProven;
    } else {
      out.first = CoverageStatus::Proven;
    }
  } else if (result.is_sat()) {
    std::set<QuantityId> bin_qs;
    for (const auto& f : set.bins) formula_quantities(f, bin_qs);
    if (auto m = solver->model()) out.second = witness_values(hir, *m, bin_qs);
    out.first = CoverageStatus::NotProven;
  }
  solver->pop();
  return out;
}

BinCheckReport bin_check(Solver* solver, std::chrono::milliseconds timeout, bool certify,
                         Report& report, const adl2::axioms::AxiomSet* axioms,
                         const std::vector<std::pair<AssertName, QFormula>>* recon_facts,
                         const Hir& hir, const BinSetEnc& set, const RegionCtx& region_ctx,
                         std::string region_name) {
  const std::vector<Over>& overs = set.overs;
  const std::vector<Under>& unders = set.unders;
  std::size_t n = set.bins.size();
  std::size_t proven = 0;
  std::size_t total = n * (n > 0 ? n - 1 : 0) / 2;
  for (std::size_t i = 0; i < n; ++i) {
    for (std::size_t j = i + 1; j < n; ++j) {
      if (bins_disjoint(solver, timeout, certify, report, axioms, recon_facts, region_ctx,
                        overs[i], overs[j], region_name)) {
        ++proven;
      }
    }
  }
  auto cov = bin_coverage(solver, timeout, certify, report, axioms, recon_facts, hir, set,
                          region_ctx, unders);
  BinCheckReport br;
  br.region = std::move(region_name);
  br.variable = set.variable;
  br.n_bins = n;
  br.disjoint_pairs_proven = proven;
  br.disjoint_pairs_total = total;
  br.coverage = cov.first;
  br.gap_witness = std::move(cov.second);
  return br;
}

}  // namespace adl2::analysis
