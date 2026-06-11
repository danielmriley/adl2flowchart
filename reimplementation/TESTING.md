# ADL2 testing strategy

The legacy lesson: every soundness bug was found by *adversarial
hand-crafting* because there was no executable definition of region
semantics. ADL2's strategy is built around an oracle hierarchy, with the
reference interpreter at the center.

## 1. Oracle hierarchy

1. **Reference interpreter (adl-interp)** — the executable spec.
   Ground truth for "event e passes region R" within the checked
   fragment.
2. **Legacy `smash`** — transitional oracle: verdict-level comparison on
   the corpus until the parity gate (PLAN Phase 7), then retired.
3. **CutLang** (optional, env-gated) — external anchor for SPEC
   **[VERIFY]** items and for interpreter differential runs where a
   CutLang installation is available.

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
| Axiom tests | rust test | each catalog axiom: (a) holds on every generated physical event, (b) the historical counterexample for prohibited axioms stays rejected |
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
