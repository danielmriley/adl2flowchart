# BUILD_REPORT — ADL2 workspace, final integration pass

Date: 2026-06-11. Scope: the whole `reimplementation/adl2` cargo
workspace (Phases 1–6 built; Phase 7 parity material drafted in
`../PARITY_DRAFT.md`; Phase 8 cross-file not started, by plan).

## 1. What exists

Ten crates, exactly the SPEC_ARCHITECTURE §1 layout (~21k lines of Rust;
binary name `smash2`):

| Crate | Contents | Status |
|---|---|---|
| **adl-syntax** | hand-written lexer (underscore-split rule, NEWLINE for greedy productions, no signed literals), recursive-descent parser (one fn per EBNF nonterminal), spanned AST, multi-error diagnostics with resynchronization, canonical `--dump-ast` | complete |
| **adl-sema** | case-insensitive resolution, interned `Collection`/`Quantity` identity model, pure-alias unification, define inlining w/ cycle errors, fragment tagging, HIR, ext_objs/ext_lib/property_vars ingestion | complete |
| **adl-interp** | reference interpreter (the executable spec, SPEC_LANGUAGE §4 exactly), JSONL event model with `float_roundtrip` exact loads | complete |
| **adl-formula** | `Formula` IR, NNF `not()` with Dual swap, type-enforced `Over`/`Under` polarity, `LinAtom` non-finite rejection, HIR→Formula encoder incl. element-existence guards and the OPEN-1 Dual k=3 expansion | complete |
| **adl-axioms** | audited 10-row catalog (ORD SZ0 SUB UNI NNEG DPHI TAG TWIN EPRED IDOM), ground instantiation to fixpoint, twin-pair detection, prohibited-axiom regressions | complete |
| **adl-solver** | `Solver` trait; z3-native primary (feature `native`, default), SMT-LIB subprocess secondary (`(error)` ⇒ Unknown), exact rational constants both ways, shared conformance battery | complete |
| **adl-analysis** | statement-granularity encoding, interval fast path, incremental pairwise engine (empty/disjoint/subset/overlap), OPEN-2 twin cap, witness search w/ layered model refinement + interpreter re-validation (production), bin partition checks, human report + versioned JSON (`schema_version: 1`), `--fail-on` | complete |
| **adl-viz** | deterministic DOT flowchart + AST from resolved HIR (never raw AST) | complete |
| **adl-difftest** | toy event generator, random case generator (`casegen`), sampling oracle, property + metamorphic + regression batteries, `deep` feature | complete |
| **adl-cli** | `smash2 check / verify / run / dot`; machine-clean stdout, diagnostics on stderr; feature forwarding for subprocess-only builds | complete |

Supporting: `scripts/corpus_gate.sh` (68-file parse+resolve gate),
`BUILD_NOTES.md` (dated build log + deviations), `COUNTEREXAMPLES.md`
(CE-1…CE-6, mechanically-found engine bugs, all fixed + locked).

## 2. Test inventory (counts from this pass)

`cargo test --workspace` — **356 passed / 0 failed / 0 ignored**:

| Crate | Suites | Tests |
|---|---|---|
| adl-syntax | lib 1, lexer 26, parser 42, error_quality 7, corpus_gate 1, snapshots 37 | 114 |
| adl-sema | lib 5, identity 15, snapshots 4 | 24 |
| adl-interp | lib 1, spec4_semantics 50, doctests 3 | 54 |
| adl-formula | lib 9, encoder 30, laws 7, doctest 1 (+3 compile_fail) | 47 |
| adl-axioms | lib 3, axioms_hold 6 | 9 |
| adl-solver | lib 5, conformance 5 | 10 |
| adl-analysis | lib 6, golden_battery 37, analysis_behaviors 3 | 46 |
| adl-viz | lib 7, dot_snapshots 5 | 12 |
| adl-difftest | lib 3, formula_interp_smoke 1, generator 5, metamorphic 6, prop_encoder_vs_interp 1, regressions 6 | 22 |
| adl-cli | main 1, cli 17 | 18 |
| **Total** | | **356** |

Alternate configurations, all green this pass:

