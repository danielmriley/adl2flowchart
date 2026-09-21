# Conversion playbook

Inventory → draft → emit → validate. The locked type is `HepToAdlDraft` in `schema/heptoadl-draft.schema.json`.

## 1. Inventory

Use `hep-code-read`. Read the files the user named. If they named a paper, inventory only cuts that appear in the text they provided.

Write a working list:

- collections and their parents
- object filters
- event filters, in order
- named kinematics
- histos / tables if booked
- gaps (commented ID, missing isolation formula, unseen helper)

Do not execute the analysis.

## 2. Draft

Fill every required field. `unresolved` and `assumptions` stay in the file even when empty.

Rules:

- A select is a string copied from evidence, rewritten only into ADL syntax (`pT(Jet) > 30`, not a new threshold)
- A parent region is listed in `parents`, not restated as duplicate selects
- `take` for unions is `Union(A, B)` or the tutorial `object name : Union(A, B)` form
- If two sources disagree, keep one in the draft and put the other in `unresolved`

Validate the JSON against the schema when `scripts/validate-manifest.py` can see it (any `fixtures/*.draft.json` is checked). For a user file, keep the same shape.

## 3. Emit

Use `adl-authoring`. One ADL file from one draft.

- Emit only filled fields
- Each unresolved item becomes `# TODO: <what> -- <why>` next to the relevant block, or a sibling `*.unresolved.md`
- Assumptions become a short comment block at the top, or a list in the sibling file
- Match `fixtures/ex01_selection.adl` for simple selections and `fixtures/CMS-SUS-21-009_slim.adl` for a real CMS skeleton

## 4. Validate

If `smash2` exists:

```
smash2 verify path/to/analysis.adl
```

Report the command and its stdout/stderr. Do not upgrade "it looks like the tutorials" into a smash2 result.

If `smash2` is missing, say so. Optionally compare structure to a fixture (object/region names, inheritance). That is a style check, not a proof.

## Discovery, not vendoring

https://gitlab.cern.ch/cms-analysis is a pointer for finding public analysis names. Do not clone it from this plugin. Do not add those trees under `examples/`.

## Done

Hand the user three artifacts: draft, ADL, unresolved. If they asked for one file, the ADL must still carry the TODOs.
