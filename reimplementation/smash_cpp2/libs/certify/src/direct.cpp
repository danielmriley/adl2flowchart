#include "direct.hpp"

#include "constraint.hpp"
#include "saturate.hpp"

#include <utility>

namespace adl2::certify {
namespace {

constexpr std::size_t MAX_CONSTRAINTS = 64;

Certificate leaf_cert(const std::vector<adl2::sema::Rat>& lambdas) {
  std::vector<QRat> ms;
  ms.reserve(lambdas.size());
  for (const auto& lam : lambdas) {
    QRat q;
    q.value = lam;
    ms.push_back(std::move(q));
  }
  return Certificate(CertNode::farkas(std::move(ms)));
}

std::optional<std::pair<adl2::sema::QuantityId, const adl2::sema::Rat*>> sole_term(
    const Constraint& c) {
  const adl2::sema::QuantityId* q = nullptr;
  const adl2::sema::Rat* a = nullptr;
  for (const auto& qc : c.coeffs) {
    if (qc.second.is_zero()) continue;
    if (a) return std::nullopt;
    q = &qc.first;
    a = &qc.second;
  }
  if (!a) return std::nullopt;
  return std::make_pair(*q, a);
}

std::optional<adl2::sema::Rat> recip_abs(const adl2::sema::Rat& a) {
  return adl2::sema::Rat::one().checked_div(a.abs());
}

}  // namespace

std::optional<Certificate> construct_bounds(
    const std::vector<adl2::formula::QFormula>& formulas) {
  Saturated sat = saturate(formulas);
  if (sat.has_false) {
    return Certificate(CertNode::contradiction());
  }
  if (leftmost_or_index(sat.items)) return std::nullopt;

  std::vector<Constraint> cons = collect_constraints(sat.items);
  if (cons.empty() || cons.size() > MAX_CONSTRAINTS) return std::nullopt;

  std::vector<adl2::sema::Rat> zeros(cons.size(), adl2::sema::Rat::zero());

  for (std::size_t i = 0; i < cons.size(); ++i) {
    bool ground = true;
    for (const auto& qc : cons[i].coeffs) {
      if (!qc.second.is_zero()) {
        ground = false;
        break;
      }
    }
    if (!ground) continue;
    auto lam = zeros;
    lam[i] = adl2::sema::Rat::one();
    if (farkas_refutes(cons, lam)) return leaf_cert(lam);
  }

  for (std::size_t i = 0; i < cons.size(); ++i) {
    auto si = sole_term(cons[i]);
    if (!si) continue;
    for (std::size_t j = i + 1; j < cons.size(); ++j) {
      auto sj = sole_term(cons[j]);
      if (!sj) continue;
      if (si->first.id != sj->first.id) continue;
      if (si->second->is_negative() == sj->second->is_negative()) continue;
      auto li = recip_abs(*si->second);
      auto lj = recip_abs(*sj->second);
      if (!li || !lj) continue;
      auto lam = zeros;
      lam[i] = *li;
      lam[j] = *lj;
      if (farkas_refutes(cons, lam)) return leaf_cert(lam);
    }
  }
  return std::nullopt;
}

}  // namespace adl2::certify