- `cargo test --workspace --all-features` — same suites with
  `adl-difftest/deep` enabled: property battery at 100 000 cases,
  metamorphic battery at 6 × 10 000 cases. **356 passed / 0 failed**
  (result recorded in §3; see the soak line).
- `cargo test -p adl-solver --no-default-features` — 8 tests, subprocess
  backend only.
- `cargo test -p adl-analysis --no-default-features --test golden_battery`
  — 37 tests through the subprocess backend end-to-end.
- `cargo build -p adl-cli --no-default-features` — subprocess-only binary.

Hygiene: `cargo clippy --workspace --all-targets -- -D warnings` clean;
`cargo fmt --check` clean.

## 3. Golden / corpus / property / determinism status

- **Golden battery** (ported legacy suite, 30 script checks as 37
  assertions): green, native and subprocess backends. Covers the
  dual-encoding regression suite, the June-2026 audit suite, error
  reporting, JSON schema, determinism, no-solver POSSIBLY cap, fail-on.
- **Corpus gate**: `scripts/corpus_gate.sh` → all **68/68**
  `examples/**/*.adl` parse + resolve with zero error-severity
  diagnostics (only notes/warnings).
- **Property battery** (TESTING §2 encoder-vs-interpreter): 2 000 cases
  per default run; deep run at 100 000 cases green this pass. Asserts the
  four soundness properties (DISJOINT ⇒ no sampled event in both;
  OVERLAPPING ⇒ interpreter-validated witness; SUBSET ⇒ no sampled
  counterexample; EMPTY ⇒ no sampled member).
- **Metamorphic battery**: six transforms (swap, reject≡select-not,
  double negation, inline-vs-named define, inherit-vs-paste, pure
  rename) × 250 default / 10 000 deep — identical verdict summaries AND
  identical interpreter membership per event. Green.
- **Regressions**: CE-1…CE-6 locked (false PROVEN DISJOINT/EMPTY/SUBSET
  from unguarded negation; lossy float load; truncated model values;
  model-dependent witness verdicts; witness realizer gaps).
- **Determinism**: two `smash2 verify` runs on
  `examples/CMS/CMS-SUS-16-033_Delphes.adl` byte-identical for BOTH the
  human report and `--json` (verified this pass; also locked as tests in
  adl-cli and adl-analysis).
- **Parity vs legacy** (`../PARITY_DRAFT.md`): 25/25 goldens compared;
  21 verdict-identical, 4 differences — 3 classified adl2-better
  (independent_jet_index, tag_index: interpreter-validated PROVEN
  OVERLAPPING replaces legacy's no-shared-dimension POSSIBLY cap;
  inf_constant: §4.4-exact constant-false ⇒ PROVEN DISJOINT replaces
  legacy's dropped-cut POSSIBLY) and 1 spec-change (collection_quant
  allhard/unbounded: TESTING §3 witness re-validation downgrades to
  POSSIBLY while OPEN-1 is unresolved). 0 legacy-better. Performance:
  0.36 s vs legacy 0.88 s on SUS-16-033 Delphes (≤ 2× criterion met).
- Soak (from the hardening pass, same engine): 3 × 2 000 + 1 × 10 000
  property cases; 15 × 400 metamorphic rounds (~36k comparisons) — all
  green post-fix.

## 4. Deviations from spec (full dated log in BUILD_NOTES.md)

1. **Element-existence guards** on exact comparison leaves
   (`size(C) > i` conjoined per indexed quantity) — *extension* of the
   SPEC_ANALYSIS §1 "comparison → LinAtom" row, required for soundness
   under NNF negation (CE-1/2/3 were live false PROVEN verdicts without
   it). Spec should adopt the guard conjunction at next revision.
2. **Grammar extensions over the spec EBNF** (each corpus-required,
   each a real AST node): reject-in-object, take binders/alias,
   counts commas, bare histo edge lists, open-ended slices, trailing
   underscore (`JET_`), particle-list define bodies, `type` region tag,
   concrete path-token rule.
