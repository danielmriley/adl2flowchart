# PIPELINE_REPORT — Phase 10 event pipeline, real-sample end-to-end

Date: 2026-06-13. Machine: 12 logical cores, Linux 6.8, release builds.
Scope: SPEC_EVENT_PIPELINE.md §1–§7, validated against the pinned real
Delphes sample. This report records the shipped features, the e2e run on
real data, and the independent-oracle coverage that backs each layer.

## 1. Sample provenance

The smallest viable real Delphes file — CutLang's own tutorial sample,
already blessed by the upstream project (their `binder/Tutorial.ipynb` runs
`exHistos` ≡ our `examples/tutorials/ex02_histograms.adl` over it).

| field | value |
|---|---|
| file | `T2tt_700_50.root` (cached locally as `delphes_T2tt_700_50.root`) |
| size | 71,452,474 bytes |
| sha256 | `04fae8b1d94809f799741af8351f9448b84370122b780ccf03df3b74531b89fc` (verified, matches the pin) |
| ROOT version | 6.18.04 |
| tree / events | `Delphes`, 20,000 events |
| origin | `wget https://www.dropbox.com/s/zza28peyjy8qgg6/T2tt_700_50.root` (CutLang tutorial); fetched by `scripts/fetch_delphes_sample.sh`, sha256-checked before use |

Network status this run: the sample was already present on the machine at
`/tmp/delphes_T2tt_700_50.root` with the exact pinned sha256, so **no
download was needed** and the real e2e ran on the genuine Delphes file (not
a synthetic fallback). The fetch script remains the reproducible path for a
clean machine.

ADL: `examples/tutorials/ex02_histograms.adl`,
sha256 `c957b352c3da0e7682728ceefb2291a57bd808de0792e7099ffce29ff09bfa39`
(2 selection regions — `baseline`, `singlelepton`; 13 histograms incl. a
TH2D `hj1ptMET` and a variable-bin TH1D `hmetvarbin`).

## 2. Shipped features (Phase 10a–10d, all wired into the run path)

The full pipeline is reachable from one command:

```
smash2 run ex02_histograms.adl T2tt_700_50.root --profile delphes --histos out/
```

- **Native Delphes ingestion** (`adl-ingest`, oxyroot 0.1.25 pinned):
  TClonesArray leaf branches → canonical `adl_interp::Event`; profile is a
  pure data table (branch patterns → canonical keys, unit scales,
  tag-derivation, weight source). `--profile delphes` streams the ROOT file
  through the run loop; `smash2 ingest … -o events.jsonl` materializes
  byte-deterministic JSONL; `--emit-script DIR` writes the uproot oracle
  `to_jsonl.py`.
- **Faithful mapping diagnostics** (never a guess): per-collection unmapped
  leaf reports, the LHE-multiweight-present note, dropped gen-level/size
  leaves. `--verbose` lists every dropped leaf.
- **Weights + cutflows** (`adl-interp` cutflow accumulator): per-region
  ordered steps (all / select / reject / inherit-as-one-step / trigger);
  `raw` + `sumw` + `sumw2` + `errors`; bin appendix; positional weight
  composition ([DECIDE-W1]).
- **Histogram completion**: TH1D, variable-bin TH1D, and TH2D accumulators
  (ROOT global-bin order, the seven fill-time moments); `histos.json` v2.
- **Native `out.root`** (`rootfile` crate, pure Rust): per-region
  `TDirectory`s, histos keyed by bare name, the `__cutflow_raw`/`_wt` pair
  per region with TAxis `fLabels` = verbatim step text, and the
  `smash2_provenance` TNamed. Byte-deterministic (pinned datime + zeroed
  UUIDs). `--flat-names` keeps the v1 flat layout for hadd users.
- **Bridges**: `make_histos.C` (ROOT) and `to_root.py` (uproot) — renderers
  of `histos.json`, validated never to disagree with `out.root`.
- **Provenance** (§6) embedded identically in every output:
  `cutflow.json`, `histos.json`, `out.root` (TNamed), and `--json`.
- **Scale** (§5): streaming JSONL reader (O(one chunk) memory), chunked
  parallel loop (C=4096, ascending-chunk-index fold) that is
  byte-identical for any `--jobs`. `--jobs 0` = all cores.

