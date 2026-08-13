# `adl2_formula`

Polarity-aware Formula IR + HIR encoder (Rust `adl-formula`, SPEC_ARCHITECTURE §5).

Soundness direction is a type: only `Formula::over` / `Formula::under`
construct `Over` / `Under` wrappers around Unknown/Dual-free `QFormula`.

Headers: `libs/formula/include/adl2/formula/`

Oracle dump: `smash2_cpp check --dump-formula` vs `smash2 check --dump-formula`
(fail-closed allowlist in `cpp/tests/formula_gate_files.txt`).
