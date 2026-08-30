# smash_cpp2

C++ production ADL toolchain. Run analyses over events first. Prove
region relations after the interpreter agrees with the file.

`smash_cpp2` is the product name. Libraries keep the `adl2_*` CMake
names. Language decisions are the smash3 closed contract
(`LANGUAGE.md`). The parser is the smash2_cpp `parse_*` recursive
descent, not Flex or Bison. smash3 is the dump-ast / verify oracle.

This tree implements the smash3 product loop. `run` over JSONL or
ingested ROOT, `check` (dumps and `--json`), subprocess `verify`,
`verify --cross` / `--json` / `--combine DIR`, `smash_cpp2-recheck`,
`objects`, `dot` / `dot --ast`, `ingest`, and `run --histos`. Farkas
is the smash2_cpp certify kernel, not a rewrite.

## Install

Stock `cmake` ≥ 3.20 and `g++` (C++17). No Flex, no Bison, no libz3.
Some images default `CXX` to clang without libstdc++; use `CXX=g++`.

```bash
CXX=g++ cmake -S reimplementation/smash_cpp2 -B reimplementation/smash_cpp2/build -DCMAKE_BUILD_TYPE=Release
cmake --build reimplementation/smash_cpp2/build -j
alias smash_cpp2=$PWD/reimplementation/smash_cpp2/build/smash_cpp2
```

## Daily loop

```bash
# 1. evaluate regions over events (the meaning)
smash_cpp2 run analysis.adl events.jsonl
smash_cpp2 run --json analysis.adl events.jsonl
smash_cpp2 run --histos /tmp/histos analysis.adl events.jsonl
smash_cpp2 ingest --profile delphes -o events.jsonl file.root
smash_cpp2 run --profile delphes --histos /tmp/histos analysis.adl file.root

# 2. parse + resolve (dumps stay smash3-identical)
smash_cpp2 check analysis.adl
smash_cpp2 check --json analysis.adl
smash_cpp2 check --dump-ast analysis.adl
smash_cpp2 check --dump-hir analysis.adl
smash_cpp2 check --dump-quantities analysis.adl
smash_cpp2 check --dump-formula analysis.adl
smash_cpp2 check --dump-axioms analysis.adl

# object table and Graphviz DOT (flowchart, or AST with --ast)
smash_cpp2 objects analysis.adl
smash_cpp2 dot analysis.adl
smash_cpp2 dot --ast analysis.adl

# pairwise verdicts (subprocess z3 on PATH)
smash_cpp2 verify analysis.adl
smash_cpp2 verify --json analysis.adl
smash_cpp2 verify --cross a.adl b.adl

# portable smash2-combine/2 certificates; replay offline, no solver
smash_cpp2 verify --combine /tmp/bundles analysis.adl
smash_cpp2-recheck /tmp/bundles
```

`run` is listed first in `--help`. The interpreter is the meaning.
`verify` uses SMT-LIB `z3 -in`; there is no libz3 link.
`--combine DIR` writes one `smash2-combine/2` JSON file per certified
PROVEN DISJOINT pair. `smash_cpp2-recheck` and `smash3-recheck` both
replay those files. Producer is `smash_cpp2`. Schema is not `/1`.

## Grammar edits

Do not reach for Flex/Bison. The statement layer is LALR-hostile
(column-1 `define`, contextual `bins`, path tokens, particle lists,
bin-body fork). Ordinary expression productions stay recursive descent.

1. Edit `grammar.ebnf`.
2. Add a `method_map.txt` row (`production`, `parse_*`, `generate|hook`).
3. Add one row to `libs/syntax/include/adl2/syntax/stmt_dispatch.hpp`
   (section keyword or region-stmt keyword). `bins` stays a named hook.
4. Write `parse_*`. If `grammar_check.py` reports a new FIRST overlap,
   either change the grammar or list it in `grammar_hooks.txt`.
5. `python3 reimplementation/smash_cpp2/scripts/grammar_check.py`
   (reads EBNF + tables; does not parse ADL).
6. Compare `check --dump-ast` to smash3.

Language meaning is in `LANGUAGE.md`. Those items are closed.

## Dump gates

Stdout of `smash_cpp2 check --dump-ast`, `--dump-hir`,
`--dump-quantities`, `--dump-formula`, and `--dump-axioms` must match
smash3 on every `examples/**/*.adl` file (146).

```bash
smash3=./reimplementation/smash3/target/release/smash3
cpp2=./reimplementation/smash_cpp2/build/smash_cpp2
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_ast_corpus.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_hir_corpus.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_formula_tutorials.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_formula_corpus.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_axioms_tutorials.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_axioms_corpus.sh
```

## Run gate (U04)

Stdout of `smash_cpp2 run` must match smash3 `run` on the two tutorial
files plus `ex02_events.jsonl`.

```bash
events=reimplementation/adl2/crates/adl-difftest/tests/fixtures/ex02_events.jsonl
smash3=$smash3 cpp2=$cpp2 events=$events \
  reimplementation/smash_cpp2/scripts/run_tutorials.sh
```

## Ingest and histos (U10)

`ingest --profile delphes` must match the frozen Delphes JSONL and
smash3 ingest on the same file. `run --histos DIR` writes
`histos.json`, `cutflow.json`, bridges, and `out.root`. The TNamed
key is `smash2_provenance`. JSON matches smash3 after substituting
`smash_cpp2 0.1.0` for `smash3 0.1.0`. `--no-root` skips `out.root`.

```bash
fix=reimplementation/adl2/crates/adl-ingest/fixtures
smash3=$smash3 cpp2=$cpp2 \
  reimplementation/smash_cpp2/scripts/u10_accept.sh
```

## Objects and DOT gates

Stdout of `smash_cpp2 objects`, `dot`, and `dot --ast` must match smash3
on every `examples/**/*.adl` file (146).

```bash
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_objects_tutorials.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_dot_tutorials.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_dot_ast_tutorials.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_objects_corpus.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_dot_corpus.sh
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_dot_ast_corpus.sh
```

## Check --json (U13)

`smash_cpp2 check --json` writes one smash3-schema diagnostic array on
stdout. Keys stay `col, end, file, help, label, line, message,
severity, start`. A clean file is `[]`. Errors exit 1. `--json` cannot
combine with `--dump-*` (exit 2).

```bash
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/u13_accept.sh
```

## Verify corpus gate

`scripts/verify_corpus_gate.sh` runs `verify` on all 146 corpus files
against smash3. It fails on a `summary:` mismatch, any UNKNOWN, a
PROVEN DISJOINT rise, or drift from the pin (1900 pairs, 813 PD, 76
PO, 45 candidate, 966 possibly, 0 unknown).

```bash
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/verify_corpus_gate.sh
```

`scripts/ci_gates.sh` runs grammar-check (no ADL parse), then the dump,
run, ingest, cross, check --json, and verify-corpus gates. GitHub job
`smash_cpp2` runs grammar-check first, then invokes `ci_gates.sh`.