`out.root` structure on the real sample (uproot key listing):

```
smash2_provenance
baseline/{hmet, hj1ptMET, hmetvarbin, hnjets, hjet{1,2,3}pt, hjet{1,2,3}eta,
          baseline__cutflow_raw, baseline__cutflow_wt}
singlelepton/{hlep1pt, hlep1eta, hlep1ptMET,
              singlelepton__cutflow_raw, singlelepton__cutflow_wt}
```

## 3. End-to-end run on the real sample (SPEC §7)

Each layer is checked by an **independent oracle**. Throwaway oracle
scripts live in `/tmp/e2e/` for this run; the committed env-gated test
`adl-cli/tests/ingest.rs::delphes_sample_ingestion_fidelity_end_to_end`
encodes assertion 1 permanently.

### Assertion 1 — ingestion fidelity (oracle: uproot, independent of oxyroot)

Native `smash2 ingest` JSONL **byte-identical** to the generated
`to_jsonl.py` (uproot 5.7.4) JSONL, all 20,000 events:

```
native sha256  = e1a5499b37569d46b7d0a6e8a2a9f40c9b48500049f44240a9fad9eb18e629be
scripted sha256 = e1a5499b37569d46b7d0a6e8a2a9f40c9b48500049f44240a9fad9eb18e629be  ✓
```

First-event probe values pinned (SPEC §1.1): `Jet[0].pt = 719.5091552734375`,
`MET.pt = 653.098876953125`, `weight = 1.0`. The committed env-gated test
(`SMASH2_RUN_DELPHES_E2E=1`) passes green on this sample (83 s incl. the
uproot oracle subprocess).

### Assertion 2 — cutflow correctness (oracle: uproot + numpy, independent recompute)

Every step of both regions matches an independent uproot+numpy recompute
exactly (b-tag bit-0 masking, the union lepton count, and the
inheritance-as-one-step all reproduced from raw branches):

```
baseline:     20000 → 20000(ALL) → 16879(≥3 jets) → 14194(≥1 b) →
              10835(jet1 pT>200) → 10309(MET>100) → 8930(MET>200)
singlelepton: 20000 → 8930(inherit baseline) → 1336(exactly 1 lepton)
errors = 0 everywhere
```

All 10 steps: oracle == `cutflow.json` raw counts. The committed golden
`adl-difftest/tests/cutflow_golden.rs` pins the synthetic-fixture cutflow
shape; this real-sample recompute is the independent §7-item-2 check.

### Assertion 3 — distribution sanity (catches mapping transpositions)

On the accepted-event histograms read back from `out.root`:

| check | criterion | measured |
|---|---|---|
| hmet mean | ∈ [200,800] GeV (high-MET signal) | 424.9 GeV ✓ |
| hmet flow | 0 < under+over < entries | under 0, over 31, entries 8930 ✓ |
| hnjets mode | ∈ [2,8] | 4.5 ✓ |
| hjet1eta mean | \|mean\| < 0.2 (η symmetry) | 0.0175 ✓ |
| pt axes | zero negative-axis content | all non-negative ✓ |
| weighted vs raw | equal (sample weights all 1.0) | equal everywhere ✓ |

### Assertion 4 — round-trip (oracle: uproot read-back of out.root)

- `baseline__cutflow_raw` bin labels are the **verbatim** statement texts
  (`select size(goodJets) >= 3`, …) and contents = step raw counts ✓
- TH2D `hj1ptMET` `values(flow=True)` total matches `histos.json` (8930) ✓
- variable-bin `hmetvarbin` edges preserved `[0,10,20,50,100,500]` ✓
- `smash2_provenance` TNamed title parses as JSON with input sha256
  `04fae8b1…` and ADL sha256 `c957b352…` ✓
- Bridge cross-check: `to_root.py` → `histos.root`, all 13 histograms
  byte-agree with native `out.root` (0 mismatches) — the Phase-9
  invariant holds on real data ✓

### Assertion 5 — determinism at scale (§5)

`--jobs 1`, `--jobs 8`, and a second `--jobs 1` run all produce
**byte-identical** `histos.json`, `cutflow.json`, `out.root`,
`make_histos.C`, and `to_root.py` on the full 20,000-event sample.

