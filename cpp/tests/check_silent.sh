#!/usr/bin/env bash
# Bare `check` is silent on stdout when the file resolves (Rust smash2 parity).
set -euo pipefail
BIN="$1"
FILE="$2"
out="$("$BIN" check "$FILE")"
if [[ -n "$out" ]]; then
  echo "expected empty stdout from smash2_cpp check, got:" >&2
  printf '%s\n' "$out" >&2
  exit 1
fi
