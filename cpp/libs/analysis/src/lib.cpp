#include "adl2/analysis/analysis.hpp"

#include "adl2/analysis/reconcile.hpp"
#include "adl2/analysis/refute.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/certify/bundle.hpp"
#include "adl2/certify/certify.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/formula/lin.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/interp/sample.hpp"
#include "adl2/sema/dump.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/solver/solver.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <functional>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace adl2::analysis {
namespace {

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::interp::Event;
using adl2::interp::EvalError;
using adl2::interp::Interp;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
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

/// Named formulas + Farkas tree for one certified claim (Rust `CertPayload`).
struct CertPayload {
  std::vector<std::pair<AssertName, QFormula>> asserts;
  adl2::certify::Certificate cert;
  bool whole = true;
};

struct Certified {
  std::optional<bool> flag;
  std::optional<CertPayload> payload;
};

void note_failure(Report& report, const SatResult& r);
Certified certify_named_formulas(bool certify, const std::optional<std::vector<AssertName>>& core,
                                 const std::vector<std::pair<AssertName, QFormula>>& extra,
                                 const adl2::axioms::AxiomSet* axioms,
                                 const std::vector<std::pair<AssertName, QFormula>>* recon_facts,
                                 bool keep_payload);

/// Accumulator for `--combine` (origins, recon derivation chains, bundles).
struct CombineAcc {
  bool enabled = false;
  const Hir* hir = nullptr;
  std::map<AssertName, CoreItem> origins;
  std::map<AssertName, adl2::certify::DerivedFact> recon_chains;
  std::vector<adl2::certify::CombineBundle> bundles;
};

std::string size_label(const Hir& hir, QuantityId q);

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
                 const adl2::axioms::AxiomSet& axioms,
                 const std::set<QuantityId>& extra) {
  std::set<QuantityId> all_q;
  for (const auto& r : unit.regions) all_q.insert(r.quantities.begin(), r.quantities.end());
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

/// `¬(R⁻)`: the under-projection is the conjunction of statement unders, so
/// its exact negation is the disjunction of their NNF negations (smash2
/// `negated_under`). Asserting each `¬uᵢ` separately would be `∧ ¬uᵢ` —
/// the dual, which makes UNSAT easier and fabricates PROVEN SUBSET.
QFormula negated_under(const RegionCtx& inner) {
  std::vector<QFormula> parts;
  parts.reserve(inner.unders.size());
  for (const auto& u : inner.unders) {
    parts.push_back(u.qformula().qnot());
  }
  return QFormula::of_or(std::move(parts));
}

/// `UNSAT(Ax ∧ outer⁺ ∧ ¬(inner⁻))` ⇒ outer ⊆ inner. Named QSUB{k} / QSUBNEG
/// so certify can replay the core. When certify is on, `Some(false)` is not a
/// subset claim (smash2 `subset_proof`).
bool region_subset(Solver& s, const RegionCtx& outer, const RegionCtx& inner, bool certify,
                   std::chrono::milliseconds timeout, Report& report,
                   const adl2::axioms::AxiomSet* axioms,
                   const std::vector<std::pair<AssertName, QFormula>>* recon_facts) {
  s.push();
  std::vector<std::pair<AssertName, QFormula>> extra;
  extra.reserve(outer.overs.size() + 1);
  std::size_t k = 0;
  for (const auto& p : outer.overs) {
    AssertName name = AssertName::make("QSUB" + std::to_string(k++));
    QFormula f = p.second.qformula();
    s.assert_formula(f, name);
    extra.emplace_back(name, std::move(f));
  }
  AssertName neg_name = AssertName::make("QSUBNEG");
  QFormula neg = negated_under(inner);
  s.assert_formula(neg, neg_name);
  extra.emplace_back(neg_name, std::move(neg));
  SatResult r = s.check(timeout);
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

void file_fail_closed(Report& report, std::string msg) {
  Diagnostic d;
  d.class_ = DiagnosticClass::FailClosed;
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

const char* catalog_assumption_by_id(const std::string& id) {
  const auto* cat = adl2::axioms::catalog();
  int n = adl2::axioms::catalog_size();
  for (int i = 0; i < n; ++i) {
    if (adl2::axioms::axiom_id_str(cat[i].id) == id) return cat[i].assumption;
  }
  return "";
}

adl2::certify::AssertSource assert_source(const CombineAcc& acc, const AssertName& name,
                                          bool whole) {
  auto it = acc.origins.find(name);
  if (it == acc.origins.end()) return adl2::certify::AssertSource::unattributed();
  const CoreItem& c = it->second;
  if (c.origin == CoreItem::Origin::Cut) {
    return adl2::certify::AssertSource::cut(c.region, c.line, c.text, whole);
  }
  return adl2::certify::AssertSource::axiom(c.id, c.statement, catalog_assumption_by_id(c.id));
}

std::string query_role(const std::string& name, const std::string& sub, const std::string& sup) {
  if (name == "QSUBNEG") {
    return "negation of the under-projection of the " + sup + " element predicate";
  }
  return "over-projection of the " + sub + " element predicate (conjunct " + name + ")";
}

std::string bundle_label(const Hir& hir, QuantityId q) {
  const Quantity& qq = hir.table.quantity(q);
  if (qq.kind == QuantityKind::ElemProp && qq.index.kind == ElemIndexKind::FromFront &&
      qq.index.n == adl2::axioms::GENERIC_INDEX) {
    return adl2::sema::collection_ref(hir, qq.coll) + "[*]." + hir.table.prop_display(qq.prop) +
           " (any one element of the collection)";
  }
  return size_label(hir, q);
}

void push_bundle(CombineAcc& acc, const std::string& region_a, const std::string& region_b,
                 const CertPayload& payload, Report& report) {
  if (!acc.enabled || !acc.hir) return;
  std::vector<adl2::certify::BundleAssert> asserts;
  std::vector<adl2::certify::DerivedFact> derived_facts;
  asserts.reserve(payload.asserts.size());
  for (const auto& nf : payload.asserts) {
    auto it = acc.recon_chains.find(nf.first);
    adl2::certify::AssertSource src;
    if (it != acc.recon_chains.end()) {
      bool seen = false;
      for (const auto& d : derived_facts) {
        if (d.name == it->second.name) {
          seen = true;
          break;
        }
      }
      if (!seen) derived_facts.push_back(it->second);
      src = adl2::certify::AssertSource::derived(it->second.name);
    } else {
      src = assert_source(acc, nf.first, payload.whole);
    }
    asserts.push_back(adl2::certify::BundleAssert::make(nf.first.value, nf.second, std::move(src)));
  }
  const Hir* hir = acc.hir;
  adl2::certify::BundleParts parts;
  parts.region_a = region_a;
  parts.region_b = region_b;
  parts.asserts = std::move(asserts);
  parts.derived_facts = std::move(derived_facts);
  parts.certificate = payload.cert;
  auto bundle = adl2::certify::CombineBundle::make(std::move(parts), [hir](std::uint32_t q) {
    return bundle_label(*hir, QuantityId{q});
  });
  if (bundle.replay()) {
    acc.bundles.push_back(std::move(bundle));
  } else {
    file_fail_closed(report, "BUNDLE WITHHELD for " + region_a + " vs " + region_b +
                                 ": the assembled certificate bundle does not replay "
                                 "(most likely a reconciliation fact used as a given "
                                 "without an embedded derivation). The verdict stands on "
                                 "the analysis; the portable artifact does not, so none "
                                 "was written.");
  }
}

/// Interval-path certification. Disagreement is a diagnostic, never a demotion.
void certify_interval_pair(PairReport& pr, const std::vector<RefutingPart>& parts,
                           const RegionCtx& ca, const RegionCtx& cb, bool certify, Report& report,
                           CombineAcc& acc) {
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
  std::vector<std::pair<AssertName, QFormula>> whole;
  for (const auto& p : parts) {
    const AssertName& name = p.src();
    bool dup = false;
    for (const auto& n : whole) {
      if (n.first == name) {
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
    whole.emplace_back(name, *f);
  }
  std::vector<QFormula> whole_fs;
  whole_fs.reserve(whole.size());
  for (const auto& w : whole) whole_fs.push_back(w.second);
  if (auto cert = adl2::certify::certify_bounds(whole_fs)) {
    pr.certified = true;
    pr.certificate_size = whole.size();
    CertPayload payload;
    payload.asserts = std::move(whole);
    payload.cert = std::move(*cert);
    payload.whole = true;
    push_bundle(acc, pr.a, pr.b, payload, report);
    return;
  }
  std::vector<std::pair<AssertName, QFormula>> lean;
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
    lean.emplace_back(p.src(), QFormula::of_atom(p.atom));
  }
  std::vector<QFormula> lean_fs;
  lean_fs.reserve(lean.size());
  for (const auto& w : lean) lean_fs.push_back(w.second);
  if (auto cert = adl2::certify::certify_bounds(lean_fs)) {
    pr.certified = true;
    pr.certificate_size = lean.size();
    CertPayload payload;
    payload.asserts = std::move(lean);
    payload.cert = std::move(*cert);
    payload.whole = false;
    push_bundle(acc, pr.a, pr.b, payload, report);
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
Certified certify_named_formulas(bool certify, const std::optional<std::vector<AssertName>>& core,
                                 const std::vector<std::pair<AssertName, QFormula>>& extra,
                                 const adl2::axioms::AxiomSet* axioms,
                                 const std::vector<std::pair<AssertName, QFormula>>* recon_facts,
                                 bool keep_payload) {
  Certified out;
  if (!certify) return out;
  if (!core || core->empty()) {
    out.flag = false;
    return out;
  }
  std::map<AssertName, QFormula> fmap;
  for (const auto& e : extra) fmap[e.first] = e.second;
  if (axioms) {
    for (std::size_t i = 0; i < axioms->instances.size(); ++i) {
      fmap[AssertName::make("AX" + std::to_string(i))] = axioms->instances[i].formula;
    }
  }
  if (recon_facts) {
    for (const auto& e : *recon_facts) fmap[e.first] = e.second;
  }
  std::vector<std::pair<AssertName, QFormula>> named;
  named.reserve(core->size());
  for (const auto& n : *core) {
    auto it = fmap.find(n);
    if (it == fmap.end()) {
      out.flag = false;
      return out;
    }
    named.emplace_back(n, it->second);
  }
  std::vector<QFormula> formulas;
  formulas.reserve(named.size());
  for (const auto& nf : named) formulas.push_back(nf.second);
  auto r = adl2::certify::certify_unsat(formulas, adl2::certify::Budget::with_defaults());
  if (r.is_certified()) {
    out.flag = true;
    if (keep_payload) {
      CertPayload payload;
      payload.asserts = std::move(named);
      payload.cert = std::move(r.certificate);
      payload.whole = true;
      out.payload = std::move(payload);
    }
    return out;
  }
  out.flag = false;
  return out;
}

PairReport interval_or_solver_pair(const Hir& hir, const adl2::sema::ExtDecls& ext,
                                   const Interp& interp, const RegionEnc& ra, const RegionEnc& rb,
                                   const RegionCtx& ca, const RegionCtx& cb, Solver* solver,
                                   std::chrono::milliseconds timeout, Report& report, bool certify,
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
    certify_interval_pair(pr, d->parts, ca, cb, certify, report, acc);
    return pr;
  }
  if (auto empty_a = ca.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + ra.name + " provably selects no events (" + empty_a->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    certify_interval_pair(pr, empty_a->parts(), ca, cb, certify, report, acc);
    return pr;
  }
  if (auto empty_b = cb.intervals.self_empty()) {
    pr.kind = VerdictKind::ProvenDisjoint;
    pr.reason = "region " + rb.name + " provably selects no events (" + empty_b->human() +
                "), so the pair cannot intersect";
    pr.proof_path = ProofPath::Interval;
    certify_interval_pair(pr, empty_b->parts(), ca, cb, certify, report, acc);
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
      pr.reason += (core && !core->empty()) ? "solver unsat core" : "solver unsat";
    } else {
      pr.kind = VerdictKind::ProvenDisjoint;
      pr.reason = core && !core->empty() ? "solver unsat core" : "solver unsat";
    }
    return pr;
  }
  solver->pop();

  bool one_in_two = region_subset(*solver, c1, c2, certify, timeout, report, axioms, recon_facts);
  bool two_in_one = region_subset(*solver, c2, c1, certify, timeout, report, axioms, recon_facts);
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

std::optional<bool> memb(const Interp& interp, std::size_t idx, const Event& e) {
  EvalError err;
  return interp.eval_region_membership_idx(idx, e, err);
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
      if (memb(interp, ia, e) == true && memb(interp, ib, e) == true) {
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
      if (memb(interp, sub, e) == true && memb(interp, sup, e) == false) {
        ++sample_refutations;
        flag = false;
        file_contradiction(diag, std::string("SAMPLING GATE refuted PROVEN SUBSET (") + label +
                                     ") for " + report.a + " vs " + report.b +
                                     ": a sampled event is in the subset region but not the "
                                     "superset; claim withdrawn");
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
                                   ": an adversarial probe is in the subset region but not the "
                                   "superset; claim withdrawn");
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
    if (memb(interp, idx, e) == true) {
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
  SatResult r = s.check(timeout);
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

struct PredImplies {
  bool a_in_b = false;
  bool b_in_a = false;
  std::optional<CertPayload> a_chain;
  std::optional<CertPayload> b_chain;
};

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

struct ReconRun {
  std::map<std::string, std::size_t> counts;
  std::vector<std::pair<AssertName, QFormula>> facts;
  std::vector<ReconReport> ledger;
  std::vector<ReconNearMissReport> near_misses;
};

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

}  // namespace

Report analyze_hir(Hir& hir, const std::string& src, const adl2::sema::ExtDecls& ext,
                   const AnalysisOptions& opts) {
  retag_opaque_externals(hir);
  UnitEnc unit = encode_unit(hir, src);

  std::set<QuantityId> qs;
  for (const auto& r : unit.regions) qs.insert(r.quantities.begin(), r.quantities.end());
  adl2::axioms::AxiomSet axioms;
  if (opts.solver != SolverChoice::NoSolver || opts.reconcile) {
    axioms = adl2::axioms::emit_axioms(hir, ext, qs);
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
        certify_interval_pair(dummy, empty->parts(), ctxs[i], ctxs[i], true, report, no_bundle);
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
        auto cert = certify_named_formulas(opts.certify, core_names, extra, &axioms,
                                           &recon_run.facts, false);
        rr.empty = (cert.flag == false) ? EmptyStatus::Candidate : EmptyStatus::Proven;
        rr.empty_proof = ProofPath::SolverCore;
      } else {
        rr.empty = EmptyStatus::NotProven;
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
          opts.timeout, report, opts.certify, solver ? &axioms : nullptr, &recon_run.facts, acc);
      gate_pair(pr, unit.regions[i].idx, unit.regions[j].idx, interp, gate_events, refute_probes,
                report, gate_refutations, refute_refutations);
      report.pairwise.push_back(std::move(pr));
    }
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
