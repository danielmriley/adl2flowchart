#!/usr/bin/env bash
# P3 axiom dump gate over cpp/tests/axioms_gate_files.txt.
# Default: C++ well-formedness. CROSS_ORACLE=1 byte-diffs smash2.
#
# Both dump commands must exit 0 and emit a dump starting with `unit:`.
# A crash, usage error, empty dump, or mismatch fails the gate.
#
# Usage (from repo root):
#   cpp/scripts/dump_axioms_corpus_gate.sh
#   COUNT_ONLY=1 cpp/scripts/dump_axioms_corpus_gate.sh
set -euo pipefail

EXPECTED_FILES=108

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

LIST="$ROOT/cpp/tests/axioms_gate_files.txt"
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
  echo "axioms allowlist count: ${#FILES[@]} (pinned $EXPECTED_FILES)"
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

dump_ok() {
  local dump=$1
  [[ -s "$dump" ]] || return 1
  [[ "$(head -n 1 "$dump")" == unit:* ]] || return 1
  grep -q '^axioms ' "$dump" || return 1
}

if [[ "$GATE_ORACLE" == "1" ]]; then
  echo "==> axiom dump-diff ${#FILES[@]} files (optional smash2 cross-check vs smash2_cpp)"
else
  echo "==> axiom dump ${#FILES[@]} files (C++-only; CROSS_ORACLE=1 for smash2 diff)"
fi
for rel in "${FILES[@]}"; do
  f="$ROOT/$rel"
  if [[ ! -f "$f" ]]; then
    fail=$((fail + 1))
    failures+=("$rel (missing file)")
    echo "FAIL $rel: missing file" >&2
    continue
  fi
  set +e
  "$SMASH2_CPP" check --dump-axioms "$f" >"$tmpdir/cpp.dump" 2>"$tmpdir/cpp.err"
  rc_cpp=$?
  rc_rust=0
  if [[ "$GATE_ORACLE" == "1" ]]; then
    "$SMASH2_RUST" check --dump-axioms "$f" >"$tmpdir/rust.dump" 2>"$tmpdir/rust.err"
    rc_rust=$?
  fi
  set -e

  reason=""
  if [[ "$rc_cpp" -ne 0 ]]; then
    reason="smash2_cpp --dump-axioms exited $rc_cpp"
  elif ! dump_ok "$tmpdir/cpp.dump"; then
    reason="cpp dump missing or does not start with unit:/axioms"
  elif [[ "$GATE_ORACLE" == "1" ]]; then
    if [[ "$rc_rust" -ne 0 ]]; then
      reason="rust smash2 --dump-axioms exited $rc_rust"
    elif ! dump_ok "$tmpdir/rust.dump"; then
      reason="rust dump missing or does not start with unit:/axioms"
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
echo "dump-axioms corpus gate: OK=$ok FAIL=$fail TOTAL=$total (allowlist ${#FILES[@]})"
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
  echo "dump-axioms corpus gate: PASS (byte-for-byte vs smash2; both sides exit 0)"
else
  echo "dump-axioms corpus gate: PASS (C++ dumps well-formed; CROSS_ORACLE=1 for smash2 diff)"
fi
