# `adl2_rdgen` — EBNF → recursive-descent C++

Compile-time host tool that reads the frozen collaborator grammar
([`grammar.ebnf`](grammar.ebnf)) and emits `parse_X` method bodies for
`adl2_syntax`. **Not Bison. Not Flex.** Generated code stays the same
hand-auditable recursive-descent style as today’s `parser.cpp`
(`adl2::syntax::Parser`, `peek` / `advance` / token vector). ADR-002’s
rejection of LALR still holds; ADR-011 records this amendment.

This file is the plan. Usage and CLI live in
[`tools/rdgen/README.md`](tools/rdgen/README.md). The EBNF ↔ `parse_X`
table is [`tools/rdgen/method_map.txt`](tools/rdgen/method_map.txt).

## Do I need a second full tool?

**A host compiler, not a second smash2.** Collaborators still type

```bash
CXX=g++ cmake -S cpp -B cpp/build
cmake --build cpp/build
```

CMake builds `adl2_rdgen` first (small C++17 executable, **no**
`adl2_*` libraries, **no** z3, **no** Python/flex/bison). A
`add_custom_command` then runs it with `grammar.ebnf` as an explicit
`DEPENDS`. Changing the EBNF rebuilds the generated include and then
`adl2_syntax`. Users of `smash2_cpp` never invoke the generator.

That *is* a secondary binary — the same niche bison would have occupied —
but it is a **build-time** tool, not a product CLI. Writing it in C++
keeps the published toolchain “stock cmake + g++”.

## Why a custom emitter (and not bison)

Legacy LALR hid 87 conflicts, a `NOT` token the lexer never produced,
hyphen-eating identifiers, and signed-literal lexing (ADR-002). A stock
`.y` also cannot state the things this grammar actually needs:

| Constraint | Why a table generator loses |
|---|---|
| Indent-only `object-define` | Layout, not a token |
| Contextual `bins` | Ident, not a keyword; bare-line vs `bin-stmt` |
| `path-token` | Arg-position only; greedy lex swallows exprs |
| Particle-list juxtaposition | `pT(jets[0] jets[1])` — two postfix, no comma |
| NEWLINE as a token | Statement recovery / `nl_before` |
| No signed-literal lex | Sign is grammatical (`signed-num`, `unary`) |
| Case-insensitive keywords | Lexer policy, not a production |
| dump-ast 146-file pin | AST shape is load-bearing |

So the generator **emits recursive-descent `parse_X`**, and leaves a
named **hook** wherever the EBNF is intentionally incomplete.

## Name and layout

| Item | Choice |
|---|---|
| CMake target / binary | `adl2_rdgen` |
| Namespace | `adl2::rdgen` (host tool only; **not** linked into smash2) |
| Sources | `cpp/tools/rdgen/` |
| Input grammar | `cpp/grammar.ebnf` (explicit CMake `DEPENDS`) |
| Input map | `cpp/tools/rdgen/method_map.txt` |
| Generated output | `${build}/libs/syntax/rdgen/parser_expr.inc.hpp` |
| Committed golden | `cpp/libs/syntax/generated/parser_expr.inc.hpp` (review + ctest) |

`parser.hpp` stays the class declaration (methods + private helpers).
The generator fills **method bodies**, not the class shape, until a later
phase proves we can emit declarations without fighting hooks.

## How CMake wires it

```
adl2_rdgen (host exe)
    ↓  --check --emit-expr
grammar.ebnf + method_map.txt + parser.hpp
    ↓
parser_expr.inc.hpp     (build tree)
    ↓  #include
parser.cpp  →  adl2_syntax  →  smash2_cpp
```

- `add_subdirectory(tools/rdgen)` **before** `libs/syntax` so the
  custom command can `DEPENDS adl2_rdgen`.
- `OBJECT_DEPENDS` on `parser.cpp` so a grammar edit rebuilds the TU.
- Generated sources live in the **build** directory. A committed golden
  is diffed by `ctest` (`adl2_rdgen_expr_golden`) so PRs show the C++
  we emit without making the golden the compile input (no stale-in-tree
  bootstrap).

## Shape-checked emission (fail closed)

The tool parses our EBNF dialect (`(* *)` comments, hyphenated names,
`{ }`, `[ ]`, `"literals"`) and classifies each production:

| Shape | EBNF pattern | First-slice emit? |
|---|---|---|
| **Alias** | `A = B` | yes — `parse_condition` |
| **LeftAssoc** | `A = B { (op\|op) B }` | yes — `or` / `and` / `+−` / `*/^` |
| **PrefixUnary** | `A = (op) A \| B` | yes — `not-expr`, `unary` |
| **OptionalSuffix** | `A = B [ "?" … ]` | yes — `ternary` |
| **KeywordSeq** | `"reject" condition` | yes — reject / trigger / cut |
| **Choice** | `A = B \| C \| …` | later — dispatchers |
| **TokenClass** | `cmp-op = ">" \| …` | no — `peek_cmp_op` |
| **Hook / Other** | indent, bins, path, … | never from EBNF alone |

