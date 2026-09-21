---
name: hep-to-adl
description: Convert HEP analysis code or notes into ADL. Use when asked to convert this analysis to ADL, write ADL for a CMSSW/NanoAOD/coffea/Delphes selection, or translate C++/Python HEP cuts into object/region blocks.
---

# HEP → ADL

Fill `HepToAdlDraft` first. Emit ADL only from a filled draft. Never invent a physics cut.

Schema: `schema/heptoadl-draft.schema.json`
Playbook: `references/conversion-playbook.md`
Style: delegate writing to `adl-authoring`
Code skim: delegate inventory to `hep-code-read`

## When

- "convert this analysis to ADL"
- "write ADL for …"
- HEP code or paper notes → CutLang / smash2 description

## Do not

- Guess missing pT, eta, isolation, ID, trigger, or overlap cuts
- Silent-fill from "typical CMS" or a nearby analysis
- Vendor or clone https://gitlab.cern.ch/cms-analysis (public discovery only)
- Touch `reimplementation/` or `legacy_parser/` sources
- Claim a smash2 proof unless you ran `smash2 verify` and quote its output

## Draft type

```
HepToAdlDraft = {
  source: { language, paths[], framework? },
  info: { title?, experiment?, id?, sqrtS?, lumi?, datatier? },
  objects: [{ name, take, selects[], rejects[], notes? }],
  regions: [{ name, parents[], selects[], notes? }],
  defines: [{ name, expr }],
  histos?: [{ name, … }],
  tables?: [{ name, … }],
  unresolved: [{ what, why, suggested_question? }],
  assumptions: [string]
}
```

`unresolved` is required. Empty array only when every cut in the source is in the draft.

## Steps

1. Inventory with `hep-code-read`. Record objects, defines, regions, histos, tables, and every cut you cannot map.
2. Write a draft JSON that validates against the schema. Keep it next to the output ADL as `*.draft.json` when the user wants artifacts.
3. Emit tutorial-style ADL from that draft only (`adl-authoring`). Put each unresolved item in the ADL as `# TODO: <what> -- <why>` or in a sibling `*.unresolved.md`. Do both for long lists.
4. Validate if a binary is present. Prefer `smash2 verify <file>`. If `smash2` is missing, say so and stop. Do not treat parse-by-eye as a proof.

## In-repo goldens

Read these before emitting. Do not copy the whole CMS tree into the plugin.

- Plugin short goldens: `fixtures/ex01_selection.adl`, `fixtures/ex03_objreco.adl`, `fixtures/ex04_syntaxes.adl`, `fixtures/CMS-SUS-21-009_slim.adl`
- Filled draft example: `fixtures/ex01_selection.draft.json`
- Full tutorials: `examples/tutorials/*.adl` in this repo
- Real analyses: `examples/CMS/*.adl` (style and scale, not a dump target)

Public CMS analysis trees for discovery (do not vendor): https://gitlab.cern.ch/cms-analysis

## Output order

1. The filled draft (JSON or a markdown table of the same fields)
2. The ADL text
3. The unresolved list and the assumptions list

If the user only wants ADL, still keep unresolved as `# TODO:` comments inside that file.