3. **PHASE0 resolutions implemented as specified** (not deviations, but
   build-time defaults to revisit when CutLang is probed): OPEN-1 Dual
   k=3 with empty case in plus; OPEN-2 oriented + twin axiom + SAT cap;
   OPEN-3 `[-n]` reserved/Unsupported; OPEN-4 `~=` → `!=` with warning;
   OPEN-5 size aliases; case-insensitive resolution.
4. **Tag properties keep exact-name identity** (legacy's
   `ctag → isBTag` over-merge rejected — audit-Bug-6 family).
5. **§4.4 non-finite split**: computed constant non-finite ⇒ `False`
   (exact); non-finite *literal* ⇒ cannot construct an atom ⇒ `Unknown`.
6. **`encode_region` takes `&mut Hir`** (OPEN-1 expansion interns
   quantities). Opaque-undeclared externals re-tagged exact-over-free
   verifier-side only (§2 model caveat; legacy golden requires it).
7. Two adl-cli verify snapshots regenerated after the guard fix
   (leaf counts + `size(jets)` shared dimension — reviewed diff).

## 5. Known gaps (honest list)

- **OPEN-1…OPEN-5 unprobed against CutLang** — all running on PHASE0
  fallback strategies; collection_quant allhard/unbounded stays POSSIBLY
  until OPEN-1 resolves (see PARITY_DRAFT diff #4).
- **Witness search is heuristic in general**: overlaps realizable only
  at non-dyadic equality vertices that all 6 retry models miss downgrade
  to POSSIBLY (sound; internal diagnostic filed; empirical residual
  < 1 per ~36k comparisons).
- **Witness realizer is all-pass per base collection**: models needing a
  partially-failing parent collection downgrade to POSSIBLY (sound,
  tested).
- **Interval fast path** tracks single-quantity atoms only; 2-term
  differences fall through to z3.
- **Region-empty explanation via the interval path prints an empty core
  list** ("— UNSAT: " with no items) on define_arith/inf_constant —
  verdict and warning correct, explanation text incomplete (cosmetic).
- **Subset explanations carry no unsat core** (flag + human line only).
- **Bin sets** report per boundary-list statement; cross-set
  interactions are independent checks.
- **Coverage counter counts formula leaves** (incl. guard atoms), not
  source-level comparisons — revisit at the Phase-7 report review.
- **cargo-fuzz target not in-tree** (SPEC_ARCHITECTURE §3 named it;
  corpus gate + error-recovery battery currently stand in). Add before
  Phase-7 sign-off.
- **MissingProperty soft-failures** on user data lacking declared
  properties sit outside the guard scheme (event model §4.1 declares
  objects carry their properties; samplers and realizer comply).
- **Phase 7 sign-off and Phase 8 cross-file** not done: PARITY_DRAFT.md
  awaits Daniel's classification sign-off; `PARITY.md`, legacy
  deprecation, and the `CrossLink` pass are future work.

## 6. Exact commands

```bash
cd /home/daniel/Projects/adl2flowchart/reimplementation/adl2

# Build
cargo build --workspace                 # dev
cargo build --release -p adl-cli        # optimized smash2
cargo build -p adl-cli --no-default-features   # subprocess-only solver

# Tests
cargo test --workspace                  # full suite (356 tests)
cargo test --workspace --all-features   # deep: 100k property / 10k metamorphic cases (~40 min)
PROPTEST_CASES=500 cargo test -p adl-difftest   # tune battery size
cargo test -p adl-solver --no-default-features
cargo test -p adl-analysis --no-default-features --test golden_battery
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check

# Gates
scripts/corpus_gate.sh                  # 68/68 examples parse+resolve

# Run the tool (binary: target/debug/smash2 after build)
target/debug/smash2 check FILE.adl...
target/debug/smash2 verify FILE.adl [--json] [--no-solver] [--fail-on=overlap,gap,empty,non-exact]
target/debug/smash2 run FILE.adl EVENTS.jsonl [--json]
target/debug/smash2 dot FILE.adl [--ast] | dot -Tpdf -o out.pdf

# Legacy comparison (do not modify legacy_parser/)
cd ../../legacy_parser && ./smash -r tests/golden/<name>.adl
```
