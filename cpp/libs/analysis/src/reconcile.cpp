#include "adl2/analysis/reconcile.hpp"

#include "adl2/analysis/encode.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/sema/quantity.hpp"

#include <optional>
#include <utility>

namespace adl2::analysis {
namespace {

using adl2::axioms::GENERIC_INDEX;
using adl2::axioms::encode_elem_pred_generic;
using adl2::formula::DiagTable;
using adl2::formula::Formula;
using adl2::sema::Collection;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::ElemPredId;
using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::Symbol;

std::optional<Formula> lower(Hir& hir, const std::vector<ElemPredId>& preds,
                             CollectionId base) {
  DiagTable diags;
  std::vector<Formula> parts;
  parts.reserve(preds.size());
  for (auto pid : preds) {
    const auto& node = hir.elem_pred(pid).node;
    auto f = encode_elem_pred_generic(hir.table, node, base, GENERIC_INDEX, diags);
    if (!f) return std::nullopt;
    parts.push_back(std::move(*f));
  }
  if (parts.empty()) return Formula::ttrue();
  if (parts.size() == 1) return std::move(parts.front());
  return Formula::of_and(std::move(parts));
}

std::vector<ReconNearMiss> near_misses(Hir& hir, const ExtDecls& ext) {
  struct Chain {
    CollectionId id;
    Symbol base;
    std::vector<ElemPredId> preds;
  };
  std::vector<Chain> chains;
  for (std::size_t i = 0; i < hir.table.collections().size(); ++i) {
    CollectionId id{static_cast<std::uint32_t>(i)};
    if (hir.table.collection(id).kind != CollectionKind::Filtered) continue;
    Symbol base;
    std::vector<ElemPredId> preds;
    if (hir.table.filter_chain(id, base, preds)) {
      chains.push_back(Chain{id, base, std::move(preds)});
    }
  }
  std::vector<ReconNearMiss> out;
  for (std::size_t i = 0; i < chains.size(); ++i) {
    for (std::size_t j = i + 1; j < chains.size(); ++j) {
      if (chains[i].base == chains[j].base) continue;
      std::string name_a = hir.symbols.display(chains[i].base);
      std::string name_b = hir.symbols.display(chains[j].base);
      bool known_a = ext.base_collection(name_a) != nullptr;
      bool known_b = ext.base_collection(name_b) != nullptr;
      if (known_a && known_b) continue;
      CollectionId probe = hir.table.intern_collection(Collection::of_base(chains[i].base));
      auto fa = lower(hir, chains[i].preds, probe);
      auto fb = lower(hir, chains[j].preds, probe);
      if (!fa || !fb) continue;
      if (*fa == *fb) {
        ReconNearMiss n;
        n.coll_a = chains[i].id;
        n.coll_b = chains[j].id;
        n.base_a = std::move(name_a);
        n.base_b = std::move(name_b);
        out.push_back(std::move(n));
      }
    }
  }
  return out;
}

}  // namespace

std::set<QuantityId> ReconEnc::quantities() const {
  std::set<QuantityId> out;
  for (const auto& c : candidates) {
    out.insert(c.size_a);
    out.insert(c.size_b);
    formula_quantities(c.phi_a, out);
    formula_quantities(c.phi_b, out);
  }
  return out;
}

ReconEnc build_recon(Hir& hir, const ExtDecls& ext) {
  auto cands = hir.table.reconciliation_candidates();
  ReconEnc enc;
  for (const auto& pair : cands) {
    CollectionId a = pair.first;
    CollectionId b = pair.second;
    Symbol base_sym;
    std::vector<ElemPredId> preds_a;
    std::vector<ElemPredId> preds_b;
    if (!hir.table.filter_chain(a, base_sym, preds_a)) continue;
    Symbol ignore;
    if (!hir.table.filter_chain(b, ignore, preds_b)) continue;
    if (ext.base_collection(hir.symbols.display(base_sym)) == nullptr) {
      ReconSkip s;
      s.coll_a = a;
      s.coll_b = b;
      s.reason =
          "base `" + std::string(hir.symbols.display(base_sym)) +
          "` is not a known detector object, so a shared spelling "
          "cannot be assumed to mean a shared input";
      enc.skipped.push_back(std::move(s));
      continue;
    }
    CollectionId base = hir.table.intern_collection(Collection::of_base(base_sym));
    auto phi_a = lower(hir, preds_a, base);
    auto phi_b = lower(hir, preds_b, base);
    if (!phi_a || !phi_b) {
      ReconSkip s;
      s.coll_a = a;
      s.coll_b = b;
      s.reason =
          "a cut references a composite/peer element that cannot ground "
          "onto one shared element";
      enc.skipped.push_back(std::move(s));
      continue;
    }
    ReconCandidate c;
    c.size_a = hir.table.intern_quantity(Quantity::size(a));
    c.size_b = hir.table.intern_quantity(Quantity::size(b));
    c.phi_a = std::move(*phi_a);
    c.phi_b = std::move(*phi_b);
    c.coll_a = a;
    c.coll_b = b;
    enc.candidates.push_back(std::move(c));
  }
  enc.near_misses = near_misses(hir, ext);
  return enc;
}

}  // namespace adl2::analysis
