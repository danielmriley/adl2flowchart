#!/usr/bin/env bash
# Compare smash_cpp2 check --dump-ast vs smash3 on every examples/**/*.adl.
# Stdout only. Expected count: 146.
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
expected=146
if [[ ${#files[@]} -ne "$expected" ]]; then
  echo "error: expected $expected corpus files, found ${#files[@]}" >&2
  exit 1
fi
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
for f in "${files[@]}"; do
  rel="${f#"$root"/}"
  "$smash3" check --dump-ast "$f" >"$tmpdir/s3.out" 2>"$tmpdir/s3.err"
  "$cpp2" check --dump-ast "$f" >"$tmpdir/c2.out" 2>"$tmpdir/c2.err"
  if diff -q "$tmpdir/s3.out" "$tmpdir/c2.out" >/dev/null; then
    ok=$((ok + 1))
  else
    echo "FAIL $rel"
    fail=$((fail + 1))
  fi
done
echo "corpus dump-ast: ok=$ok fail=$fail total=$((ok + fail))"
if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
