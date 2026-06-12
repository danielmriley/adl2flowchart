# Histogram production (Phase 9) — final report

Date: 2026-06-12. Scope: `smash2 run --histos` end to end, the `rootfile`
crate, the `.C`/`.py` bridges, and the CSV/SVG quick-looks. All ratified
decisions implemented: fill-time weighted moments, v1 flat region-prefixed
names, raw-fill-count `fEntries`, vendored uproot StreamerInfo blob,
uncompressed v1 records, standalone `rootfile` workspace crate that smash2
depends on.

## What shipped

- **Accumulator** (`adl-interp/src/histo.rs`): per-region `histo`
  statements fill once on full region acceptance, weighted by the region's
  own numeric `weight` product. ROOT `TH1::Fill` binning (`x < lo` →
  underflow, `x >= hi` → overflow, open top edge), per-bin Σw/Σw²
  (`Sumw2`) including flow bins, raw fill count for `fEntries`, and the
  four stats moments (Σw, Σw², Σw·x, Σw·x²) accumulated **at fill time,
  in-range only** (ROOT `GetStats` convention) so `GetMean`/`GetStdDev`
  and `hadd`-merged stats are exact, not binned approximations.
- **Canonical `histos.json`**: name, title, region, edges, sumw, sumw2,
  under/overflow (w and w²), entries, the four moments. Single source of
  truth; every other output renders from the same in-memory `HistoSet`,
  so no two outputs can disagree.
- **Native `out.root`** (`crates/rootfile`, pure Rust, zero deps): v1 of
  `SPEC_ROOT_WRITER.md` — small-format TFile header (fVersion 62400),
  TKey v4 records, single root TDirectory, uncompressed records
  (`fObjlen == fNbytes - fKeylen`), TH1D v3 / TH1 v8 / TAxis v10 streams
  with always-written `fSumw2`, vendored uproot TStreamerInfo blob
  (sha256-pinned, provenance in `crates/rootfile/fixtures/PROVENANCE.md`),
  terminal free-list segment. Datime + UUIDs pinned in the CLI path so
  out.root is byte-identical across runs. `--no-root` opts out.
  API: `RootFile::create().add_th1d(name, &H1Spec { … })?.finish(path)`;
  a strict re-parsing reader (`rootfile::reader`) backs the tests.
- **Bridges** (`adl-cli/src/cmd/bridges.rs`): `make_histos.C`
  (self-contained ROOT macro: `Sumw2()` → `SetBinContent/SetBinError`
  over bins 0..N+1 → `SetEntries` → `PutStats`) and `to_root.py`
  (uproot 5 `to_TH1x` with exact entries/moments). `--csv` and `--svg`
  add dependency-free quick-looks.
- **Naming**: flat `<region '/'→'_'>_<histo>` (e.g. `baseline_hmet`) in
  all formats — stable across runs, identical between out.root and the
  bridges, hadd-mergeable.

## Validation evidence (this final pass, 2026-06-12, this machine)

Build/lint/test battery, run from `reimplementation/adl2`:

| Gate | Result |
|---|---|
| `cargo build --workspace` | green |
| `cargo test --workspace` (default features, z3-native) | **428 passed / 0 failed** |
| `cargo test --workspace --no-default-features` (SMT-LIB subprocess backend) | **428 passed / 0 failed** |
| `cargo test --workspace --all-features` (deep: 100k encoder-vs-interp + 10k metamorphic property cases) | **428 passed / 0 failed** (100k prop battery 1277 s, metamorphic 6/6 in 409 s) |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `scripts/corpus_gate.sh` | all 68 corpus files parse + resolve clean |
| Golden batteries | adl-analysis `golden_battery` 37/37; adl-difftest `histo_golden` 2/2 (ex02 histos.json golden + determinism/consistency) |

Determinism, byte-level (each test run individually, plus a manual
double-run of the real binary on `ex02_histograms.adl`):

- `verify` default report and `verify --json`: byte-identical across runs
  (test `verify_report_is_byte_identical_across_runs` + manual `cmp`).
- `run --histos`: `histos.json`, `out.root`, `make_histos.C`, `to_root.py`
  all byte-identical across two independent runs (tests
  `run_histos_writes_canonical_json_deterministically`,
  `run_histos_native_root_is_byte_identical_across_runs`,
  `run_histos_bridges_are_byte_identical_across_runs`; manual `cmp` on all
  four files agreed).
- `dot_is_byte_identical_across_runs`, rootfile
  `builds_are_byte_deterministic_when_pinned`: pass.

Oracles (env-gated; what actually ran here):

