# ADL Analysis: CMS-SUS-16-032_Delphes.adl

**Analysis:** Search for the pair production of third-generation squarks with two-body decays to a bottom or charm quark and a neutralino in proton-proton collisions at sqrts = 13 TeV

**Experiment:** CMS | **ID:** SUS-16-032 | **Luminosity:** 35.9 fb^-1 | **sqrt(s):** 13.0 TeV
**Publication:** Phys. Lett. B 778 (2018) 263 | **arXiv:** 1707.07274 | **DOI:** 10.1016/j.physletb.2018.01.012
**Date:** 2026-04-16

---

## Tool Results Summary

| Tool | Status |
|------|--------|
| `parse_adl_file` | Success |
| `build_dependency_graph` | Success |
| `check_disjoint_objects` | Success |
| `check_disjoint_regions` | Success |

---

## Parsed Structure

| Category | Count |
|----------|-------|
| Objects  | 7 (+ 1 commented out) |
| Defines  | 7 |
| Regions  | 10 active + 0 commented out |
| Tables   | 0 |

### Objects

| Object | Base (take) | Selection Cuts |
|--------|-------------|----------------|
| **jets** | `Jet` | pT > 25 GeV; \|Eta\| < 2.4 |
| **bjets** | `jets` | BTag == 1 (b-tagged subset of `jets`) |
| **cjets** | `jets` | cTag == 1 (c-tagged subset of `jets`; note: ad-hoc c-tag, not available in native Delphes) |
| **muons** | `Muon` | pT > 10 GeV; \|eta\| < 2.4; D0 < 2 mm |
| **electrons** | `Electron` | pT > 10 GeV; \|eta\| < 2.4; D0 < 2 mm |
| **leptons** | `electrons` + `muons` (two `take` lines = union) | none (passthrough) |
| **MET** | `MissingET` | none (alias) |

**Notable observations:**
- `bjets` and `cjets` are both derived from `jets` (subset relations).
- `leptons` is a union of `electrons` and `muons`.
- `vetotracks` object is commented out (lines 48-53) — isolation not available in Delphes.

### Defined Variables

| Variable | Expression | Dependencies |
|----------|-----------|--------------|
| `MTj1` | `sqrt( 2*jets[0].pT * MET*(1-cos(MET.phi + jets[0].phi )))` | `jets`, `MET` |
| `MTj2` | `sqrt( 2*jets[1].pT * MET*(1-cos(MET.phi + jets[1].phi )))` | `jets`, `MET` |
| `MCT` | `2 * jets[0].pT * jets[1] * (1 + cos( dphi( jets[0], jets[1] )))` | `jets` |
| `dphimin2j` | `min(dphi(jets[0:1], MET))` | `jets`, `MET` |
| `dphimin` | `min(dphi(jets[0:2], MET))` | `jets`, `MET` |
| `HT01` | `jets[0] + jets[1]` | `jets` |
| `HTbc` | `sum(bjets.pT) + sum(cjets.pT)` | `bjets`, `cjets` |

---

### Region: noncompressed

Baseline for the noncompressed (large mass-splitting) search: two b-tagged jets plus high MET, targeting sbottom-like decays.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `size(jets) [] 2 4` | 2 to 4 jets in event (topology constraint) |
| 2 | select | `size(electrons) + size(muons) == 0` | Lepton veto (hadronic channel) |
| 3 | select | `jets[0].pT > 100` | Leading-jet pT threshold |
| 4 | select | `jets[0].BTag == 1` | Leading jet b-tagged |
| 5 | select | `jets[1].pT > 75` | Sub-leading-jet pT threshold |
| 6 | select | `jets[1].BTag == 1` | Sub-leading jet b-tagged |
| 7 | select | `jets[2].pT > 30` | Third jet pT threshold (if present) |
| 8 | select | `jets[3].pT > 30` | Fourth jet pT threshold (if present) |
| 9 | select | `MET.pT > 250` | Missing transverse momentum from neutralinos |
| 10 | select | `MCT > 150` | Contransverse mass — discriminates sbottom pair from ttbar |

### Region: noncompressedHT1

