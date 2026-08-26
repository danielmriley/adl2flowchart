# `adl2_axioms`

Audited axiom catalog + emitters. Ported from smash2_cpp
`cpp/libs/axioms`. smash3 `check --dump-axioms` is the dump oracle.

Every background fact asserted into an UNSAT proof lives in one 19-entry
catalog. Prohibited-by-history axioms stay prohibited:

- mere mention of `C[i]` implying `size(C) > i`
- substring TAG matching (`btagDeepB` is not `{0,1}`)

`dPhi` / `dEta` are oriented (`LANGUAGE.md`). DPHI is the −π…π range
fact on `dPhi` (bound widened one ulp). TWIN is `x = y ∨ x = −y` for
reversed-argument oriented pairs. SAT-direction caps at POSSIBLY when
reversed twins appear; that cap is later-unit verify, not this dump.

`EPRED` / `EPRES` are catalogued and emitted. XSUB/XEQ remain
catalog-only (`emit_round` does not produce them). This unit dumps
emitted instances. It does not call a solver or implement `verify`.

Headers: `libs/axioms/include/adl2/axioms/`

```bash
./reimplementation/smash_cpp2/build/smash_cpp2 check --dump-axioms \
  examples/tutorials/ex01_selection.adl
```

Stdout must match smash3.
