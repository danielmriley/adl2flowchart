# smash_cpp2

C++ production ADL toolchain. Run analyses over events first. Prove
region relations after the interpreter agrees with the file.

`smash_cpp2` is the product name. Libraries keep the `adl2_*` CMake
names. Language decisions are the smash3 closed contract
(`LANGUAGE.md`). The parser is the smash2_cpp `parse_*` recursive
descent, not Flex or Bison. smash3 is the dump-ast / verify oracle.

U07 of this tree implements `run` over JSONL events, `check` dumps,
and `verify` via a subprocess `z3` solver. Later units add ingest
and ROOT histos. Farkas is the smash2_cpp certify kernel, not a
rewrite.

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

# 2. parse + resolve (dumps stay smash3-identical)
smash_cpp2 check analysis.adl
smash_cpp2 check --dump-ast analysis.adl
smash_cpp2 check --dump-hir analysis.adl
smash_cpp2 check --dump-quantities analysis.adl
smash_cpp2 check --dump-formula analysis.adl
smash_cpp2 check --dump-axioms analysis.adl

# pairwise verdicts (subprocess z3 on PATH)
smash_cpp2 verify analysis.adl
```

`run` is listed first in `--help`. The interpreter is the meaning.
`verify` uses SMT-LIB `z3 -in`; there is no libz3 link.

## Grammar edits

1. Edit `grammar.ebnf`.
2. Add or update a dump-ast comparison against smash3.
3. Change the `parse_*` named in `BISON_MAP.md`.

Language meaning is in `LANGUAGE.md`. Those items are closed.

## Dump gates (U03/U05/U06, still required)

Stdout of `smash_cpp2 check --dump-ast`, `--dump-hir`,
`--dump-quantities`, `--dump-formula`, and `--dump-axioms` must match
smash3 on the tutorial files. The first three stay 146/146 on the full
`examples/` corpus; dump-formula and dump-axioms tutorials are the
U05/U06 gates (full corpus is stretch).

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
