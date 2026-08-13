#!/usr/bin/env bash
# P4 DOT dump gate: byte-for-byte diff of
#   smash2_cpp dot FILE            vs  smash2 dot FILE            (flowchart)
#   smash2_cpp dot --ast FILE      vs  smash2 dot --ast FILE      (AST)
# over the fail-closed allowlist in cpp/tests/dot_gate_files.txt.
#
# Both dump commands must exit 0 and emit a dump starting with `digraph `.
# A crash, usage error, empty dump, or mismatch fails the gate.
#
# Usage (from repo root):
#   cpp/scripts/dump_dot_corpus_gate.sh
#   COUNT_ONLY=1 cpp/scripts/dump_dot_corpus_gate.sh
set -euo pipefail

EXPECTED_FILES=39

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

LIST="$ROOT/cpp/tests/dot_gate_files.txt"
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
  echo "dot allowlist count: ${#FILES[@]} (pinned $EXPECTED_FILES)"
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
  local first
  first="$(head -n 1 "$dump")"
  [[ "$first" == digraph\ * ]] || return 1
}

compare_one() {
  local rel=$1
  local flag=$2   # "" or "--ast"
  local kind=$3
  local f="$ROOT/$rel"

  local rust_args=(dot)
  local cpp_args=(dot)
  if [[ -n "$flag" ]]; then
    rust_args+=("$flag")
    cpp_args+=("$flag")
  fi
  rust_args+=("$f")
  cpp_args+=("$f")

  set +e
  "$SMASH2_RUST" "${rust_args[@]}" >"$tmpdir/rust.dump" 2>"$tmpdir/rust.err"
  rc_rust=$?
  "$SMASH2_CPP" "${cpp_args[@]}" >"$tmpdir/cpp.dump" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  set -e

  local reason=""
  if [[ "$rc_rust" -ne 0 ]]; then
    reason="rust smash2 $kind exited $rc_rust"
  elif [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp $kind exited $rc_cpp"
  elif ! dump_ok "$tmpdir/rust.dump"; then
    reason="rust $kind dump missing or does not start with 'digraph '"
  elif ! dump_ok "$tmpdir/cpp.dump"; then
    reason="cpp $kind dump missing or does not start with 'digraph '"
  elif ! diff -q "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >/dev/null; then
    reason="$kind dump mismatch"
  fi

  if [[ -z "$reason" ]]; then
    return 0
  fi
  echo "FAIL $rel ($kind): $reason" >&2
  if [[ "$reason" == *mismatch ]]; then
    diff -u "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >"$tmpdir/udiff" || true
    head -80 "$tmpdir/udiff" >&2 || true
  else
    echo "  rust stderr:" >&2
    head -20 "$tmpdir/rust.err" >&2 || true
    echo "  cpp stderr:" >&2
    head -20 "$tmpdir/cpp.err" >&2 || true
  fi
  echo "$reason"
  return 1
}

echo "==> DOT dump-diff ${#FILES[@]} files (flowchart + AST; Rust oracle vs smash2_cpp)"
for rel in "${FILES[@]}"; do
  f="$ROOT/$rel"
  if [[ ! -f "$f" ]]; then
    fail=$((fail + 1))
    failures+=("$rel (missing file)")
    echo "FAIL $rel: missing file" >&2
    continue
  fi

  file_ok=1
  if ! reason=$(compare_one "$rel" "" "flowchart"); then
    file_ok=0
    failures+=("$rel (flowchart: $reason)")
  fi
  if ! reason=$(compare_one "$rel" "--ast" "ast"); then
    file_ok=0
    failures+=("$rel (ast: $reason)")
  fi

  if [[ "$file_ok" -eq 1 ]]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
  fi
done

total=$((ok + fail))
echo "dump-dot corpus gate: OK=$ok FAIL=$fail TOTAL=$total (allowlist ${#FILES[@]})"
if [[ "$fail" -ne 0 ]]; then
  echo "failed files:" >&2
  printf '  %s\n' "${failures[@]}" >&2
  exit 1
fi
if [[ "$ok" -ne "$EXPECTED_FILES" ]]; then
  echo "error: expected $EXPECTED_FILES passing dumps, got ok=$ok" >&2
  exit 1
fi
echo "dump-dot corpus gate: PASS (byte-for-byte vs Rust smash2; both sides exit 0)"
