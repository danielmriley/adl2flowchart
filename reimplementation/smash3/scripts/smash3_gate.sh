#!/usr/bin/env bash
# Prove smash3 can parse, run, and verify a real tutorial file.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace="$(cd "$here/.." && pwd)"
repo_root="$(cd "$workspace/../.." && pwd)"
ex="$repo_root/examples/tutorials/ex01_selection.adl"
events="$workspace/crates/adl-difftest/tests/fixtures/ex02_events.jsonl"

echo "smash3_gate: building"
cargo build -q --manifest-path "$workspace/Cargo.toml" -p adl-cli
bin="$workspace/target/debug/smash3"
if [[ ! -x "$bin" ]]; then
  echo "smash3_gate: missing $bin" >&2
  exit 1
fi

help="$("$bin" --help)"
if ! printf '%s\n' "$help" | awk '/^Commands:/{p=1;next} p&&NF{print; exit}' | grep -q '^  run'; then
  echo "smash3_gate: first listed subcommand is not run" >&2
  printf '%s\n' "$help" | sed -n '/^Commands:/,/^$/p' >&2
  exit 1
fi

"$bin" check "$ex"
"$bin" objects "$ex" >/tmp/smash3_objects.txt
"$bin" dot "$ex" >/tmp/smash3_dot.txt
"$bin" run "$repo_root/examples/tutorials/ex02_histograms.adl" "$events" >/tmp/smash3_run.txt
"$bin" verify --no-refute-gate "$ex" >/tmp/smash3_verify.txt

if ! grep -q 'PROVEN\|POSSIBLY\|OVERLAP\|DISJOINT\|EMPTY' /tmp/smash3_verify.txt; then
  echo "smash3_gate: verify produced no verdict text" >&2
  head -40 /tmp/smash3_verify.txt >&2
  exit 1
fi

echo "smash3_gate: check + run + verify + objects + dot ok"
