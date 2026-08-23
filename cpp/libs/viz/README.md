# `adl2_viz`

Flowchart and AST Graphviz DOT from the resolved HIR (Rust `adl-viz`).
Depends on `adl2_sema` only — never on raw AST-only paths for meaning.

Public API (`include/adl2/viz/viz.hpp`):

- `flowchart_dot(hir)` — object lineage (`take`/`union`/`comb`/…) and
  regions with ordered membership statements + inheritance edges
- `ast_dot(hir)` — node-per-subexpression forest of defines, object
  predicates, and region cuts

Output is deterministic: declaration order, ids from stable HIR indices,
never hash/pointer order. DOT escaping: `"`, `\`, newline → `\"` `\\` `\n`.

CLI: `smash2_cpp dot [--ast] <file.adl>` (DOT on stdout; diags on stderr).
Oracle gate: `cpp/scripts/dump_dot_corpus_gate.sh` vs Rust `smash2 dot`.
