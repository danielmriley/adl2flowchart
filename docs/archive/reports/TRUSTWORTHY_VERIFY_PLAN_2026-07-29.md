# Trustworthy `smash2 verify` — status pointer (2026-07-29)

Canonical plan lives in the Cursor plan file
`Trustworthy verify path` (do not treat this note as a duplicate of that
plan). This file is a short status / navigation pointer for the archive.

## Goal (one line)

PROVEN OVERLAPPING only with interpreter-accepted event evidence; PROVEN
DISJOINT / EMPTY / SUBSET only when the encoded query is independently
certified **and** live refute/sampling gates do not find a counterexample —
never a silent false PROVEN.

## Milestone status

| Milestone | Scope | Status |
|---|---|---|
| **M1** | In-verify adversarial refute gate (default on; `--no-refute-gate`) | **done** |
| **M2** | Certify EMPTY / SUBSET / bin UNSAT; CANDIDATE when uncertified | **done** |
| **M3** | Rational `Event` model + Rat eval / witness path | **done** |
| **M4** | Encoder fold realignment + corpus verify gate + trust-surface docs + `fold_vs_f64_semantics` | **done** |
| **M5** | CI locks the same refute/cert paths verify uses | deferred / follow-on |

## Product verification (parent, 2026-07-29)

- `cms.adl` with defaults (certify + refute): ~4s (was hanging >120s when
  subset/bin cert dumped the full axiom frame into Farkas search; now
  core-scoped only, fail-closed when no core).
- CE-8…14 / C1–C6 / I-ORACLE-ARMED: no PROVEN DISJOINT (`f64_fold_regressions`,
  `refute_gate`). **Superseded by the M4 landing (2026-07-30):** with the
  interpreter exact those pairs are genuine partitions, so the contract there
  is now PROVEN DISJOINT *plus* the historic witness no longer being a member
  of both regions.
- Metamorphic + `prop_encoder_vs_interp`: green.
- Corpus gate: **PASS** — pairs=1893, proven_disjoint=794 (= baseline),
  unknown=0, 138/138 exit 0. After the M4 landing: pairs=1894,
  proven_disjoint=**794** (unchanged), proven_overlapping 53→72, unknown=0,
  139/139 exit 0.

## M4 landing (2026-07-30)

The encoder was still folding constants by f64 emulation after M3 moved the
interpreter to exact rationals — a live false PROVEN DISJOINT (CE-15). M4
gave both sides one value model (`adl_sema::num`), replaced the exact-f64
fold gate with a faithfulness predicate, and moved approximate comparison
thresholds to `fl(k)`. Separately, the loader and the `NNEG`/`TAG` axioms
now agree on which events exist (CE-16) — they did not, and the widened
battery caught the engine refuting its own true REGION EMPTY claim.

## Deliverables

- `adl-analysis/src/refute.rs` + engine/CLI `--no-refute-gate` + JSON `refute`
- Uniform `certify_*` for empty / subset / bins; SCHEMA_VERSION 4
- `Event` / JSONL / eval / witness on `Rat` for the rational fragment
- `scripts/verify_corpus_gate.sh` + `baselines/corpus_verify.json`
- `crates/adl-formula/tests/fold_vs_f64_semantics.rs`
- README trust table: honest PROVEN meanings (no “guarantees no false PROVEN”)

## Related

- Soundness testing architecture:
  [`SOUNDNESS_TESTING_SYSTEM_2026-07-29.md`](./SOUNDNESS_TESTING_SYSTEM_2026-07-29.md)
- Counterexample lock file: `docs/archive/adl2/COUNTEREXAMPLES.md`
- Product README trust surface: `reimplementation/README.md`
