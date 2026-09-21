# hep-to-adl

Agent Plugin that teaches agents to convert HEP analysis code (C++/Python/other) into [ADL](https://cern.ch/adl) for smash2 / CutLang. Skills plus a Cursor `/hep-to-adl` command and a glob-scoped `.adl` rule so Customize can list the plugin. No Marketplace listing. smash2 stays the product.

The locked intermediate type is `HepToAdlDraft` (`schema/heptoadl-draft.schema.json`). Emit ADL only from a filled draft. Missing cuts stay as `# TODO:` comments or a sibling `*.unresolved.md`.

## Layout

```
hep-to-adl/
  plugin.json                 # Agent Plugins 1.0.0 source of truth
  .cursor-plugin/plugin.json  # Cursor local-load mirror
  .claude-plugin/plugin.json  # Claude Code plugin-dir mirror
  commands/hep-to-adl.md      # slash command /hep-to-adl
  rules/hep-to-adl.mdc        # glob-scoped .adl rule
  skills/{hep-to-adl,adl-authoring,hep-code-read}/
  fixtures/                   # short goldens (ex01, ex03, ex04, slim CMS SUS)
  references/
  schema/
  scripts/validate-manifest.py
  scripts/install-local.sh
```

## Install

### Cursor

Copy to a real directory. Do not symlink the checkout out.

```bash
./scripts/install-local.sh --dry-run
./scripts/install-local.sh
# dest: ~/.cursor/plugins/local/hep-to-adl
```

Override with `--dest DIR` or `HEPTADL_INSTALL_DIR`. Then enable the local plugin in Cursor and reload. Customize should list hep-to-adl (commands and rules). Type `/hep-to-adl` in an Agent chat. Root `plugin.json` is the portable manifest. `.cursor-plugin/plugin.json` mirrors name, version, and description. Cursor discovers `commands/`, `rules/`, and `skills/` from those folders (Course Kit style; no explicit paths).

Do not submit this folder to cursor.directory or the Cursor Marketplace.

### Claude Code

Point Claude Code at this plugin root (the directory that contains `skills/` and `.claude-plugin/plugin.json`):

```bash
claude --plugin-dir /path/to/adl2flowchart/hep-to-adl
```

Or copy skills into the user skills path:

```bash
mkdir -p ~/.claude/skills
cp -R skills/* ~/.claude/skills/
```

### Codex

Codex discovers `SKILL.md` trees. Copy the three skill folders:

```bash
mkdir -p ~/.codex/skills
cp -R skills/* ~/.codex/skills/
```

`~/.agents/skills` is the other user path Codex scans. Root `plugin.json` is the Agent Plugins 1.0.0 package. Do not publish this plugin to the ChatGPT/Codex plugin directory.

## Validate

```bash
pip install jsonschema
python3 scripts/validate-manifest.py
```

That check loads the vendored Agent Plugins schema in `schema/plugin.schema.json`, matches Cursor/Claude host manifests, requires the `/hep-to-adl` command and `.adl` rule, and validates `fixtures/*.draft.json` against `HepToAdlDraft`.

## Commands

| Command | Opens |
|---|---|
| `/hep-to-adl` | **hep-to-adl** (draft → ADL → unresolved; delegates to **hep-code-read** and **adl-authoring**) |

One slash command. The other skills are steps, not extra `/` entries.

## Workflow

1. `/hep-to-adl` (or the `hep-to-adl` skill) when the ask is HEP → ADL
2. `hep-code-read` to inventory CMSSW / NanoAOD / coffea / Delphes without running the code
3. Fill a draft
4. `adl-authoring` to emit tutorial-style `object` / `take` / `select` / `reject` / `region`
5. If `smash2` is on `PATH`, run `smash2 verify <file>` and quote the output. If it is missing, say so.

Public CMS analysis names (do not vendor or clone from here): https://gitlab.cern.ch/cms-analysis

In-repo corpus to read, not to duplicate: `examples/tutorials/`, `examples/CMS/`.

## Out of scope

- smash2 / Rust / Z3 / `examples/golden`
- Auto-cloning CERN gitlab
- Inventing physics results
- An MCP server. A parse/verify wrapper can wait until it can wrap `smash2` with zero false confidence.