Low-HT slice of the noncompressed search, binned in MCT.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `noncompressed` | Inherit noncompressed baseline |
| 2 | select | `HT01 [] 200 500` | HT (leading + subleading jet) low bin |
| 3 | bin | `MCT 150 250 350 450` | MCT binning (4 edges → 3 bins) |

### Region: noncompressedHT2

Medium-HT slice of the noncompressed search.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `noncompressed` | Inherit noncompressed baseline |
| 2 | select | `HT01 [] 500 1000` | HT medium bin |
| 3 | bin | `MCT 150 250 350 450 600` | MCT binning (5 edges → 4 bins) |

### Region: noncompressedHT3

High-HT slice of the noncompressed search.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `noncompressed` | Inherit noncompressed baseline |
| 2 | select | `HT01 > 1000` | HT high bin |
| 3 | bin | `MCT 150 250 350 450 600 800` | MCT binning (6 edges → 5 bins) |

### Region: compressed

Baseline for the compressed (small mass-splitting) search: ISR-tagging topology with a c-tagged jet, targeting stop → c + neutralino.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `size(jets) [] 2 4` | 2 to 4 jets |
| 2 | select | `size(electrons) + size(muons) == 0` | Lepton veto |
| 3 | select | `jets[0].pT > 100` | Leading-jet pT (ISR candidate) |
| 4 | select | `jets[0].BTag == 0 and jets[0].cTag == 0` | Leading jet has no heavy-flavor tag (ISR jet) |
| 5 | select | `jets[1].BTag == 0 and jets[1].cTag == 1` | Sub-leading jet c-tagged but not b-tagged |
| 6 | select | `MET.pT > 250` | MET cut |
| 7 | select | `dphimin > 0.4` | min Δφ between first two jets and MET (QCD suppression) |

### Region: compressednb1

Compressed search with exactly one b-jet, low HTbc.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `compressed` | Inherit compressed baseline |
| 2 | select | `size(bjets) == 1` | Exactly one b-jet |
| 3 | select | `HTbc < 100` | Low heavy-flavor HT |
| 4 | bin | `MET.pT 250 300 500 750 1000` | MET binning (5 edges → 4 bins) |

### Region: compressednb2

Compressed search with one b-jet, 2-D binning in (MET, HTbc).

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `compressed` | Inherit compressed baseline |
| 2 | select | `size(bjets) == 1` | Exactly one b-jet |
| 3 | bin | `MET.pT [] 250 300 and HTbc < 100` | 2D bin |
| 4 | bin | `MET.pT [] 250 300 and HTbc [] 100 200` | 2D bin |
| 5 | bin | `MET.pT [] 300 500 and HTbc < 100` | 2D bin |
| 6 | bin | `MET.pT [] 300 500 and HTbc [] 100 200` | 2D bin |
| 7 | bin | `MET.pT > 500 and HTbc < 100` | 2D bin |
| 8 | bin | `MET.pT > 500 and HTbc [] 100 200` | 2D bin |

### Region: compressednc1

Compressed search with exactly one c-jet, low HTbc.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `compressed` | Inherit compressed baseline |
| 2 | select | `size(cjets) == 1` | Exactly one c-jet |
| 3 | select | `HTbc < 100` | Low heavy-flavor HT |
| 4 | bin | `MET.pT 250 300 500 750 1000` | MET binning |

### Region: compressednc2

Compressed search with one c-jet, 2-D binning in (MET, HTbc).

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `compressed` | Inherit compressed baseline |
| 2 | select | `size(cjets) == 1` | Exactly one c-jet |
| 3 | bin | `MET.pT [] 250 300 and HTbc < 100` | 2D bin |
| 4 | bin | `MET.pT [] 250 300 and HTbc [] 100 200` | 2D bin |
| 5 | bin | `MET.pT [] 300 500 and HTbc < 100` | 2D bin |
| 6 | bin | `MET.pT [] 300 500 and HTbc [] 100 200` | 2D bin |
| 7 | bin | `MET.pT [] 500 750 and HTbc < 100` | 2D bin |
| 8 | bin | `MET.pT [] 500 750 and HTbc [] 100 200` | 2D bin |
| 9 | bin | `MET.pT > 750 and HTbc < 100` | 2D bin |
| 10 | bin | `MET.pT > 750 and HTbc [] 100 200` | 2D bin |

### Region: compressednbnc0

