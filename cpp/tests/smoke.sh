#!/usr/bin/env bash
# Manual smoke script (CI uses ctest). Run from repo root after building:
#   cmake -S cpp -B cpp/build && cmake --build cpp/build && cpp/tests/smoke.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="${ROOT}/cpp/build/smash2_cpp"
FIX="${ROOT}/cpp/tests/fixtures/tiny.adl"
test -x "$BIN"
"$BIN" --help | grep -q check
"$BIN" check "$FIX" | grep -q "check: ok"
bash "${ROOT}/cpp/tests/dump_parity.sh" "$BIN" "$FIX" "${FIX}.dump"
echo "smoke: ok"
