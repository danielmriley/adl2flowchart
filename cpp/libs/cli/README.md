# `adl2_cli` / `smash2_cpp`

Wires libraries into the `smash2_cpp` binary. **Does not own core logic.**

P1: `check [--dump-ast]` calls `adl2::syntax::parse_source` / `dump_ast` only.
Later phases wire sema → analysis → certify without moving that logic into cli.
