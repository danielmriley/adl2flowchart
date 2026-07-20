# ADL Analysis: CMS-SUS-16-017.adl

**Search for new physics in all-hadronic events with top quarks and W bosons using the Razor variables**

**Experiment:** CMS | **ID:** SUS-16-017

---

## Structure

| Category | Count |
|----------|-------|
| Objects  | 21    |
| Defines  | 18    |
| Regions  | 15    |
| Tables   | 0     |

---

## Objects

### AK4 Jet Hierarchy

| Object | Takes From | Key Cuts |
|--------|-----------|----------|
| `AK4jets` | `JET` | pT > 30, \|η\| < 2.4 |
| `bjetsLoose` | `AK4jets` | btagDeepB > 0.152 |
| `bjetsMedium` | `AK4jets` | btagDeepB > 0.4941 |
| `bjetsTight` | `AK4jets` | btagDeepB > 0.8001 |
| `AK4jetsNopho` | `AK4jets` | ΔR(jet, photon) ≥ 0.4 or pT ratio outside [0.5, 2.0] |
| `megajets` | `AK4jets` | fmegajets == 2 |
| `megajetsNopho` | `AK4jetsNopho` | fmegajets == 2 |

### AK8 Jet Hierarchy

| Object | Takes From | Key Cuts |
|--------|-----------|----------|
| `AK8jets` | `FJET` | pT > 200, \|η\| < 2.4 |
| `WjetsMasstag` | `AK8jets` | softdrop mass in [65, 105] GeV |
| `Wjets` | `WjetsMasstag` | τ₂/τ₁ ≤ 0.4 (W-tagged) |
| `WjetsAntitag` | `WjetsMasstag` | τ₂/τ₁ > 0.4 (W-antitagged) |
| `topjetsMasstag` | `AK8jets` | pT > 400, softdrop mass in [105, 210] GeV |
| `topjetsMasstag0b` | `topjetsMasstag` | btagDeepB < 0.1522 (0-b-tagged) |
| `topjets` | `topjetsMasstag` | btagDeepB ≥ 0.1522, τ₃/τ₂ < 0.46 (top-tagged) |
| `topjetsAntitag` | `topjetsMasstag` | btagDeepB < 0.1522, τ₃/τ₂ ≥ 0.46 (top-antitagged) |

### Leptons and Photons

| Object | Takes From | Key Cuts |
|--------|-----------|----------|
| `muonsVeto` | `MUO` | pT > 5, \|η\| < 2.4, softId, miniIso < 0.2 |
| `muonsSel` | `MUO` | pT > 10, \|η\| < 2.4, miniIso < 0.15 (tighter) |
| `electronsVeto` | `ELE` | pT > 5, \|η\| < 2.5, miniIso < 0.1 |
| `electronsSel` | `ELE` | pT > 10, \|η\| < 2.5, \|η\| outside [1.442, 1.556] (ECAL gap veto), miniIso < 0.1 |
| `tausVeto` | `TAU` | pT > 18, \|η\| < 2.5, MVA score ≥ 4 |
| `photons` | `PHO` | pT > 80, \|η\| < 2.5 |

---

## Defined Variables (18)

| Variable | Purpose |
|----------|---------|
| `MR`, `Rsq` | Razor variables (di-megajet frame) |
| `Rsqe`, `Rsqm` | Razor R² with single-lepton MET correction |
| `Rsqee`, `Rsqmm` | Razor R² with di-lepton MET correction |
| `MRNopho`, `Rsqpho` | Razor variables in photon+jets control region |
| `dphimegajets`, `dphimegajetsNopho` | Azimuthal angle between megajet hemispheres |
| `METLVe`, `METLVm` | MET 4-vectors with lepton correction (e/μ) |
| `METLVee`, `METLVmm` | MET 4-vectors with di-lepton correction |
| `METLVpho` | MET 4-vector with photon correction |
| `MTe`, `MTm` | Transverse mass with electron / muon |
| `mZ` | Di-lepton invariant mass (Z candidate) |

---

## Regions (15)

Two parallel channel structures — **W-category** and **Top-category** — each with a preselection-based SR and four control regions, plus standalone lepton, Z, and photon CRs.

### Common Preselection

| # | Cut | Purpose |
|---|-----|---------|
| 1 | `size(megajets) == 2` | Require 2 megajet hemispheres |
| 2 | `MR > 300` | Minimum Razor MR |
| 3 | `Rsq > 0.15` | Minimum Razor R² |
| 4 | `dphimegajets < 2.8` | Back-to-back veto |
| 5 | `size(tausVeto) == 0` | Tau veto |

