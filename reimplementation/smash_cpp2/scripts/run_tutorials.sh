#!/usr/bin/env bash
# Compare smash_cpp2 run vs smash3 run on the two tutorial files.
# Stdout only.
set -euo pipefail
root="${root:-$(cd "$(dirname "$0")/../../.." && pwd)}"
cpp2="${cpp2:-$root/reimplementation/smash_cpp2/build/smash_cpp2}"
smash3="${smash3:-$root/reimplementation/smash3/target/release/smash3}"
events="${events:-$root/reimplementation/adl2/crates/adl-difftest/tests/fixtures/ex02_events.jsonl}"
if [[ ! -x "$cpp2" ]]; then
  echo "missing smash_cpp2: $cpp2" >&2
  exit 2
fi
if [[ ! -x "$smash3" ]]; then
  echo "missing smash3 oracle: $smash3" >&2
  exit 2
fi
if [[ ! -f "$events" ]]; then
  echo "missing events: $events" >&2
  exit 2
fi
ok=0
fail=0
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
for f in \
  "$root/examples/tutorials/ex01_selection.adl" \
  "$root/examples/tutorials/ex02_histograms.adl"
do
  rel="${f#"$root"/}"
  "$smash3" run "$f" "$events" >"$tmpdir/s3.out" 2>"$tmpdir/s3.err"
  "$cpp2" run "$f" "$events" >"$tmpdir/c2.out" 2>"$tmpdir/c2.err"
  if diff -q "$tmpdir/s3.out" "$tmpdir/c2.out" >/dev/null; then
    echo "OK   $rel"
    ok=$((ok + 1))
  else
    echo "FAIL $rel"
    diff -u "$tmpdir/s3.out" "$tmpdir/c2.out" | head -80
    fail=$((fail + 1))
  fi
done
echo "tutorial run: ok=$ok fail=$fail"
if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
