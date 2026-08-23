#!/usr/bin/env bash
# smash2_cpp verify corpus gate.
# Default: C++-only. Every examples/*.adl must exit 0, UNKNOWN must stay 0,
# and PROVEN DISJOINT must not rise above the smash2 ledger pin.
# CROSS_ORACLE=1 also requires each file's `summary:` line to match smash2.
#
# Usage (from repo root):
#   cpp/scripts/verify_corpus_gate.sh
#
# Env:
#   SMASH2_CPP / SMASH2_RUST  binary overrides
#   SKIP_BUILD=1              skip cmake/cargo
#   CROSS_ORACLE=1            also byte-compare `summary:` vs smash2
#   VERIFY_CORPUS_OUT=dir     keep per-file .out/.err
#   VERIFY_FILE_TIMEOUT=secs  per-file timeout (default: 180)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
# shellcheck source=gate_common.sh
source "$ROOT/cpp/scripts/gate_common.sh"
gate_prepare

baseline="$ROOT/reimplementation/adl2/baselines/corpus_verify.json"
if [[ ! -f "$baseline" ]]; then
  echo "verify_corpus_gate: missing baseline $baseline" >&2
  exit 1
fi

examples="$ROOT/examples"
mapfile -t files < <(find "$examples" -name '*.adl' | sort)
count="${#files[@]}"
expected=146
if [[ "$count" -ne "$expected" ]]; then
  echo "verify_corpus_gate: expected $expected corpus files, found $count" >&2
  exit 1
fi
echo "verify_corpus_gate: found $count ADL files under $examples"

if [[ -n "${VERIFY_CORPUS_OUT:-}" ]]; then
  OUT="$VERIFY_CORPUS_OUT"
  mkdir -p "$OUT"
else
  OUT="$(mktemp -d "${TMPDIR:-/tmp}/verify_corpus_gate.XXXXXX")"
  trap 'rm -rf "$OUT"' EXIT
fi
echo "verify_corpus_gate: writing per-file output under $OUT"

fail=0
timeout_secs="${VERIFY_FILE_TIMEOUT:-180}"
i=0
for f in "${files[@]}"; do
  i=$((i + 1))
  name=$(echo "$f" | sed "s|^$ROOT/||; s|/|_|g")
  set +e
  if command -v timeout >/dev/null 2>&1; then
    timeout --signal=KILL "${timeout_secs}s" \
      "$SMASH2_CPP" verify "$f" >"$OUT/$name.out" 2>"$OUT/$name.err"
    rc=$?
  else
    "$SMASH2_CPP" verify "$f" >"$OUT/$name.out" 2>"$OUT/$name.err"
    rc=$?
  fi
  set -e
  if [[ $rc -eq 137 || $rc -eq 124 ]]; then
    echo "TIMEOUT(${timeout_secs}s): $f" >&2
    fail=$((fail + 1))
  elif [[ $rc -ne 0 ]]; then
    echo "NONZERO($rc): $f" >&2
    fail=$((fail + 1))
  elif ! grep -q '^summary:' "$OUT/$name.out"; then
    echo "NO SUMMARY: $f" >&2
    fail=$((fail + 1))
  fi

  if [[ "$GATE_ORACLE" == "1" && $rc -eq 0 ]]; then
    set +e
    if command -v timeout >/dev/null 2>&1; then
      timeout --signal=KILL "${timeout_secs}s" \
        "$SMASH2_RUST" verify "$f" >"$OUT/$name.rust.out" 2>"$OUT/$name.rust.err"
      rc_rust=$?
    else
      "$SMASH2_RUST" verify "$f" >"$OUT/$name.rust.out" 2>"$OUT/$name.rust.err"
      rc_rust=$?
    fi
    set -e
    if [[ $rc_rust -ne 0 ]]; then
      echo "RUST NONZERO($rc_rust): $f" >&2
      fail=$((fail + 1))
    else
      cpp_sum=$(grep '^summary:' "$OUT/$name.out" | head -n 1 || true)
      rust_sum=$(grep '^summary:' "$OUT/$name.rust.out" | head -n 1 || true)
      if [[ "$cpp_sum" != "$rust_sum" ]]; then
        echo "SUMMARY MISMATCH: $f" >&2
        echo "  rust: $rust_sum" >&2
        echo "  cpp:  $cpp_sum" >&2
        fail=$((fail + 1))
      fi
    fi
  fi

  if (( i % 5 == 0 || i == count )); then
    echo "verify_corpus_gate: progress $i/$count" >&2
  fi
done

if [[ "$fail" -ne 0 ]]; then
  echo "verify_corpus_gate: FAIL — $fail file(s)" >&2
  exit 1
fi

agg=$(grep -h '^summary:' "$OUT"/*.out \
  | grep -oE '[0-9]+ proven disjoint|[0-9]+ proven overlapping|[0-9]+ candidate overlapping|[0-9]+ possibly overlapping|[0-9]+ unknown|[0-9]+ pairs?' \
  | awk '
    /pair/                  {pairs+=$1}
    /proven disjoint/       {dis+=$1}
    /proven overlapping/    {ov+=$1}
    /candidate overlapping/ {cand+=$1}
    /possibly overlapping/  {pos+=$1}
    /unknown/               {unk+=$1}
    END{
      printf "pairs=%d\nproven_disjoint=%d\nproven_overlapping=%d\ncandidate_overlapping=%d\npossibly=%d\nunknown=%d\n",
             pairs,dis,ov,cand,pos,unk
    }')

eval "$agg"
echo "verify_corpus_gate: aggregate — pairs=$pairs proven_disjoint=$proven_disjoint proven_overlapping=$proven_overlapping candidate_overlapping=$candidate_overlapping possibly=$possibly unknown=$unknown"

python3 - "$baseline" "$pairs" "$proven_disjoint" "$proven_overlapping" \
    "$candidate_overlapping" "$possibly" "$unknown" "$count" <<'PY'
import json, sys

path, pairs, dis, ov, cand, pos, unk, files = sys.argv[1:]
base = json.load(open(path))
cur = {
    "files": int(files),
    "pairs": int(pairs),
    "proven_disjoint": int(dis),
    "proven_overlapping": int(ov),
    "candidate_overlapping": int(cand),
    "possibly": int(pos),
    "unknown": int(unk),
}

base_dis = int(base["proven_disjoint"])
ok = True
print(f"verify_corpus_gate: baseline proven_disjoint={base_dis}  current={cur['proven_disjoint']}")
if cur["proven_disjoint"] > base_dis:
    print(
        f"verify_corpus_gate: FAIL — PROVEN DISJOINT rose "
        f"{base_dis} → {cur['proven_disjoint']} (soundness regression)",
        file=sys.stderr,
    )
    ok = False
elif cur["proven_disjoint"] < base_dis:
    print(
        f"verify_corpus_gate: NOTE — PROVEN DISJOINT decreased "
        f"{base_dis} → {cur['proven_disjoint']} (allowed tightening; update baseline if intentional)"
    )

if cur["unknown"] > 0:
    print(f"verify_corpus_gate: FAIL — UNKNOWN={cur['unknown']}", file=sys.stderr)
    ok = False

for key in ("pairs", "proven_overlapping", "candidate_overlapping", "possibly", "files"):
    b, c = int(base.get(key, -1)), cur[key]
    if b >= 0 and b != c:
        print(f"verify_corpus_gate: NOTE — {key} baseline={b} current={c}")

sys.exit(0 if ok else 1)
PY

echo "verify_corpus_gate: PASS — $count files exit 0; UNKNOWN=0; PROVEN DISJOINT ≤ baseline."