### W-Category Regions

| Region | Inherits | Role | Distinguishing Cuts |
|--------|----------|------|---------------------|
| `WcategorySR` | preselection | Signal region | W-tagged, 0 top-tags, 0 leptons |
| `WcategoryCRQ` | preselection | QCD CR | W-antitagged, 0 top-tags, 0 leptons |
| `WcategoryCRT` | preselection | Top CR | W-tagged, ≥1 top-tag, 0 leptons |
| `WcategoryCRW` | preselection | W CR | W-tagged, 0 top-tags, 1 lepton |
| `WcategoryCRL` | — | Di-lepton CR | 1 μ + 1 e, MT cuts |
| `WcategoryCRZ` | — | Z→ll CR | OS same-flavour pair, mZ window |
| `WcategoryCRG` | — | γ+jets CR | ≥1 photon, photon-corrected Razor |

### Top-Category Regions

| Region | Inherits | Role | Distinguishing Cuts |
|--------|----------|------|---------------------|
| `TopcategorySR` | preselection | Signal region | Top-tagged, W-antitagged, 0 leptons |
| `TopcategoryCRQ` | preselection | QCD CR | Top-antitagged, 0 leptons |
| `TopcategoryCRT` | preselection | Top CR | Top-tagged, 1 lepton |
| `TopcategoryCRW` | preselection | W CR | Top-tagged, W-antitagged, 1 lepton |
| `TopcategoryCRL` | — | Di-lepton CR | Same as WcategoryCRL |
| `TopcategoryCRZ` | — | Z→ll CR | Same as WcategoryCRZ |
| `TopcategoryCRG` | — | γ+jets CR | Same as WcategoryCRG |

---

## Dependency Graph

**60 nodes, 51 edges** — no cross-file dependencies, no warnings.

| Type | Count | Names |
|------|-------|-------|
| Builtins | 6 | JET, FJET, MUO, ELE, TAU, PHO |
| Objects | 21 | (see above) |
| Defines | 18 | (see above) |
| Regions | 15 | (see above) |

---

## Object Disjointness

**210 pairs checked: 161 DISJOINT, 49 POSSIBLY_OVERLAPPING, 0 UNKNOWN**

All overlapping pairs are by design — derived objects are subsets of their parent:

- `AK4jets` overlaps with `bjetsLoose/bjetsMedium/bjetsTight`, `AK4jetsNopho`, `megajets`, `megajetsNopho`
- `AK8jets` overlaps with `WjetsMasstag`, `Wjets`, `WjetsAntitag`, `topjetsMasstag`, and top sub-categories
- `WjetsMasstag` overlaps with W-tag/antitag and top-tag sub-hierarchies (both are AK8 subsets with different mass windows that partially overlap around 105 GeV)
- `muonsVeto` overlaps `muonsSel`; `electronsVeto` overlaps `electronsSel` (looser collections contain the tighter)
- `topjetsMasstag0b`, `topjets`, `topjetsAntitag` overlap with each other and their parent

**Note:** `Wjets` vs `WjetsAntitag` and `topjets` vs `topjetsAntitag` are semantically disjoint (they split on τ-ratio thresholds), but the checker correctly reports POSSIBLY_OVERLAPPING because the cut variable (`tau2/tau1`) is a complex ratio expression that cannot be proven contradictory syntactically.

---

## Region Disjointness

**105 pairs checked: 28 DISJOINT, 77 POSSIBLY_OVERLAPPING, 0 UNKNOWN**

The 28 disjoint pairs are all cross-category (W-category vs Top-category) or SR vs QCD/Top CR pairs where the jet tagger cuts create explicit contradictions detectable by the checker. The 77 possibly-overlapping pairs are mostly within the same category, where CRs share the same preselection but no additional contradicting cuts are present at the structured-cut level.

---

## Notes

- The `electronsSel` object correctly uses the `][` (anti-range) operator to exclude the ECAL barrel–endcap transition region (`|η| outside [1.442, 1.556]`).
- The lepton/Z/photon control regions (`CRL`, `CRZ`, `CRG`) are identical between W-category and Top-category — they likely differ only in the jet tagger requirements embedded in their full cut lists.
- The `preselection` region acts as a pure base class; it is never used directly as a signal or control region.
