# `adl2_cli` / `smash2_cpp`

Wires libraries into the `smash2_cpp` binary. **Does not own core logic.**

P2: `check [--dump-ast|--dump-hir|--dump-quantities]` calls
`adl2::syntax::parse_source` / `dump_ast` or `adl2::sema::analyze_str` /
`hir_dump` / `quantity_table_dump`. Links `adl2_syntax` + `adl2_sema`.

**Bare `check` (no dump flag) is parse-only in P2.** It does not run name
resolution. Rust `smash2 check` always resolves. This is an intentional
contract, not silent under-parity: help text and a stderr note say so.
`--dump-ast` is also parse-only so a sema bug cannot break the P1 corpus
gate. `--dump-hir` / `--dump-quantities` run sema. A later phase can wire
bare `check` to `analyze_str` without moving that logic into cli.
