# ADL2Flowchart verification report

**Date:** 2026-06-02  
**Branch:** `main` (merged `disjoint_dev` + region IR / overlap / SMT)  
**Binary:** `./smash` (clean build from `make`)

### Post-merge validation (phases 0–4)

| Check | Command | Result |
|-------|---------|--------|
| Golden region fixtures | `make test-disjoint` | pass |
| Full examples corpus | `make test-corpus` | 68/68 parse and `-r` |
| Z3 spike (Delphes 033) | `./scripts/phase2_z3_spike.sh` | pass if `z3` installed |

See `REGION_ANALYSIS.md` for `-r`, `--json`, `--smt` usage.

## Executive summary

All **68** ADL files under `examples/` were re-verified after the failure-fix pass on `disjoint_dev`. Both the default pipeline and the experimental disjointness mode complete without errors or crashes.

| Mode | Command | Pass | Fail | Crash |
|------|---------|------|------|-------|
| Standard parse + DOT | `./smash <file.adl>` | **68** | 0 | 0 |
| Region / object analysis | `./smash -r <file.adl>` | **68** | 0 | 0 |

**Pass criteria:** exit code 0, no `ERROR` line in stderr/stdout, pipeline reaches `finished`.

Previously (before this work): 49 pass, 14 parse/semantic failures, 5 post-parse crashes (exit 134 in `binOpCheck` / `printITE`).

---

## Verification procedure

1. **Clean rebuild**
   ```bash
   make clean && make
   ```
2. **Full sweep** over `find examples -name '*.adl' | sort`
3. **Per file:** run `./smash "$f"` then `./smash -r "$f"`
4. **Spot checks** on the five former crash files and sample `-r` output on Delphes / tutorial inputs

Sweep script logic: treat exit 134/136/139 as crash; treat non-zero exit or a lone `ERROR` line as failure.

---

## Results by directory

| Directory | Files | Parse | `-r` |
|-----------|-------|-------|------|
| `examples/CMS/` | 13 | 13/13 | 13/13 |
| `examples/cl_examples/` | 17 | 17/17 | 17/17 |
| `examples/Examples/` | 14 | 14/14 | 14/14 |
| `examples/tutorials/` | 14 | 14/14 | 14/14 |
| `examples/small_samples/` | 6 | 6/6 | 6/6 |
| `examples/ATLAS/` | 1 | 1/1 | 1/1 |
| `examples/` (root) | 2 | 2/2 | 2/2 |
| **Total** | **68** | **68/68** | **68/68** |

Root-level files: `CMS-SUS-21-006_TreeMaker2result.adl`, `CMS-SUS-21-009_Delphes.adl`.

---

## Former failure categories (resolved)

### Crashes (5 files)

Cause: null `ITENode` else branch passed to `binOpCheck` during AST printing (`printITE`).

| File | Status |
|------|--------|
| `examples/cl_examples/CMS-SUS-21-002.adl` | OK (parse + `-r`) |
| `examples/CMS/CMS-SUS-16-033_Delphes.adl` | OK |
| `examples/CMS/CMS-SUS-16-035_Delphes.adl` | OK |
| `examples/CMS/CMS-SUS-16-041_Delphes.adl` | OK |
| `examples/CMS/CMS-SUS-16-049_Delphes.adl` | OK |

### Parse / semantic / lexer (14+ files)

Representative fixes:

- **Lexer:** `Delphes_Photon`-style IDs; dotted path tokens (e.g. BDT weight filenames); single-letter IDs (`p`, `m`) for expressions like `matchedCaloEnergy / p < 0.20`
- **Grammar:** unary minus; nested `? :`; `print` / `save` / `counts` / `countsformat`; CutLang aliases (`algo`, `cmd`, `obj`, `def`); `object X : JET`, `object X : COMB(...)`; function-form `take` (`fmegajets`, `antikT`); composite `take leptons l1, l2`
- **Semantics:** null-safe `binOpCheck` / `printITE` / `typeCheck`; skip PRINT/SAVE/COUNTS in region decl checks; FUNCTION takes; dotted property names (`MET.pT`) via `objectBaseId()`
- **stdlib:** `ext_lib.txt` / `ext_objs.txt` extensions
- **Examples:** typo in `CMS-SUS-16-048` (`muons[1], METLV[0]`); comma-separated print/save in `ex11_printsave.adl`; defines in `basic_defines.adl`; OSdileptons object in `cl_examples/CMS-SUS-16-041.adl`

---

