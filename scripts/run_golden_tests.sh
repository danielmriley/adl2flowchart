#!/usr/bin/env bash
# Golden tests for region analysis (-r). Expects ./smash at repo root.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SMASH="${SMASH:-$ROOT/smash}"
GOLDEN="$ROOT/tests/golden"

if [[ ! -x "$SMASH" ]]; then
  echo "Build smash first: make"
  exit 1
fi

fail=0
check() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local out
  out=$("$SMASH" -r "$file" 2>&1) || { echo "FAIL parse: $file"; fail=1; return; }
  if echo "$out" | grep -q "$pattern"; then
    echo "OK   $label ($file)"
  else
    echo "FAIL $label ($file) — expected pattern: $pattern"
    echo "$out" | tail -30
    fail=1
  fi
}

check "$GOLDEN/disjoint_pt.adl" "PROVEN DISJOINT" "disjoint pT intervals"
check "$GOLDEN/disjoint_pt.adl" "SR_low vs SR_high" "pairwise line"
check "$GOLDEN/overlap_met.adl" "POSSIBLY OVERLAPPING\|OVERLAP (SMT" "MET overlap heuristic or SMT"
check "$GOLDEN/size_bjets.adl" "SR_ge2 vs SR_ge4" "size pairwise present"

if command -v z3 >/dev/null 2>&1; then
  out=$("$SMASH" -r --smt "$GOLDEN/size_bjets.adl" 2>&1) || fail=1
  if echo "$out" | grep -q "OVERLAP (SMT sat)\|POSSIBLY OVERLAPPING"; then
    echo "OK   SMT overlap/subset size(bjets)"
  else
    echo "FAIL SMT size(bjets) — expected overlap sat"
    echo "$out" | tail -20
    fail=1
  fi
  out=$("$SMASH" -r --smt "$GOLDEN/disjoint_pt.adl" 2>&1) || fail=1
  if echo "$out" | grep -q "PROVEN DISJOINT"; then
    echo "OK   SMT/heuristic disjoint pT"
  else
    echo "FAIL disjoint pT with --smt"
    fail=1
  fi
else
  echo "SKIP z3 not installed (SMT golden tests)"
fi

json_out=$("$SMASH" -r --json /tmp/adl_golden.json "$GOLDEN/disjoint_pt.adl" 2>&1) || fail=1
if [[ -f /tmp/adl_golden.json ]] && grep -q '"proven_disjoint"' /tmp/adl_golden.json; then
  echo "OK   JSON export"
else
  echo "FAIL JSON export"
  fail=1
fi

if [[ $fail -eq 0 ]]; then
  echo "All golden tests passed."
  exit 0
fi
exit 1