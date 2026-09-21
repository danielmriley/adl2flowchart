#!/usr/bin/env bash
# verify_corpus_gate.sh — smash3 verify corpus gate (trustworthy-verify M4).
#
# Runs `smash3 verify` over every `.adl` under examples/, aggregates pairwise
# verdict counts, and compares PROVEN DISJOINT against the committed baseline
# in baselines/corpus_verify.json.
#
# Invariants (fail closed):
#   - every file exits 0
#   - UNKNOWN == 0
#   - proven_disjoint must NOT rise above baseline (decreases are allowed)
#
# Usage:  scripts/verify_corpus_gate.sh
# Env:    SMASH3_BIN=path   skip the build and use this binary
#         VERIFY_CORPUS_OUT=dir   keep per-file .out/.err (default: mktemp)
#         VERIFY_FILE_TIMEOUT=secs   per-file timeout (default: 180)
#         SMASH3_VERIFY_ARGS=...     extra args (e.g. --no-refute-gate for a fast canary)
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace="$(cd "$here/.." && pwd)"
repo_root="$(cd "$workspace/../.." && pwd)"
examples="$(cd "$repo_root/examples" && pwd)"
baseline_json="$workspace/baselines/corpus_verify.json"

if [[ ! -f "$baseline_json" ]]; then
    echo "verify_corpus_gate: missing baseline $baseline_json" >&2
    exit 1
fi

mapfile -t files < <(find "$examples" -name '*.adl' | sort)
count="${#files[@]}"
echo "verify_corpus_gate: found $count ADL files under $examples"

if [[ -n "${SMASH3_BIN:-}" ]]; then
    smash3="$SMASH3_BIN"
    echo "verify_corpus_gate: using SMASH3_BIN=$smash3"
else
    echo "verify_corpus_gate: building smash3 (subprocess solver)..."
    cargo build --release -q --manifest-path "$workspace/Cargo.toml" \
        -p adl-cli --no-default-features
    smash3="$workspace/target/release/smash3"
fi
if [[ ! -x "$smash3" ]]; then
    echo "verify_corpus_gate: binary not executable: $smash3" >&2
    exit 1
fi

# Native-linked builds need libz3.so; subprocess builds ignore this.
if [[ -d /tmp/z3lib ]]; then
    export LD_LIBRARY_PATH="/tmp/z3lib:${LD_LIBRARY_PATH:-}"
fi

if [[ -n "${VERIFY_CORPUS_OUT:-}" ]]; then
    OUT="$VERIFY_CORPUS_OUT"
    mkdir -p "$OUT"
else
    OUT="$(mktemp -d "${TMPDIR:-/tmp}/verify_corpus_gate.XXXXXX")"
    trap 'rm -rf "$OUT"' EXIT
fi
echo "verify_corpus_gate: writing per-file output under $OUT"

fail=0
i=0
timeout_secs="${VERIFY_FILE_TIMEOUT:-180}"
# shellcheck disable=SC2206
extra_args=( ${SMASH3_VERIFY_ARGS:-} )
for f in "${files[@]}"; do
    i=$((i + 1))
    # Stable, filesystem-safe name mirroring the corpus-sweep skill.
    name=$(echo "$f" | sed "s|^$repo_root/||; s|/|_|g")
    set +e
    if command -v timeout >/dev/null 2>&1; then
        timeout --signal=KILL "${timeout_secs}s" \
            "$smash3" verify "${extra_args[@]}" "$f" >"$OUT/$name.out" 2>"$OUT/$name.err"
        rc=$?
    else
        "$smash3" verify "${extra_args[@]}" "$f" >"$OUT/$name.out" 2>"$OUT/$name.err"
        rc=$?
    fi
    set -e
    if [[ $rc -eq 137 || $rc -eq 124 ]]; then
        echo "TIMEOUT(${timeout_secs}s): $f" >&2
        fail=$((fail + 1))
    elif [[ $rc -ne 0 ]]; then
        echo "NONZERO($rc): $f" >&2
        fail=$((fail + 1))
    fi
    if (( i % 5 == 0 || i == count )); then
        echo "verify_corpus_gate: progress $i/$count" >&2
    fi
done

if [[ "$fail" -ne 0 ]]; then
    echo "verify_corpus_gate: FAIL — $fail file(s) with nonzero exit (baseline: 0)" >&2
    exit 1
fi

# Aggregate like adl2-corpus-sweep.
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

if [[ "${unknown:-0}" -gt 0 ]]; then
    echo "verify_corpus_gate: FAIL — UNKNOWN=$unknown (must be 0)" >&2
    exit 1
fi

# Compare against committed baseline via a tiny Python helper (stdlib only).
python3 - "$baseline_json" "$pairs" "$proven_disjoint" "$proven_overlapping" \
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

# Soft informational diffs (do not fail the gate).
for key in ("pairs", "proven_overlapping", "candidate_overlapping", "possibly", "files"):
    b, c = int(base.get(key, -1)), cur[key]
    if b >= 0 and b != c:
        print(f"verify_corpus_gate: NOTE — {key} baseline={b} current={c}")

sys.exit(0 if ok else 1)
PY

echo "verify_corpus_gate: PASS — $count files exit 0; UNKNOWN=0; PROVEN DISJOINT ≤ baseline."
