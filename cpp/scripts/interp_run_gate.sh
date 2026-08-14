#!/usr/bin/env bash
# P3 interpreter run gate on pinned (adl, jsonl) pairs.
# Default: C++ well-formedness. CROSS_ORACLE=1 byte-diffs smash2 stdout.
#
# Both commands must exit 0. A crash, usage error, or stdout mismatch
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

# shellcheck source=gate_common.sh
source "$ROOT/cpp/scripts/gate_common.sh"
gate_prepare

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

ok=0
fail=0
failures=()

if [[ "$GATE_ORACLE" == "1" ]]; then
  echo "==> interp run-diff ${#PAIRS[@]} pairs (full stdout; optional smash2 cross-check)"
else
  echo "==> interp run ${#PAIRS[@]} pairs (C++-only; CROSS_ORACLE=1 for smash2 diff)"
fi
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
  "$SMASH2_CPP" run "$ROOT/$adl" "$ROOT/$jsonl" >"$tmpdir/cpp.out" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  rc_rust=0
  if [[ "$GATE_ORACLE" == "1" ]]; then
    "$SMASH2_RUST" run "$ROOT/$adl" "$ROOT/$jsonl" >"$tmpdir/rust.out" 2>"$tmpdir/rust.err"
    rc_rust=$?
  fi
  set -e

  reason=""
  if [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp run exited $rc_cpp"
  elif [[ ! -s "$tmpdir/cpp.out" ]]; then
    reason="smash2_cpp run produced empty stdout"
  elif [[ "$GATE_ORACLE" == "1" ]]; then
    if [[ "$rc_rust" -ne 0 ]]; then
      reason="rust smash2 run exited $rc_rust"
    elif [[ ! -s "$tmpdir/rust.out" ]]; then
      reason="rust run produced empty stdout"
    elif ! diff -q "$tmpdir/rust.out" "$tmpdir/cpp.out" >/dev/null; then
      reason="stdout mismatch"
    fi
  fi

  if [[ -z "$reason" ]]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
    failures+=("$adl ($reason)")
    echo "FAIL $adl: $reason" >&2
    if [[ "$reason" == *mismatch ]]; then
      diff -u "$tmpdir/rust.out" "$tmpdir/cpp.out" >"$tmpdir/udiff" || true
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
if [[ "$GATE_ORACLE" == "1" ]]; then
  echo "interp run gate: PASS (full stdout byte-for-byte vs smash2; both sides exit 0)"
else
  echo "interp run gate: PASS (C++ run well-formed; CROSS_ORACLE=1 for smash2 diff)"
fi
