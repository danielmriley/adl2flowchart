# ADL2 testing strategy

The legacy lesson: every soundness bug was found by *adversarial
hand-crafting* because there was no executable definition of region
semantics. ADL2's strategy is built around an oracle hierarchy, with the
reference interpreter at the center.

## 1. Oracle hierarchy

1. **Reference interpreter (adl-interp)** — the executable spec and sole
   reference. Ground truth for "event e passes region R" within the
   checked fragment. Spec ambiguities are resolved by project decision
   (Daniel + collaborators) recorded in the spec, not by probing an
   external tool.
2. **Legacy `smash`** — transitional oracle: verdict-level comparison on
   the corpus until the parity gate (PLAN Phase 7), then retired.

## 2. Test layers (all in CI from the phase that creates them)

| Layer | Tooling | What it locks |
|---|---|---|
| Lexer/parser unit tests | rust test | token/grammar rules incl. every SPEC divergence |
| AST snapshots | insta | canonical tree dumps for ~15 corpus files + all goldens; any grammar change shows as a reviewed diff |
| Parser fuzzing | cargo-fuzz | no panics/hangs on arbitrary input |
| Corpus gate | script | all 68 `examples/**.adl` parse with zero errors; mixed and/or precedence lint report empty or acknowledged |
| Interpreter unit tests | rust test | §4 semantics: ordering, union, ternary, bands, div-by-zero, bins |
| Golden verdicts | script (ported) | the ~40 legacy golden checks, including the dual-encoding regression suite (reject-OR band, OR-with-unencodable-branch, not-tag, define-under-OR, tag indices) and the audit regressions (empty-∀, define-arith, angular order, union size, non-finite constants, btag discriminant) — every one of these was once a live false verdict; they are the project's immune system |
| **Property-based: encoder vs interpreter** | proptest | generate random regions over a fixed quantity vocabulary; sample events on a grid + random; assert: PROVEN DISJOINT ⇒ no sampled event in both; PROVEN OVERLAPPING witness ⇒ interpreter accepts the witness event in both regions; PROVEN SUBSET ⇒ no sampled counterexample; REGION EMPTY ⇒ no sampled member |
| Metamorphic tests | proptest | verdict invariances: swap(A,B) symmetry; `reject c` ≡ `select not c`; double negation; inlined vs named define; inherited vs textually-pasted region; renamed pure-alias object — all must give identical verdicts |
| Axiom tests | rust test | each `emit_axioms` catalog axiom: (a) holds on every generated physical event, (b) the historical counterexample for prohibited axioms stays rejected. **Exception:** XSUB/XEQ are derived by the analysis engine's reconciliation pass (never by `emit_axioms`), so they are outside this battery; their nets are the targeted `cross_file.rs` reconciliation tests, the scripted-solver classification tests in the engine, `examples/golden/cross/` pins, and the dedicated reconcile oracle below |
| **Property-based: reconcile oracle** | proptest | random tight/loose same-base filter chains + size-cut regions analyzed with `reconcile: true` (candidates arise intra-unit, so the single-unit oracle applies); every verdict — including those resting on a derived XSUB/XEQ size fact — must hold on the sampled events (256 cases; 4096 under `deep`) |
| Solver backend conformance | rust test | native and subprocess backends agree on a fixed query battery (sat/unsat/model/core/timeout behavior) |
| Determinism | script | two runs ⇒ byte-identical report and JSON |
| Differential vs legacy | script | Phase 7 parity gate: verdict matrix diff over corpus; every difference classified (ADL2-better / legacy-better / spec change) and signed off |

## 3. Witness validation (closing the loop)

Every SAT-direction proof is *re-validated through the interpreter*: the
model is converted to a synthetic event (free/opaque quantities get the
model's values) and both regions must accept it. This runs in production,
not just tests — a failed validation downgrades the verdict to POSSIBLY
and files an internal-error diagnostic. The verifier can then never
display a witness the interpreter rejects (the strongest practical form
of the soundness contract).

## 4. CI

GitHub Actions: build (stable Rust, clippy -D warnings, rustfmt), unit +
snapshot + golden + corpus + determinism on every PR; fuzz smoke (60 s)
and full proptest battery nightly; z3 installed in the image; subprocess
backend job runs with the native feature disabled. The legacy `make test`
continues to run until the parity gate retires it.
