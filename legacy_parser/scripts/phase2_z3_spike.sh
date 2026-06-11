#!/usr/bin/env bash
# Phase-2 validation: Z3 on CMS-SUS-16-033 Delphes (size(bjets) SRs).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SMASH="${SMASH:-$ROOT/smash}"
FILE="$ROOT/../examples/CMS/CMS-SUS-16-033_Delphes.adl"
cd "$ROOT"

if [[ ! -x "$SMASH" ]]; then make -C "$ROOT"; fi
if ! command -v z3 >/dev/null 2>&1; then
  echo "SKIP: z3 not installed"
  exit 0
fi

out=$("$SMASH" -r "$FILE" 2>&1)
echo "$out" | grep -E "SR[0-9]+ vs SR[0-9]+" | head -20 || true
if echo "$out" | grep -qE "PROVEN OVERLAPPING \[SMT\]|SMT proven_overlap=[1-9]|PROVEN DISJOINT.*UNSAT"; then
  echo "OK   Phase-2 Z3 spike on Delphes 033"
  exit 0
fi
echo "FAIL Phase-2 spike — expected SMT proven overlap or disjoint"
echo "$out" | grep -E "Pairwise|Summary|z3:" | tail -15
exit 1