Compressed search with no heavy-flavor jets — pure ISR + MET signature.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | select | `compressed` | Inherit compressed baseline |
| 2 | select | `size(bjets) + size(cjets) == 0` | No b-jets and no c-jets |
| 3 | bin | `MET.pT 300 500 750 1000 1250` | MET binning (5 edges → 4 bins) |

### Commented-Out Regions

None. (A `vetotracks` object is commented out, and a sibling "compressednbnc with >0 secondary vertices" region is mentioned in a comment on line 152 but not defined.)

---

## Dependency Graph

**28 nodes, 59 edges.** No undefined references detected.

### Node Breakdown

| Type | Count | Names |
|------|-------|-------|
| Builtin | 4 | Jet, Muon, Electron, MissingET |
| Object  | 7 | jets, bjets, cjets, muons, electrons, leptons, MET |
| Define  | 7 | MTj1, MTj2, MCT, dphimin2j, dphimin, HT01, HTbc |
| Region  | 10 | noncompressed, noncompressedHT1, noncompressedHT2, noncompressedHT3, compressed, compressednb1, compressednb2, compressednc1, compressednc2, compressednbnc0 |
| Table   | 0 | — |

### Dependency Hierarchy

```
Builtins
 ├── Jet
 │    └── jets  ─────────────────┐
 │          ├── bjets            │
 │          └── cjets            │
 ├── Muon                        │
 │    └── muons ────┐            │
 ├── Electron       │            │
 │    └── electrons │            │
 │          └── leptons (union of electrons + muons)
 └── MissingET
      └── MET

Defines (reference objects)
 ├── MTj1      ← jets, MET
 ├── MTj2      ← jets, MET
 ├── MCT       ← jets
 ├── dphimin2j ← jets, MET
 ├── dphimin   ← jets, MET
 ├── HT01      ← jets
 └── HTbc      ← bjets, cjets

Regions
 noncompressed ← jets, electrons, muons, MET, MCT
   ├── noncompressedHT1 ← noncompressed, HT01, MCT
   ├── noncompressedHT2 ← noncompressed, HT01, MCT
   └── noncompressedHT3 ← noncompressed, HT01, MCT

 compressed ← jets, electrons, muons, MET, dphimin
   ├── compressednb1    ← compressed, bjets, HTbc, MET
   ├── compressednb2    ← compressed, bjets, HTbc, MET
   ├── compressednc1    ← compressed, cjets, HTbc, MET
   ├── compressednc2    ← compressed, cjets, HTbc, MET
   └── compressednbnc0  ← compressed, bjets, cjets, MET
```

### Edge Types

| Kind | Count | Description |
|------|-------|-------------|
| `take` | 8 | Object inheritance (5 from builtins, 3 object-to-object plus 2 union members) |
| `reference` | 51 | Variable/region expression dependencies |

---

## Object Disjointness Analysis

**21 pairs checked:** 16 disjoint, 5 possibly overlapping, 0 unknown.

### Disjoint Pairs (16)

| Object A | Object B | Reason |
|----------|----------|--------|
| jets | muons | Different base types (Jet vs Muon) |
| jets | electrons | Different base types (Jet vs Electron) |
| jets | leptons | Different base types (Jet vs Electron/Muon) |
| jets | MET | Different base types (Jet vs MissingET) |
| bjets | muons | Jet-derived vs Muon |
| bjets | electrons | Jet-derived vs Electron |
| bjets | leptons | Jet-derived vs lepton union |
| bjets | MET | Jet-derived vs MissingET |
| cjets | muons | Jet-derived vs Muon |
| cjets | electrons | Jet-derived vs Electron |
| cjets | leptons | Jet-derived vs lepton union |
| cjets | MET | Jet-derived vs MissingET |
| muons | electrons | Distinct builtin particle types |
| muons | MET | Muon vs MissingET |
| electrons | MET | Electron vs MissingET |
| leptons | MET | Leptons vs MissingET |

### Possibly Overlapping Pairs (5)

| Object A | Object B | Reason |
|----------|----------|--------|
| jets | bjets | `bjets` is a strict subset of `jets` (additional BTag cut) |
| jets | cjets | `cjets` is a strict subset of `jets` (additional cTag cut) |
| bjets | cjets | Both derived from `jets`; definitions do not forbid a jet being both b- and c-tagged |
| leptons | muons | `leptons` union contains `muons` |
| leptons | electrons | `leptons` union contains `electrons` |

