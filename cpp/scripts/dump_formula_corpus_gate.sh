#!/usr/bin/env bash
# P3 formula dump gate: byte-for-byte diff of
#   smash2_cpp check --dump-formula  vs  smash2 check --dump-formula
# over the fail-closed allowlist in cpp/tests/formula_gate_files.txt.
#
# Both dump commands must exit 0 and emit a dump starting with `unit:`.
# A crash, usage error, empty dump, or mismatch fails the gate.
#
# Usage (from repo root):
#   cpp/scripts/dump_formula_corpus_gate.sh
#   COUNT_ONLY=1 cpp/scripts/dump_formula_corpus_gate.sh
set -euo pipefail

EXPECTED_FILES=172

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

LIST="$ROOT/cpp/tests/formula_gate_files.txt"
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
  echo "formula allowlist count: ${#FILES[@]} (pinned $EXPECTED_FILES)"
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

dump_ok() {
  local dump=$1
  [[ -s "$dump" ]] || return 1
  [[ "$(head -n 1 "$dump")" == unit:* ]] || return 1
}

echo "==> formula dump-diff ${#FILES[@]} files (Rust oracle vs smash2_cpp)"
for rel in "${FILES[@]}"; do
  f="$ROOT/$rel"
  if [[ ! -f "$f" ]]; then
    fail=$((fail + 1))
    failures+=("$rel (missing file)")
    echo "FAIL $rel: missing file" >&2
    continue
  fi
  set +e
  "$SMASH2_RUST" check --dump-formula "$f" >"$tmpdir/rust.dump" 2>"$tmpdir/rust.err"
  rc_rust=$?
  "$SMASH2_CPP" check --dump-formula "$f" >"$tmpdir/cpp.dump" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  set -e

  reason=""
  if [[ "$rc_rust" -ne 0 ]]; then
    reason="rust smash2 --dump-formula exited $rc_rust"
  elif [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp --dump-formula exited $rc_cpp"
  elif ! dump_ok "$tmpdir/rust.dump"; then
    reason="rust dump missing or does not start with unit:"
  elif ! dump_ok "$tmpdir/cpp.dump"; then
    reason="cpp dump missing or does not start with unit:"
  elif ! diff -q "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >/dev/null; then
    reason="dump mismatch"
  fi

  if [[ -z "$reason" ]]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
    failures+=("$rel ($reason)")
    echo "FAIL $rel: $reason" >&2
    if [[ "$reason" == *mismatch ]]; then
      diff -u "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >"$tmpdir/udiff" || true
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
echo "dump-formula corpus gate: OK=$ok FAIL=$fail TOTAL=$total (allowlist ${#FILES[@]})"
if [[ "$fail" -ne 0 ]]; then
  echo "failed files:" >&2
  printf '  %s\n' "${failures[@]}" >&2
  exit 1
fi
if [[ "$ok" -ne "$EXPECTED_FILES" ]]; then
  echo "error: expected $EXPECTED_FILES passing dumps, got ok=$ok" >&2
  exit 1
fi
echo "dump-formula corpus gate: PASS (byte-for-byte vs Rust smash2; both sides exit 0)"
