# SPEC: Event Pipeline — ingestion, cutflows, histogram completion, scale

Status: pre-implementation spec (Phase 10), written 2026-06-12 against probes run on
this machine. No production code exists for any section. Companion specs:
`SPEC_ROOT_WRITER.md` (the writer this extends), `SPEC_LANGUAGE.md` §4 (event model +
region semantics), `PLAN.md` Phase 10. Governing rules (ratified): experiment specifics
live in **converter profiles**, never the core event model; anything unfaithful is a
**diagnostic**, never a guess; convention-dependent choices are explicit **[DECIDE]**
entries; independent oracles per layer; byte-deterministic outputs; never weaken a test.

Probe artifacts referenced below (throwaway, not shipped):
- `/tmp/oxyroot_probe/` — Rust probe of oxyroot 0.1.25 against a real Delphes file.
- `/tmp/delphes_T2tt_700_50.root` — CutLang's own tutorial Delphes sample
  (`binder/Tutorial.ipynb`: `wget https://www.dropbox.com/s/zza28peyjy8qgg6/T2tt_700_50.root`,
  run there with `filetype=DELPHES adlfile=exHistos`). 71,452,474 bytes, ROOT 6.18.04,
  tree `Delphes`, 20,000 events.
  sha256 `04fae8b1d94809f799741af8351f9448b84370122b780ccf03df3b74531b89fc`.

---

## 1. INGESTION — converter profiles, Delphes first

### 1.1 Decision: (c) both — native oxyroot reader primary, generated uproot script as independent oracle

Evidence (probe run 2026-06-12, `/tmp/oxyroot_probe/src/main.rs`):

- **oxyroot 0.1.25 CAN read Delphes object branches.** The blocker we expected
  (TClonesArray split branches) is not one: the *leaves* of a split TClonesArray are
  ordinary sub-branches (`Jet.PT`, item type `float[]`) and
  `tree.branch("Jet.PT").as_iter::<Slice<f32>>()` yields per-event jagged slices.
  Verified values: event 0 `Jet.PT = [719.50916, 215.90102, …]`, `Jet.BTag` as
  `Slice<u32>`, `MissingET.MET`, `Event.Weight` all read.
- **Byte-exact agreement with uproot 5.7.4** on the full sample: 14 leaves × 20,000
  events = 574,171 values, f64 checksum `42096694.032` identical from both readers;
  ΣJet.BTag = 28,752 identical.
- **Fast**: the 14-leaf full-file read took 29 ms ≈ 685k events/s single-threaded
  (release build) — far above the §5 target.
- **Limits found**: oxyroot reads leaf sub-branches only — fine, the profile never needs
  TRef/TRefArray members (`Electron.Particle`, `Jet.Constituents`). Crate is v0.1.x and
  low-activity ("dormant" per the June research; its TH1-writing absence is why
  `rootfile` exists, see SPEC_ROOT_WRITER §5 item 5) — that risk is real but bounded: we pin
  the version, the corpus of files we must read (Delphes 3.4.x trees) is frozen, and the
  oracle script (below) detects any read infidelity in CI.

So: ship **both**, with native as the default path.

- **(a) native**: new crate `adl-ingest`. `smash2 run file.adl events.root
  --profile delphes` streams events straight from the TTree into the existing
  `adl_interp::Event` (crates/adl-interp/src/event.rs) — no intermediate JSONL on disk.
  `smash2 ingest events.root --profile delphes -o events.jsonl` materializes JSONL for
  debugging/fixtures (canonical JSON, byte-deterministic).
- **(b) oracle script**: `smash2 ingest --emit-script DIR` writes `to_jsonl.py`
  (uproot 5.x, generated exactly like `to_root.py` — see adl-cli `bridges.rs`; venv
  documented the same way, `.venv-uproot` already pins uproot 5.7.4). CI (env-gated,
  §7) asserts native JSONL == script JSONL **byte-identical** on the probe sample.
  The script is also the no-Rust fallback for collaborators.

### 1.2 Delphes profile: branch → canonical event model mapping

