# hep-to-adl (moved)

This directory is a pointer, not the plugin.

**Canonical source:** [danielmriley/skills/hep-to-adl](https://github.com/danielmriley/skills/tree/main/hep-to-adl)

[danielmriley/skills](https://github.com/danielmriley/skills) is the workspace for the HEP→ADL Cursor plugin (marketplace sibling of Course Kit). Do not edit a plugin copy in this repository.

This repo (`adl2flowchart`) keeps the analysis toolchain: `smash2` under `reimplementation/`, the shared ADL corpus under `examples/`, and the C++ / legacy parsers. smash2 and `examples/` stay here. Plugin work belongs in skills.

## Install

Clone [danielmriley/skills](https://github.com/danielmriley/skills) and run **that** tree's installer (copy, do not symlink):

```bash
# typical checkout: ~/.skills/skills  (or ~/src/skills)
git clone https://github.com/danielmriley/skills.git ~/.skills/skills
~/.skills/skills/hep-to-adl/scripts/install-local.sh
# dest: ~/.cursor/plugins/local/hep-to-adl
```

Then in Cursor: Reload Window and type `/hep-to-adl`. Write-up: [hep-to-adl/README.md](https://github.com/danielmriley/skills/blob/main/hep-to-adl/README.md) in skills.

`adl2flowchart/hep-to-adl/scripts/install-local.sh` is gone and is not authoritative.
