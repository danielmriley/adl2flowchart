#!/usr/bin/env bash
set -euo pipefail
root="${root:-$(cd "$(dirname "$0")/../../.." && pwd)}"
cpp2="${cpp2:-$root/reimplementation/smash_cpp2/build/smash_cpp2}"
smash3="${smash3:-$root/reimplementation/smash3/target/release/smash3}"
timeout_secs="${VERIFY_FILE_TIMEOUT:-180}"

if [[ ! -x "$cpp2" ]]; then
  echo "missing smash_cpp2: $cpp2" >&2
  exit 2
fi
if [[ ! -x "$smash3" ]]; then
  echo "missing smash3 oracle: $smash3" >&2
  exit 2
fi

mapfile -t files < <(find "$root/examples" -name '*.adl' | sort)
if [[ ${#files[@]} -ne 146 ]]; then
  echo "error: expected 146 corpus files, found ${#files[@]}" >&2
  exit 1
fi

export root cpp2 smash3 timeout_secs
python3 - "$root" "$cpp2" "$smash3" "$timeout_secs" "${files[@]}" <<'PY'
import re, subprocess, sys

root, cpp2, smash3, timeout_s = sys.argv[1:5]
files = sys.argv[5:]
timeout = int(timeout_s)
sum_re = re.compile(r"^summary:.*$", re.M)
pd_re = re.compile(r"(\d+) proven disjoint")
unk_re = re.compile(r"(\d+) unknown")
pairs_re = re.compile(r"(\d+) pairs?")
po_re = re.compile(r"(\d+) proven overlapping")
cand_re = re.compile(r"(\d+) candidate overlapping")
pos_re = re.compile(r"(\d+) possibly overlapping")

def run(bin, f):
    p = subprocess.run([bin, "verify", f], capture_output=True, text=True, timeout=timeout)
    return p.returncode, p.stdout

def parse(stdout):
    m = sum_re.search(stdout)
    line = m.group(0) if m else ""
    def g(rx):
        mm = rx.search(line)
        return int(mm.group(1)) if mm else 0
    return line, {
        "pairs": g(pairs_re),
        "pd": g(pd_re),
        "po": g(po_re),
        "cand": g(cand_re),
        "pos": g(pos_re),
        "unk": g(unk_re),
    }

agg_s3 = {k: 0 for k in ("pairs", "pd", "po", "cand", "pos", "unk")}
agg_c2 = {k: 0 for k in ("pairs", "pd", "po", "cand", "pos", "unk")}
fail = 0
for i, f in enumerate(files, 1):
    rel = f[len(root) + 1 :] if f.startswith(root) else f
    try:
        rc3, o3 = run(smash3, f)
        rc2, o2 = run(cpp2, f)
    except subprocess.TimeoutExpired:
        print(f"TIMEOUT {rel}", flush=True)
        fail += 1
        continue
    if rc3 != 0 or rc2 != 0:
        print(f"NONZERO {rel} smash3={rc3} cpp2={rc2}", flush=True)
        fail += 1
    l3, a3 = parse(o3)
    l2, a2 = parse(o2)
    for k in agg_s3:
        agg_s3[k] += a3[k]
        agg_c2[k] += a2[k]
    if l3 != l2:
        print(f"MISMATCH {rel}", flush=True)
        print(f"  smash3: {l3}")
        print(f"  cpp2:   {l2}")
        fail += 1
    if i % 20 == 0 or i == len(files):
        print(f"verify_corpus_gate: progress {i}/{len(files)}", flush=True)

print(
    f"verify_corpus_gate: smash3 pairs={agg_s3['pairs']} pd={agg_s3['pd']} "
    f"po={agg_s3['po']} cand={agg_s3['cand']} pos={agg_s3['pos']} unk={agg_s3['unk']}"
)
print(
    f"verify_corpus_gate: cpp2   pairs={agg_c2['pairs']} pd={agg_c2['pd']} "
    f"po={agg_c2['po']} cand={agg_c2['cand']} pos={agg_c2['pos']} unk={agg_c2['unk']}"
)
if agg_c2["pd"] > agg_s3["pd"]:
    print(
        f"verify_corpus_gate: FAIL — PROVEN DISJOINT rose "
        f"{agg_s3['pd']} → {agg_c2['pd']}",
        file=sys.stderr,
    )
    sys.exit(1)
if agg_c2["unk"] or agg_s3["unk"]:
    print("verify_corpus_gate: FAIL — UNKNOWN > 0", file=sys.stderr)
    sys.exit(1)
if fail:
    print(f"verify_corpus_gate: FAIL — {fail} file(s)", file=sys.stderr)
    sys.exit(1)
pin = {"pairs": 1900, "pd": 813, "po": 76, "cand": 45, "pos": 966, "unk": 0}
if agg_c2 != pin or agg_s3 != pin:
    print(
        f"verify_corpus_gate: FAIL — aggregate != pin {pin} "
        f"cpp2={agg_c2} smash3={agg_s3}",
        file=sys.stderr,
    )
    sys.exit(1)
print(
    f"verify_corpus_gate: PASS — {len(files)} files; UNKNOWN=0; "
    f"PROVEN DISJOINT {agg_c2['pd']} = smash3 {agg_s3['pd']} = pin 813"
)
PY
