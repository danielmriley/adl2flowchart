#!/usr/bin/env bash
# P4 DOT dump gate over cpp/tests/dot_gate_files.txt (flowchart + AST).
# Default: C++ well-formedness. CROSS_ORACLE=1 byte-diffs smash2.
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

# shellcheck source=gate_common.sh
source "$ROOT/cpp/scripts/gate_common.sh"
gate_prepare
echo "==> smash2_cpp=$SMASH2_CPP"

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
  "$SMASH2_CPP" "${cpp_args[@]}" >"$tmpdir/cpp.dump" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  rc_rust=0
  if [[ "$GATE_ORACLE" == "1" ]]; then
    "$SMASH2_RUST" "${rust_args[@]}" >"$tmpdir/rust.dump" 2>"$tmpdir/rust.err"
    rc_rust=$?
  fi
  set -e

  local reason=""
  if [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp $kind exited $rc_cpp"
  elif ! dump_ok "$tmpdir/cpp.dump"; then
    reason="cpp $kind dump missing or does not start with 'digraph '"
  elif [[ "$GATE_ORACLE" == "1" ]]; then
    if [[ "$rc_rust" -ne 0 ]]; then
      reason="rust smash2 $kind exited $rc_rust"
    elif ! dump_ok "$tmpdir/rust.dump"; then
      reason="rust $kind dump missing or does not start with 'digraph '"
    elif ! diff -q "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >/dev/null; then
      reason="$kind dump mismatch"
    fi
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

if [[ "$GATE_ORACLE" == "1" ]]; then
  echo "==> DOT dump-diff ${#FILES[@]} files (flowchart + AST; optional smash2 cross-check)"
else
  echo "==> DOT dump ${#FILES[@]} files (flowchart + AST; C++-only; CROSS_ORACLE=1 for smash2 diff)"
fi
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
if [[ "$GATE_ORACLE" == "1" ]]; then
  echo "dump-dot corpus gate: PASS (byte-for-byte vs smash2; both sides exit 0)"
else
  echo "dump-dot corpus gate: PASS (C++ dumps well-formed; CROSS_ORACLE=1 for smash2 diff)"
fi
