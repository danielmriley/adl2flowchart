#!/usr/bin/env bash
# P2 HIR / quantity-table dump gate: byte-for-byte diff of
#   smash2_cpp check --dump-hir         vs  smash2 check --dump-hir
#   smash2_cpp check --dump-quantities  vs  smash2 check --dump-quantities
# over the fail-closed allowlist in cpp/tests/hir_gate_files.txt.
#
# Both dump commands must exit 0 and emit a dump starting with `unit:`.
# A crash, usage error, empty dump, or mismatch fails the gate.
#
# The claimed set size is pinned (EXPECTED_FILES). Shrinking or growing
# hir_gate_files.txt without updating that pin fails CI — same idea as
# dump-ast's expected=146.
#
# Usage (from repo root):
#   cpp/scripts/dump_hir_corpus_gate.sh
#   COUNT_ONLY=1 cpp/scripts/dump_hir_corpus_gate.sh   # count pin only
#
# Env:
#   SMASH2_CPP   path to C++ binary (default: cpp/build/smash2_cpp)
#   SMASH2_RUST  path to Rust smash2 (default: reimplementation/adl2/target/release/smash2)
#   SKIP_BUILD=1 skip cmake/cargo build steps
#   COUNT_ONLY=1 only assert the allowlist length (no binaries / no diffs)
set -euo pipefail

# Pinned claimed-set size. Bump this only when intentionally expanding
# (or shrinking) the HIR dump-diff allowlist, in the same commit as the
# list change. 14 tutorials + 24 goldens.
EXPECTED_FILES=38

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

LIST="$ROOT/cpp/tests/hir_gate_files.txt"
SMASH2_CPP="${SMASH2_CPP:-$ROOT/cpp/build/smash2_cpp}"
SMASH2_RUST="${SMASH2_RUST:-$ROOT/reimplementation/adl2/target/release/smash2}"

test -f "$LIST" || { echo "missing allowlist $LIST" >&2; exit 2; }

mapfile -t FILES < <(grep -v '^#' "$LIST" | grep -v '^[[:space:]]*$' | sed 's/[[:space:]]*$//')
if [[ "${#FILES[@]}" -ne "$EXPECTED_FILES" ]]; then
  echo "error: expected $EXPECTED_FILES allowlist files, found ${#FILES[@]}" >&2
  echo "update EXPECTED_FILES in $0 if the claimed set intentionally changed" >&2
  exit 1
fi

if [[ "${COUNT_ONLY:-0}" == "1" ]]; then
  echo "hir allowlist count: ${#FILES[@]} (pinned $EXPECTED_FILES)"
  exit 0
fi

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  # Some images default CXX to clang without libstdc++; prefer g++ when unset.
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

dump_ok() {
  local dump=$1
  [[ -s "$dump" ]] || return 1
  [[ "$(head -n 1 "$dump")" == unit:* ]] || return 1
}

diff_one() {
  local flag=$1
  local f=$2
  local rel=$3
  set +e
  "$SMASH2_RUST" check "$flag" "$f" >"$tmpdir/rust.dump" 2>"$tmpdir/rust.err"
  rc_rust=$?
  "$SMASH2_CPP" check "$flag" "$f" >"$tmpdir/cpp.dump" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  set -e

  local reason=""
  if [[ "$rc_rust" -ne 0 ]]; then
    reason="rust smash2 $flag exited $rc_rust"
  elif [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp $flag exited $rc_cpp"
  elif ! dump_ok "$tmpdir/rust.dump"; then
    reason="rust $flag dump missing or does not start with unit:"
  elif ! dump_ok "$tmpdir/cpp.dump"; then
    reason="cpp $flag dump missing or does not start with unit:"
  elif ! diff -q "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >/dev/null; then
    reason="$flag dump mismatch"
  fi

  if [[ -z "$reason" ]]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
    failures+=("$rel $flag ($reason)")
    echo "FAIL $rel $flag: $reason" >&2
    if [[ "$reason" == *mismatch ]]; then
      diff -u "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >"$tmpdir/udiff" || true
      head -60 "$tmpdir/udiff" >&2 || true
    else
      echo "  rust stderr:" >&2
      head -20 "$tmpdir/rust.err" >&2 || true
      echo "  cpp stderr:" >&2
      head -20 "$tmpdir/cpp.err" >&2 || true
    fi
  fi
}

echo "==> HIR/quantity dump-diff ${#FILES[@]} files × 2 dumps (Rust oracle vs smash2_cpp)"
for rel in "${FILES[@]}"; do
  f="$ROOT/$rel"
  if [[ ! -f "$f" ]]; then
    fail=$((fail + 1))
    failures+=("$rel (missing file)")
    echo "FAIL $rel: missing file" >&2
    continue
  fi
  diff_one --dump-hir "$f" "$rel"
  diff_one --dump-quantities "$f" "$rel"
done

total=$((ok + fail))
expected_dumps=$((EXPECTED_FILES * 2))
echo "dump-hir corpus gate: OK=$ok FAIL=$fail TOTAL=$total (allowlist ${#FILES[@]} files × 2)"
if [[ "$fail" -ne 0 ]]; then
  echo "failed files:" >&2
  printf '  %s\n' "${failures[@]}" >&2
  exit 1
fi
if [[ "$ok" -ne "$expected_dumps" ]]; then
  echo "error: expected $expected_dumps passing dumps, got ok=$ok" >&2
  exit 1
fi
echo "dump-hir corpus gate: PASS (byte-for-byte vs Rust smash2; both sides exit 0)"
