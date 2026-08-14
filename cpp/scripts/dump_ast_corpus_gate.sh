#!/usr/bin/env bash
# P1 dump-ast corpus gate over the 146-file examples/ corpus.
# Default: C++ well-formedness. CROSS_ORACLE=1 byte-diffs smash2.
#
# Both dump commands must exit 0 and emit a dump starting with `File`.
# A crash, usage error, empty dump, or mismatch fails the gate.
# (Do not swallow dump-command failures with `|| true`.)
#
# Usage (from repo root):
#   cpp/scripts/dump_ast_corpus_gate.sh
#
# Env:
#   SMASH2_CPP   path to C++ binary (default: cpp/build/smash2_cpp)
#   SMASH2_RUST  path to Rust smash2 (only when CROSS_ORACLE=1)
#   SKIP_BUILD=1 skip cmake/cargo build steps
#   CROSS_ORACLE=1  optional smash2 byte-diff (C++ stands alone by default)
#   CORPUS_ROOT  override examples/ root
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
# shellcheck source=gate_common.sh
source "$ROOT/cpp/scripts/gate_common.sh"
gate_prepare

CORPUS_ROOT="${CORPUS_ROOT:-$ROOT/examples}"
test -d "$CORPUS_ROOT" || { echo "missing corpus at $CORPUS_ROOT" >&2; exit 2; }

mapfile -t FILES < <(find "$CORPUS_ROOT" -name '*.adl' | sort)
expected=146
if [[ "${#FILES[@]}" -ne "$expected" ]]; then
  echo "error: expected $expected corpus files, found ${#FILES[@]}" >&2
  exit 1
fi

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

ok=0
fail=0
failures=()

dump_ok() {
  local dump=$1
  [[ -s "$dump" ]] || return 1
  [[ "$(head -n 1 "$dump")" == "File" ]] || return 1
}

if [[ "$GATE_ORACLE" == "1" ]]; then
  echo "==> dump-diff ${#FILES[@]} files (optional smash2 cross-check vs smash2_cpp)"
else
  echo "==> dump-ast ${#FILES[@]} files (C++-only; CROSS_ORACLE=1 for smash2 diff)"
fi
for f in "${FILES[@]}"; do
  rel="${f#"$CORPUS_ROOT"/}"
  set +e
  "$SMASH2_CPP" check --dump-ast "$f" >"$tmpdir/cpp.dump" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  rc_rust=0
  if [[ "$GATE_ORACLE" == "1" ]]; then
    "$SMASH2_RUST" check --dump-ast "$f" >"$tmpdir/rust.dump" 2>"$tmpdir/rust.err"
    rc_rust=$?
  fi
  set -e

  reason=""
  if [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp exited $rc_cpp"
  elif ! dump_ok "$tmpdir/cpp.dump"; then
    reason="cpp dump missing or does not start with File"
  elif [[ "$GATE_ORACLE" == "1" ]]; then
    if [[ "$rc_rust" -ne 0 ]]; then
      reason="rust smash2 exited $rc_rust"
    elif ! dump_ok "$tmpdir/rust.dump"; then
      reason="rust dump missing or does not start with File"
    elif ! diff -q "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >/dev/null; then
      reason="dump mismatch"
    fi
  fi

  if [[ -z "$reason" ]]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
    failures+=("$rel ($reason)")
    echo "FAIL $rel: $reason" >&2
    if [[ "$reason" == "dump mismatch" ]]; then
      # Display-only: head may SIGPIPE diff; do not treat that as a dump-command success.
      diff -u "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >"$tmpdir/udiff" || true
      head -40 "$tmpdir/udiff" >&2 || true
    else
      echo "  rust stderr:" >&2
      head -20 "$tmpdir/rust.err" >&2 || true
      echo "  cpp stderr:" >&2
      head -20 "$tmpdir/cpp.err" >&2 || true
    fi
  fi
done

echo "dump-ast corpus gate: OK=$ok FAIL=$fail TOTAL=${#FILES[@]}"
if [[ "$fail" -ne 0 ]]; then
  echo "failed files:" >&2
  printf '  %s\n' "${failures[@]}" >&2
  exit 1
fi
if [[ "$GATE_ORACLE" == "1" ]]; then
  echo "dump-ast corpus gate: PASS (byte-for-byte vs smash2; both sides exit 0)"
else
  echo "dump-ast corpus gate: PASS (C++ dumps well-formed; CROSS_ORACLE=1 for smash2 diff)"
fi
