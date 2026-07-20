#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SMASH="${SMASH:-$ROOT/smash}"
cd "$ROOT"
EXAMPLES="${EXAMPLES:-$ROOT/../examples}"

if [[ ! -x "$SMASH" ]]; then make; fi

# Golden files exercising ADL2-only grammar the legacy parser never had.
SKIP=(
  "golden/features-sort_01.adl"   # region-level sort statement
)
skip() { local f; for f in "${SKIP[@]}"; do [[ "$1" == *"$f" ]] && return 0; done; return 1; }

echo "=== Parse sweep (examples/*.adl) ==="
n=0
bad=0
while IFS= read -r -d '' f; do
  if skip "$f"; then echo "SKIP $f (ADL2-only grammar)"; continue; fi
  n=$((n+1))
  if ! "$SMASH" "$f" >/dev/null 2>&1; then
    echo "FAIL $f"
    bad=$((bad+1))
  fi
done < <(find "$EXAMPLES" -name '*.adl' -print0)
echo "Parsed $n files, failures: $bad"

echo "=== Region analysis sweep (-r) ==="
rbad=0
while IFS= read -r -d '' f; do
  if skip "$f"; then continue; fi
  if ! "$SMASH" -r "$f" >/dev/null 2>&1; then
    echo "FAIL -r $f"
    rbad=$((rbad+1))
  fi
done < <(find "$EXAMPLES" -name '*.adl' -print0)
echo "Region analysis $n files, failures: $rbad"

if [[ $bad -ne 0 || $rbad -ne 0 ]]; then exit 1; fi
echo "Corpus validation OK."