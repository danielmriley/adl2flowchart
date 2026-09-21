# Changelog

## 0.1.3

- Stop Cursor treating hep-to-adl as an Agent Plugin (skills-only). The
  Agent Plugins `$schema` manifest moved to `agent-plugins/plugin.json`.
  Cursor now loads `.cursor-plugin/plugin.json` like pstack.
- `skills/hep-to-adl/SKILL.md` is a mode skill (`mode: true`) so
  `/hep-to-adl` appears in the Agent slash menu like `/poteto-mode`.

## 0.1.2

- pstack-like `hep-to-adl-agent` plus Cursor `displayName` so Agent chat
  can route `/hep-to-adl` the same way `/poteto-mode` routes.

## 0.1.1

- Cursor-visible `/hep-to-adl` command and a glob-scoped `.adl` rule.
  Skills-only v0.1.0 loaded (`loadUserLocalPlugin hep-to-adl loaded`)
  but did not appear in Customize. Directory discovery, same as
  Course Kit: no explicit `commands` / `rules` / `skills` paths.

## 0.1.0

- First release: three skills, HepToAdlDraft, fixtures, local install.