Source of truth for the right column: `adl-interp/src/event.rs` (collections / `met` /
`scalars` / `triggers`, canonical lowercase keys). All Delphes leaves are f32 (widened
to f64) except the u32 tag masks. Branch list probed from the sample (matches Delphes
3.4.x `delphes/classes/DelphesClasses.h`); CutLang's reader
(`/tmp/cutlang_src/CLA/delphes.C`) cited as precedent where conventions are needed.

| Delphes branch | canonical | notes |
|---|---|---|
| `Jet.PT/Eta/Phi/Mass` | `Jet[i].pt/eta/phi/m` | |
| `Jet.BTag` (u32 bitmask) | `Jet[i].btag` ∈ {0,1} | **[DECIDE-I1]** below |
| `Jet.TauTag` (u32 bitmask) | `Jet[i].tautag` ∈ {0,1} | **[DECIDE-I1]** applies too |
| `Electron.PT/Eta/Phi/Charge` | `Electron[i].pt/eta/phi/q` | mass: **[DECIDE-I2]** |
| `Muon.PT/Eta/Phi/Charge` | `Muon[i].pt/eta/phi/q` | mass: **[DECIDE-I2]** |
| `Photon.PT/Eta/Phi/E` | `Photon[i].pt/eta/phi/e` | |
| `FatJet.*` (same leaves as Jet) | `FatJet[i].*` | name: **[DECIDE-I3]** |
| `MissingET.MET` / `MissingET.Phi` | `MET.pt` / `MET.phi` | first (only) element; `MissingET.Eta` dropped (a transverse vector has no η; CutLang ignores it too) |
| `ScalarHT.HT` | scalar `HT` | |
| `Event.Weight` | event weight (§4) | **[DECIDE-I4]** |
| `Weight.Weight` (LHE multiweights) | dropped in v1 | diagnostic once per file: "N LHE weights present, not mapped" |
| `Event.*` other, `GenJet`, `GenMissingET`, `*_size`, `fUniqueID/fBits` | dropped | `_size` redundant with jagged lengths; gen-level out of scope v1 |
| triggers | none | Delphes has no HLT bits; `triggers` map stays empty, an ADL `trigger` over a missing flag keeps its existing missing-flag diagnostic — never a guessed pass |

Ordering: Delphes writes collections pT-descending — verified on **all 20,000 events ×
5 collections** (0 violations). The profile still runs the PHASE0 validate-don't-sort
check from `event.rs` (`NotPtDescending` refusal); a Delphes file that violates it is a
hard, faithful error.

**[DECIDE-I1] BTag/TauTag bitmask → flag.** Delphes packs one bit per working point
(`BitNumber 0` = default in standard cards). Options: (i) bit 0 only — the card's
default WP; (ii) any-bit-set — CutLang precedent (`delphes.C:410`:
`set_isbtagged_77((bool)jet->BTag)`). Recommend (i) bit 0, profile option
`btag_bit = N` for non-default cards; record per-run choice in provenance (§6).
Diagnostic when higher bits are set but unused.

**[DECIDE-I2] Lepton mass.** Delphes stores no lepton mass leaf. Options: (i) PDG
constants (e 0.000511, μ 0.105658 GeV — CutLang precedent `delphes.C:195`); (ii) 0.
Affects invariant-mass cuts at the per-mille level. Recommend (i) PDG, as profile
constants (still profile-side, not core-model-side).

**[DECIDE-I3] FatJet collection name.** `FatJet` (Delphes spelling) vs `AK8Jet`/`fjet`
aliases in `ext_objs.txt`. Recommend: canonical `fatjet`, aliases via the existing
base-collection spelling map.

**[DECIDE-I4] Which weight branch.** `Event.Weight` (generator weight, what CutLang
uses) vs `Weight.Weight[0]`. Recommend `Event.Weight`; the sample has both = 1.0 so the
e2e run (§7) cannot distinguish — needs a weighted sample or collaborator sign-off.

### 1.3 NanoAOD profile (v2, sketch) and PHYSLITE (future)

