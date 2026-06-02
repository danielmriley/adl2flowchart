#!/usr/bin/env bash
# Phase-2 validation: Z3 on CMS-SUS-16-033 Delphes (size(bjets) SRs).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SMASH="${SMASH:-$ROOT/smash}"
FILE="$ROOT/examples/CMS/CMS-SUS-16-033_Delphes.adl"

if [[ ! -x "$SMASH" ]]; then make -C "$ROOT"; fi
if ! command -v z3 >/dev/null 2>&1; then
  echo "SKIP: z3 not installed"
  exit 0
fi

out=$("$SMASH" -r --smt "$FILE" 2>&1)
echo "$out" | grep -E "SR[0-9]+ vs SR[0-9]+" | head -20
if echo "$out" | grep -q "SMT disjoint=\|OVERLAP (SMT"; then
  echo "OK   Phase-2 Z3 spike on Delphes 033"
  exit 0
fi
echo "FAIL Phase-2 spike — no SMT pairwise results"
exit 1