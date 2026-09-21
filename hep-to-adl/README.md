# hep-to-adl

Cursor plugin that teaches agents to convert HEP analysis code (C++/Python/other) into [ADL](https://cern.ch/adl) for smash2 / CutLang. Packaged like marketplace **pstack**: `.cursor-plugin/plugin.json` is what Cursor loads, `/hep-to-adl` is a **mode skill**, and `hep-to-adl-agent` is the routing target. No Marketplace listing. smash2 stays the product.

The locked intermediate type is `HepToAdlDraft` (`schema/heptoadl-draft.schema.json`). Emit ADL only from a filled draft. Missing cuts stay as `# TODO:` comments or a sibling `*.unresolved.md`.

## Usage

```
/hep-to-adl convert this CMSSW analyzer…
```

Same slash-menu path as `/poteto-mode`. The `hep-to-adl` mode skill routes to `hep-to-adl-agent`, which reads the skill, fills `HepToAdlDraft`, and emits ADL. `commands/hep-to-adl.md` stays as a backup slash entry. The other skills are steps, not extra `/` entries.

## Layout

```
hep-to-adl/
  .cursor-plugin/plugin.json  # Cursor primary (displayName, skills, agents, commands)
  .claude-plugin/plugin.json  # Claude Code plugin-dir mirror
  agent-plugins/plugin.json   # Agent Plugins 1.0.0 file — not on the Cursor scan path
  agents/hep-to-adl-agent.md  # routing target for /hep-to-adl
  commands/hep-to-adl.md      # backup slash command
  rules/hep-to-adl.mdc        # glob-scoped .adl rule
  skills/{hep-to-adl,adl-authoring,hep-code-read}/
  assets/logo.svg             # geometric mark (not a CERN logo)
  fixtures/                   # short goldens (ex01, ex03, ex04, slim CMS SUS)
  references/
  schema/
  scripts/validate-manifest.py
  scripts/install-local.sh
```

There is **no** root `plugin.json`. A root file with `$schema: https://agent-plugins.org/…` makes Cursor load the folder as an Agent Plugin (skills + MCP only) and ignore `commands/`, `rules/`, and mode-skill slash entries.

## Install

### Cursor

Copy to a real directory. Do not symlink the checkout out.

```bash
./scripts/install-local.sh --dry-run
./scripts/install-local.sh
# dest: ~/.cursor/plugins/local/hep-to-adl
```

Override with `--dest DIR` or `HEPTADL_INSTALL_DIR`. Then in Cursor: **Reload Window**, enable the local plugin if needed, and type `/hep-to-adl` in an Agent chat the same way you type `/poteto-mode`. Customize should list **HEP to ADL**.

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

The portable Agent Plugins 1.0.0 package, if you need it, is `agent-plugins/plugin.json`. Do not copy that file to the plugin root.

### Codex

Codex discovers `SKILL.md` trees. Copy the three skill folders:

```bash
mkdir -p ~/.codex/skills
cp -R skills/* ~/.codex/skills/
```

`~/.agents/skills` is the other user path Codex scans. Do not publish this plugin to the ChatGPT/Codex plugin directory.

## Validate

```bash
pip install jsonschema
python3 scripts/validate-manifest.py
```

That check refuses an Agent Plugins `$schema` at the plugin root, requires the Cursor manifest (`displayName`, `skills`, `agents`, `commands`), the `/hep-to-adl` mode skill, the backup command, the `.adl` rule, and `agents/hep-to-adl-agent.md`, and validates `fixtures/*.draft.json` against `HepToAdlDraft`. If `agent-plugins/plugin.json` is present it is checked against the vendored Agent Plugins schema.

## Commands

| Command | Opens |
|---|---|
| `/hep-to-adl` | **hep-to-adl** mode skill → **hep-to-adl-agent** (draft → ADL → unresolved; delegates to **hep-code-read** and **adl-authoring**) |

One slash entry. The other skills are steps, not extra `/` entries.

## Workflow

1. `/hep-to-adl` (mode skill) when the ask is HEP → ADL
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
