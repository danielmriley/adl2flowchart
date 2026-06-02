#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SMASH="${SMASH:-$ROOT/smash}"
cd "$ROOT"

if [[ ! -x "$SMASH" ]]; then make; fi

echo "=== Parse sweep (examples/*.adl) ==="
n=0
bad=0
while IFS= read -r -d '' f; do
  n=$((n+1))
  if ! "$SMASH" "$f" >/dev/null 2>&1; then
    echo "FAIL $f"
    bad=$((bad+1))
  fi
done < <(find examples -name '*.adl' -print0)
echo "Parsed $n files, failures: $bad"

echo "=== Region analysis sweep (-r) ==="
rbad=0
while IFS= read -r -d '' f; do
  if ! "$SMASH" -r "$f" >/dev/null 2>&1; then
    echo "FAIL -r $f"
    rbad=$((rbad+1))
  fi
done < <(find examples -name '*.adl' -print0)
echo "Region analysis $n files, failures: $rbad"

if [[ $bad -ne 0 || $rbad -ne 0 ]]; then exit 1; fi
echo "Corpus validation OK."