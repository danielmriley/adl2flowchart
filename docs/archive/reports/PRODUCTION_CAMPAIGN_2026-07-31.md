# The production-trustworthiness campaign — 2026-07-30/31

Orchestrated record: four agent waves plus one CI-driven hotfix, each
verified three times (agent gate → orchestrator hands-on re-verification →
CI), landing the plan from "how do we bring verify to production
trustworthiness in the wild".

## The organizing principle

Every real soundness bug this project has ever found was a **premise
violated at a model boundary** — identity (lossy renders), arithmetic
semantics (f64 vs exact), definedness (absent properties), domain
(negative pt) — never a wrong inference. The campaign walked each premise
up the ladder: *by construction* where possible, *enforced at the
boundary* where not, *guarded fail-closed + adversarially audited*
otherwise.

## What landed (chronological, all on `main`)

| Commit | Wave | Content |
|---|---|---|
| `129ab36` | W1a | **M3 rational interpreter** (events as `Rat`; `NumVal{Exact,Approx}`; encoder and interpreter share `adl_sema::num` — constant-fold divergence impossible *by construction*) + **M4 encoder realignment** (`is_exact_valued` flattening licence) + **loader = axiom domain** (NNEG narrowed off mass; pt/e/MET.pt/tags enforced at load; battery/refute probes clamped inside E) + the adversarial **refute gate** |
| `22a4c62` | W1b | **SPEC_PRESENCE_MODEL.md** — Phase B design (Lemma E, invariant E-i, EPRES recovery), reviewed and accepted; its predicted abs-fold-bypass instance confirmed live |
| `6ce493c` | W2a | **Universal certificates**: closed-form Farkas certificates for the interval fast path (bound provenance in `Iv`; `certify_bounds` construct-then-replay), certified XSUB/XEQ derivation chains embedded in bundles, **schema `smash2-combine/2`** (quantities dictionary, per-assert sources, producer, input SHA-256, no timestamp), `smash2-recheck` re-derives chains and prints the unauthenticated-prose caveat |
| `733a731` | W2b | **Persistent z3**: one child per solver, `(reset)`+script per query (incremental push/pop rejected WITH EVIDENCE — z3's incremental core returns different models → witnesses → 14 verdicts flipped; byte-identical output was the constraint), sentinel-echo framing, sticky Bug-5 errors, restart-on-death |
| `002301f` | hotfix | **Inherit canonicalization** (`flatten_inherits`): Inherit encoded as ONE named assert → unminimizable cores → certifier 2²⁰ case-split → budget → subset silently weaker on one rendering. Found by CI's metamorphic battery post-push (fresh entropy found what two local green runs missed). CE-17 pinned; CE-7's certification-tier wobble root-caused retroactively and its test upgraded to exact equality |
| `6191333` | W3 | **Trust surface**: per-claim annotations `[certified · gate N/N · probes M · assumes: …]`, the `== trust ==` block, diagnostics triage (fail-closed notes vs INTERNAL CONTRADICTIONS), loud+reversible matrix suppression (`--matrix`, numbered legend), ledger file attribution + `--recon=related`, witness one-line summaries, loud solver-failure reporting + `--fail-on=unknown`; JSON strictly additive |
| `573f354` | W4 | **Input hardening**: parser depth cap 64 (bisected: parens overflow at 144 on 2 MiB debug stacks; corpus max depth 9; the comparison-chain desugar hole was found by pinning the parser's accounting against an independent iterative `Expr::depth`), SIGPIPE → 141, numeral cap 4096 + subquadratic `from_decimal_parts` (1.34 s → 0.01 s), `1_000` misparse now a helpful error, diagnostic output windowing, **201-case adversarial battery (~4 s)** |
| `0058a26` | merge | The **SIGPIPE × persistent-solver collision** fix (process-global `SIG_DFL` killed smash2 on writes to a dead solver child, defeating the fail-closed EPIPE contract — replaced by a broken-pipe panic hook exiting 141; `smash2-recheck`, which owns no child pipes, keeps `SIG_DFL`) + metamorphic `max_shrink_iters = 128` (a failing case is by construction certifier-expensive; the default budget turned one CI failure into an hour of shrinking) |

## Standing numbers (tip of the campaign)

- Suite: **84 test binaries / 949 tests / 0 failures**; CI green both jobs.
- Corpus: **139 files / 1 894 pairs — 794 PROVEN DISJOINT (100 % certified,
  was 28 %) / 72 PROVEN OVERLAPPING (100 % witness-validated) / 45
  candidate / 983 possibly / 0 unknown.** The tuple was byte-stable through
  every wave except the audited, explained M4 recoveries
  (+19 overlapping, all interpreter-validated).
- Performance: 13-file CMS cross matrix 539 s → **169 s**; corpus sweep
  326 s → **76 s**; z3 spawns for the 3-file merge 345 → 2.

## What the failures taught (kept deliberately)

1. **Local green is necessary, not sufficient.** CI's metamorphic run,
   with fresh entropy, found the inherit divergence two full local gates
   missed. The corpus/pins are regression nets; only property exploration
   finds new shapes.
2. **Two individually-correct designs can collide in a rare path.** The
   SIGPIPE disposition and the solver's EPIPE contract conflicted only
   when a solver child dies mid-session — exactly one test reaches that.
3. **Orchestration hygiene**: agents must never `pkill` by process name
   (one killed the orchestrator's verification run); parallel agents get
   worktree isolation and merge through a single gated queue.

## Backlog (ordered; owner-visible)

1. **Phase B: presence model** (SPEC_PRESENCE_MODEL.md) — closes the last
   known unsoundness class (absent-property positive arm, incl. the
   abs-fold bypass); restores the three complement re-pins and the
   withdrawn corpus proofs; turns proof premise P2-definedness into a
   theorem.
2. Certifier unit-propagation before case-splitting (CE-7/CE-17 family;
   heavy-tail CI cost).
3. Per-claim bundle verdicts for EMPTY/bins; the `whole: false`
   conjunct-attribution residual.
4. EventScalar totality-whitelist confirmation (EventVar/Trigger
   hard-error verification).
5. Region-count / event-line-size caps (product policy).
6. rustfmt baseline commit; then `cargo fmt --check` in CI.
7. External corpus runs (ADL_NPS, ATLAS) and publication of the proof +
   audit as a technical note — the standing invitation to break it.

Companion documents: `VERIFY_ENGINE_2026-07-31.md` (the engine as it now
stands), `SOUNDNESS_PROOF_2026-07-25.md` (premises → theorems mapping,
updated by the campaign), `AUDIT_2026-07-28_VALIDATION_ENGINE.md`,
`SOUNDNESS_TESTING_SYSTEM_2026-07-29.md`, `SPEC_PRESENCE_MODEL.md`.
