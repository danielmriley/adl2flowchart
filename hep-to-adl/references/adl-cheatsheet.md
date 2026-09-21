# ADL cheatsheet

Tutorial voice and operators come from `examples/tutorials/` and arXiv:2101.09031 (ADL / CutLang). This page is a pointer, not a second grammar.

## Keywords (new files)

| Keyword | Role |
|---|---|
| `info analysis` / `info adl` | Metadata (`title`, `experiment`, `id`, `sqrtS`, `lumi`, `datatier`, …) |
| `object NAME` | Particle collection |
| `take INPUT` | Parent collection (builtin or earlier object) |
| `select EXPR` | Keep objects or events that pass |
| `reject EXPR` | Drop objects or events that match |
| `define NAME = EXPR` | Alias used more than once |
| `region NAME` | Event selection; may inherit a parent region by naming it |
| `histo` | Booking (execution auxiliary) |
| `bin` | Disjoint partition of a region (part of the physics algorithm) |
| `table` | Efficiency / weight lookup |
| `weight`, `trigger` | Event weights and trigger requirements |

See `GRAMMAR_NOTES.md` for aliases and where grammar deltas go.

## Builtin collections (tutorials)

`Ele` / `Electron`, `Muo` / `Muon`, `Tau`, `Photon` / `PHO`, `Jet` / `JET`, `FJet` (fat jet in some CMS files), `Trk` / `Track`, `METLV` / `MissingET`. Case is treated loosely in CutLang. New files should pick one spelling per file.

## Object patterns

```
object goodJets
  take Jet
  select pT(Jet) > 50
  select abs(eta(Jet)) < 2.4

object goodbJets
  take goodJets
  select BTag(goodJets) == 1

object leptons : Union(goodEles, goodMuos)

object AK4jets
  take Jet
  select pT(Jet) > 30
  reject dR(Jet, photons) < 0.3
```

Goldens: `fixtures/ex01_selection.adl`, `fixtures/CMS-SUS-21-009_slim.adl`.

## Region patterns

```
region baseline
  select ALL
  select size(goodJets) >= 3
  select pT(goodJets[0]) > 200

region singleelectron
  baseline
  select size(goodEles) == 1
```

`bin` lines partition a region and must not overlap. Histograms are not bins (`ex02_histograms.adl` vs `ex06_bins.adl`).

## Operators

From tutorial comments and arXiv:2101.09031 Table 12 / A.5.2:

- Compare: `>`, `<`, `>=`, `<=`, `==`, `!=`, `~=`
- Inclusive range: `X [] low high`
- Exclusive / outside: `X ][ low high`
- Logic: `and`, `or`
- Ternary: `cond ? a : b`
- LV add: `Ele[0] + Ele[1]` or juxtaposition `Ele[0] Ele[1]`
- Index: `jets[0]` (leading pT). Slice: `jets[0:1]`
- Common functions: `pT`, `eta`, `phi`, `m`, `q`, `abs`, `sqrt`, `size`, `sum`, `min`, `dR`, `dPhi` / `dphi`, `dEta`

## Reconstruction

`define Zeecand = Ele[0] + Ele[1]` then `select m(Zeecand) [] 70 110` (`fixtures/ex03_objreco.adl`).

## Syntax dialects (do not mix)

`fixtures/ex04_syntaxes.adl`: `obj[i]` vs `obj_i`; `pT(obj)` vs `{obj}Pt`; `take Jet` vs `object x : Jet`.

## arXiv:2101.09031

- Table 7: predefined objects
- Table 9 / 11: HEP and math functions
- Table 12: comparison / range / logic
- §4.5 / A.10.2: weights
- §4.6 / A.9.9: hit-and-miss efficiencies
- A.8: tables

## smash2

When `smash2` is on `PATH`, parse/verify with `smash2 verify <file>`. Quote the tool output. Absence of the binary is not a pass.
