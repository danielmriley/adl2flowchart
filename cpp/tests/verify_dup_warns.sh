#!/usr/bin/env bash
# smash2_cpp verify must surface duplicate-region warnings and disambiguate
# the pair names so CMS-style files are not reported as R vs R.
set -euo pipefail
BIN="$1"
FILE="$2"
out="$(mktemp)"
err="$(mktemp)"
trap 'rm -f "$out" "$err"' EXIT
set +e
"$BIN" verify --no-solver "$FILE" >"$out" 2>"$err"
status=$?
set -e
if [[ "$status" -gt 2 ]]; then
  echo "verify exited $status" >&2
  cat "$err" >&2
  exit 1
fi
if ! grep -q "duplicate region" "$err"; then
  echo "expected duplicate-region warning on stderr, got:" >&2
  cat "$err" >&2
  exit 1
fi
if ! grep -qE 'SR@[0-9]+' "$out"; then
  echo "expected disambiguated SR@line names on stdout, got:" >&2
  cat "$out" >&2
  exit 1
fi
