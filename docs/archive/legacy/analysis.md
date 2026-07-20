# ADL Analysis: CMS-SUS-16-048_Delphes.adl

**Analysis:** Search for new physics in events with two soft oppositely charged leptons and missing transverse momentum in proton-proton collisions at sqrt(s) = 13 TeV

**Experiment:** CMS | **ID:** SUS-16-048 | **Luminosity:** 35.9 fb^-1 | **sqrt(s):** 13.0 TeV
**Publication:** Phys. Lett. B 782 (2018) 440 | **arXiv:** 1801.01846
**Date:** 2026-04-05

---

## Tool Results Summary

| Tool | Status |
|------|--------|
| `parse_adl_file` | Success |
| `build_dependency_graph` | Success |
| `check_disjoint_objects` | Success |
| `check_disjoint_regions` | Success (only 1 region -- nothing to compare) |

---

## Parsed Structure

| Category | Count |
|----------|-------|
| Objects  | 6     |
| Defines  | 7     |
| Regions  | 1 (active) + 3 commented out |
| Tables   | 0     |

### Objects

| Object | Base (take) | Selection Cuts |
|--------|-------------|----------------|
| **muons** | `Muon` | pT in [3.5, 30] GeV; \|eta\| < 2.4 |
| **electrons** | `Electron` | pT in [3.5, 30] GeV; \|eta\| < 2.5 |
| **leptons** | `electrons` + `muons` | Union -- no additional cuts |
| **jets** | `Jet` | pT > 25 GeV; \|eta\| < 2.4 |
| **bjets** | `jets` | BTag == 1 |
| **MET** | `MissingET` | None |

Notable: `leptons` is a union object created by taking from two user-defined objects rather than a single builtin. `bjets` is a strict subset of `jets` with an additional b-tag requirement.

### Defined Variables

| Variable | Expression | Dependencies |
|----------|-----------|--------------|
| `dilepton` | `leptons[0] + leptons[1]` | leptons |
| `dielectron` | `electrons[0] + electrons[1]` | electrons |
| `dimuon` | `muons[0] + muons[1]` | muons |
| `HT` | `sum(jets.pT)` | jets |
| `MTl1` | `sqrt(2 * leptons[0].pT * MET.MET * (1 - cos(MET.phi - leptons[0].phi)))` | leptons, MET |
| `MTl2` | `sqrt(2 * leptons[1].pT * MET.MET * (1 - cos(MET.phi - leptons[1].phi)))` | leptons, MET |
| `Mtautau` | `fMtautau(leptons[0], leptons[1], MET)` | leptons, MET |

### Region: CharginoDimuonPresel

This is the only active region. It implements the chargino dimuon preselection following the cutflow table from the Les Houches recasting comparison study.

| # | Type | Statement | Physics Purpose |
|---|------|-----------|-----------------|
| 1 | weight | xsec = 0.688016 | Cross-section weight |
| 2 | select | size(muons) == 2 | Require exactly 2 muons |
| 3 | select | muons[0].pT in [5, 30] | Leading muon pT window |
| 4 | select | muons[0].charge * muons[1].charge == -1 | Opposite-sign requirement |
| 5 | select | dimuon.pT > 3 | Dimuon system pT |
| 6 | select | dimuon.mass in [4, 50] | Dimuon mass window |
| 7 | select | dimuon.mass outside [9, 10.5] | Upsilon veto (][ operator) |
| 8 | select | MET.MET in [125, 200] | MET window |
| 9 | weight | trigger = 0.65 | Trigger efficiency weight |
| 10 | select | size(jets) >= 0 | No-op (always true, cutflow placeholder) |
| 11 | select | size(jets) >= 1 | Require at least 1 jet (ISR) |
| 12 | select | HT > 100 | Minimum scalar pT sum |
| 13 | select | MET/HT in (0.6, 1.4) | MET significance (compound `and` condition) |
| 14 | select | size(bjets) == 0 | b-jet veto |
| 15 | select | Mtautau outside [0, 160] | Tau-tau veto (][ operator) |
| 16 | select | MTl1 < 70 and MTl2 < 70 | Transverse mass upper bounds |

