#!/usr/bin/env bash
# Golden tests for region analysis (-r). Expects ./smash at repo root.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SMASH="${SMASH:-$ROOT/smash}"
GOLDEN="$ROOT/tests/golden"
cd "$ROOT"

if [[ ! -x "$SMASH" ]]; then
  echo "Build smash first: make"
  exit 1
fi

fail=0

# check FILE PATTERN LABEL [EXTRA_ARGS...]
check() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  shift 3
  local out
  out=$("$SMASH" -r "$@" "$file" 2>&1) || { echo "FAIL parse: $file"; fail=1; return; }
  if echo "$out" | grep -qE "$pattern"; then
    echo "OK   $label"
  else
    echo "FAIL $label ($file) — expected pattern: $pattern"
    echo "$out" | grep -E "vs |dropped|leaves" | tail -10
    fail=1
  fi
}

# check_absent FILE PATTERN LABEL — pattern must NOT appear
check_absent() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local out
  out=$("$SMASH" -r "$file" 2>&1) || { echo "FAIL parse: $file"; fail=1; return; }
  if echo "$out" | grep -qE "$pattern"; then
    echo "FAIL $label ($file) — forbidden pattern present: $pattern"
    echo "$out" | grep -E "$pattern" | head -3
    fail=1
  else
    echo "OK   $label"
  fi
}

# ---- encoding structure ----
check "$GOLDEN/ite_conditional_dphi.adl" "SR_ite: .*\(exact\) \(1 OR\)" "ITE encoded exactly as guarded OR"
check "$GOLDEN/or_met.adl" "\(1 OR\)" "OR clause encoded"

# ---- heuristic + SMT disjointness (sound direction) ----
check "$GOLDEN/disjoint_pt.adl" "PROVEN DISJOINT" "disjoint pT intervals"
check "$GOLDEN/disjoint_pt.adl" "SR_low vs SR_high" "pairwise line present"
check "$GOLDEN/disjoint_pt.adl" "PROVEN DISJOINT" "heuristic disjoint pT (--no-smt)" --no-smt
check "$GOLDEN/disjoint_jet_index.adl" "PROVEN DISJOINT" "disjoint same jet index intervals"

# ---- overlap proofs ----
check "$GOLDEN/overlap_met.adl" "PROVEN OVERLAPPING|POSSIBLY OVERLAPPING" "MET overlap"
check "$GOLDEN/size_bjets.adl" "SR_ge2 vs SR_ge4" "size pairwise present"

# ---- soundness regression suite: each of these was a false or missed
# ---- verdict before the dual-encoding rewrite
check_absent "$GOLDEN/or_unencodable_branch.adl" "SR_orcut vs SR_lowmet: PROVEN DISJOINT" \
  "OR with unencodable branch must not prove disjoint"
check "$GOLDEN/reject_and_band.adl" "PROVEN DISJOINT" \
  "reject of AND-band proves disjoint (De Morgan)"
check "$GOLDEN/not_tag.adl" "SR_btag vs SR_nobtag: PROVEN DISJOINT" \
  "not <tag cut> proves complementary regions disjoint"
check "$GOLDEN/define_under_or.adl" "SR_a vs SR_b: PROVEN DISJOINT" \
  "define referenced under OR stays disjunctive"
check_absent "$GOLDEN/tag_index.adl" "SR_lead_btag vs SR_sub_nobtag: PROVEN DISJOINT" \
  "different jet indices must not alias into one tag variable"

