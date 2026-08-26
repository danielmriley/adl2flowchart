#pragma once

/// Element-predicate encoder for EPRED / EPRES (Rust `adl-axioms`
/// `encode_elem_pred` / `encode_pred_exact`). Internal to adl2_axioms.

#include "adl2/formula/formula.hpp"
#include "adl2/sema/hir.hpp"

#include <cstdint>
#include <optional>
#include <set>

namespace adl2::axioms {

/// Encode a filter predicate onto `coll[index]` as an exact QFormula.
/// Un-encodable top-level And conjuncts are dropped (sound EPRED weakening).
std::optional<adl2::formula::QFormula> encode_elem_pred(
    adl2::sema::QuantityTable& table, const adl2::sema::HNode& node,
    adl2::sema::CollectionId coll, std::uint32_t index);

/// Every `ElemSelfProp` the predicate mentions.
void collect_self_props(const adl2::sema::HNode& node, std::set<adl2::sema::PropId>& out);

/// Does the filter keep ONLY elements that HAVE `prop`? Fail-closed: anything
/// outside the recognised absorbing-arithmetic shape answers false.
bool requires_present(const adl2::sema::HNode& pred, adl2::sema::PropId prop);

}  // namespace adl2::axioms
