---
name: adl2-build-test
description: Builds and tests the ADL2 Rust workspace at reimplementation/adl2 (the `smash2` CLI / `adl-cli` crate) on a machine that has NO system libz3, which makes a plain `cargo build --release` fail at link with "unable to find library -lz3". Use this WHENEVER building, testing, running, or smoke-testing anything under reimplementation/adl2 — including running smash2, cargo test, the corpus gate, or the difftest battery — even if the user just says "build it", "run the tests", or "check it compiles" and doesn't mention z3 or linking. Use it the moment you hit "-lz3", "z3-sys", a linker error in this workspace, or need to choose between the native vs subprocess solver backend.
allowed-tools: Read Edit Write Bash Grep Glob
---

Workspace root: `reimplementation/adl2`
Binary: `smash2` (crate `adl-cli`). Subcommands: `check`, `verify`, `run`, `dot`, `objects`, `ingest`.
Toolchain on this machine: cargo/rustc 1.93 (edition 2024, `rust-version = "1.93"`).

Run all `cargo` commands from the workspace root:

```bash
cd reimplementation/adl2
```

## Why this skill exists (the libz3 situation)

The PRIMARY solver backend is **native libz3** (default feature `native` → `z3` 0.20 / `z3-sys` 0.11), which links the system shared lib with `-lz3`. **This machine has no `libz3-dev` / no `libz3.so` on the loader path.** So a plain build links-fails:

```
rust-lld: error: unable to find library -lz3
```

The SECONDARY backend (SMT-LIB2 subprocess) is always compiled and needs no linking; it shells out to a `z3`/`cvc5` binary on PATH. If a `z3` binary is on PATH (any recent version — the subprocess speaks plain SMT-LIB2), this backend works with no linking at all. With native off AND no solver binary reachable, verdicts degrade to `POSSIBLY`.

Pick a workaround below by what you need.

## Workaround A — build/run the CLI (no native z3, subprocess backend)

Build the SINGLE package with native off:

```bash
cargo build --release -p adl-cli --no-default-features
```

The binary lands at `target/release/smash2`.

CRITICAL: `--no-default-features` at the **workspace** level does NOT help. Feature unification turns `native` back on through another member — `adl-difftest` depends on `adl-analysis` with default features, so `cargo build --no-default-features` (whole workspace) still tries to link `-lz3` and fails. You MUST scope to `-p adl-cli`.

## Workaround B — full native test battery (~549 tests, incl. difftest oracle)

The differential property oracle in `adl-difftest` (encoder-vs-interpreter) REQUIRES the native backend, and `adl-analysis` dev-deps pull `adl-difftest`, so the native path can't be skipped for the whole workspace. Borrow a `libz3.so` already built by another project on this machine and point the linker at it. The z3 C ABI is stable, so `z3-sys` 0.11 links fine against a newer libz3 (4.16):

```bash
mkdir -p /tmp/z3lib
cand=$(ls -t "$HOME"/Projects/*/target/*/build/z3-sys-*/out/z3-*/bin/libz3.so 2>/dev/null | head -1)
echo "borrowing: $cand"
ln -sf "$cand" /tmp/z3lib/libz3.so
RUSTFLAGS="-L native=/tmp/z3lib" LD_LIBRARY_PATH=/tmp/z3lib cargo test --release --workspace
```

`$cand` is whatever `z3-sys-*/out/.../libz3.so` the `ls` finds (any project that builds `z3-sys` with the `static-link-z3` feature produces one — z3's C ABI is stable, so a newer libz3 links fine against `z3-sys` 0.11). If it's empty, widen the search root, build z3 in such a project, or install system `libz3-dev` so the plain native build links directly.

`LD_LIBRARY_PATH` is needed at RUN time too (tests dlopen the lib), not just for linking — keep both env vars on the same command.

## Workaround C — test the crates that don't need z3

Crates with no solver dependency test directly:

```bash
cargo test -p adl-syntax
cargo test -p adl-sema      # exact-rational core: src/rat.rs (Rat)
cargo test -p adl-formula
cargo test -p adl-interp
cargo test -p adl-axioms --lib   # see note below: integration test pulls native z3
```

CAUTION: `cargo test -p adl-axioms` (without `--lib`) link-FAILS with `-lz3`.
adl-axioms dev-depends on `adl-difftest` (for the `tests/axioms_hold.rs`
property test), and `adl-difftest → adl-analysis` defaults to `native`, so the
integration test target tries to link z3. `--lib` runs only the in-crate unit
tests and links clean; run the full `axioms_hold` battery via Workaround B.
Also run each pure crate as its OWN `cargo test -p X` — combining several in one
`-p a -p b ...` invocation re-unifies features and drags adl-axioms' native test
in, re-triggering the `-lz3` failure.

`adl-sema` now owns the analyzer's exact numeric core: `adl_sema::Rat` (`crates/adl-sema/src/rat.rs`, a newtype over `num_rational::BigRational` with decimal-literal semantics — `0.3` is `3/10` exactly). `adl-formula`, `adl-axioms`, `adl-analysis/interval.rs`, and `adl-solver` all carry their atom/coefficient/bound numerics as `Rat` instead of f64, so `cargo test -p adl-sema` is a fast guard on the arithmetic that PROVEN verdicts rest on. This added three workspace deps in the root `Cargo.toml` `[workspace.dependencies]`: `num-rational`, `num-bigint`, `num-traits` — a clean `cargo build` will fetch them on first run.

The solver crate over its subprocess backend (no native link):

```bash
cargo test -p adl-solver --no-default-features
```

`adl-analysis` and `adl-difftest` cannot be tested this way — `adl-difftest` pulls the native backend, so use Workaround B for those two.

## Smoke-test the binary

```bash
# build (Workaround A) first, then:
BIN=target/release/smash2

# parse + resolve a corpus file (clean run is silent, exit 0)
$BIN check examples/*.adl

# full analysis on one file; --verbose puts backend + timing on stderr
$BIN verify --verbose examples/<file>.adl

# force the no-solver fast path (verdicts capped at POSSIBLY)
$BIN verify --no-solver examples/<file>.adl
```

Output discipline: stdout is machine-clean; diagnostics/progress go to stderr; `--verbose` adds the active solver backend and timing to stderr. To see which backend is live, run `verify --verbose` and read stderr. To use the native backend at runtime, export `LD_LIBRARY_PATH=/tmp/z3lib` (Workaround B) before running; otherwise it falls back to the `z3` binary on PATH, then to the no-solver degradation.

## Corpus gate (parse + resolve over all examples)

```bash
scripts/corpus_gate.sh
```

This builds `smash2` (debug, `-p adl-cli`) and runs `smash2 check` over every `.adl` under `examples` (expects 68 files). It exits 1 if any file has error-severity diagnostics, naming failures on stderr.

NOTE: `corpus_gate.sh` calls `cargo build ... -p adl-cli` WITHOUT `--no-default-features`, so on this libz3-less machine it will link-fail at the build step. To run the gate as-is, either build native via Workaround B env first, or run the check manually with the subprocess build:

```bash
cargo build --release -p adl-cli --no-default-features
target/release/smash2 check $(find examples -name '*.adl' | sort)
```

## Quick decision guide

- "Just build / run smash2" → Workaround A.
- "Run the full test suite / difftest / adl-analysis tests" → Workaround B.
- "Test only the pure crates fast" → Workaround C.
- Hit `-lz3` link error → you ran the wrong scope; use `-p adl-cli --no-default-features` (build) or Workaround B (native tests).
