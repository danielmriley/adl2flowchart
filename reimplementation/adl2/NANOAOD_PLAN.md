# NanoAOD ingest support — implementation plan

Goal: `smash2 ingest --profile nanoaod <file.root>` and `smash2 run --profile
nanoaod <adl> <file.root>` work on real CMS NanoAOD, with the generated
`to_jsonl.py` uproot oracle producing **byte-identical** JSONL to the native
reader (the existing validation contract).

## Verified facts (real file: `samples/nanoaod/nanoAOD_2015_CMS_Open_Data_ttbar.root`, 200 evts)

- Tree: `Events`. Counters: `n<Coll>` (`nJet`, `nMuon`, …), oxyroot type
  **`uint32_t`** (Delphes uses `<Coll>_size` / `int32_t`).
- Collection leaves: `<Coll>_<leaf>` (underscore), jagged: `Jet_pt`=`float[]`,
  `Muon_charge`=`int32_t[]`. (Delphes uses `<Coll>.<leaf>` / dot.)
- MET/weight are **flat per-event scalars**: `MET_pt`,`MET_phi`,`genWeight` =
  scalar `float` (Delphes: one-element collection branches read via a counter).
- Triggers `HLT_*` = scalar `bool` (v1: not mapped — hundreds, analysis-specific).
- All physics collections are **pT-descending** (0 violations / 200 evts) → the
  canonical-model invariant holds; no re-sort needed.
- oxyroot 0.1.25 reads all shapes (`as_iter::<u32>`, `Slice<f32>`, `as_iter::<f32>`,
  `as_iter::<bool>`) — load-bearing dependency confirmed.

## Design — profile-data-driven, no per-experiment branches in the reader

1. **`Naming` on `Profile`** (new): `leaf_sep` (`"."` vs `"_"`), `counter`
   style (`SizeSuffix` `<b>_size` vs `NPrefix` `n<b>`), `flat_event_vars: bool`
   (MET/scalars/weight are flat scalars vs one-element branches). Helpers
   `Profile::leaf_branch(b,l)` and `Profile::counter_branch(b)`.
2. **`read_counter`** accepts `uint32_t` as well as `int32_t`.
3. **`read_scalar_flat`** (new): one `float`/`double` per entry, no counter →
   `Vec<f64>`; wrapped all-`Some` to reuse the existing emit path unchanged.
4. **`read_tree`** picks flat vs one-element readers for MET/scalars/weight by
   `naming.flat_event_vars`; all full branch names go through `leaf_branch` /
   `counter_branch`.
5. **`classify_rest`** made naming-aware (counter detection + sep split) and
   skips consumed flat-scalar names; NanoAOD's ~900 extra branches summarize as
   unknown families (diagnostics only, never an error).
6. **`nanoaod()` profile** + register in `by_name` + `KNOWN_PROFILES`:
   Jet/Electron/Muon/Tau/Photon/FatJet → `jet/electron/muon/tau/photon/fatjet`;
   leaves pt/eta/phi/mass(`m`); `Jet_btagDeepB`→`btag` (F32 float discriminant);
   `*_charge`→`q` (I32). MET = `MET_pt`/`MET_phi` (flat). Weight = `genWeight`
   (flat). Mass from the file's `_mass` leaves (no PDG constants needed).
7. **`script.rs` (`to_jsonl_py`)** mirrors the naming + flat-scalar reads so the
   oracle stays an independent but faithful cross-check.

## Tests / validation
- Unit: `nanoaod()` shape + `decides()`; `leaf_branch`/`counter_branch`.
- **Differential oracle (the strong check)**: native `ingest` vs generated
  `to_jsonl.py` (uproot) on the real sample → byte-identical JSONL. Gated behind
  an env var + the uproot venv (mirrors the existing Delphes E2E gating).
- E2E: `smash2 run --profile nanoaod <small.adl> <sample>` produces per-region
  results without error.
- `scripts/fetch_nanoaod_sample.sh` (mirrors `fetch_delphes_sample.sh`) using
  `skhep_testdata`'s `nanoAOD_2015_CMS_Open_Data_ttbar.root`.
- Full `cargo test` battery stays green; clippy clean.

## Out of scope (v1)
HLT trigger mapping; RNTuple NanoAOD; columnar/batched scale reads (separate
roadmap item). Sample `.root` is gitignored, fetched on demand.
