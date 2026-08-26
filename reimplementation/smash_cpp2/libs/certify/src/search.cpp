#include "search.hpp"

#include "fm.hpp"
#include "saturate.hpp"

#include <limits>
#include <sstream>
#include <utility>

namespace adl2::certify {
namespace {

std::size_t saturating_mul(std::size_t a, std::size_t b) {
  if (a != 0 && b > std::numeric_limits<std::size_t>::max() / a) {
    return std::numeric_limits<std::size_t>::max();
  }
  return a * b;
}

std::size_t saturating_add(std::size_t a, std::size_t b) {
  if (a > std::numeric_limits<std::size_t>::max() - b) {
    return std::numeric_limits<std::size_t>::max();
  }
  return a + b;
}

}  // namespace

Searcher::Searcher(const Budget& budget) : budget_(&budget) {
  fill_cap_ = saturating_add(saturating_mul(budget.max_atoms, budget.max_atoms), 64);
}

std::pair<bool, CertNode> Searcher::refute(
    const std::vector<adl2::formula::QFormula>& conj, std::size_t depth,
    std::string& reason) {
  if (depth > MAX_DEPTH) {
    std::ostringstream os;
    os << "budget: case-split depth exceeded " << MAX_DEPTH;
    reason = os.str();
    return {false, CertNode{}};
  }

  Saturated sat = saturate(conj);
  if (sat.has_false) {
    return {true, CertNode::contradiction()};
  }

  auto j = leftmost_or_index(sat.items);
  if (!j) {
    std::vector<Constraint> cons = collect_constraints(sat.items);
    if (cons.size() > budget_->max_atoms) {
      std::ostringstream os;
      os << "budget: leaf has " << cons.size() << " atoms (max " << budget_->max_atoms
         << ")";
      reason = os.str();
      return {false, CertNode{}};
    }
    LeafResult leaf = solve_leaf(cons, fill_cap_);
    switch (leaf.kind) {
      case LeafResultKind::Refuted: {
        std::vector<QRat> lam;
        lam.reserve(leaf.multipliers.size());
        for (auto& m : leaf.multipliers) {
          QRat q;
          q.value = std::move(m);
          lam.push_back(std::move(q));
        }
        return {true, CertNode::farkas(std::move(lam))};
      }
      case LeafResultKind::Feasible:
        reason = "branch satisfiable: real-feasible leaf";
        return {false, CertNode{}};
      case LeafResultKind::TooBig: {
        std::ostringstream os;
        os << "shape: fourier-motzkin fill-in exceeded " << fill_cap_;
        reason = os.str();
        return {false, CertNode{}};
      }
    }
    reason = "shape: fourier-motzkin unknown leaf result";
    return {false, CertNode{}};
  }

  const auto& ds = disjuncts(sat.items[*j]);
  std::vector<CertNode> branches;
  branches.reserve(ds.size());
  for (const auto& d : ds) {
    ++branches_;
    if (branches_ > budget_->max_branches) {
      std::ostringstream os;
      os << "budget: case-split count exceeded " << budget_->max_branches;
      reason = os.str();
      return {false, CertNode{}};
    }
    auto child = build_child(sat.items, *j, d);
    auto sub = refute(child, depth + 1, reason);
    if (!sub.first) return {false, CertNode{}};
    branches.push_back(std::move(sub.second));
  }
  return {true, CertNode::split(std::move(branches))};
}

}  // namespace adl2::certify