## 4. Measured throughput and memory

End-to-end real-sample `run --profile delphes --histos … --json` (native
71 MB ROOT read + ingest-to-JSONL + parse + eval over 2 regions/13 histos +
write all 5 outputs):

| jobs | wall | peak RSS |
|---|---|---|
| default (12) | ~0.55 s (3 runs: 0.54/0.55/0.57) | 187 MB |
| 1 | ~0.99 s | 135 MB |

The real-sample run is **read-bound** (the §1.1 probe measured raw native
branch reading alone at ~685k events/s; the in-memory ingest-to-JSONL on
the profile path is the current cost). The §5 ≥100k events/s loop target is
met with headroom on the committed criterion bench
(`adl-difftest`, `--features bench`): ex02 (the heavy end, incl. a TH2D)
~306k events/s parallel / ~55k serial; a light 1-region/3-histo ADL hits
685k events/s on 1M synthetic events. RSS stays far under the §5 1 GiB
bound (1M-event synthetic stream: 19 MB at `--jobs 1`, 147 MB at default —
proves the reader never buffers the file).

## 5. Oracle coverage matrix

| layer | oracle | gate |
|---|---|---|
| ingestion | uproot 5.7.4 `to_jsonl.py`, byte-diff | env-gated CI test (real sample) + 2 committed fixtures |
| cutflow | uproot+numpy independent recompute | golden (synthetic) + real-sample recompute this run |
| TH1D/varbin/TH2D | uproot read-back + byte-diff vs `to_TH2x`/`to_TAxis` | `rootfile/tests/uproot_oracle.rs` (ROOTFILE_REQUIRE_UPROOT=1) |
| out.root (wired path) | uproot read-back | `adl-cli/tests/cli.rs::ex02_out_root_read_back_by_uproot` |
| bridges | uproot + ROOT macro round-trip | env-gated cli tests |
| provenance | JSON parse of TNamed title + sha256 match | round-trip oracle |
| determinism | byte-diff `--jobs 1` vs `--jobs 8` vs rerun | committed CLI test + real-sample this run |

## 6. Known gaps (named, faithful)

- **[DECIDE-I4] weight branch unverified by a weighted sample.** This
  sample's `Event.Weight` is 1.0 for all 20,000 events, so weighted == raw
  everywhere and the choice between `Event.Weight` and `Weight.Weight[0]`
  cannot be distinguished here. Ratification needs a weighted Delphes
  sample or collaborator sign-off. The 20,000 LHE multiweights
  (`Weight.Weight`) are reported-and-dropped (v1), not mapped.
- **Provenance `tool` has no git short hash.** `smash2 0.1.0` (no
  `+<git>`); a build-time injection seam (like `out.root` fDatime) is
  deferred so determinism survives across commits. Recorded in BUILD_NOTES.
- **`ingest -o` sibling `events.provenance.json`** (§6 JSONL carrier) is
  still not written by the standalone ingest path; `run`'s outputs all
  carry provenance.
- **Mid-selection histoList fill points.** ex02 references `jetHistos`
  after two different MET cuts; the run honestly diagnoses
  ("filled once on full region acceptance") rather than guessing per-cut
  fill semantics — a faithful limitation, not a silent wrong answer.
- **NanoAOD / PHYSLITE profiles**: spec'd (§1.3), not built. Only
  `delphes` ships.
- **`seed` in provenance** is wired but always `None` from `run` (synthetic
  seeding only).

## 7. Reproduce

```bash
# sample (sha256-checked); already cached here as /tmp/delphes_T2tt_700_50.root
scripts/fetch_delphes_sample.sh

PATH=".venv-uproot/bin:$PATH"
# env-gated committed e2e (ingestion fidelity)
SMASH2_RUN_DELPHES_E2E=1 SMASH2_DELPHES_SAMPLE=/path/to/sample.root \
  cargo test -p adl-cli --test ingest -- delphes_sample_ingestion_fidelity_end_to_end

# full pipeline
./target/release/smash2 run examples/tutorials/ex02_histograms.adl \
  /path/to/sample.root --profile delphes --histos out/ --json
```
