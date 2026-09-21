# Fixtures

Short goldens for the HEP→ADL plugin. They are copies or slims of in-repo tutorials, not a second CMS corpus.

| File | Role |
|---|---|
| `ex01_selection.adl` | Object / region / inheritance (`examples/tutorials/ex01_selection.adl`) |
| `ex03_objreco.adl` | `define` + LV add + mass window (`ex03_objreco.adl`) |
| `ex04_syntaxes.adl` | Legal dialects. Do not mix these in new files. |
| `ex01_selection.draft.json` | Filled `HepToAdlDraft` for ex01 |
| `CMS-SUS-21-009_slim.adl` | Slim CMS skeleton with `# TODO:` for commented cuts |
| `CMS-SUS-21-009_slim.draft.json` | Draft for that slim file |
| `CMS-SUS-21-009_slim.unresolved.md` | Sibling unresolved list |

Full tutorials: `examples/tutorials/*.adl`. Full CMS/ATLAS files: `examples/CMS/`, `examples/ATLAS/` if present. Do not copy those trees into this plugin.

`scripts/validate-manifest.py` checks that the `*.draft.json` files match `schema/heptoadl-draft.schema.json`.