NanoAOD is flat (counter + `<obj>_<prop>` arrays) — strictly easier to read than
Delphes; both oxyroot and the script handle it. Sketch:
`nJet`/`Jet_pt/eta/phi/mass` → `Jet[i].*`; `Jet_btagDeepFlavB ≥ WP` → `btag`
(**[DECIDE-N1]** tagger + era working point — per-profile-version constants, e.g.
2018 UL DeepJet medium 0.2783, validated against CERN Open Data); lepton ID flags
(`Muon_tightId`, `Electron_cutBased`) → **[DECIDE-N2]** which ID level gates the
collection vs is exposed as a property; `MET_pt/phi` vs `PuppiMET_pt/phi` →
**[DECIDE-N3]**; `genWeight` → event weight; `HLT_*` (bool) → `triggers` (the first
profile to populate them). PHYSLITE (ATLAS, xAOD-derived): future; needs ragged
`AnalysisElectronsAuxDyn.pt`-style branches — re-probe oxyroot then; the script path
works today via uproot.

Profile contract (all profiles): a pure data table — branch patterns → canonical
keys, unit scale factors, tag-derivation rules, weight source, trigger map. Core
`Event`/`Interp` never see experiment names.

---

## 2. CUTFLOWS — our own design (explicitly not CutLang-compatible)

Per region, an ordered list of **steps**. Step 0 is `all` (every event processed).
Then exactly one step per *membership-affecting* statement of the region, in
declaration order (`HirRegionStmt` order, adl-sema/src/hir.rs):

| statement | step label (verbatim source text of the statement) | survivor predicate |
|---|---|---|
| `select c` | e.g. `select MET > 200` | events surviving prior steps ∧ c |
| `reject c` | `reject nbjets == 0` | prior ∧ ¬c |
| inheritance (bare region name) | `preselection` | prior ∧ parent's **whole** predicate, as **one** step — the parent's own table holds its breakdown |
| `trigger t` | `trigger mu_trig` | prior ∧ flag |

`weight`, `histo`, `bin`, `sort`, `print`, `save`, `table` contribute **no step**
(non-membership per SPEC_LANGUAGE §4.2 / eval.rs header).

Per step we record: `raw` (u64 events surviving steps ≤ i), `sumw` (Σ of the effective
weight, §4, over survivors), `sumw2` (Σw²), `errors` (u64). A hard evaluation error at
step i (`EvalError`) counts the event as **failing** step i and increments `errors` —
a faithful diagnostic, never a guessed pass; `errors > 0` is surfaced in the stdout
table and JSON, and `--fail-on` can gate on it.

**Bins**: `bin` partitions without constraining membership (eval.rs `BinOutcome`).
Each `bin` statement gets an appendix entry, filled only from events passing the whole
region: per-bin `raw/sumw/sumw2` arrays (boundary bins `[b0,b1)…[bn,∞)`), plus an
`out` bucket for below-`b0`/non-value (`BinOutcome::Boundary { bin: None }`) and a
`failed` count; boolean bins get `true`/`false` buckets.

### Emissions (all three from one accumulator; single source of truth)

1. **`cutflow.json` (canonical)** — schema `version: 1`, top-level `provenance` (§6),
   `total {raw, sumw, sumw2}` over all processed events, then `regions: [{name,
   steps: [{kind, label, raw, sumw, sumw2, errors}], bins: […]}]` in declaration
   order. Same canonical JSON writer discipline as `histos.json`
   (adl-interp/src/histo.rs `JsonWriter`: sorted/declared key order, ryu shortest
   floats) ⇒ byte-deterministic.
2. **TH1D pair in `out.root`** per region: `<flat region name>__cutflow_raw` and
   `__cutflow_wt` (double underscore = reserved namespace; a user histo that collides
   gets the existing name-collision diagnostic). `nbins = #steps`, axis `[0, nsteps)`,
   **bin i+1 labeled with the step's verbatim statement text** via TAxis `fLabels` —
   a `rootfile` extension: `fLabels` becomes a real `THashList` of `TObjString`s
   instead of the null pointer of SPEC_ROOT_WRITER §2; the object-any
   first-occurrence encoding needed is already specced there (kNewClassTag path), plus
   vendored `TObjString` streamer (same uproot-blob method, §2 "StreamerInfo record").
   Raw histo: contents = `raw_i`, fSumw2 = `raw_i` (Poisson); weighted histo:
   contents = `sumw_i`, fSumw2 = `sumw2_i` (proper Sumw2 — errors are
   √Σw² per step). `fEntries` = events processed; stats moments via the binned
   approximation of SPEC_ROOT_WRITER §4(b) (never zeros).
