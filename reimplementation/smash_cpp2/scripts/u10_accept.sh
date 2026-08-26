#!/usr/bin/env bash
# U10 acceptance: smash_cpp2 ingest + --histos vs smash3 on real binaries.
set -euo pipefail
root="${root:-$(cd "$(dirname "$0")/../../.." && pwd)}"
cpp2="${cpp2:-/tmp/smash-cpp2-u10-build/smash_cpp2}"
smash3="${smash3:-$root/reimplementation/smash3/target/release/smash3}"
fix="${fix:-$root/reimplementation/adl2/crates/adl-ingest/fixtures}"
events="${events:-$root/reimplementation/adl2/crates/adl-difftest/tests/fixtures/ex02_events.jsonl}"
adl="${adl:-$root/examples/tutorials/ex02_histograms.adl}"
work="${work:-/tmp/u10-accept}"
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

"$cpp2" ingest --profile delphes -o "$work/c2.jsonl" "$fix/delphes_mini.root" \
  >"$work/c2.ingest.out" 2>"$work/c2.ingest.err" || true
"$smash3" ingest --profile delphes -o "$work/s3.jsonl" "$fix/delphes_mini.root" \
  >"$work/s3.ingest.out" 2>"$work/s3.ingest.err" || true

if cmp -s "$work/c2.jsonl" "$fix/delphes_mini.expected.jsonl"; then
  note "ingest vs expected.jsonl: IDENTICAL"
else
  bad "ingest vs expected.jsonl"
  cmp "$work/c2.jsonl" "$fix/delphes_mini.expected.jsonl" || true
fi
if cmp -s "$work/c2.jsonl" "$work/s3.jsonl"; then
  note "ingest vs smash3: IDENTICAL"
else
  bad "ingest vs smash3"
  cmp "$work/c2.jsonl" "$work/s3.jsonl" || true
fi

set +e
"$cpp2" ingest --profile delphes -o "$work/bad.jsonl" "$fix/delphes_badorder.root" \
  >"$work/c2.bad.out" 2>"$work/c2.bad.err"
c2_bad=$?
"$smash3" ingest --profile delphes -o "$work/s3-bad.jsonl" "$fix/delphes_badorder.root" \
  >"$work/s3.bad.out" 2>"$work/s3.bad.err"
s3_bad=$?
"$cpp2" ingest --profile delphes -o "$work/nan.jsonl" "$fix/delphes_nan.root" \
  >"$work/c2.nan.out" 2>"$work/c2.nan.err"
c2_nan=$?
"$smash3" ingest --profile delphes -o "$work/s3-nan.jsonl" "$fix/delphes_nan.root" \
  >"$work/s3.nan.out" 2>"$work/s3.nan.err"
s3_nan=$?
set -e

if [[ "$c2_bad" -eq 1 && "$s3_bad" -eq 1 ]]; then
  note "badorder: both exit 1"
else
  bad "badorder exits c2=$c2_bad s3=$s3_bad"
fi
if grep -q 'not pT-descending' "$work/c2.bad.err"; then
  note "badorder: c2 stderr has not pT-descending"
else
  bad "badorder stderr missing refusal"
  cat "$work/c2.bad.err"
fi
if [[ "$c2_nan" -eq 1 && "$s3_nan" -eq 1 ]]; then
  note "nan: both exit 1"
else
  bad "nan exits c2=$c2_nan s3=$s3_nan"
fi
if grep -q 'non-finite' "$work/c2.nan.err"; then
  note "nan: c2 stderr has non-finite"
else
  bad "nan stderr missing refusal"
  cat "$work/c2.nan.err"
fi

mkdir -p "$work/c2-h" "$work/s3-h" "$work/c2-nr"
"$cpp2" run --histos "$work/c2-h" "$adl" "$events" >"$work/c2.run.out" 2>"$work/c2.run.err"
"$smash3" run --histos "$work/s3-h" "$adl" "$events" >"$work/s3.run.out" 2>"$work/s3.run.err"
"$cpp2" run --histos "$work/c2-nr" --no-root "$adl" "$events" >/dev/null

set +e
python3 - "$work/c2-h/histos.json" "$work/s3-h/histos.json" histos.json <<'PY'
import pathlib, sys
a, b, label = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
ta = a.read_text().replace("smash_cpp2 0.1.0", "smash3 0.1.0")
tb = b.read_text()
if ta == tb:
    print(f"{label} after tool-string subst: IDENTICAL")
    raise SystemExit(0)
print(f"FAIL {label} after tool-string subst")
import difflib
for line in difflib.unified_diff(tb.splitlines(), ta.splitlines(), fromfile="smash3", tofile="smash_cpp2", lineterm=""):
    print(line)
raise SystemExit(1)
PY
h_histos=$?
python3 - "$work/c2-h/cutflow.json" "$work/s3-h/cutflow.json" cutflow.json <<'PY'
import pathlib, sys
a, b, label = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), sys.argv[3]
ta = a.read_text().replace("smash_cpp2 0.1.0", "smash3 0.1.0")
tb = b.read_text()
if ta == tb:
    print(f"{label} after tool-string subst: IDENTICAL")
    raise SystemExit(0)
print(f"FAIL {label} after tool-string subst")
import difflib
for line in difflib.unified_diff(tb.splitlines(), ta.splitlines(), fromfile="smash3", tofile="smash_cpp2", lineterm=""):
    print(line)
raise SystemExit(1)
PY
h_cut=$?
set -e
if [[ "$h_histos" -ne 0 ]]; then fail=1; fi
if [[ "$h_cut" -ne 0 ]]; then fail=1; fi

if [[ -f "$work/c2-h/out.root" ]] && grep -a -q smash2_provenance "$work/c2-h/out.root"; then
  note "out.root exists and contains smash2_provenance"
else
  bad "out.root missing smash2_provenance"
fi
if [[ -f "$work/c2-nr/histos.json" && -f "$work/c2-nr/cutflow.json" && ! -e "$work/c2-nr/out.root" ]]; then
  note "--no-root: histos.json+cutflow.json present, out.root absent"
else
  bad "--no-root layout"
  ls -la "$work/c2-nr"
fi

if "$cpp2" --help | awk '/^Commands:/{p=1;next} p&&NF{print $1; exit}' | grep -qx run; then
  note "--help lists run first"
else
  bad "--help run not first"
  "$cpp2" --help | head -20
fi

if ldd "$cpp2" | grep -E 'libz3|libCore|libRIO' >/dev/null; then
  bad "ldd has libz3 or CERN ROOT"
  ldd "$cpp2"
else
  note "ldd: no libz3, no libCore/libRIO"
fi

if [[ "$fail" -ne 0 ]]; then
  echo "u10_accept: FAIL"
  exit 1
fi
echo "u10_accept: PASS"
