#include "detail.hpp"

#include "adl2/analysis/witness.hpp"

#include <cmath>
#include <initializer_list>
#include <map>
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

Rat wish_rat(double v) {
  auto r = Rat::from_decimal_f64(v);
  return r ? *r : Rat::zero();
}

QFormula tightened(const Hir& hir, const QFormula& f) {
  switch (f.kind) {
    case QFormula::Kind::True:
      return QFormula::ttrue();
    case QFormula::Kind::False:
      return QFormula::ffalse();
    case QFormula::Kind::And: {
      std::vector<QFormula> items;
      items.reserve(f.items.size());
      for (const auto& p : f.items) items.push_back(tightened(hir, p));
      return QFormula::of_and(std::move(items));
    }
    case QFormula::Kind::Or: {
      std::vector<QFormula> items;
      items.reserve(f.items.size());
      for (const auto& p : f.items) items.push_back(tightened(hir, p));
      return QFormula::of_or(std::move(items));
    }
    case QFormula::Kind::Atom: {
      bool exact_grid = true;
      for (const auto& t : f.atom.terms()) {
        QuantityKind k = hir.table.quantity(t.second).kind;
        if (k != QuantityKind::Size && k != QuantityKind::Present) {
          exact_grid = false;
          break;
        }
      }
      if (exact_grid) return QFormula::of_atom(f.atom);
      Rat eps = wish_rat(WITNESS_EPS);
      auto rebuild = [&](Rel rel, Rat k) {
        return QFormula::of_atom(LinAtom::make(f.atom.terms(), rel, std::move(k)));
      };
      switch (f.atom.rel()) {
        case Rel::Lt:
        case Rel::Le:
          return rebuild(f.atom.rel(), f.atom.constant() - eps);
        case Rel::Gt:
        case Rel::Ge:
          return rebuild(f.atom.rel(), f.atom.constant() + eps);
        case Rel::Eq:
          return QFormula::of_atom(f.atom);
        case Rel::Ne:
          return QFormula::of_or({rebuild(Rel::Le, f.atom.constant() - eps),
                                  rebuild(Rel::Ge, f.atom.constant() + eps)});
      }
      return QFormula::of_atom(f.atom);
    }
  }
  return QFormula::of_atom(f.atom);
}

std::optional<Model> refined_model(Solver& solver, const Hir& hir,
                                   const std::set<QuantityId>& mentioned,
                                   const std::vector<QFormula>& interior,
                                   std::chrono::milliseconds timeout, Report* failures) {
  auto note = [&](const SatResult& r) {
    if (!failures) return;
    if (!r.is_process_failure() && !r.is_solver_error()) return;
    if (!failures->solver_failures) failures->solver_failures = SolverFailures{};
    if (r.is_process_failure()) failures->solver_failures->spawn++;
    else failures->solver_failures->errors++;
    if (failures->solver_failures->first_reason.empty() && r.is_unknown()) {
      failures->solver_failures->first_reason = r.reason;
    }
  };

  std::map<QuantityId, double> lo_hints;
  std::map<QuantityId, double> hi_hints;
  std::vector<QuantityId> dphi_hints;
  auto need_elem = [&](CollectionId coll, std::uint32_t i) {
    auto sq = hir.table.quantity_id(Quantity::size(coll));
    if (!sq) return;
    double need = static_cast<double>(i);
    auto it = lo_hints.find(*sq);
    if (it == lo_hints.end()) lo_hints.emplace(*sq, need);
    else if (need > it->second) it->second = need;
  };
  std::set<QuantityId> bulk_hints;
  for (auto q : mentioned) {
    const Quantity& qq = hir.table.quantity(q);
    if (qq.kind == QuantityKind::ExternalFn &&
        hir.symbols.key(qq.name).rfind("reduce.", 0) == 0 && !qq.args.empty() &&
        qq.args.front().kind == QuantityArgKind::Collection) {
      if (auto sq = hir.table.quantity_id(Quantity::size(qq.args.front().coll))) {
        bulk_hints.insert(*sq);
      }
    }
  }
  for (auto q : mentioned) {
    const Quantity& qq = hir.table.quantity(q);
    switch (qq.kind) {
      case QuantityKind::ElemProp:
        if (qq.index.kind == ElemIndexKind::FromFront) need_elem(qq.coll, qq.index.n);
        break;
      case QuantityKind::AngularSep:
        if (qq.ang == AngKind::DPhi) dphi_hints.push_back(q);
        for (const ParticleRef* p : {&qq.a, &qq.b}) {
          if (p->kind == ParticleKind::Elem && p->index.kind == ElemIndexKind::FromFront) {
            need_elem(p->coll, p->index.n);
          }
        }
        break;
      case QuantityKind::Size:
        hi_hints[q] = MAX_REALIZED_F;
        break;
      default:
        break;
    }
  }

  std::vector<QFormula> lo_atoms;
  for (const auto& kv : lo_hints) {
    lo_atoms.push_back(
        QFormula::of_atom(LinAtom::single(kv.first, Rel::Gt, wish_rat(kv.second))));
  }
  std::vector<QFormula> hi_atoms;
  for (const auto& kv : hi_hints) {
    hi_atoms.push_back(
        QFormula::of_atom(LinAtom::single(kv.first, Rel::Le, wish_rat(kv.second))));
  }
  std::vector<QFormula> zero_atoms;
  for (auto q : dphi_hints) {
    zero_atoms.push_back(QFormula::of_atom(LinAtom::single(q, Rel::Eq, Rat::zero())));
  }
  // Dyadic, strictly inside [−π, π). Smash2 `DPHI_WISH_BOUND`.
  constexpr double DPHI_WISH_BOUND = 3.140625;
  for (auto q : dphi_hints) {
    hi_atoms.push_back(
        QFormula::of_atom(LinAtom::single(q, Rel::Ge, wish_rat(-DPHI_WISH_BOUND))));
    hi_atoms.push_back(
        QFormula::of_atom(LinAtom::single(q, Rel::Le, wish_rat(DPHI_WISH_BOUND))));
  }
  std::vector<QFormula> bulk_atoms;
  for (auto sq : bulk_hints) {
    bulk_atoms.push_back(
        QFormula::of_atom(LinAtom::single(sq, Rel::Ge, wish_rat(MAX_REALIZED_F))));
  }

  auto try_with = [&](std::initializer_list<const std::vector<QFormula>*> groups)
      -> std::optional<Model> {
    solver.push();
    for (const auto* group : groups) {
      for (const auto& a : *group) solver.assert_formula(a, std::nullopt);
    }
    SatResult result = solver.check(timeout);
    std::optional<Model> m;
    if (result.is_sat()) m = solver.model();
    solver.pop();
    note(result);
    return m;
  };

  std::optional<Model> base = solver.model();
  if (auto m = try_with({&zero_atoms, &interior, &lo_atoms, &hi_atoms, &bulk_atoms})) {
    return m;
  }
  if (auto m = try_with({&interior, &lo_atoms, &hi_atoms, &bulk_atoms})) return m;
  if (auto m = try_with({&zero_atoms, &interior, &lo_atoms, &hi_atoms})) return m;
  if (auto m = try_with({&interior, &lo_atoms, &hi_atoms})) return m;
  if (auto m = try_with({&lo_atoms, &hi_atoms})) return m;
  if (auto m = try_with({&hi_atoms})) return m;
  return base;
}

}  // namespace adl2::analysis
