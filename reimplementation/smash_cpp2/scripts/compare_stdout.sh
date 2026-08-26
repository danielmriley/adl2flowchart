#!/usr/bin/env bash
# Compare smash_cpp2 stdout to smash3. First failing diff is printed.
# Usage: compare_stdout.sh [--corpus] objects|dot|dot-ast
set -euo pipefail
root="${root:-$(cd "$(dirname "$0")/../../.." && pwd)}"
cpp2="${cpp2:-$root/reimplementation/smash_cpp2/build/smash_cpp2}"
smash3="${smash3:-$root/reimplementation/smash3/target/release/smash3}"
corpus=0
if [[ "${1:-}" == "--corpus" ]]; then
  corpus=1
  shift
fi
kind="${1:-}"
case "$kind" in
  objects) args=(objects) ;;
  dot) args=(dot) ;;
  dot-ast) args=(dot --ast) ;;
  *)
    echo "usage: $0 [--corpus] objects|dot|dot-ast" >&2
    exit 2
    ;;
esac
if [[ ! -x "$cpp2" ]]; then
  echo "missing smash_cpp2: $cpp2" >&2
  exit 2
fi
if [[ ! -x "$smash3" ]]; then
  echo "missing smash3 oracle: $smash3" >&2
  exit 2
fi
if [[ "$corpus" -eq 1 ]]; then
  mapfile -t files < <(find "$root/examples" -name '*.adl' | sort)
  expected=146
  if [[ ${#files[@]} -ne "$expected" ]]; then
    echo "error: expected $expected corpus files, found ${#files[@]}" >&2
    exit 1
  fi
  label="corpus $kind"
else
  shopt -s nullglob
  files=("$root"/examples/tutorials/*.adl)
  if [[ ${#files[@]} -eq 0 ]]; then
    echo "no tutorial files under $root/examples/tutorials" >&2
    exit 2
  fi
  label="tutorials $kind"
fi
ok=0
fail=0
shown=0
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT
for f in "${files[@]}"; do
  rel="${f#"$root"/}"
  "$smash3" "${args[@]}" "$f" >"$tmpdir/s3.out" 2>"$tmpdir/s3.err" || true
  "$cpp2" "${args[@]}" "$f" >"$tmpdir/c2.out" 2>"$tmpdir/c2.err" || true
  if diff -q "$tmpdir/s3.out" "$tmpdir/c2.out" >/dev/null; then
    ok=$((ok + 1))
  else
    echo "FAIL $rel"
    if [[ "$shown" -eq 0 ]]; then
      diff -u "$tmpdir/s3.out" "$tmpdir/c2.out" | head -80
      shown=1
    fi
    fail=$((fail + 1))
  fi
done
echo "$label: ok=$ok fail=$fail total=$((ok + fail))"
if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
