#!/usr/bin/env python3
"""Validate hep-to-adl manifests against Agent Plugins 1.0.0 and the locked draft type."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

PLUGIN_ROOT = Path(__file__).resolve().parents[1]
MANIFEST = PLUGIN_ROOT / "plugin.json"
SCHEMA = PLUGIN_ROOT / "schema" / "plugin.schema.json"
DRAFT_SCHEMA = PLUGIN_ROOT / "schema" / "heptoadl-draft.schema.json"
CURSOR_MANIFEST = PLUGIN_ROOT / ".cursor-plugin" / "plugin.json"
CLAUDE_MANIFEST = PLUGIN_ROOT / ".claude-plugin" / "plugin.json"
SKILLS_DIR = PLUGIN_ROOT / "skills"
COMMANDS_DIR = PLUGIN_ROOT / "commands"
RULES_DIR = PLUGIN_ROOT / "rules"
AGENTS_DIR = PLUGIN_ROOT / "agents"
FIXTURES_DIR = PLUGIN_ROOT / "fixtures"
EXPECTED_VERSION = "0.1.2"
REQUIRED_SKILLS = ("hep-to-adl", "adl-authoring", "hep-code-read")
REQUIRED_COMMANDS = ("hep-to-adl",)
REQUIRED_RULES = ("hep-to-adl",)
REQUIRED_AGENTS = ("hep-to-adl-agent",)
REQUIRED_FIXTURES = (
    "ex01_selection.adl",
    "ex03_objreco.adl",
    "ex04_syntaxes.adl",
    "CMS-SUS-21-009_slim.adl",
    "ex01_selection.draft.json",
)
FRONTMATTER = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)
COMPONENT_PATH_KEYS = ("commands", "rules", "skills", "agents")
CURSOR_DISPLAY_NAMES = ("HEP to ADL", "hep-to-adl")
CURSOR_CATEGORIES = ("developer-tools", "utilities")
CURSOR_TAGS = ("hep", "adl", "cms", "analysis")


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    raise SystemExit(1)


def load_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"missing {path.relative_to(PLUGIN_ROOT)}")
    except json.JSONDecodeError as exc:
        fail(f"{path.relative_to(PLUGIN_ROOT)} is not JSON: {exc}")


def require_jsonschema():
    try:
        from jsonschema import Draft202012Validator
    except ImportError:
        fail("jsonschema is required. Install with: pip install jsonschema")
    return Draft202012Validator


def validate_schema(instance: object, schema: object, label: str) -> None:
    validator_cls = require_jsonschema()
    validator_cls.check_schema(schema)
    errors = sorted(validator_cls(schema).iter_errors(instance), key=lambda e: list(e.path))
    if errors:
        details = "; ".join(f"{'/'.join(str(p) for p in e.path) or '<root>'}: {e.message}" for e in errors)
        fail(f"{label} failed schema: {details}")


def require_mapping(value: object, label: str) -> dict:
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def check_dual_manifest(portable: dict, host_path: Path) -> None:
    host = require_mapping(load_json(host_path), str(host_path.relative_to(PLUGIN_ROOT)))
    for key in ("name", "version", "description"):
        if host.get(key) != portable.get(key):
            fail(
                f"{host_path.relative_to(PLUGIN_ROOT)} {key}={host.get(key)!r} "
                f"does not match plugin.json {key}={portable.get(key)!r}"
            )


def parse_frontmatter(path: Path, required: tuple[str, ...]) -> dict[str, str]:
    rel = path.relative_to(PLUGIN_ROOT)
    if not path.is_file():
        fail(f"missing {rel}")
    text = path.read_text(encoding="utf-8")
    match = FRONTMATTER.match(text)
    if not match:
        fail(f"{rel} needs YAML frontmatter with {', '.join(required)}")
    fields: dict[str, str] = {}
    for line in match.group(1).splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        fields[key.strip()] = value.strip().strip("\"'")
    for key in required:
        if not fields.get(key):
            fail(f"{rel} frontmatter is missing {key}")
    return fields


def check_skill(name: str) -> None:
    fields = parse_frontmatter(SKILLS_DIR / name / "SKILL.md", ("name", "description"))
    if fields.get("name") != name:
        fail(f"skills/{name}/SKILL.md frontmatter name={fields.get('name')!r} expected {name!r}")


def check_command(name: str) -> None:
    fields = parse_frontmatter(COMMANDS_DIR / f"{name}.md", ("name", "description"))
    if fields.get("name") != name:
        fail(f"commands/{name}.md frontmatter name={fields.get('name')!r} expected {name!r}")


def check_rule(name: str) -> None:
    parse_frontmatter(RULES_DIR / f"{name}.mdc", ("description",))


def check_agent(name: str) -> None:
    fields = parse_frontmatter(AGENTS_DIR / f"{name}.md", ("name", "description"))
    if fields.get("name") != name:
        fail(f"agents/{name}.md frontmatter name={fields.get('name')!r} expected {name!r}")
    if fields.get("is_background") != "true":
        fail(f"agents/{name}.md frontmatter is_background={fields.get('is_background')!r} expected 'true'")


def check_cursor_chat_fields(host: dict) -> None:
    display = host.get("displayName")
    if display not in CURSOR_DISPLAY_NAMES:
        fail(
            f".cursor-plugin/plugin.json displayName={display!r} "
            f"expected one of {CURSOR_DISPLAY_NAMES}"
        )
    category = host.get("category")
    if category not in CURSOR_CATEGORIES:
        fail(
            f".cursor-plugin/plugin.json category={category!r} "
            f"expected one of {CURSOR_CATEGORIES}"
        )
    tags = host.get("tags")
    if not isinstance(tags, list) or set(tags) != set(CURSOR_TAGS):
        fail(f".cursor-plugin/plugin.json tags={tags!r} expected {list(CURSOR_TAGS)}")
    if host.get("skills") != "./skills/":
        fail(f".cursor-plugin/plugin.json skills={host.get('skills')!r} expected './skills/'")
    if host.get("agents") != "./agents/":
        fail(f".cursor-plugin/plugin.json agents={host.get('agents')!r} expected './agents/'")
    logo = host.get("logo")
    if logo is not None:
        if not isinstance(logo, str) or not logo:
            fail(".cursor-plugin/plugin.json logo must be a relative path string")
        logo_path = (PLUGIN_ROOT / logo).resolve()
        try:
            logo_path.relative_to(PLUGIN_ROOT.resolve())
        except ValueError:
            fail(f".cursor-plugin/plugin.json logo={logo!r} escapes the plugin root")
        if not logo_path.is_file():
            fail(f".cursor-plugin/plugin.json logo={logo!r} does not exist")


def check_declared_component_paths(host: dict, host_label: str) -> None:
    """If a host manifest lists commands/rules/skills/agents, those paths must exist."""
    for key in COMPONENT_PATH_KEYS:
        if key not in host:
            continue
        raw = host[key]
        paths = raw if isinstance(raw, list) else [raw]
        for item in paths:
            if not isinstance(item, str) or not item:
                fail(f"{host_label} {key} entries must be relative path strings")
            target = (PLUGIN_ROOT / item).resolve()
            try:
                target.relative_to(PLUGIN_ROOT.resolve())
            except ValueError:
                fail(f"{host_label} {key}={item!r} escapes the plugin root")
            if not target.exists():
                fail(f"{host_label} {key}={item!r} does not exist")


def main() -> None:
    portable = require_mapping(load_json(MANIFEST), "plugin.json")
    schema = require_mapping(load_json(SCHEMA), "schema/plugin.schema.json")
    validate_schema(portable, schema, "plugin.json")

    if portable.get("name") != "hep-to-adl":
        fail(f"plugin.json name={portable.get('name')!r} expected 'hep-to-adl'")
    if portable.get("version") != EXPECTED_VERSION:
        fail(f"plugin.json version={portable.get('version')!r} expected {EXPECTED_VERSION!r}")

    check_dual_manifest(portable, CURSOR_MANIFEST)
    check_dual_manifest(portable, CLAUDE_MANIFEST)
    cursor_host = require_mapping(load_json(CURSOR_MANIFEST), ".cursor-plugin/plugin.json")
    check_declared_component_paths(cursor_host, ".cursor-plugin/plugin.json")
    check_cursor_chat_fields(cursor_host)

    for skill in REQUIRED_SKILLS:
        check_skill(skill)
    for command in REQUIRED_COMMANDS:
        check_command(command)
    for rule in REQUIRED_RULES:
        check_rule(rule)
    for agent in REQUIRED_AGENTS:
        check_agent(agent)

    for fixture in REQUIRED_FIXTURES:
        path = FIXTURES_DIR / fixture
        if not path.is_file():
            fail(f"missing fixtures/{fixture}")

    draft_schema = require_mapping(load_json(DRAFT_SCHEMA), "schema/heptoadl-draft.schema.json")
    for draft_path in sorted(FIXTURES_DIR.glob("*.draft.json")):
        draft = load_json(draft_path)
        validate_schema(draft, draft_schema, str(draft_path.relative_to(PLUGIN_ROOT)))

    print("OK: plugin.json matches Agent Plugins 1.0.0")
    print("OK: Cursor and Claude Code host manifests match name/version/description")
    print("OK: Cursor displayName, skills, and agents are declared")
    print("OK: required skills, commands, rules, agents, and fixtures are present")
    print("OK: fixture drafts match HepToAdlDraft")


if __name__ == "__main__":
    main()
