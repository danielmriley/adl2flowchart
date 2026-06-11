#!/usr/bin/env bash
# corpus_gate.sh — corpus parse+resolve gate (PLAN Phase 1 / Phase 6).
#
# Every ADL file in examples/ must parse AND resolve with zero error-severity
# diagnostics through `smash2 check` (adl-syntax parser + adl-sema resolver).
# `check` keeps stdout machine-clean and prints diagnostics to stderr, so a
# clean run is silent and exits 0; any error-severity diagnostic exits 1.
#
# Usage:  scripts/corpus_gate.sh
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
workspace="$(cd "$here/.." && pwd)"
examples="$(cd "$workspace/../../examples" && pwd)"

mapfile -t files < <(find "$examples" -name '*.adl' | sort)
count="${#files[@]}"
echo "corpus_gate: found $count ADL files under $examples"

expected=68
if [[ "$count" -ne "$expected" ]]; then
    echo "corpus_gate: WARNING expected $expected corpus files, found $count" >&2
fi

echo "corpus_gate: building smash2..."
cargo build -q --manifest-path "$workspace/Cargo.toml" -p adl-cli
smash2="$workspace/target/debug/smash2"

# `check` takes every file in one invocation and exits 1 if ANY file has
# error-severity diagnostics; per-file failures are named on stderr.
if ! "$smash2" check "${files[@]}"; then
    echo "corpus_gate: one or more files failed to parse/resolve" >&2
    exit 1
fi
echo "corpus_gate: all $count files parsed and resolved clean."
