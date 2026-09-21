#!/usr/bin/env bash
# Compare smash3 check --dump-ast to smash2 on the example corpus.
# smash2 is the oracle. Byte identity is the gate.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
smash3_ws="$(cd "$here/.." && pwd)"
smash2_ws="$(cd "$smash3_ws/../adl2" && pwd)"
repo_root="$(cd "$smash3_ws/../.." && pwd)"
examples="$(cd "$repo_root/examples" && pwd)"

echo "parity_dump_ast: building smash3"
cargo build -q --manifest-path "$smash3_ws/Cargo.toml" -p adl-cli
s3="$smash3_ws/target/debug/smash3"

echo "parity_dump_ast: building smash2"
cargo build -q --manifest-path "$smash2_ws/Cargo.toml" -p adl-cli --no-default-features
s2="$smash2_ws/target/debug/smash2"

fail=0
ok=0
while IFS= read -r f; do
  d2="$("$s2" check --dump-ast "$f" 2>/dev/null || true)"
  d3="$("$s3" check --dump-ast "$f" 2>/dev/null || true)"
  if [[ "$d2" == "$d3" ]]; then
    ok=$((ok + 1))
  else
    fail=$((fail + 1))
    echo "DIFF $f" >&2
  fi
done < <(find "$examples" -name '*.adl' | sort)

echo "parity_dump_ast: ok=$ok fail=$fail"
if [[ "$fail" -ne 0 ]]; then
  exit 1
fi