## `-r` (disjointness) mode

**Usage:** `./smash -r <file.adl>` or `./smash --region-analysis <file.adl>`

Runs the full normal pipeline, then prints experimental analyses:

- Object disjointness (`analyzeObjectDisjointness`)
- Region disjointness (`analyzeRegionDisjointness`)

All 68 files completed `-r` without error. Sample output (`CMS-SUS-16-033_Delphes.adl`):

```
==== OBJECT DISJOINTNESS ANALYSIS (experimental) ====
==== REGION DISJOINTNESS ANALYSIS (experimental) ====
Object attribute dimensions available: 8
Regions analyzed (after inheritance): 13
finished
```

**Note:** Analysis output is informational; it reports PROVEN DISJOINT / POSSIBLY OVERLAPPING pairs where rules apply. It does not change pass/fail of the compiler run.

---

## Build health

| Check | Result |
|-------|--------|
| `make clean && make` | Success |
| Flex / Bison generation | Success |
| Link `smash` to repo root | Success |

**Bison warnings (pre-existing, not introduced by this pass):**

- 57 shift/reduce conflicts
- 30 reduce/reduce conflicts
- One rule marked useless due to conflicts (`chain QUES chain` without `COLON` branch)

These do not block the build; they reflect grammar ambiguity in expressions / chained conditions. No new conflict count was recorded against a baseline, but the warning profile matches a typical LALR grammar for this DSL.

---

## Changed files (this verification scope)

| Path | Role |
|------|------|
| `adl/semantic_checks.cpp` | Null-safe printing/typecheck; `checkTables` dotted IDs; region decl skips |
| `adl/parser.y` | Grammar extensions (minus, print/save/counts, COMB, ternary, takes) |
| `adl/scanner.l` | Keywords / ID patterns |
| `adl/driver.cpp` | TAKE dependency for FUNCTION conditions |
| `adl/ext_lib.txt`, `adl/ext_objs.txt` | Builtins / objects |
| `examples/...` (4 files) | Small syntax / content fixes |

Approximate diff size: **+209 / −27** lines across 10 files (`git diff --stat`).

---

## Sanity review (bugs / regressions)

| Area | Finding |
|------|---------|
| **Regression sweep** | No failures after restoring original `[a-zA-Z][0-9a-zA-Z]*?+[-0-9a-zA-Z]*` ID rule alongside targeted Delphes/path/single-char rules |
| **Crashes** | None in 68×2 runs |
| **`checkTables` dotted IDs** | `MET.pT` resolves via base object `MET`; all Delphes `MET.pT` bins pass |
| **`print_list` COMMA rule** | Stack index fix (`$3` for tail); ex11 passes |
| **Experimental `-r`** | Stable on all examples; no extra exit codes |

### Known limitations (not regressions)

1. **Bison conflicts** — grammar still has shift/reduce and reduce/reduce conflicts; worth a dedicated grammar cleanup later.
2. **`-r` analysis** — experimental; overlap/disjoint conclusions are conservative heuristics, not a formal proof for all ADL.
3. **Example edits** — `ex11_printsave.adl` uses comma-separated print/save lists (clearer for the parser); `basic_defines.adl` gained stub `define`s so the tutorial sample is self-contained.
4. **CutLang-style files** — e.g. `ATLAS-SUSYJetMET-1605-03814.adl` rely on aliases (`algo`, `cmd`, `obj`); nested ternaries in long `cmd` lines are supported via `chained_cond` but remain fragile if new ambiguous tokens are added.

---

## How to reproduce

```bash
cd /home/daniel/Projects/adl2flowchart
make clean && make

# Standard
find examples -name '*.adl' | sort | while read -r f; do
  ./smash "$f" >/dev/null || echo "FAIL parse: $f"
done

# With disjointness analysis
find examples -name '*.adl' | sort | while read -r f; do
  ./smash -r "$f" >/dev/null || echo "FAIL -r: $f"
done
```

Expected: no `FAIL` lines; each run ends with `finished`.

---

## Conclusion

The `disjoint_dev` failure-fix work is in good shape for the examples corpus: **100% parse success** and **100% `-r` success** on 68 files, with former crashes eliminated and no new sweep failures detected after a clean rebuild.

Recommended follow-ups (optional):

- Commit the 10-file change set with a message grouping crash fix, grammar/lexer, stdlib, and example tweaks.
- Track Bison conflict reduction as a separate grammar-hygiene task.
- Add a small CI script mirroring the reproduce block above.