A production marked `generate` in `method_map.txt` **must** classify as
a known emit shape; otherwise the build fails. Changing `or-expr` to
something the emitter does not understand fails closed instead of
emitting a wrong AST.

Operator literals map to existing `TokKind` / `BinOp` / `UnaryOp`
values. Same-op catalog groups (`or` / `||`) become a
`while (check A \|\| check B)` loop; mixed catalog groups (`+` / `-`)
become the current `for (;;)` switch. New words in a LeftAssoc group
do not inherit a `BinOp`: they match `Ident` and set `Expr::bin_key`.

## Fluid grammar (in progress)

Sibling inherit (`xor`→`or`) is the rejected design. The replacement
contract is [`tools/rdgen/FLUID.md`](tools/rdgen/FLUID.md): explicit
aliases only (`||`/`&&`/`!`); new words keep their own dump key.

## Small grammar edits (no C++)

Production membership is precedence, not meaning. See
[`tools/rdgen/FLUID.md`](tools/rdgen/FLUID.md).

- **Alias table** ([`tools/rdgen/aliases.txt`](tools/rdgen/aliases.txt)):
  `||`→`or`, `&&`→`and`, `!`→`not`. These dump as the canonical key.
- Sitting next to `"or"` does **not** make a new word an alias.
- A new word (e.g. `"xor"` in `or-expr`) keeps its own key. dump
  `Binary op=xor`. sema is **Unsupported**, never `Or`, never
  `ArithOp::Add`.
- **Forbidden:** sibling inherit (`xor`→`KwOr`/`BinOp::Or`).
- Unknown punctuation (`@@`) still fails closed (needs a lexer token).
- **`sel`:** out of this slice. Statement keywords need generated
  dispatch (slice 2).

```
or-expr = and-expr { ("or"|"||"|"xor") and-expr } ;
```

`xor` lexes as `Ident`. The generated `or-expr` parser matches that
lexeme (case-insensitive) and builds `Binary op=xor`. Catalog forms
`or` / `||` still build `Binary op=or`. The grammar author does not
edit `lexer.cpp`, `token.hpp`, or `parser.cpp`.

`ctest` `adl2_rdgen_mutate_parse` rebuilds a mutated
`parser_expr.inc.hpp` from `grammar.ebnf` with the `xor` edit above
and checks the AST.

## What stays hand-written (hooks)

These are named in the map and **must** exist on `Parser`. The generator
never invents their bodies from the EBNF comment.

- **Layout:** `parse_object_define` (`at_column_one`), section/stmt
  recovery (`synchronize_statement`, `nl_before`)
- **Contextual `bins`:** `parse_region_stmt` + `is_ident_text("bins")`
- **`path-token`:** `parse_path_token` (arg position, deprecation)
- **Particle lists:** `extend_particle_list`
- **Comparison extras:** chain-to-`and`, OPEN-4 `~=` warning,
  `parse_band_suffix`
- **Postfix extras:** `->` member (not in EBNF), `nl_before` index,
  trailing `_`
- **`sort-stmt`:** absorb to end of statement
- **`parse_derived_candidate`:** not in the EBNF (object-block extra)
- **Lexer:** stays hand-written. Do not start a lexer generator.

## Phases

0. Host tool + checker + expression ladder.
1. **Done.** Ternary, reject / trigger / cut, alias table, operator
   identity (`xor` is its own key; mutate-parse pins `Binary op=xor`).
2. Dispatchers (`section`, `region-stmt`) that only call hooks.
3. **Stop.** Do not generate indent / bins / path / particle-list /
   comparison-chain. Those stay hooks.

`parser.hpp` emission (declarations) is a later optional step. The
class’s private helpers are not grammar.

## Acceptance

- `adl2_rdgen --check` is part of the `adl2_syntax` build graph.
- `ctest` includes `adl2_rdgen_unit` and `adl2_rdgen_expr_golden`.
- `smash2_cpp_dump_ast_tiny` / `bins_and_path` stay green.
- Corpus dump-ast (146) stays well-formed; `CROSS_ORACLE=1` byte-diff
  is unchanged when that job runs.
- No flex, bison, Python, or Rust added to the `adl2-cpp` CI job.
- No new `examples/*.adl`.

## What this is not

- Not a replacement for [`BISON_MAP.md`](BISON_MAP.md) (onboarding).
- Not a rewrite of ADR-010 (C++ port, smash2 forever-oracle).
- Not permission to copy `legacy_parser/`’s `.y` into this tree.
