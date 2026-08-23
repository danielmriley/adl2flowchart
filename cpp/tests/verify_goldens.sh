#!/usr/bin/env bash
# Pin smash2_cpp verify on a handful of goldens. Interval-only files run
# without z3. SAT-side overlap pins run only when z3 is on PATH.
# CROSS_ORACLE=1 also requires each file's `summary:` line to match smash2.
set -euo pipefail

BIN="${1:-}"
REPO="${2:-}"
if [[ -z "$BIN" || -z "$REPO" ]]; then
  echo "usage: verify_goldens.sh <smash2_cpp> <repo-root>" >&2
  exit 2
fi
if [[ ! -x "$BIN" ]]; then
  echo "missing smash2_cpp at $BIN" >&2
  exit 2
fi

SMASH2_RUST="${SMASH2_RUST:-$REPO/reimplementation/adl2/target/release/smash2}"
CROSS="${CROSS_ORACLE:-0}"

run_one() {
  local file=$1
  local needle=$2
  local out err rc
  out="$(mktemp)"
  err="$(mktemp)"
  set +e
  "$BIN" verify "$file" >"$out" 2>"$err"
  rc=$?
  set -e
  if [[ $rc -ne 0 ]]; then
    echo "verify exited $rc: $file" >&2
    cat "$err" >&2
    rm -f "$out" "$err"
    return 1
  fi
  if ! grep -q "$needle" "$out"; then
    echo "expected /$needle/ in $file, got:" >&2
    cat "$out" >&2
    rm -f "$out" "$err"
    return 1
  fi
  if [[ "$CROSS" == "1" ]]; then
    if [[ ! -x "$SMASH2_RUST" ]]; then
      echo "CROSS_ORACLE=1 but smash2 missing at $SMASH2_RUST" >&2
      rm -f "$out" "$err"
      return 1
    fi
    local rust_out rust_sum cpp_sum
    rust_out="$(mktemp)"
    set +e
    "$SMASH2_RUST" verify "$file" >"$rust_out" 2>/dev/null
    rc=$?
    set -e
    if [[ $rc -ne 0 ]]; then
      echo "smash2 verify exited $rc: $file" >&2
      rm -f "$out" "$err" "$rust_out"
      return 1
    fi
    cpp_sum=$(grep '^summary:' "$out" | head -n 1 || true)
    rust_sum=$(grep '^summary:' "$rust_out" | head -n 1 || true)
    if [[ "$cpp_sum" != "$rust_sum" ]]; then
      echo "summary mismatch: $file" >&2
      echo "  rust: $rust_sum" >&2
      echo "  cpp:  $cpp_sum" >&2
      rm -f "$out" "$err" "$rust_out"
      return 1
    fi
    rm -f "$rust_out"
  fi
  rm -f "$out" "$err"
}

g="$REPO/examples/golden"
run_one "$g/disjoint_01.adl" "proven disjoint"
run_one "$g/empty_01.adl" "EMPTY REGIONS"

if command -v z3 >/dev/null 2>&1; then
  run_one "$g/overlap_01.adl" "proven overlapping"
  run_one "$g/presence_01_complementary_rejects.adl" "proven overlapping"
else
  echo "verify_goldens: skip SAT-side overlap pins (no z3 on PATH)"
fi
