# `adl2_axioms`

Audited axiom catalog + emitters (Rust `adl-axioms`, ADR-008).

Every background fact asserted into an UNSAT proof lives in one 19-entry
catalog. Prohibited-by-history axioms stay prohibited:

- mere mention of `C[i]` implying `size(C) > i`
- substring TAG matching (`btagDeepB` is not `{0,1}`)

`EPRED` / `EPRES` are catalogued and emitted: size-guarded filter-predicate
facts (`encode_elem_pred`) and definedness facts for properties the filter
DECIDES. XSUB/XEQ remain catalog-only (Rust emits those from analysis, not
`emit_round`).

**CombSize** catalog and emitter match: projection equality, `size(K) >= 0`,
same-source disjoint `size(C) < 2 => size(K) = 0`, cross-source/cartesian
empty-factor `size(part)=0 => size(K)=0`, and the cuts-free cartesian
`all parts nonempty => size(K) >= 1`. The same-source positive lower bound
is deliberately omitted (value-distinctness).

There is a fail-closed `smash2_cpp check --dump-axioms` oracle gate vs
Rust `smash2 check --dump-axioms` (allowlist in
`cpp/tests/axioms_gate_files.txt`).

Headers: `libs/axioms/include/adl2/axioms/`
