---
name: adl2-corpus-sweep
description: Runs the ADL2 `smash2 verify` analyzer over the entire example corpus (68 .adl files) and aggregates pairwise verdicts (PROVEN DISJOINT / PROVEN OVERLAPPING / POSSIBLY / UNKNOWN) to detect regressions. Use this whenever you change the verifier, encoder, axioms, witness realizer, or solver backend and want to confirm corpus-wide behavior, want the current verdict baseline, or want to check a change did not regress disjointness — even if the user only says "did my change break anything" or "re-run the examples". Also use it to capture before/after sweeps and diff them.
allowed-tools: Read Edit Write Bash Grep Glob
---

Sweep the whole example corpus through `smash2 verify`, aggregate the per-file verdict summaries into corpus totals, and diff a before/after pair to catch regressions. The load-bearing invariant: **PROVEN DISJOINT count must not rise** unless you intended it to.

## Prerequisites

Build `smash2` first (see the **adl2-build-test** skill):

```bash
cargo build --release -p adl-cli --no-default-features \
  --manifest-path reimplementation/adl2/Cargo.toml
```

Binary: `reimplementation/adl2/target/release/smash2`

The binary is **dynamically linked against libz3.so** and will not start without it. Export this for every invocation below:

```bash
export LD_LIBRARY_PATH=/tmp/z3lib:$LD_LIBRARY_PATH
```

(Symptom if you forget: `error while loading shared libraries: libz3.so: cannot open shared object file`.)

The corpus is `examples` — 68 `.adl` files across `ATLAS`, `CMS`, `cl_examples`, `Examples`, `tutorials`, `small_samples`, and `bad/` (intentionally malformed but still exits 0, resolving to 0 regions). List them:

```bash
find examples -name '*.adl' | sort
```

## Run the sweep

Captures stdout (`.out`) and stderr (`.err`) per file and flags any nonzero exit. Change `OUT` between runs (e.g. `before` / `after`).

```bash
export LD_LIBRARY_PATH=/tmp/z3lib:$LD_LIBRARY_PATH
B=reimplementation/adl2/target/release/smash2
OUT=/tmp/sweep_after
rm -rf "$OUT"; mkdir -p "$OUT"
fail=0
while IFS= read -r f; do
  name=$(echo "$f" | sed 's|/|_|g')
  "$B" verify "$f" >"$OUT/$name.out" 2>"$OUT/$name.err"
  rc=$?
  [ $rc -ne 0 ] && { echo "NONZERO($rc): $f"; fail=$((fail+1)); }
done < <(find examples -name '*.adl' | sort)
echo "files with nonzero exit: $fail   (baseline: 0)"
```

Each `.out` ends with a line like:

```
summary: 21 pairs — 0 proven disjoint, 21 proven overlapping, 0 possibly overlapping, 0 unknown
```

For solver backend / region / pair counts per file, add `--verbose` (writes to stderr):

```
ATLASEXOT1704.0384_Delphes.adl: solver=z3-native; regions=7; pairs=21
```

## Aggregate verdicts

Sum every per-file `summary:` line into corpus totals:

```bash
OUT=/tmp/sweep_after
grep -h '^summary:' "$OUT"/*.out \
| grep -oE '[0-9]+ proven disjoint|[0-9]+ proven overlapping|[0-9]+ possibly overlapping|[0-9]+ unknown|[0-9]+ pairs?' \
| awk '
  /pair/                 {pairs+=$1}
  /proven disjoint/      {dis+=$1}
  /proven overlapping/   {ov+=$1}
  /possibly overlapping/ {pos+=$1}
  /unknown/              {unk+=$1}
  END{printf "pairs=%d disjoint=%d proven_ov=%d possibly=%d unknown=%d\n",pairs,dis,ov,pos,unk}'
```

Count files emitting an internal-bug section (header is exactly `== INTERNAL DIAGNOSTICS (bugs, please report) ==`):

```bash
grep -l 'INTERNAL DIAGNOSTICS' /tmp/sweep_after/*.out | wc -l
```

Sum total regions across the corpus (needs `--verbose`):

```bash
export LD_LIBRARY_PATH=/tmp/z3lib:$LD_LIBRARY_PATH
B=reimplementation/adl2/target/release/smash2
total=0
while IFS= read -r f; do
  r=$("$B" verify --verbose "$f" 2>&1 >/dev/null | grep -oE 'regions=[0-9]+' | grep -oE '[0-9]+')
  [ -n "$r" ] && total=$((total+r))
done < <(find examples -name '*.adl' | sort)
echo "total regions = $total   (baseline: 309)"
```

## Check for regressions

Run the sweep into `/tmp/sweep_before` (on the unchanged code), make your change, rebuild, sweep into `/tmp/sweep_after`, then diff per file:

```bash
diff -rq /tmp/sweep_before /tmp/sweep_after          # which files changed at all
for f in /tmp/sweep_before/*.out; do
  b=$(basename "$f")
  diff -u "$f" "/tmp/sweep_after/$b" && continue
  echo "^^^ changed: $b"
done | less
```

Compare just the verdict bottom line per file:

```bash
diff <(grep -H '^summary:' /tmp/sweep_before/*.out | sed 's|.*/||') \
     <(grep -H '^summary:' /tmp/sweep_after/*.out  | sed 's|.*/||')
```

**Invariants (see adl2-soundness for what they mean):**

- **Every file exits 0** (68/68), including `bad/`. A new nonzero exit is a regression.
- **PROVEN DISJOINT count must not rise.** Disjointness is the strong claim; a spurious increase means the prover is now asserting separations it cannot justify — a soundness regression. It should normally be *unchanged*. A decrease may be a correct tightening (note it).
- **PROVEN OVERLAPPING should not rise spuriously.** The witness fix this session deliberately downgraded 82 false PROVEN-OVERLAPPING candidates to POSSIBLY; do not silently reintroduce them.
- **UNKNOWN should stay 0.** A new UNKNOWN means the solver gave up where it previously decided.
- **No NEW `INTERNAL DIAGNOSTICS` file.** Baseline is 13 pre-existing (witness-realizer pT-ordering, unresolved identifiers, OPEN-1 angular ambiguity). A file that newly grows this section because of your change warrants investigation; list the newcomers:

```bash
comm -13 \
  <(grep -l 'INTERNAL DIAGNOSTICS' /tmp/sweep_before/*.out | xargs -n1 basename | sort) \
  <(grep -l 'INTERNAL DIAGNOSTICS' /tmp/sweep_after/*.out  | xargs -n1 basename | sort)
```

## Baseline numbers

Post the soundness fixes this session (native z3 backend, `solver=z3-native`). Treat as a diff target, not gospel — re-derive after any deliberate change:

| metric | value |
|---|---|
| files | 68 |
| files exit 0 | 68 / 68 |
| total regions | 309 |
| total pairs | 1832 |
| PROVEN DISJOINT | 866 |
| PROVEN OVERLAPPING | 265 |
| POSSIBLY | 701 |
| UNKNOWN | 0 |
| files with INTERNAL DIAGNOSTICS | 13 |

(The witness fix downgraded 82 false PROVEN-OVERLAPPING to POSSIBLY; overlapping was ~348 before it.)

## Cross-references

- **adl2-build-test** — how to build `smash2` and run the unit/integration tests.
- **adl2-soundness** — what PROVEN DISJOINT / OVERLAPPING / POSSIBLY / UNKNOWN mean and why the disjointness invariant matters.
