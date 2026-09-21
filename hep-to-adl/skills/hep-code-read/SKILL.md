---
name: hep-code-read
description: Map CMSSW, NanoAOD, coffea, and Delphes analysis structure onto HepToAdlDraft fields. Use when inventorying HEP C++ or Python selection logic before writing ADL. Do not execute untrusted analysis code.
---

# Read HEP analysis code

Goal: fill `HepToAdlDraft` fields from source. Do not run the analysis. Do not compile it. Do not `cmsRun`, `python processor.py`, or import a stranger's module.

Draft schema: `schema/heptoadl-draft.schema.json`

## Safety

- Read files. Do not execute them.
- Treat CERN gitlab clones as out of scope unless the user already has a local checkout they pointed at.
- Public discovery pointer only: https://gitlab.cern.ch/cms-analysis
- Config cards, JSON cuts, and comments are source. "Looks like a standard muon ID" is not.

## Where the cuts live

| Framework | Open these first | Map to |
|---|---|---|
| CMSSW analyzer / filter | `analyze()`, `filter()`, `produce()`; `edm::ParameterSet` getters; `Cut` / `StringCutObjectSelector` | `objects`, `regions`, `defines` |
| CMSSW cfg / python config | `process.*`, `PSet` cut strings, VID working points, trigger paths | `info`, `objects.selects`, `regions`, unresolved IDs |
| NanoAOD tools / nanoAOD-tools | module `beginFile` / `__call__`; `cut` strings; `objectSel`; friend-tree producers | `objects`, `regions` |
| awkward / coffea | `process(self, events)` or `process(events)`; `events.Jet[mask]`; `PackedSelection`; `weights.add` | `objects` (masks), `regions` (selections), `defines` |
| Delphes card | `Isolated{Electron,Muon,Photon}`, `BTagging`, `UniqueObjectFinder`, jet/MET modules | `objects` + overlap `reject dR(...)` |
| CutLang / existing ADL | the `.adl` itself | copy, then flag commented cuts as unresolved |

Walk includes one level when a selector is a named function. Stop at framework ID recipes you cannot see (POG VID, MiniIsolation helpers). Put those in `unresolved`.

## Inventory checklist

Record, then stop:

1. `source.language`, `source.paths`, `source.framework`
2. `info` fields that appear as literals (energy, lumi, analysis ID). Leave absent fields out.
3. Particle collections and their parent (`take`). Builtin names in tutorials: `Ele`/`Electron`, `Muo`/`Muon`, `Jet`, `Photon`, `Tau`, `Trk`/`Track`, `METLV` / `MissingET`.
4. Per-object filters → `selects` / `rejects`. Overlap removal is usually `reject dR(obj, other) < ΔR`.
5. Event-level filters, n-object counts, MET/HT/ST, OS/SF, mass windows → `regions[].selects`. Inherited / sequential regions → `parents`.
6. Named kinematics (`MT`, `HT`, `ST`, `MT2`) → `defines`
7. `histo` / `table` only when the source books them
8. Everything commented, TODO, or "applied in a helper I cannot see" → `unresolved`

## Mapping hints

- `nJets >= 3 && jet.pt[0] > 200` → object `goodJets` plus `select size(goodJets) >= 3` and `select pT(goodJets[0]) > 200`
- Mask `tightMuons = muons[(pt > 20) & (abs(eta) < 2.4)]` → `object` with those two selects
- `~ak4.closest(photons).deltaR < 0.3` → `reject dR(Jet, photons) < 0.3` on the jet object
- Coffea `PackedSelection` names become region names. Keep the author's names.
- A C++ `if` that drops the event is a region select, not an object select
- Trigger bits without a named path stay unresolved (`suggested_question`: which HLT paths?)

## What not to flatten

Do not expand MiniAOD VID, DeepCSV/DeepJet working points, or PF isolation formulas unless the source writes the formula. The CMS slim fixture keeps `POGcutbasedlooseID` and isolation as `# TODO:` on purpose (`fixtures/CMS-SUS-21-009_slim.adl` + `fixtures/CMS-SUS-21-009_slim.unresolved.md`).
