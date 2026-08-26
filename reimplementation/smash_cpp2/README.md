# smash_cpp2

C++ production ADL toolchain. Run analyses over events first. Prove
region relations after the interpreter agrees with the file.

`smash_cpp2` is the product name. Libraries keep the `adl2_*` CMake
names. Language decisions are the smash3 closed contract
(`LANGUAGE.md`). The parser is the smash2_cpp `parse_*` recursive
descent, not Flex or Bison. smash3 is the dump-ast / verify oracle.

U01 of this tree implements `check --dump-ast` and a run-first CLI.
Later units add interpreter, verify, ingest, and certify.

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
# 1. parse (U01: --dump-ast is the oracle-facing dump)
smash_cpp2 check analysis.adl
smash_cpp2 check --dump-ast analysis.adl

# later units
smash_cpp2 run analysis.adl events.jsonl
smash_cpp2 verify analysis.adl
```

`run` is listed first in `--help`. `run` is the meaning once the
interpreter unit lands. `verify` is property-tested against it.

## Grammar edits

1. Edit `grammar.ebnf`.
2. Add or update a dump-ast comparison against smash3.
3. Change the `parse_*` named in `BISON_MAP.md`.

Language meaning is in `LANGUAGE.md`. Those items are closed.

## Dump-ast gate (U01)

Stdout of `smash_cpp2 check --dump-ast` must match smash3 on every
`examples/tutorials/*.adl` file. The same dump also matches smash3 on
the full 146-file `examples/` corpus.

```bash
smash3=./reimplementation/smash3/target/release/smash3
cpp2=./reimplementation/smash_cpp2/build/smash_cpp2
for f in examples/tutorials/*.adl; do
  diff -u <("$smash3" check --dump-ast "$f") <("$cpp2" check --dump-ast "$f")
done
# full corpus
smash3=$smash3 cpp2=$cpp2 reimplementation/smash_cpp2/scripts/dump_ast_corpus.sh
```
