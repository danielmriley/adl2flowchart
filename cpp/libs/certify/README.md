# `adl2_certify`

Small trusted certificate kernel (Rust `adl-certify`). Keep thin.

Depends on **formula only**. Analysis calls this library; do not include
`adl2/analysis/`. Search is untrusted; `Certificate::replay` is the kernel.

P5 API is locked in `include/adl2/certify/certify.hpp`. Bundles
(`smash2-combine/2`) and SHA-256 live in `bundle.hpp` / `sha256.hpp`.
`smash2_cpp-recheck` is a separate CLI binary over this library.

Headers: `libs/certify/include/adl2/certify/`
