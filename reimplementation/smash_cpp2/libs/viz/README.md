# `adl2_viz`

Flowchart and AST Graphviz DOT from the resolved HIR. smash2_cpp
`cpp/libs/viz` is the algorithm. smash3 `dot` / `dot --ast` is the
stdout oracle.

Public API (`include/adl2/viz/viz.hpp`):

- `flowchart_dot(hir)` — object lineage (`take` / `union` / `comb`) and
  regions with ordered membership statements plus inheritance edges
- `ast_dot(hir)` — node-per-subexpression forest of defines, object
  predicates, and region cuts

Output is deterministic. Declaration order and HIR indices set the
node ids. DOT escaping maps `"`, `\`, and newline to `\"`, `\\`, `\n`.

CLI: `smash_cpp2 dot [--ast] <file.adl>` writes DOT on stdout and
diagnostics on stderr.
