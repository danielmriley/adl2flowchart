#pragma once

/// `adl2_viz` — Graphviz DOT from the resolved HIR (Rust `adl-viz`).
/// Depends on sema, not AST-only paths for flowchart meaning.
///
/// Output is deterministic: declaration order, ids from stable HIR indices.

#include "adl2/sema/hir.hpp"

#include <string>

namespace adl2::viz {

/// Flowchart DOT: object collections with take/union/combination lineage
/// and regions with ordered membership statements + inheritance edges.
std::string flowchart_dot(const adl2::sema::Hir& hir);

/// Resolved expression trees of every region cut, object predicate and
/// define, as a node-per-subexpression graph.
std::string ast_dot(const adl2::sema::Hir& hir);

int module_anchor();

}  // namespace adl2::viz
