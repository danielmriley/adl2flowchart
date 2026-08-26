# `adl2_formula`

Polarity-aware Formula IR + HIR encoder. Ported from smash2_cpp
`cpp/libs/formula`. smash3 `check --dump-formula` is the dump oracle.

Soundness direction is a type: only `Formula::over` / `Formula::under`
construct `Over` / `Under` wrappers around Unknown/Dual-free `QFormula`.

Unindexed collection cuts use Dual bounded expansion, k=3. Empty
collection is true in the plus branch (`LANGUAGE.md`).

Headers: `libs/formula/include/adl2/formula/`

This library dumps encoded regions. Axiom emission lives in
`libs/axioms`. It does not call a solver or implement `verify`.

```bash
./reimplementation/smash_cpp2/build/smash_cpp2 check --dump-formula \
  examples/tutorials/ex01_selection.adl
```

Stdout must match smash3.
