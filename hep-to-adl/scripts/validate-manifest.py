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
FIXTURES_DIR = PLUGIN_ROOT / "fixtures"
REQUIRED_SKILLS = ("hep-to-adl", "adl-authoring", "hep-code-read")
REQUIRED_FIXTURES = (
    "ex01_selection.adl",
    "ex03_objreco.adl",
    "ex04_syntaxes.adl",
    "CMS-SUS-21-009_slim.adl",
    "ex01_selection.draft.json",
)
FRONTMATTER = re.compile(r"^---\n(.*?)\n---\n", re.DOTALL)


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


def check_skill(name: str) -> None:
    skill_md = SKILLS_DIR / name / "SKILL.md"
    if not skill_md.is_file():
        fail(f"missing skills/{name}/SKILL.md")
    text = skill_md.read_text(encoding="utf-8")
    match = FRONTMATTER.match(text)
    if not match:
        fail(f"skills/{name}/SKILL.md needs YAML frontmatter with name and description")
    fields = {}
    for line in match.group(1).splitlines():
        if ":" not in line:
            continue
        key, value = line.split(":", 1)
        fields[key.strip()] = value.strip()
    if fields.get("name") != name:
        fail(f"skills/{name}/SKILL.md frontmatter name={fields.get('name')!r} expected {name!r}")
    if not fields.get("description"):
        fail(f"skills/{name}/SKILL.md frontmatter is missing description")


def main() -> None:
    portable = require_mapping(load_json(MANIFEST), "plugin.json")
    schema = require_mapping(load_json(SCHEMA), "schema/plugin.schema.json")
    validate_schema(portable, schema, "plugin.json")

    if portable.get("name") != "hep-to-adl":
        fail(f"plugin.json name={portable.get('name')!r} expected 'hep-to-adl'")
    if portable.get("version") != "0.1.0":
        fail(f"plugin.json version={portable.get('version')!r} expected '0.1.0'")

    check_dual_manifest(portable, CURSOR_MANIFEST)
    check_dual_manifest(portable, CLAUDE_MANIFEST)

    for skill in REQUIRED_SKILLS:
        check_skill(skill)

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
    print("OK: required skills and fixtures are present")
    print("OK: fixture drafts match HepToAdlDraft")


if __name__ == "__main__":
    main()
