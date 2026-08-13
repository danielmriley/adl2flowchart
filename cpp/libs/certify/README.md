# `adl2_certify`

Small trusted certificate kernel (Rust `adl-certify`). Keep thin.

Depends on **formula only**. Analysis calls this library; do not include
`adl2/analysis/`. Search is untrusted; `Certificate::replay` is the kernel.

P5 API is locked in `include/adl2/certify/certify.hpp`. Implementation of
`certify_unsat` / `certify_bounds` / replay is the fill; bundles and
`smash2-recheck` are out of this fill.

Headers: `libs/certify/include/adl2/certify/`
