#!/usr/bin/env bash
# Compare smash_cpp2 check --dump-ast vs smash3 on examples/tutorials/*.adl.
# Stdout only. Usage:
#   smash3=PATH cpp2=PATH [root=PATH] scripts/dump_ast_tutorials.sh
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
shopt -s nullglob
files=("$root"/examples/tutorials/*.adl)
if [[ ${#files[@]} -eq 0 ]]; then
  echo "no tutorial files under $root/examples/tutorials" >&2
  exit 2
fi
for f in "${files[@]}"; do
  name=$(basename "$f")
  if diff -q <("$smash3" check --dump-ast "$f") <("$cpp2" check --dump-ast "$f") >/dev/null; then
    echo "ok  $name"
    ok=$((ok + 1))
  else
    echo "FAIL $name"
    fail=$((fail + 1))
  fi
done
echo "tutorials dump-ast: ok=$ok fail=$fail total=$((ok + fail))"
if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
