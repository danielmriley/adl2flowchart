#!/usr/bin/env bash
# U11: smash_cpp2 verify --cross / --json and run --json vs smash3.
set -euo pipefail
root="${root:-$(cd "$(dirname "$0")/../../.." && pwd)}"
cpp2="${cpp2:-/tmp/smash-cpp2-u11-build/smash_cpp2}"
smash3="${smash3:-$root/reimplementation/smash3/target/release/smash3}"
events="${events:-$root/reimplementation/adl2/crates/adl-difftest/tests/fixtures/ex02_events.jsonl}"
work="${work:-/tmp/u11-accept}"
rm -rf "$work"
mkdir -p "$work"

if [[ ! -x "$cpp2" ]]; then
  echo "missing smash_cpp2: $cpp2" >&2
  exit 2
fi
if [[ ! -x "$smash3" ]]; then
  echo "missing smash3: $smash3" >&2
  exit 2
fi

fail=0
note() { echo "$*"; }
bad() { echo "FAIL $*"; fail=1; }

extract_cross() {
  grep -E '^(summary:|  cross-file:)' "$1" || true
}

kinds() {
  python3 -c '
import json, sys
d = json.load(sys.stdin)
print(",".join(p["kind"] for p in d.get("pairwise", [])))
'
}

cross_dirs=(
  abs-refine
  candidate-overlap
  opaque-blocked
  opaque-cut-collision
  refine-disjoint
  xeq-equivalent
)

for d in "${cross_dirs[@]}"; do
  dir="$root/examples/golden/cross/$d"
  "$cpp2" verify --cross "$dir/a.adl" "$dir/b.adl" >"$work/c2.$d.out" 2>"$work/c2.$d.err"
  "$smash3" verify --cross "$dir/a.adl" "$dir/b.adl" >"$work/s3.$d.out" 2>"$work/s3.$d.err"
  extract_cross "$work/c2.$d.out" >"$work/c2.$d.lines"
  extract_cross "$work/s3.$d.out" >"$work/s3.$d.lines"
  echo "=== $d smash_cpp2 ==="
  cat "$work/c2.$d.lines"
  echo "=== $d smash3 ==="
  cat "$work/s3.$d.lines"
  if cmp -s "$work/c2.$d.lines" "$work/s3.$d.lines"; then
    note "cross $d: summary + cross-file MATCH"
  else
    bad "cross $d summary/cross-file mismatch"
    diff -u "$work/s3.$d.lines" "$work/c2.$d.lines" || true
  fi
done

pins=(
  "$root/examples/tutorials/ex01_selection.adl"
  "$root/examples/golden/disjoint_01.adl"
  "$root/examples/golden/empty_01.adl"
  "$root/examples/golden/overlap_01.adl"
  "$root/examples/golden/presence_01_complementary_rejects.adl"
)

for f in "${pins[@]}"; do
  rel="${f#"$root"/}"
  base="$(basename "$f" .adl)"
  "$cpp2" verify --json "$f" >"$work/c2.$base.json" 2>"$work/c2.$base.jerr"
  "$smash3" verify --json "$f" >"$work/s3.$base.json" 2>"$work/s3.$base.jerr"
  ck="$(kinds <"$work/c2.$base.json")"
  sk="$(kinds <"$work/s3.$base.json")"
  echo "json kinds $rel cpp2=[$ck] smash3=[$sk]"
  if [[ "$ck" == "$sk" ]]; then
    note "verify --json $rel kinds MATCH"
  else
    bad "verify --json $rel kinds mismatch"
  fi
done

adl="$root/examples/tutorials/ex01_selection.adl"
"$cpp2" run --json "$adl" "$events" >"$work/c2.run.jsonl" 2>"$work/c2.run.err"
"$smash3" run --json "$adl" "$events" >"$work/s3.run.jsonl" 2>"$work/s3.run.err"
set +e
python3 - "$work/c2.run.jsonl" "$work/s3.run.jsonl" <<'PY'
import pathlib, sys
a = pathlib.Path(sys.argv[1]).read_text().replace("smash_cpp2 0.1.0", "smash3 0.1.0")
b = pathlib.Path(sys.argv[2]).read_text()
if a == b:
    print("run --json ex01 after tool-string subst: IDENTICAL")
    raise SystemExit(0)
print("FAIL run --json ex01 after tool-string subst")
import difflib
for line in difflib.unified_diff(b.splitlines(), a.splitlines(), fromfile="smash3", tofile="smash_cpp2", lineterm=""):
    print(line)
raise SystemExit(1)
PY
if [[ $? -ne 0 ]]; then fail=1; fi
set -e

if "$cpp2" --help | awk '/^Commands:/{p=1;next} p&&NF{print $1; exit}' | grep -qx run; then
  note "--help lists run first"
else
  bad "--help run not first"
fi

if ldd "$cpp2" | grep -E 'libz3|libCore|libRIO' >/dev/null; then
  bad "ldd has libz3 or CERN ROOT"
else
  note "ldd: no libz3, no libCore/libRIO"
fi

if [[ "$fail" -ne 0 ]]; then
  echo "u11_accept: FAIL"
  exit 1
fi
echo "u11_accept: PASS"
