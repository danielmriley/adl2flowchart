# Shared setup for C++ corpus gates. Sourced, not executed.
# smash2_cpp stands alone. Rust smash2 is an optional CROSS_ORACLE=1 check.
#
# Env:
#   CROSS_ORACLE=1  build/require smash2 and byte-diff against it
#   SKIP_BUILD=1    do not cmake/cargo
#   SMASH2_CPP / SMASH2_RUST  binary overrides
#
# After sourcing, call gate_prepare. Sets GATE_ORACLE=1 when the rust
# cross-check is on.

gate_prepare() {
  ROOT="${ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
  SMASH2_CPP="${SMASH2_CPP:-$ROOT/cpp/build/smash2_cpp}"
  SMASH2_RUST="${SMASH2_RUST:-$ROOT/reimplementation/adl2/target/release/smash2}"
  GATE_ORACLE=0
  if [[ "${CROSS_ORACLE:-0}" == "1" ]]; then
    GATE_ORACLE=1
  fi

  if [[ "${SKIP_BUILD:-0}" != "1" ]]; then
    if [[ -z "${CXX:-}" ]] && command -v g++ >/dev/null 2>&1; then
      export CXX=g++
    fi
    echo "==> building smash2_cpp"
    cmake -S "$ROOT/cpp" -B "$ROOT/cpp/build" -DCMAKE_BUILD_TYPE=Release
    cmake --build "$ROOT/cpp/build" -j"$(nproc)"
    if [[ "$GATE_ORACLE" == "1" ]]; then
      echo "==> building Rust smash2 (optional CROSS_ORACLE; no native z3)"
      (
        cd "$ROOT/reimplementation/adl2"
        cargo build --release -p adl-cli --no-default-features
      )
    fi
  fi

  test -x "$SMASH2_CPP" || { echo "missing smash2_cpp at $SMASH2_CPP" >&2; exit 2; }
  if [[ "$GATE_ORACLE" == "1" ]]; then
    test -x "$SMASH2_RUST" || { echo "missing smash2 at $SMASH2_RUST (CROSS_ORACLE=1)" >&2; exit 2; }
  fi
}
