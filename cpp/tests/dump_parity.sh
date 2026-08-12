#!/usr/bin/env bash
# Byte-for-byte dump parity vs a committed golden (same contract as the
# corpus gate, for a single fixture — no Rust smash2 required).
# Usage: dump_parity.sh <smash2_cpp> <file.adl> <golden.dump>
set -euo pipefail
if [[ $# -ne 3 ]]; then
  echo "usage: $0 <smash2_cpp> <file.adl> <golden.dump>" >&2
  exit 2
fi
BIN=$1
ADL=$2
GOLDEN=$3
test -x "$BIN" || { echo "missing binary $BIN" >&2; exit 2; }
test -f "$ADL" || { echo "missing adl $ADL" >&2; exit 2; }
test -f "$GOLDEN" || { echo "missing golden $GOLDEN" >&2; exit 2; }

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT

set +e
"$BIN" check --dump-ast "$ADL" >"$tmp" 2>/dev/null
rc=$?
set -e
if [[ "$rc" -ne 0 ]]; then
  echo "error: $BIN check --dump-ast exited $rc for $ADL" >&2
  exit 1
fi
if [[ ! -s "$tmp" ]] || [[ "$(head -n 1 "$tmp")" != "File" ]]; then
  echo "error: dump for $ADL is empty or does not start with File" >&2
  exit 1
fi
if ! diff -q "$GOLDEN" "$tmp" >/dev/null; then
  echo "error: dump for $ADL does not match golden $GOLDEN" >&2
  diff -u "$GOLDEN" "$tmp" >&2 || true
  exit 1
fi