**Observation:** Step 10 (`size(jets) >= 0`) is trivially true and serves no filtering purpose. It appears as a cutflow placeholder.

### Commented-Out Regions

Three additional regions are present but commented out:

- **DileptonPresel** -- Generalized dilepton preselection (Table 1 of paper)
- **CharginoDielectronPresel** -- Dielectron channel equivalent
- **StopDimuonPresel** -- Stop search dimuon preselection

---

## Dependency Graph

**18 nodes, 17 edges.** No cross-file dependencies. No warnings.

### Node Breakdown

| Type | Count | Names |
|------|-------|-------|
| Builtin | 4 | Muon, Electron, Jet, MissingET |
| Object | 6 | muons, electrons, leptons, jets, bjets, MET |
| Define | 7 | dilepton, dielectron, dimuon, HT, MTl1, MTl2, Mtautau |
| Region | 1 | CharginoDimuonPresel |

### Dependency Hierarchy

```
Builtins:     Muon       Electron       Jet       MissingET
                |            |            |            |
Objects:      muons     electrons       jets          MET
                 \        /    \         / \          / \
                  \      /      \       /   \        /   \
Objects:        leptons          bjets   HT  \      /     \
                / | \                         \    /       \
Defines:  dilepton  \   \                  MTl1, MTl2, Mtautau
              dielectron  dimuon

Region:   CharginoDimuonPresel
            (uses: muons, dimuon, jets, bjets, MET, HT, Mtautau, MTl1, MTl2)
```

### Edge Types

| Kind | Count | Description |
|------|-------|-------------|
| `take` | 7 | Object inheritance (e.g., muons takes from Muon) |
| `reference` | 10 | Variable/expression dependencies (e.g., HT references jets) |

---

## Object Disjointness Analysis

**15 pairs checked:** 12 disjoint, 3 possibly overlapping, 0 unknown.

### Disjoint Pairs (12)

All pairs involving objects from different particle types are trivially disjoint:

| Object A | Object B | Reason |
|----------|----------|--------|
| muons | electrons | Different particle types (muon vs electron) |
| muons | jets | Different particle types (muon vs jet) |
| muons | bjets | Different particle types (muon vs jet) |
| muons | MET | Different particle types (muon vs missinget) |
| electrons | jets | Different particle types (electron vs jet) |
| electrons | bjets | Different particle types (electron vs jet) |
| electrons | MET | Different particle types (electron vs missinget) |
| leptons | jets | Different particle types (electron,muon vs jet) |
| leptons | bjets | Different particle types (electron,muon vs jet) |
| leptons | MET | Different particle types (electron,muon vs missinget) |
| jets | MET | Different particle types (jet vs missinget) |
| bjets | MET | Different particle types (jet vs missinget) |

### Possibly Overlapping Pairs (3)

| Object A | Object B | Reason |
|----------|----------|--------|
| muons | leptons | `leptons` is a union containing `muons` -- overlap by construction |
| electrons | leptons | `leptons` is a union containing `electrons` -- overlap by construction |
| jets | bjets | `bjets` is a subset of `jets` (takes from jets + BTag filter) -- overlap by construction |

These overlaps are intentional and expected: `leptons` is the combined collection, and `bjets` is a strict subset of `jets`.

---

## Region Disjointness Analysis

Only one active region exists (`CharginoDimuonPresel`), so pairwise region disjointness analysis is not applicable. The three commented-out regions were not analyzed.

---

## Notes

- The `dilepton` and `dielectron` variables are defined but not used in the active region (they would be used in the commented-out regions).
- The analysis uses two weights: a cross-section weight (0.688016) and a trigger efficiency weight (0.65).
- Compound `and` conditions (lines 70, 73) were correctly parsed into separate structured cuts.
- The anti-range `][` operator (lines 64, 72) was correctly parsed with op `<>`, representing exclusion windows for the Upsilon resonance and tau-tau hypothesis vetoes.
