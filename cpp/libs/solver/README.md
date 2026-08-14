# adl2_solver — SMT-LIB2 subprocess backend (Rust `adl-solver`)

P4 fills the **subprocess** backend only (`z3 -in` on PATH). There is no
native libz3 link.

SAT/model `check` is `(reset)` + the whole script + `(check-sat)` — z3's
incremental core returns different models. UNSAT `check_unsat` holds
decls/axioms and sends frame deltas (`push` / assert / `check-sat` / `pop`).
`classify` is the single Bug-5 mapping: `(error …)` / `unsupported`
/ `unknown` / `timeout` are `SatResult::Unknown`, never Unsat.

Headers: `libs/solver/include/adl2/solver/`
