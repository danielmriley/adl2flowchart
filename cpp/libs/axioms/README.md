# `adl2_axioms`

Audited axiom catalog + emitters (Rust `adl-axioms`, ADR-008).

Every background fact asserted into an UNSAT proof lives in one 19-entry
catalog. Prohibited-by-history axioms stay prohibited:

- mere mention of `C[i]` implying `size(C) > i`
- substring TAG matching (`btagDeepB` is not `{0,1}`)

`EPRED` / `EPRES` are catalogued; their emitters are not yet fully ported
(vacuity/size facts still emit). XSUB/XEQ remain catalog-only (Rust emits
those from analysis, not `emit_round`).

Headers: `libs/axioms/include/adl2/axioms/`
