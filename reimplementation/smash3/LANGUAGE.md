# smash3 language decisions

These are closed for smash3. The interpreter is the meaning. Dual hedges
stay only where the encoder cannot pick one reading without lying.

| Item | smash3 decision | Where it lives |
|---|---|---|
| Unindexed collection cut (`pt(jets)` at region level) | Dual bounded expansion, k=3. Empty collection is true in the plus branch. | `adl-formula` encoder, audit Bug 1 |
| `dPhi` / `dEta` | Oriented quantities. Range axiom −π…π. Twin axiom `x=y ∨ x=−y`. SAT-direction caps at POSSIBLY when reversed twins appear. | `adl-sema` Quantity, `adl-axioms` |
| Index base and `[-n]` | 0-based. `jets[-1].pt` is `FromBack(1)` in fragment. `COMB(jets[-1] …)` and similar stay Unsupported. | `adl-sema` `ElemIndex::FromBack` |
| `~=` | Same parse as `!=`. One warning per file. Not "approximately equal". | `adl-syntax` lexer |
| `size` / `Size` / `count` | Case-insensitive aliases of the size quantity. | `adl-sema` |
| Name resolution | Case-insensitive. Diagnostics keep the source spelling. | `adl-sema` |
| Division by zero / non-finite | The enclosing comparison is false. Non-finite literals cannot build atoms. | `adl-interp`, `adl-sema::Rat` |
| `and` / `or` | Standard precedence. `or` binds looser. | `grammar.ebnf` |
| Solver | SMT-LIB subprocess to a `z3` binary is the default. Native libz3 is opt-in. | `adl-solver` |
| Numerics | Exact rationals on the checked fragment. `0.3` is `3/10`. | `adl-sema::Rat` |
| Missing elements | A comparison over an absent element is false. Encodings guard with presence. | `adl-interp`, `adl-formula` |

Grammar edits start in `grammar.ebnf`. Add a dump-ast test next to the
edit. Then change the matching `parse_*` in `crates/adl-syntax/src/parser.rs`.
`BISON_MAP.md` names the function for each nonterminal.

Flex and Bison are not the implementation. The statement layer is
layout-sensitive and contextual. Those rules stay in named parse
functions.
