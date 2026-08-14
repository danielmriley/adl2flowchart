#include "fm.hpp"

#include "adl2/sema/quantity.hpp"

#include <cstdint>
#include <map>
#include <optional>
#include <utility>

namespace adl2::certify {
namespace {

struct Row {
  std::map<adl2::sema::QuantityId, adl2::sema::Rat> coeffs;
  adl2::sema::Rat b;
  bool strict = false;
  std::vector<adl2::sema::Rat> prov;
};

enum class RowStatus { Contradiction, Satisfiable, Open };

RowStatus classify(const Row& row) {
  if (!row.coeffs.empty()) return RowStatus::Open;
  const bool contradiction = row.strict ? !row.b.is_positive() : row.b.is_negative();
  return contradiction ? RowStatus::Contradiction : RowStatus::Satisfiable;
}

std::optional<adl2::sema::QuantityId> pick_var(const std::vector<Row>& rows) {
  std::optional<adl2::sema::QuantityId> best;
  for (const auto& r : rows) {
    if (r.coeffs.empty()) continue;
    adl2::sema::QuantityId q = r.coeffs.begin()->first;
    if (!best || q < *best) best = q;
  }
  return best;
}

Row combine(const adl2::sema::Rat& mp, const Row& p, const adl2::sema::Rat& mn,
            const Row& n) {
  Row out;
  for (const auto& kv : p.coeffs) {
    out.coeffs.emplace(kv.first, mp * kv.second);
  }
  for (const auto& kv : n.coeffs) {
    adl2::sema::Rat term = mn * kv.second;
    auto it = out.coeffs.find(kv.first);
    if (it == out.coeffs.end()) {
      out.coeffs.emplace(kv.first, std::move(term));
    } else {
      it->second = it->second + term;
    }
  }
  for (auto it = out.coeffs.begin(); it != out.coeffs.end();) {
    if (it->second.is_zero()) {
      it = out.coeffs.erase(it);
    } else {
      ++it;
    }
  }

  out.b = (mp * p.b) + (mn * n.b);
  const std::size_t nprov =
      p.prov.size() < n.prov.size() ? p.prov.size() : n.prov.size();
  out.prov.reserve(nprov);
  for (std::size_t i = 0; i < nprov; ++i) {
    out.prov.push_back((mp * p.prov[i]) + (mn * n.prov[i]));
  }
  // Motzkin: combined relation is strict iff either parent is strict.
  out.strict = p.strict || n.strict;
  return out;
}

}  // namespace

LeafResult solve_leaf(const std::vector<Constraint>& cons, std::size_t fill_cap) {
  const std::size_t n = cons.size();
  std::vector<Row> rows;
  rows.reserve(n);

  for (std::size_t i = 0; i < n; ++i) {
    Row row;
    row.b = cons[i].b;
    row.strict = cons[i].strict;
    row.prov.assign(n, adl2::sema::Rat::zero());
    row.prov[i] = adl2::sema::Rat::one();
    for (const auto& qc : cons[i].coeffs) {
      row.coeffs.emplace(qc.first, qc.second);
    }
    switch (classify(row)) {
      case RowStatus::Contradiction: {
        LeafResult r;
        r.kind = LeafResultKind::Refuted;
        r.multipliers = std::move(row.prov);
        return r;
      }
      case RowStatus::Satisfiable:
        break;
      case RowStatus::Open:
        rows.push_back(std::move(row));
        break;
    }
  }

  while (auto v = pick_var(rows)) {
    std::vector<Row> pos;
    std::vector<Row> neg;
    std::vector<Row> next;
    for (auto& row : rows) {
      auto it = row.coeffs.find(*v);
      if (it == row.coeffs.end()) {
        next.push_back(std::move(row));
        continue;
      }
      const std::int32_t sgn = it->second.signum();
      if (sgn == 1) {
        pos.push_back(std::move(row));
      } else if (sgn == -1) {
        neg.push_back(std::move(row));
      } else {
        next.push_back(std::move(row));
      }
    }
    rows.clear();

    for (const auto& p : pos) {
      for (const auto& nrow : neg) {
        const adl2::sema::Rat& cp = p.coeffs.at(*v);
        const adl2::sema::Rat& cn = nrow.coeffs.at(*v);
        const adl2::sema::Rat mp = -cn;
        const adl2::sema::Rat& mn = cp;
        Row row = combine(mp, p, mn, nrow);
        switch (classify(row)) {
          case RowStatus::Contradiction: {
            LeafResult r;
            r.kind = LeafResultKind::Refuted;
            r.multipliers = std::move(row.prov);
            return r;
          }
          case RowStatus::Satisfiable:
            break;
          case RowStatus::Open:
            next.push_back(std::move(row));
            if (next.size() > fill_cap) {
              LeafResult r;
              r.kind = LeafResultKind::TooBig;
              return r;
            }
            break;
        }
      }
    }
    rows = std::move(next);
  }

  LeafResult r;
  r.kind = LeafResultKind::Feasible;
  return r;
}

}  // namespace adl2::certify