if command -v z3 >/dev/null 2>&1; then
  check "$GOLDEN/btag_threshold.adl" "SR_no vs SR_yes: PROVEN DISJOINT" \
    "tag {0,1} axiom proves threshold complement disjoint"
  check "$GOLDEN/ratio_met.adl" "SR_ratio vs SR_lowmet: PROVEN DISJOINT.*exact" \
    "ratio cut (L/D op c) encoded exactly"
  check "$GOLDEN/collection_quant.adl" "SR_allhard vs SR_softlead: PROVEN DISJOINT" \
    "bounded quantifier + ordering proves collection cut disjoint"
  check_absent "$GOLDEN/collection_quant.adl" "SR_unbounded vs SR_softlead: PROVEN DISJOINT" \
    "unbounded collection cut must not prove disjoint"
  check "$GOLDEN/bins_partition.adl" "SR_binned \[MET\]: 3 bins; disjoint 3/3 pairs; coverage: proven" \
    "complete binning proven disjoint and covering"
  check "$GOLDEN/bins_partition.adl" "SR_gap \[MET\]: 2 bins; disjoint 1/1 pairs; coverage: not proven" \
    "incomplete binning flags possible gap"
  # ---- audit regression suite (June 2026 adversarial audit) ----
  check_absent "$GOLDEN/quant_empty_forall.adl" "SR_nojets vs SR_hardjets: PROVEN (DISJOINT|OVERLAPPING)" \
    "empty collection under all-reading: no proven verdict"
  check "$GOLDEN/define_arith.adl" "SR_a vs SR_b: PROVEN DISJOINT" \
    "define in arithmetic is inlined (no opaque free scalar)"
  check_absent "$GOLDEN/angular_order.adl" "SR_a vs SR_b: PROVEN (DISJOINT|OVERLAPPING)" \
    "reversed angular args stay convention-neutral"
  check_absent "$GOLDEN/union_size.adl" "provably selects no events" \
    "union take must not get subset size axiom"
  check_absent "$GOLDEN/inf_constant.adl" "PROVEN OVERLAPPING" \
    "non-finite constant cut becomes Unknown, not dropped assert"
  check "$GOLDEN/btag_discriminant.adl" "SR_a vs SR_b: PROVEN OVERLAPPING" \
    "continuous btag discriminant not forced to {0,1}"
  check "$GOLDEN/vacuous_dphi.adl" "provably selects no events" \
    "dphi range axiom catches vacuous region"
  check "$GOLDEN/vacuous_dphi.adl" "SR_dead vs SR_any: PROVEN DISJOINT" \
    "empty region disjoint from everything"
  check "$GOLDEN/reject_or_band.adl" "SR_band vs SR_mid: PROVEN OVERLAPPING" \
    "reject of OR-band proves overlap"
  check "$GOLDEN/reject_or_band.adl" "PROVEN SUBSET: SR_mid within SR_band" \
    "subset detection (SR_mid inside kept band)"
  check "$GOLDEN/or_unencodable_branch.adl" "PROVEN OVERLAPPING" \
    "overlap proved through the encodable OR branch"
  check "$GOLDEN/size_bjets.adl" "PROVEN OVERLAPPING" \
    "SMT proven overlap size(bjets)"
  check "$GOLDEN/disjoint_pt.adl" "PROVEN DISJOINT.*(UNSAT|cannot intersect)" \
    "disjoint pT verdict carries its reason"
  out=$("$SMASH" -r "$GOLDEN/independent_jet_index.adl" 2>&1) || fail=1
  if echo "$out" | grep -qE "SR_lead_high vs SR_sub_low: (PROVEN|POSSIBLY) OVERLAPPING"; then
    echo "OK   independent jet indices may overlap"
  else
    echo "FAIL independent_jet_index — expected overlap/sat"
    fail=1
  fi
else
  echo "SKIP z3 not installed (SMT golden tests)"
fi

# ---- error reporting ----
err=$("$SMASH" "$GOLDEN/bad_syntax.adl" 2>&1 || true)
if echo "$err" | grep -qE "ERROR at line 5"; then
  echo "OK   parse errors report the offending line"
else
  echo "FAIL bad_syntax.adl should error at line 5"
  echo "$err" | head -4
  fail=1
fi
if "$SMASH" "$GOLDEN/bad_syntax.adl" >/dev/null 2>&1; then
  echo "FAIL bad_syntax.adl should exit nonzero"
  fail=1
else
  echo "OK   bad input exits nonzero"
fi

# ---- JSON export ----
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
