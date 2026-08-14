#pragma once

/// Deterministic HIR / quantity-table dumps (Rust `hir_dump` / `quantity_table_dump`).

#include "adl2/sema/hir.hpp"

#include <string>

namespace adl2::sema {

std::string hir_dump(const Hir& hir);
std::string quantity_table_dump(const Hir& hir);
std::string render_node(const Hir& hir, const HNode& node);
std::string collection_ref(const Hir& hir, CollectionId id);

/// Aligned object-attribute summary (Rust `adl_sema::object_table`).
/// One row per declared collection: names, base chain, element cuts,
/// fragment status, derived size facts. `color` enables ANSI (tty only).
std::string object_table(const Hir& hir, bool color);

/// Canonical node render over in-progress tables (resolver intern keys).
std::string render_node_raw(const SymbolTable& symbols, const QuantityTable& table,
                            const std::vector<std::vector<Symbol>>& coll_names,
                            const std::vector<Symbol>& region_names,
                            const HNode& node);

}  // namespace adl2::sema