3. **stdout table** — per region, columns `step | raw | abs% | rel% | sumw ± √sumw2`
   (abs vs `all`, rel vs previous step), fixed-width, deterministic formatting.
   Suppressed under `--json` (clean machine output, PLAN P6 principle).

hadd note: both cutflow TH1Ds merge correctly under hadd (contents and Sumw2 sum;
labels match by text).

---

## 3. HISTOGRAM COMPLETION — TH2D + variable bins + per-region directories

Closes the Phase-9 deferrals (`HistoSpec::Unsupported` in adl-sema/src/hir.rs; the
"2-D/varbin skipped with stderr diagnostics" in BUILD_NOTES 2026-06-12).

**Syntax** (already lexed/parsed; ex02_histograms.adl lines 54/65/67):
2-D `histo h, "t", nx, xlo, xhi, ny, ylo, yhi, xexpr, yexpr`;
variable-bin `histo h, "t", e0 e1 … en, expr`. Sema: `HistoSpec` gains
`Uniform2D { nx, xlo, xhi, ny, ylo, yhi, xexpr, yexpr }` and
`Var1D { edges: Vec<String>, expr }` (edges canonical numeral text, strictly
increasing — else resolve-time diagnostic + Unsupported).

**Accumulator** (adl-interp/src/histo.rs): `Hist2D` — flat `Vec<f64>` of
`(nx+2)*(ny+2)` cells, ROOT global-bin order (`gbin = bx + (nx+2)*by`, x fastest),
parallel sumw2 vec, raw entries, and the seven fill-time moments (`Σw, Σw², Σwx, Σwx²,
Σwy, Σwy², Σwxy`). `Hist1DVar` — `Vec<f64>` edges, bin by binary search
(`x < e0` → underflow, `x ≥ en` → overflow; note this is ROOT histogram semantics, by
design different from the `bin` statement's open last bin, SPEC_LANGUAGE §4.3).
Fills happen on region membership with the §4 weight, exactly as `Hist1D::fill`.

**histos.json v2**: each entry gains `"type": "h1" | "h1var" | "h2"`; `h1var` carries
`"edges": […]`; `h2` carries both axes + row-major flow-inclusive `contents`/`sumw2`
and the extended stats. Existing `h1` entries unchanged (additive schema bump,
`"version": 2` at top level).

**rootfile additions** (sources: SPEC_ROOT_WRITER §2 TAxis stream; §6 TH2D bullet;
uproot `models/TH.py` `Model_TH2D_v4`/`Model_TH2_v5`, `writing/identify.py`
`to_TH2x`/`to_TAxis`):
- **Variable-bin TH1D**: TAxis v10 `fXbins` TArrayD = the n+1 edges (today "empty for
  uniform bins", §2); `fXmin = e0`, `fXmax = en`. No new streamers. ~0.5 day.
- **TH2D**: stream = TH2D v4 header → TH2 v5 (TH1 v8 base, then f64 `fScalefactor`,
  `fTsumwy`, `fTsumwy2`, `fTsumwxy`) → TArrayD of (nx+2)(ny+2). fYaxis becomes a real
  axis (uniform or fXbins). Vendored streamer blob regenerated from an uproot
  reference file containing a TH2D (same `include_bytes!` method, §2; adds TH2/TH2D
  records). Validation: uproot read-back of `values(flow=True)` 2-D array + the seven
  stats + byte-diff vs uproot `to_TH2x`, mirroring §5. 1–2 days (§6 estimate).
- **Per-region TDirectories (rootfile v2)**: exactly SPEC_ROOT_WRITER §3 — nested
  directory per region path component, objects keyed by bare histo name inside;
  cascade offsets back-patched in the in-memory buffer (§6 "circularity" note). Flat
  names remain available behind `--flat-names` for one release (hadd users). Both
  layouts hadd-safe (§3). The cutflow pair (§2) lives in its region's directory.

The bridges (`make_histos.C`, `to_root.py`) gain the same three forms — they are
renderers of histos.json and must never disagree with out.root (Phase-9 invariant).

---

## 4. EVENT WEIGHTS

One effective weight per (event, region, position):

```
w_eff(event, region, after stmt k) = w_input(event) × Π weights declared at positions ≤ k
```

- **`w_input`** — carried by the event from the profile mapping (§1.2: `Event.Weight`;
  NanoAOD `genWeight`). Core model: `Event` gains `pub weight: f64` (default 1.0);
  JSONL grows an optional top-level `"weight": <num>` (absent = 1.0). This is the only
  core-model change in this spec, and it is experiment-agnostic.
- **ADL `weight` statements** — numeric values multiply as today
  (histo.rs `region_weights`); **table weights stay [DECIDE-W2]/deferred**: a
  non-numeric weight keeps the existing contributes-1.0-with-diagnostic behavior
  (histo.rs header) and additionally marks every affected cutflow step and histogram
  `"weighted_incomplete": true` in JSON — unfaithful values are flagged, not guessed.

**[DECIDE-W1] positional vs whole-product composition.** Cutflow steps before a
`weight` statement should not carry it (CutLang applies weights sequentially), but
Phase 9 shipped whole-region products for histo fills. Recommend: **positional**
everywhere (cutflow steps use the weight in effect at their position; a histo fill
uses the weight in effect at the `histo` statement's position). Equivalent to Phase-9
behavior whenever all `weight` statements precede all `histo` statements — a corpus
lint verifies that holds today (expected: yes), making this a non-breaking refinement;
any file where it differs gets a diagnostic until ratified.

Raw vs weighted **everywhere**: cutflow steps (`raw`+`sumw`+`sumw2`, §2), histograms
(`fEntries` raw vs Σw contents + Sumw2, Phase-9 semantics unchanged), run summary
(`total.raw`/`total.sumw`), stdout table both columns. No output ever shows a weighted
number without its raw companion.

---

## 5. SCALE — streaming + deterministic parallel merge

- **Streaming input**: JSONL via `BufRead` lines (replaces the whole-file `read_jsonl`
  string path for `run`); native ingest streams basket-by-basket (oxyroot iterators
  are already streaming, §1.1). Memory: O(one chunk) per worker, never O(file).
- **Parallel event loop**: events are split into **fixed-size chunks of C = 4096
  consecutive events** (constant, independent of thread count). Workers (N = `--jobs`,
  default = cores) evaluate chunks into private partial accumulators
  (HistoSet + cutflow + diagnostics). **Reduction order is defined**: partials are
  merged by a single fold **in ascending chunk index** (worker completion order is
  irrelevant; a reorder buffer holds finished chunks until their turn). Since chunk
  boundaries and fold order are fixed, every f64 addition sequence is fixed ⇒ outputs
  are **byte-identical for any N, including N=1**. CI asserts `--jobs 1` ==
  `--jobs 8` bytes on a 100k-event synthetic run. No atomics, no shared mutable
  accumulation, ever.
- **Diagnostics ordering**: per-event diagnostics carry (chunk, line) and are emitted
  in input order after the fold — deterministic stderr too.
- **Benchmark**: `cargo bench` synthetic generator (the adl-difftest toy generator,
  seeded) through `run` with a representative ADL (ex02): target **≥ 100,000
  events/s** end-to-end on this machine at default jobs; probe headroom: raw native
  branch reading alone measured 685k events/s single-threaded (§1.1), JSON-less native
  path avoids serde entirely. Bench tracked in CI as a non-gating report; the gate is
  the determinism test, never weakened for speed.
- **Memory bound**: peak RSS ≤ O(N·C·avg_event + accumulators); asserted empirically
  (< 1 GiB on the 20k-event sample with default jobs) in the e2e test (§7).

## 6. PROVENANCE

Embedded in **every** output (cutflow.json, histos.json, out.root, `--json` report),
one canonical object, identical bytes across outputs of one run:

```json
"provenance": {
  "tool": "smash2 <semver>+<git short hash>",
  "adl": {"file": "ex02_histograms.adl", "sha256": "…"},
  "input": {"file": "T2tt_700_50.root", "sha256": "…", "events": 20000,
             "profile": "delphes/1"},
  "seed": 42,                      // present only for generated events
  "decides": {"btag_bit": 0, "lepton_mass": "pdg"}   // per-run [DECIDE] choices, §1.2
}
```

- ADL source hash = sha256 of the exact bytes parsed; input identity = basename +
  sha256 + event count (**[DECIDE-P1]**: always full-hash the input — recommended,
  71 MB hashes in ~0.2 s — vs a size+head+tail fingerprint above a threshold, with
  `"fingerprint"` key naming the scheme so it is never mistaken for a full hash).
- No wall-clock timestamps inside provenance; `out.root` fDatime stays
  pinned/injectable (SPEC_ROOT_WRITER §6) — determinism is preserved by construction.
- **out.root carrier**: a single `TNamed` key, name `smash2_provenance`, title = the
  canonical JSON string. TNamed v1 streamers are already in the vendored set
  (SPEC_ROOT_WRITER §2) — zero new streamer work, readable by `root`, uproot, and
  `rfile.Get("smash2_provenance")->GetTitle()`. (TText would drag in TAttText
  streamers for no benefit — rejected.)
- JSON outputs carry the object verbatim. The §1.1 `ingest` JSONL stays pure data
  (consumers must not need comment handling); its provenance goes in a sibling
  `events.provenance.json`.

## 7. VALIDATION — end-to-end real-sample plan

**Sample**: `T2tt_700_50.root` (header block above) — smallest viable real Delphes
file already blessed by the CutLang project (their Tutorial runs `exHistos` ≡ our
`examples/tutorials/ex02_histograms.adl` over it). Pinned by sha256; fetched by a
script into a cache dir; **env-gated CI** job `SMASH2_RUN_DELPHES_E2E=1` (loud SKIP
otherwise, same pattern as `SMASH2_RUN_ROOT_ORACLE` / `ROOTFILE_REQUIRE_UPROOT`).

E2E assertions (each layer has an **independent oracle**):

1. **Ingestion fidelity**: native `smash2 ingest` JSONL == generated `to_jsonl.py`
   JSONL, byte-identical, all 20,000 events (oracle: uproot, independent of oxyroot).
   Spot fixture: first 3 events' values pinned in a snapshot (the §1.1 probe values).
2. **Cutflow correctness**: a throwaway uproot+numpy script recomputes ≥ 3 cutflow
   steps of ex02's regions (`size(goodJets) ≥ N`, MET cut, lepton veto) directly from
   branches; raw counts must match `cutflow.json` exactly.
3. **Distribution sanity** (catches mapping transpositions that exact-count spot
   checks miss): on the accepted-events histograms — `hmet` mean ∈ [200, 800] GeV and
   0 < underflow+overflow < fEntries (T2tt(700,50) is a high-MET signal); `hnjets`
   mode ∈ [2, 8]; `hjet1eta` |mean| < 0.2 and symmetric tails (η symmetry); every
   `pt` histogram has zero negative-axis content; weighted == raw everywhere
   (sample weights are all 1.0 — also pins [DECIDE-I4] follow-up: a weighted sample
   is required before I4 ratification).
4. **Round-trip**: out.root re-read by uproot — cutflow TH1D bin labels are the
   verbatim statement texts; TH2D `hj1ptMET` values(flow=True) match histos.json;
   provenance TNamed title parses as JSON with the correct sha256s.
5. **Determinism at scale**: `--jobs 1` vs `--jobs 8` byte-identical across all
   outputs (§5); two sequential runs byte-identical (existing standard).

---

## [DECIDE] register (this spec)

| id | question | recommendation |
|---|---|---|
| I1 | Delphes BTag/TauTag bitmask → flag | bit 0; `btag_bit` profile option |
| I2 | lepton mass constants | PDG values, profile-side |
| I3 | FatJet canonical name | `fatjet` + spelling aliases |
| I4 | weight branch | `Event.Weight`; needs weighted sample to verify |
| N1–N3 | NanoAOD WPs / ID levels / MET flavor | per-profile-version constants (v2) |
| W1 | positional vs whole-product weight composition | positional + corpus lint |
| W2 | table weights | deferred; `weighted_incomplete` flagging |
| P1 | input identity hashing | always full sha256 |
