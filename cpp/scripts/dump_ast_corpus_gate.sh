#!/usr/bin/env bash
# P1 dump-ast corpus gate: byte-for-byte diff of
#   smash2_cpp check --dump-ast  vs  smash2 check --dump-ast
# over the same 146-file examples/ corpus as Rust adl-syntax corpus_gate.
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
#   SMASH2_RUST  path to Rust smash2 (default: reimplementation/adl2/target/release/smash2)
#   SKIP_BUILD=1 skip cmake/cargo build steps
#   CORPUS_ROOT  override examples/ root
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

CORPUS_ROOT="${CORPUS_ROOT:-$ROOT/examples}"
SMASH2_CPP="${SMASH2_CPP:-$ROOT/cpp/build/smash2_cpp}"
SMASH2_RUST="${SMASH2_RUST:-$ROOT/reimplementation/adl2/target/release/smash2}"

if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
  echo "==> building smash2_cpp"
  cmake -S cpp -B cpp/build -DCMAKE_BUILD_TYPE=Release
  cmake --build cpp/build -j"$(nproc)"

  echo "==> building Rust smash2 (forever oracle; no native z3)"
  (
    cd reimplementation/adl2
    # --no-default-features: works on hosts without libz3; dump-ast is syntax-only.
    cargo build --release -p adl-cli --no-default-features
  )
fi

test -x "$SMASH2_CPP" || { echo "missing smash2_cpp at $SMASH2_CPP" >&2; exit 2; }
test -x "$SMASH2_RUST" || { echo "missing smash2 at $SMASH2_RUST" >&2; exit 2; }
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

echo "==> dump-diff ${#FILES[@]} files (Rust oracle vs smash2_cpp)"
for f in "${FILES[@]}"; do
  rel="${f#"$CORPUS_ROOT"/}"
  set +e
  "$SMASH2_RUST" check --dump-ast "$f" >"$tmpdir/rust.dump" 2>"$tmpdir/rust.err"
  rc_rust=$?
  "$SMASH2_CPP" check --dump-ast "$f" >"$tmpdir/cpp.dump" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  set -e

  reason=""
  if [[ "$rc_rust" -ne 0 ]]; then
    reason="rust smash2 exited $rc_rust"
  elif [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp exited $rc_cpp"
  elif ! dump_ok "$tmpdir/rust.dump"; then
    reason="rust dump missing or does not start with File"
  elif ! dump_ok "$tmpdir/cpp.dump"; then
    reason="cpp dump missing or does not start with File"
  elif ! diff -q "$tmpdir/rust.dump" "$tmpdir/cpp.dump" >/dev/null; then
    reason="dump mismatch"
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
echo "dump-ast corpus gate: PASS (byte-for-byte vs Rust smash2; both sides exit 0)"
