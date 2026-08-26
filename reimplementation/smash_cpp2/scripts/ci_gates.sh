#!/usr/bin/env bash
# Run the smash_cpp2 acceptance gates against a smash3 oracle.
# Includes the 146-file verify_corpus_gate (PROVEN DISJOINT must not rise).
# Requires cpp2 and smash3 env vars pointing at real binaries.
set -euo pipefail

scripts="$(cd "$(dirname "$0")" && pwd)"
root="${root:-$(cd "$scripts/../../.." && pwd)}"
export root

if [[ -z "${cpp2:-}" ]]; then
  echo "missing cpp2 env var" >&2
  exit 2
fi
if [[ ! -x "$cpp2" ]]; then
  echo "missing smash_cpp2: $cpp2" >&2
  exit 2
fi
if [[ -z "${smash3:-}" ]]; then
  echo "missing smash3 env var" >&2
  exit 2
fi
if [[ ! -x "$smash3" ]]; then
  echo "missing smash3 oracle: $smash3" >&2
  exit 2
fi
export cpp2 smash3

fail=0

gate() {
  local name="$1"
  shift
  echo "=== $name ==="
  if "$@"; then
    echo "ok   $name"
  else
    echo "fail $name"
    fail=1
  fi
}

check_corpus_count() {
  local files
  mapfile -t files < <(find "$root/examples" -name '*.adl' | sort)
  if [[ ${#files[@]} -ne 146 ]]; then
    echo "error: expected 146 corpus files, found ${#files[@]}" >&2
    return 1
  fi
  echo "corpus files: 146"
}

pin_verify_summary() {
  local pins=(
    "$root/examples/tutorials/ex01_selection.adl"
    "$root/examples/golden/disjoint_01.adl"
    "$root/examples/golden/empty_01.adl"
    "$root/examples/golden/overlap_01.adl"
    "$root/examples/golden/presence_01_complementary_rejects.adl"
  )
  local work fail_pin=0 f rel
  work="$(mktemp -d)"
  for f in "${pins[@]}"; do
    rel="${f#"$root"/}"
    "$cpp2" verify "$f" >"$work/c2.out" 2>"$work/c2.err"
    "$smash3" verify "$f" >"$work/s3.out" 2>"$work/s3.err"
    grep '^summary:' "$work/c2.out" >"$work/c2.sum" || true
    grep '^summary:' "$work/s3.out" >"$work/s3.sum" || true
    if [[ ! -s "$work/c2.sum" || ! -s "$work/s3.sum" ]]; then
      echo "FAIL $rel missing summary:"
      echo "cpp2:"; cat "$work/c2.out"
      echo "smash3:"; cat "$work/s3.out"
      fail_pin=1
      continue
    fi
    if cmp -s "$work/c2.sum" "$work/s3.sum"; then
      echo "ok   $rel $(cat "$work/c2.sum")"
    else
      echo "FAIL $rel summary mismatch"
      diff -u "$work/s3.sum" "$work/c2.sum" || true
      fail_pin=1
    fi
  done
  rm -rf "$work"
  [[ "$fail_pin" -eq 0 ]]
}

help_run_first() {
  "$cpp2" --help | awk '/^Commands:/{p=1;next} p&&NF{print $1; exit}' | grep -qx run
}

gate corpus-count check_corpus_count
gate dump-ast bash "$scripts/dump_ast_corpus.sh"
gate dump-hir bash "$scripts/dump_hir_corpus.sh"
gate dump-formula bash "$scripts/dump_formula_corpus.sh"
gate dump-axioms bash "$scripts/dump_axioms_corpus.sh"
gate compare-objects bash "$scripts/compare_stdout.sh" --corpus objects
gate compare-dot bash "$scripts/compare_stdout.sh" --corpus dot
gate compare-dot-ast bash "$scripts/compare_stdout.sh" --corpus dot-ast
gate run-tutorials bash "$scripts/run_tutorials.sh"
gate verify-summary pin_verify_summary
gate verify-corpus bash "$scripts/verify_corpus_gate.sh"
gate u10-accept bash "$scripts/u10_accept.sh"
gate u11-accept bash "$scripts/u11_accept.sh"
gate u13-accept bash "$scripts/u13_accept.sh"
gate help-run-first help_run_first

if [[ "$fail" -ne 0 ]]; then
  echo "ci_gates: FAIL"
  exit 1
fi
echo "ci_gates: PASS"
