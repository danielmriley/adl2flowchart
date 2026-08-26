#!/usr/bin/env bash
set -euo pipefail
root="${root:-$(cd "$(dirname "$0")/../../.." && pwd)}"
cpp2="${cpp2:-/tmp/smash-cpp2-u13-coord-build/smash_cpp2}"
smash3="${smash3:-$root/reimplementation/smash3/target/release/smash3}"
work="${work:-/tmp/u13-accept}"
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

ex01="$root/examples/tutorials/ex01_selection.adl"
ex07="$root/examples/tutorials/ex07_chi2optimize.adl"

set +e
"$cpp2" check --json "$ex01" >"$work/c2.ex01.json" 2>"$work/c2.ex01.err"
c2_ex01=$?
"$smash3" check --json "$ex01" >"$work/s3.ex01.json" 2>"$work/s3.ex01.err"
s3_ex01=$?
set -e
if [[ "$c2_ex01" -eq 0 && "$s3_ex01" -eq 0 ]] && cmp -s "$work/c2.ex01.json" "$work/s3.ex01.json"; then
  note "check --json ex01: IDENTICAL []"
else
  bad "check --json ex01 rc cpp2=$c2_ex01 smash3=$s3_ex01"
  echo "cpp2: $(cat "$work/c2.ex01.json")"
  echo "smash3: $(cat "$work/s3.ex01.json")"
fi

set +e
"$cpp2" check --json "$ex07" >"$work/c2.ex07.json" 2>"$work/c2.ex07.err"
c2_ex07=$?
"$smash3" check --json "$ex07" >"$work/s3.ex07.json" 2>"$work/s3.ex07.err"
s3_ex07=$?
set -e
if [[ "$c2_ex07" -eq 0 && "$s3_ex07" -eq 0 ]] && cmp -s "$work/c2.ex07.json" "$work/s3.ex07.json"; then
  note "check --json ex07: IDENTICAL (labels + ~=)"
else
  bad "check --json ex07"
  diff -u "$work/s3.ex07.json" "$work/c2.ex07.json" || true
fi

cms="$root/examples/CMS-SUS-21-006_TreeMaker2result.adl"
set +e
"$cpp2" check --json "$cms" >"$work/c2.cms.json" 2>"$work/c2.cms.err"
c2_cms=$?
"$smash3" check --json "$cms" >"$work/s3.cms.json" 2>"$work/s3.cms.err"
s3_cms=$?
set -e
if [[ "$c2_cms" -eq 0 && "$s3_cms" -eq 0 ]] && cmp -s "$work/c2.cms.json" "$work/s3.cms.json"; then
  note "check --json CMS-SUS-21-006_TreeMaker2result: IDENTICAL (path label)"
else
  bad "check --json CMS-SUS-21-006_TreeMaker2result"
  diff -u "$work/s3.cms.json" "$work/c2.cms.json" || true
fi

set +e
"$cpp2" check --json --dump-ast "$ex01" >/dev/null 2>"$work/c2.dump.err"
c2_dump=$?
"$smash3" check --json --dump-ast "$ex01" >/dev/null 2>"$work/s3.dump.err"
s3_dump=$?
set -e
if [[ "$c2_dump" -eq 2 && "$s3_dump" -eq 2 ]]; then
  note "check --json --dump-ast: both exit 2"
else
  bad "check --json --dump-ast rc cpp2=$c2_dump smash3=$s3_dump"
fi

set +e
"$cpp2" check --json /tmp/smash-cpp2-u13-missing.adl >/dev/null 2>"$work/c2.miss.err"
c2_miss=$?
"$smash3" check --json /tmp/smash-cpp2-u13-missing.adl >/dev/null 2>"$work/s3.miss.err"
s3_miss=$?
set -e
if [[ "$c2_miss" -eq 2 && "$s3_miss" -eq 2 ]]; then
  note "check --json missing file: both exit 2"
else
  bad "check --json missing rc cpp2=$c2_miss smash3=$s3_miss"
fi

cat >"$work/bad.adl" <<'ADL'
region SR
  select
ADL
set +e
"$cpp2" check --json "$work/bad.adl" >"$work/c2.bad.json" 2>"$work/c2.bad.err"
c2_bad=$?
"$smash3" check --json "$work/bad.adl" >"$work/s3.bad.json" 2>"$work/s3.bad.err"
s3_bad=$?
set -e
if [[ "$c2_bad" -eq 1 && "$s3_bad" -eq 1 ]]; then
  note "check --json syntax error: both exit 1"
else
  bad "check --json syntax error rc cpp2=$c2_bad smash3=$s3_bad"
fi
python3 - "$work/c2.bad.json" <<'PY'
import json, sys
rows = json.load(open(sys.argv[1]))
need = ["col", "end", "file", "help", "label", "line", "message", "severity", "start"]
if not rows:
    raise SystemExit("empty error array")
keys = list(rows[0].keys())
if keys != need:
    raise SystemExit(f"key order {keys} != {need}")
print("check --json error object: smash3 key order")
PY

if ! grep -q 'SMT-LIB subprocess' "$root/reimplementation/smash_cpp2/LANGUAGE.md"; then
  bad "LANGUAGE.md solver row does not say SMT-LIB subprocess"
else
  note "LANGUAGE.md solver: SMT-LIB subprocess"
fi
if grep -q 'Native libz3 is opt-in' "$root/reimplementation/smash_cpp2/LANGUAGE.md"; then
  bad "LANGUAGE.md still claims native libz3 is opt-in"
fi

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
  echo "u13_accept: FAIL"
  exit 1
fi
echo "u13_accept: PASS"