- **uproot read-back (rootfile suite), FORCED** —
  `ROOTFILE_REQUIRE_UPROOT=1 cargo test -p rootfile --test uproot_oracle`
  with the pinned `.venv-uproot` (uproot 5.7.4, Python 3.12.3): all 3
  pass — `uproot_reads_back_our_file_exactly`,
  `uproot_lists_and_reads_multi_histo_file`, and
  `vendored_fixtures_match_freshly_generated_uproot_reference` (the
  vendored StreamerInfo blob + TH1D payload fixture byte-match a freshly
  uproot-generated reference at test time; fixture sha256s re-verified
  against PROVENANCE.md).
- **Byte-diff vs uproot**: our TH1D record payload is byte-identical to
  uproot's for the pinned reference histogram (offline gold test
  `payload_matches_uproot_reference_bytes` + the fixture-drift oracle
  above). Whole-file divergences are intentional and documented
  (record order, exact-size keys list, nfree=1) — readers follow
  pointers, and uproot reads our files natively.
- **End-to-end ex02 oracle (ad hoc, this pass)**: `smash2 run
  ex02_histograms.adl` over the committed 200-event fixture → uproot
  read out.root back **exactly**: 10/10 histograms, flow-inclusive
  values, raw `fSumw2` arrays, axis edges, `fEntries`, all four
  `fTsumw*` moments, titles, `fNcells` — zero mismatches; all 14
  streamer classes present.
- **CLI uproot bridge oracle, FORCED** — `SMASH2_RUN_UPROOT_ORACLE=1`
  with the venv's python3 on PATH: `to_root.py` executed and uproot read
  the result back (test `uproot_script_round_trips_when_available`,
  ran, not skipped). Additionally (ad hoc): the bridge-produced
  `histos.root` and the native `out.root` are **equivalent** across all
  10 histograms (values, fSumw2, entries, moments, titles, fNcells).
- **ROOT binary / hadd: SKIPPED — no `root`/`hadd` on this machine.**
  `SMASH2_RUN_ROOT_ORACLE=1` was set; the test reported
  "skipping: `root` not on PATH" and passed as a loud skip. The hadd
  smoke test is likewise env-gated and unexercised here. First machine
  with a ROOT install should run:
  `SMASH2_RUN_ROOT_ORACLE=1 cargo test -p adl-cli --test cli root_macro_round_trips_when_root_available`
  and an `hadd merged.root a.root b.root` over two `run --histos` outputs.

## Known gaps (deferred by design)

- **Per-region TDirectories** — v2 (flat names are hadd-safe today).
- **ZLIB compression** — v2 (uncompressed records are valid ROOT and keep
  byte-diffs trivial; histogram files are KB-scale).
- **TH2D / 2-D histograms** — deferred; each is skipped with a stderr
  diagnostic and absent from all outputs (rest of the file still fills).
- **Variable-bin histograms** — same deferred-with-diagnostic treatment.
- **Weight tables** — only the region's own numeric `weight` statements
  apply; inherited weights and table weights deferred.
- **Mid-selection fill points** — a `histoList` referenced more than once
  fills once on full region acceptance, with a diagnostic.
- **ROOT-binary validation** — never run on this machine (no ROOT
  install); uproot is the executed oracle. The macro/hadd tests exist and
  are one env var away on a machine with ROOT.

## Demo commands

From `reimplementation/adl2` (paths relative to that directory):

```bash
# Build everything
cargo build --workspace

# Fill ex02's histograms over the committed 200-event fixture; writes
# histos.json + out.root + make_histos.C + to_root.py (+ CSV/SVG):
./target/debug/smash2 run ../../examples/tutorials/ex02_histograms.adl \
  crates/adl-difftest/tests/fixtures/ex02_events.jsonl \
  --histos out/ --csv --svg

# Same without the native .root:
./target/debug/smash2 run ../../examples/tutorials/ex02_histograms.adl \
  crates/adl-difftest/tests/fixtures/ex02_events.jsonl \
  --histos out/ --no-root

# Read the native file back with uproot:
.venv-uproot/bin/python -c \
  'import uproot; f = uproot.open("out/out.root"); \
   print(f.keys()); print(f["baseline_hnjets"].values(flow=True))'

# Or build histos.root via either bridge:
( cd out && root -l -b -q make_histos.C )   # needs ROOT
.venv-uproot/bin/python out/to_root.py       # needs uproot+numpy

# Force the oracle suites:
ROOTFILE_REQUIRE_UPROOT=1 cargo test -p rootfile --test uproot_oracle
PATH="$PWD/.venv-uproot/bin:$PATH" SMASH2_RUN_UPROOT_ORACLE=1 \
  cargo test -p adl-cli --test cli uproot_script_round_trips_when_available
```
