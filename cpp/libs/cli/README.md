# `adl2_cli` / `smash2_cpp`

Wires libraries into the `smash2_cpp` binary. **Does not own core logic.**

P2: `check [--dump-ast|--dump-hir|--dump-quantities]` calls
`adl2::syntax::parse_source` / `dump_ast` or `adl2::sema::analyze_str` /
`hir_dump` / `quantity_table_dump`. Links `adl2_syntax` + `adl2_sema`.

`--dump-ast` stays parse-only so a sema bug cannot break the P1 corpus
gate. Later phases wire formula → analysis → certify without moving that
logic into cli.
