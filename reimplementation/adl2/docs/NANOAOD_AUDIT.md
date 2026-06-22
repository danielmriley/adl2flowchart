# NanoAOD ingest audit

Audit of `smash2 ingest/run --profile nanoaod` (the native ROOT reader, the
`nanoaod()` profile, and the generated `to_jsonl.py` uproot oracle). Method:
a multi-agent adversarial audit plus direct testing against the real CMS Open
Data 2015 ttbar sample (200 events) and the scikit-hep RNTuple sample.

## Headline

The ingest **engine** is sound: native reader vs uproot oracle is
**byte-identical on all 200 events**, `run --profile nanoaod` matches an
independent JSONL count exactly (24 SR / 120 presel for a jets≥2 ∧ MET>40
selection), the RNTuple-format sample fails closed (exit 1), and uproot-written
files are correctly rejected by the oxyroot pin.

The v1 **profile** had one critical wrong-physics bug (now fixed) and has
coverage gaps + error-path asymmetries (recommended follow-ups below).

## FIXED — critical: b-tag discriminant keyed `btag` → silent zero selection

`profile.rs` emitted the continuous DeepCSV discriminant under the property
key `btag`. Two problems:
1. Real ADL cuts the discriminant by its NanoAOD spelling, `select btagDeepB >
   0.8`. `btagDeepB` resolves to a *different* identity (`DeepBof`) than the
   emitted `btag` (`isBTag`), so the cut found no property and **silently
   selected zero jets, with zero errors**. Verified: `select btagDeepB < 0`
   → 0/200 before, 186/200 after.
2. `btag` is an exact-name {0,1} tag bit (TAG axiom asserts `btag ∈ {0,1}`),
   but the emitted values are continuous (−1.0 on this file) — a latent
   `verify` soundness hazard.

Fix (`profile.rs`): emit under `f32_leaf("btagDeepB", "btagDeepB")`. Both
problems resolved; native==oracle byte-identity preserved; golden fixture
regenerated; all ingest tests green.

## Follow-ups — DONE

The coverage gaps and error-path asymmetries below were subsequently
implemented (profile.rs, reader.rs, script.rs, plus 7 property spellings in
`legacy_parser/adl/property_vars.txt`) and verified: native==oracle stays
**byte-identical** on the real sample with the expanded profile; the new cuts
(`jetId`, `btagDeepFlavB`, `tightId`, …) select correctly with zero errors;
full battery green (552). Specifics:
- **Coverage:** Jet now maps `btagDeepFlavB` + `btagCSVV2` (F32) and `jetId` +
  `puId` (I32); FatJet maps `btagDeepB` + `btagCSVV2` (no `FatJet_btagDeepFlavB`
  branch exists); Muon maps `tightId`/`looseId` (new `bool[]` `LeafKind::Bool`
  → 0/1) and `pfIsoId` (`uint8` via the widened I32). Each spelling resolves to
  its own identity (none collides with the {0,1} `btag`/`ctag`/`tautag` TAG
  axiom).
- **Reader/oracle symmetry:** `read_leaf_flat` dispatches on `item_type_name()`
  — F32 accepts `float[]`/`double[]`; I32 accepts int8…uint32 per-width; Bool
  accepts `bool[]`. The presence test dropped the `starts_with(prefix)` orphan
  scan (mirrors the oracle). The oracle now validates-before-emit with atomic
  `os.replace` (no partial file on a non-finite value) and gates flat MET on
  both `MET_pt` and `MET_phi`.

The original recommendations are retained below for reference.

## Recommended follow-ups (now implemented — see above)

### Coverage gaps (the physics can't be expressed today)
- **`btagDeepFlavB` (DeepJet, the official Run-2 b-tag recommendation) and
  `btagCSVV2`** — only DeepCSV is mapped. Add as float leaves keyed to their
  ADL spellings (same one-line pattern as the `btagDeepB` fix). Caveat: gate on
  presence — FatJet does not carry `btagDeepFlavB`, and a mapped-but-absent
  branch must drop (warn), not hard-error.
- **`Jet_jetId` / `Jet_puId`** (`int32_t[]`) — standard jet-quality
  preselection (`select jetId == 6`). Zero new reader code (map as `I32`).
- **Lepton/τ IDs** (`Muon_tightId` etc., `bool[]`; iso flags `uint8_t[]`) —
  require adding `bool[]`/`uint8_t[]` handling to both `read_leaf_flat` and the
  oracle (see #4/#6 below).
- **b-tag `−1.0` sentinel** — on this 2015 Open Data file every
  `Jet_btagDeepB` is `−1.0` (DeepCSV unfilled). A `btag > 0.8` cut silently
  gets zero jets even with the correct key. Add a once-per-file diagnostic when
  a mapped discriminant is the sentinel for most objects.

### Error-path asymmetries (break native==oracle on valid-but-non-default files)
None fire on standard CMSSW NanoAOD; exposure is coffea/uproot skims.
1. **Non-finite value** — native errors with no output; oracle leaves a
   partial JSONL file (and truncates a pre-existing good one). Fix: oracle
   should validate-before-emit or temp-file + rename-on-success.
2. **Counter-absent collection with an orphan sibling leaf** — native's
   presence test (`reader.rs:510` `starts_with("Jet_")`) hard-errors; oracle
   silently omits the collection. Fix: native should test only the counter and
   mapped leaves.
3. **`double[]` collection leaves** — native refuses (`read_leaf_flat`
   requires `float[]`); oracle accepts. Fix: accept `double[]` via `Slice<f64>`
   (mirroring `read_scalar_flat`, which already accepts both).
4. **`MET_pt` without `MET_phi`** — native drops MET; oracle crashes. Fix:
   oracle should gate on both leaves.
5. **Narrower-int charge** (`int16_t[]`/`bool[]`) — native refuses; oracle
   accepts. Fix: broaden `I32` to int8/16/32 (and the read path needs
   per-width decoding, not just a type-name gate).

## Format limitations (by design / dependency)
- **RNTuple** files (newer CMS Open Data, `ROOT::RNTuple`) are not readable by
  oxyroot 0.1.25 — fail closed (exit 1).
- **uproot-written** test files are rejected (uproot 5.7.4 defaults to RNTuple
  / newer TStreamerInfo than the oxyroot pin accepts), so synthetic
  edge-case fixtures can't be authored with uproot; real CMSSW files are the
  only test inputs.

## Bottom line

The ingest engine and the native==oracle contract are real and trustworthy on
standard CMSSW NanoAOD. With the critical b-tag fix in place, b-jet selection
now works; the remaining items are coverage breadth and contract robustness on
non-standard inputs, prioritized above.
