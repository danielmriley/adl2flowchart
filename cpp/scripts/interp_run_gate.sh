#!/usr/bin/env bash
# P3 interpreter run gate: compare smash2_cpp `run` vs smash2 `run`
# on pinned (adl, jsonl) pairs. Only stdout lines starting with `event `
# are compared (cutflow / histo tables are deferred).
#
# Both commands must exit 0. A crash, usage error, or event-line mismatch
# fails the gate.
#
# Usage (from repo root):
#   cpp/scripts/interp_run_gate.sh
#   COUNT_ONLY=1 cpp/scripts/interp_run_gate.sh
set -euo pipefail

EXPECTED_PAIRS=3

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

LIST="$ROOT/cpp/tests/interp_gate_pairs.txt"
SMASH2_CPP="${SMASH2_CPP:-$ROOT/cpp/build/smash2_cpp}"
SMASH2_RUST="${SMASH2_RUST:-$ROOT/reimplementation/adl2/target/release/smash2}"

test -f "$LIST" || { echo "missing pair list $LIST" >&2; exit 2; }

mapfile -t PAIRS < <(grep -v '^#' "$LIST" | grep -v '^[[:space:]]*$' | sed 's/[[:space:]]*$//')
if [[ "${#PAIRS[@]}" -ne "$EXPECTED_PAIRS" ]]; then
  echo "error: expected $EXPECTED_PAIRS interp pairs, found ${#PAIRS[@]}" >&2
  echo "update EXPECTED_PAIRS in $0 if the claimed set intentionally changed" >&2
  exit 1
fi

if [[ "${COUNT_ONLY:-0}" == "1" ]]; then
  echo "interp pair count: ${#PAIRS[@]} (pinned $EXPECTED_PAIRS)"
  exit 0
fi

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  if [[ -z "${CXX:-}" ]] && command -v g++ >/dev/null 2>&1; then
    export CXX=g++
  fi
  echo "==> building smash2_cpp"
  cmake -S cpp -B cpp/build -DCMAKE_BUILD_TYPE=Release
  cmake --build cpp/build -j"$(nproc)"

  echo "==> building Rust smash2 (forever oracle; no native z3)"
  (
    cd reimplementation/adl2
    cargo build --release -p adl-cli --no-default-features
  )
fi

test -x "$SMASH2_CPP" || { echo "missing smash2_cpp at $SMASH2_CPP" >&2; exit 2; }
test -x "$SMASH2_RUST" || { echo "missing smash2 at $SMASH2_RUST" >&2; exit 2; }

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

ok=0
fail=0
failures=()

echo "==> interp run-diff ${#PAIRS[@]} pairs (event lines only; Rust oracle vs smash2_cpp)"
for pair in "${PAIRS[@]}"; do
  adl="${pair%% *}"
  jsonl="${pair#* }"
  adl="${adl%"${adl##*[![:space:]]}"}"
  jsonl="${jsonl#"${jsonl%%[![:space:]]*}"}"
  if [[ ! -f "$ROOT/$adl" ]]; then
    fail=$((fail + 1))
    failures+=("$adl (missing adl)")
    echo "FAIL $adl: missing file" >&2
    continue
  fi
  if [[ ! -f "$ROOT/$jsonl" ]]; then
    fail=$((fail + 1))
    failures+=("$jsonl (missing jsonl)")
    echo "FAIL $jsonl: missing file" >&2
    continue
  fi
  set +e
  "$SMASH2_RUST" run "$ROOT/$adl" "$ROOT/$jsonl" >"$tmpdir/rust.out" 2>"$tmpdir/rust.err"
  rc_rust=$?
  "$SMASH2_CPP" run "$ROOT/$adl" "$ROOT/$jsonl" >"$tmpdir/cpp.out" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  set -e
  grep '^event ' "$tmpdir/rust.out" >"$tmpdir/rust.ev" || true
  grep '^event ' "$tmpdir/cpp.out" >"$tmpdir/cpp.ev" || true

  reason=""
  if [[ "$rc_rust" -ne 0 ]]; then
    reason="rust smash2 run exited $rc_rust"
  elif [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp run exited $rc_cpp"
  elif [[ ! -s "$tmpdir/rust.ev" ]]; then
    reason="rust run produced no event lines"
  elif ! diff -q "$tmpdir/rust.ev" "$tmpdir/cpp.ev" >/dev/null; then
    reason="event-line mismatch"
  fi

  if [[ -z "$reason" ]]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
    failures+=("$adl ($reason)")
    echo "FAIL $adl: $reason" >&2
    if [[ "$reason" == *mismatch ]]; then
      diff -u "$tmpdir/rust.ev" "$tmpdir/cpp.ev" >"$tmpdir/udiff" || true
      head -80 "$tmpdir/udiff" >&2 || true
    else
      echo "  rust stderr:" >&2
      head -20 "$tmpdir/rust.err" >&2 || true
      echo "  cpp stderr:" >&2
      head -20 "$tmpdir/cpp.err" >&2 || true
    fi
  fi
done

total=$((ok + fail))
echo "interp run gate: OK=$ok FAIL=$fail TOTAL=$total (pairs ${#PAIRS[@]})"
if [[ "$fail" -ne 0 ]]; then
  echo "failed pairs:" >&2
  printf '  %s\n' "${failures[@]}" >&2
  exit 1
fi
if [[ "$ok" -ne "$EXPECTED_PAIRS" ]]; then
  echo "error: expected $EXPECTED_PAIRS passing pairs, got ok=$ok" >&2
  exit 1
fi
echo "interp run gate: PASS (event lines byte-for-byte vs Rust smash2; both sides exit 0)"