---

## Region Disjointness Analysis

**45 pairs checked** among the 10 active regions. Key findings:

- **noncompressed family vs compressed family — fully disjoint.** `noncompressed` requires `jets[0].BTag == 1` and `jets[1].BTag == 1`; `compressed` requires `jets[0].BTag == 0` and `jets[1].BTag == 0`. These conditions are mutually exclusive on the leading two jets.
- **noncompressedHT1 / HT2 / HT3 — pairwise disjoint.** Non-overlapping `HT01` ranges: [200,500], [500,1000], and >1000.
  - Note: the HT1/HT2 boundary at 500 touches under the `[]` inclusive operator — a single event with `HT01 == 500` technically passes both. Likely intended as half-open in practice.
- **compressednb1 vs compressednbnc0 — disjoint.** `size(bjets) == 1` vs `size(bjets) + size(cjets) == 0`.
- **compressednb2 vs compressednbnc0 — disjoint.** Same reason.
- **compressednc1 vs compressednbnc0 — disjoint.** `size(cjets) == 1` vs zero b- and c-jets.
- **compressednc2 vs compressednbnc0 — disjoint.** Same reason.
- **compressednb1 vs compressednb2 — possibly overlapping.** Both require `size(bjets) == 1` under `compressed`; `nb1` adds `HTbc < 100` while `nb2` bins but does not pre-select on `HTbc`, so `nb2` contains the `nb1` phase space.
- **compressednc1 vs compressednc2 — possibly overlapping.** Same structure as nb1/nb2.
- **compressednb (nb1/nb2) vs compressednc (nc1/nc2) — possibly overlapping.** `size(bjets) == 1` and `size(cjets) == 1` can both hold simultaneously because `bjets` and `cjets` are not mutually exclusive (see object disjointness). A single jet tagged with both `BTag == 1` and `cTag == 1` would be counted in each collection — in practice, experiments would resolve this with priority, but the ADL as written permits overlap.

---

## Notes

- **Commented-out cuts in baselines:** several cuts inside `noncompressed` (lines 71, 80, 81) and `compressed` (lines 102, 108, 110) are commented out — notably a ternary `size(jets) == 2 ? dphimin2 > 0.4 : dphimin > 0.4` in both, and a `min(MTj1, MTj2) > 250` in `noncompressed`.
- **Defined but unused:** `MTj1`, `MTj2`, and `dphimin2j` are defined but never referenced in any active region (their use would be inside the commented-out ternary cuts). These are dead code in the current file.
- **`dphimin` simplification:** only `compressed` uses `dphimin` (in place of the originally intended compressed-jets ternary).
- **`HT01` is a four-vector sum, not a scalar HT:** the expression `jets[0] + jets[1]` appears to yield a LorentzVector; its usage as `HT01 [] 200 500` likely relies on an implicit `.pT` or `.mass` attribute. This is ambiguous as written and may warrant explicit conversion.
- **`MCT` definition is suspicious:** the expression `2 * jets[0].pT * jets[1] * (1 + cos(...))` multiplies a scalar pT by a full four-vector `jets[1]` — typical MCT uses `jets[1].pT`. Possible typo in the ADL.
- **Ad-hoc c-tagging:** the comment on `cjets` notes that c-tagging does not exist in Delphes natively; this object is a placeholder pending genparticle matching.
- **Union by repeated `take`:** `leptons` uses two `take` statements rather than `take union(...)` — treated identically by the compiler.
- **No triggers, weights, or histograms** are defined in this analysis.
- **No tables** are defined (unlike some CMS analyses that carry efficiency tables inline).
- **Binning conventions:** regions use both 1D (`bin VAR edge1 edge2 ...`) and 2D (`bin VAR1 [] a b and VAR2 ...`) forms.
- **The HT1/HT2 and HT2/HT3 boundary overlap** at `HT01 == 500` and `HT01 == 1000` under strict `[]` inclusive interpretation is a minor ambiguity.
- **A `compressednbnc` region with >0 secondary vertices** is mentioned in a comment (line 152) but never defined.
