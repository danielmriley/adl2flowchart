# `adl2_rdgen` (host tool)

Build-time EBNF → recursive-descent emitter. See [`../../RDGEN.md`](../../RDGEN.md)
for the design. This binary is **not** installed and is **not** part of
the `smash2_cpp` CLI.

```bash
# usually invoked by CMake; useful by hand:
./cpp/build/adl2_rdgen \
  --ebnf cpp/grammar.ebnf \
  --parser-hpp cpp/libs/syntax/include/adl2/syntax/parser.hpp \
  --map cpp/tools/rdgen/method_map.txt \
  --check --dump-shapes
```

| Flag | Meaning |
|---|---|
| `--ebnf FILE` | Frozen grammar (required) |
| `--parser-hpp FILE` | `Parser` class header (required for `--check`) |
| `--map FILE` | `method_map.txt` (required for `--check` / `--emit-expr`) |
| `--check` | Completeness + generate-shape gate |
| `--dump-grammar` | Print parsed productions |
| `--dump-shapes` | Print shape classification |
| `--emit-expr FILE` | Write generated `parse_*` bodies (`-` = stdout) |
| `--emit-keywords FILE` | Extra lexer keyword-map entries (sibling synonyms) |
| `--replace FROM TO` | Rewrite EBNF text before parse (repeatable) |
| `--dump-synonyms` | Print inherited keyword synonyms |
| `--stamp FILE` | Touch FILE after success |

CMake target: `adl2_rdgen`. Library: `adl2_rdgen_lib` (unit tests only).
No dependency on `adl2_syntax` — that would be a bootstrap cycle.
