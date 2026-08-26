#!/usr/bin/env bash
# Stretch: compare smash_cpp2 --dump-hir/--dump-quantities vs smash3
# on every examples/**/*.adl. Stdout only. Count ok/fail; do not invent
# identity rules if a file drifts.
set -euo pipefail
root="${root:-$(cd "$(dirname "$0")/../../.." && pwd)}"
cpp2="${cpp2:-$root/reimplementation/smash_cpp2/build/smash_cpp2}"
smash3="${smash3:-$root/reimplementation/smash3/target/release/smash3}"
ok=0
fail=0
if [[ ! -x "$cpp2" ]]; then
  echo "missing smash_cpp2: $cpp2" >&2
  exit 2
fi
if [[ ! -x "$smash3" ]]; then
  echo "missing smash3 oracle: $smash3" >&2
  exit 2
fi
mapfile -t files < <(find "$root/examples" -name '*.adl' | sort)
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
diff_one() {
  local flag=$1
  local f=$2
  local rel="${f#"$root"/}"
  "$smash3" check "$flag" "$f" >"$tmpdir/s3.out" 2>"$tmpdir/s3.err" || true
  "$cpp2" check "$flag" "$f" >"$tmpdir/c2.out" 2>"$tmpdir/c2.err" || true
  if diff -q "$tmpdir/s3.out" "$tmpdir/c2.out" >/dev/null; then
    ok=$((ok + 1))
  else
    echo "FAIL $rel $flag"
    fail=$((fail + 1))
  fi
}
for f in "${files[@]}"; do
  diff_one --dump-hir "$f"
  diff_one --dump-quantities "$f"
done
echo "corpus dump-hir/quantities: ok=$ok fail=$fail total=$((ok + fail))"
if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
