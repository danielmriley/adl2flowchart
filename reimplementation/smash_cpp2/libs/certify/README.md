# `adl2_certify`

Small trusted certificate kernel (smash3 `adl-certify` / smash2_cpp
port). Keep thin. Not rewritten for smash_cpp2. Uncertified
solver-UNSAT stays CANDIDATE. Schema stays `smash2-combine/2`.
Producer is `smash_cpp2`. Replay does not check producer.

Depends on **formula only**. Analysis calls this library; do not include
`adl2/analysis/`. Search is untrusted; `Certificate::replay` is the kernel.

P5 API is locked in `include/adl2/certify/certify.hpp`. Bundles
(`smash2-combine/2`) and SHA-256 live in `bundle.hpp` / `sha256.hpp`.
`smash_cpp2-recheck` is a separate CLI binary over this library.

Headers: `libs/certify/include/adl2/certify/`
