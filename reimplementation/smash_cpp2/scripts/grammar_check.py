#!/usr/bin/env python3
"""Static checks on smash_cpp2/grammar.ebnf. Does not parse ADL.

Warns on undefined/unused names, left recursion, FIRST-set overlaps on
`|` alternatives (including grouped alternatives), quoted keywords
missing from the lexer, productions missing from method_map.txt, and
keyword-table drift vs grammar.ebnf.

Known LALR-hostile overlaps must be listed in grammar_hooks.txt — those
are the productions you implement as named parse_* hooks, not as
generated LALR.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EBNF = ROOT / "grammar.ebnf"
METHOD_MAP = ROOT / "method_map.txt"
HOOKS = ROOT / "grammar_hooks.txt"
LEXER_CPP = ROOT / "libs" / "syntax" / "src" / "lexer.cpp"
PARSER_CPP = ROOT / "libs" / "syntax" / "src" / "parser.cpp"
PARSER_HPP = ROOT / "libs" / "syntax" / "include" / "adl2" / "syntax" / "parser.hpp"
STMT_HPP = (
    ROOT / "libs" / "syntax" / "include" / "adl2" / "syntax" / "stmt_dispatch.hpp"
)

TERMINALS = {"ident", "number", "string", "integer", "EOF"}
CONTEXTUAL = {"bins"}
PUNCT = {
    "=",
    ",",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    "+",
    "-",
    "*",
    "/",
    "^",
    ">",
    "<",
    ">=",
    "<=",
    "==",
    "!=",
    "~=",
    "&&",
    "||",
    "and",
    "or",
    "not",
    "!",
    "?",
    ":",
    "|",
    "[]",
    "][",
    "+-",
}


def strip_comments(text: str) -> str:
    out = []
    i = 0
    while i < len(text):
        if text.startswith("(*", i):
            j = text.find("*)", i + 2)
            if j < 0:
                raise SystemExit("unclosed (* comment *)")
            i = j + 2
            continue
        out.append(text[i])
        i += 1
    return "".join(out)


TOKEN_RE = re.compile(
    r"\"([^\"]*)\"|'([^']*)'|[A-Za-z][A-Za-z0-9_-]*|[(){}\[\]|;=]|\.\.\."
)


def tokenize_rhs(rhs: str) -> list[str]:
    toks = []
    for m in TOKEN_RE.finditer(rhs):
        if m.group(1) is not None:
            toks.append('"' + m.group(1) + '"')
        elif m.group(2) is not None:
            toks.append("'" + m.group(2) + "'")
        else:
            toks.append(m.group(0))
    return toks


def parse_ebnf(text: str) -> dict[str, list[str]]:
    text = strip_comments(text)
    lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
    blob = "\n".join(lines)
    prods: dict[str, list[str]] = {}
    for m in re.finditer(
        r"(?m)^([A-Za-z][A-Za-z0-9_-]*)\s*=\s*(.*?)\s*;",
        blob,
        flags=re.S,
    ):
        prods[m.group(1)] = tokenize_rhs(m.group(2))
    return prods


def split_top_alts(tokens: list[str]) -> list[list[str]]:
    alts: list[list[str]] = []
    cur: list[str] = []
    depth = 0
    for t in tokens:
        if t in "([{":
            depth += 1
            cur.append(t)
        elif t in ")]}":
            depth -= 1
            cur.append(t)
        elif t == "|" and depth == 0:
            if cur:
                alts.append(cur)
            cur = []
        else:
            cur.append(t)
    if cur:
        alts.append(cur)
    return alts


def inner_groups(tokens: list[str]) -> list[list[str]]:
    """Top-level `(...)` groups (alternatives inside optional/repeat too)."""
    groups: list[list[str]] = []
    i = 0
    while i < len(tokens):
        t = tokens[i]
        if t in "([{":
            close = {"(": ")", "[": "]", "{": "}"}[t]
            inner: list[str] = []
            depth = 1
            i += 1
            while i < len(tokens) and depth:
                u = tokens[i]
                i += 1
                if u == t:
                    depth += 1
                    if depth > 1:
                        inner.append(u)
                elif u == close:
                    depth -= 1
                    if depth:
                        inner.append(u)
                else:
                    inner.append(u)
            groups.append(inner)
            groups.extend(inner_groups(inner))
            continue
        i += 1
    return groups


def first_of_seq(
    seq: list[str],
    prods: dict[str, list[str]],
    cache: dict[str, set[str]],
    visiting: set[str],
) -> set[str]:
    out: set[str] = set()
    nullable = True
    i = 0
    while i < len(seq) and nullable:
        t = seq[i]
        i += 1
        if t in "([{":
            close = {"(": ")", "[": "]", "{": "}"}[t]
            inner: list[str] = []
            depth = 1
            while i < len(seq) and depth:
                u = seq[i]
                i += 1
                if u == t:
                    depth += 1
                    if depth > 1:
                        inner.append(u)
                elif u == close:
                    depth -= 1
                    if depth:
                        inner.append(u)
                else:
                    inner.append(u)
            inner_first: set[str] = set()
            for alt in split_top_alts(inner) or [[]]:
                inner_first |= first_of_seq(alt, prods, cache, visiting)
            out |= inner_first - {"ε"}
            if t in "[{" or "ε" in inner_first:
                nullable = True
            else:
                nullable = False
            continue
        if t in ")]}|=":
            continue
        if t.startswith("'") or t.startswith('"'):
            out.add(t)
            nullable = False
            continue
        if t == "...":
            continue
        if t in prods:
            f = first_of_prod(t, prods, cache, visiting)
            out |= f - {"ε"}
            nullable = "ε" in f
            continue
        if t in TERMINALS:
            out.add(t)
            nullable = False
            continue
        out.add(t)
        nullable = False
    if nullable:
        out.add("ε")
    return out


def first_of_prod(
    name: str,
    prods: dict[str, list[str]],
    cache: dict[str, set[str]],
    visiting: set[str],
) -> set[str]:
    if name in cache:
        return cache[name]
    if name in visiting:
        return set()
    visiting.add(name)
    acc: set[str] = set()
    for alt in split_top_alts(prods[name]) or [[]]:
        acc |= first_of_seq(alt, prods, cache, visiting)
    visiting.remove(name)
    cache[name] = acc
    return acc


def alt_label(alt: list[str], prods: dict[str, list[str]]) -> str:
    for t in alt:
        if t.startswith("'") or t.startswith('"'):
            return t.strip("'\"")
        if t in prods or t in TERMINALS or re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*", t):
            if t not in "([{":
                return t
    return " ".join(alt[:3]) or "ε"


def left_recursive(name: str, prods: dict[str, list[str]]) -> bool:
    for alt in split_top_alts(prods[name]) or [[]]:
        for t in alt:
            if t in "([{":
                continue
            if t.startswith("'") or t.startswith('"') or t in ")]}|=":
                break
            return t == name
    return False


def quoted_words(prods: dict[str, list[str]]) -> set[str]:
    words = set()
    for toks in prods.values():
        for t in toks:
            if len(t) >= 2 and t[0] in "'\"" and t[-1] == t[0]:
                body = t[1:-1]
                if re.fullmatch(r"[A-Za-z][A-Za-z0-9]*", body):
                    words.add(body)
    return words


def lexer_keywords(path: Path) -> set[str]:
    text = path.read_text()
    return {m.group(1).lower() for m in re.finditer(r'\{\s*"([^"]+)"\s*,\s*TokKind::', text)}


def parser_symbols(paths: list[Path]) -> set[str]:
    text = "\n".join(p.read_text() for p in paths)
    return set(re.findall(r"\b([A-Za-z_][A-Za-z0-9_]*)\b", text))


def load_tab_map(path: Path, ncols: int) -> list[list[str]]:
    rows = []
    for raw in path.read_text().splitlines():
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) != ncols:
            raise SystemExit(f"{path.name}: bad row: {raw!r}")
        rows.append(parts)
    return rows


def table_quoted(path: Path, table: str) -> set[str]:
    text = path.read_text()
    # "select", RegionStmtHook::  or  "info", SectionHook::
    pat = rf'"([^"]+)"\s*,\s*{table}::'
    return set(re.findall(pat, text))


def first_quoted_of(name: str, prods: dict[str, list[str]], cache: dict[str, set[str]]) -> set[str]:
    out = set()
    for tok in first_of_prod(name, prods, cache, set()):
        if tok.startswith("'") or tok.startswith('"'):
            out.add(tok.strip("'\""))
    return out


def check_overlaps(
    name: str,
    tokens: list[str],
    prods: dict[str, list[str]],
    cache: dict[str, set[str]],
    hooks: set[tuple[str, str]],
    errors: list[str],
    warnings: list[str],
) -> None:
    alts = split_top_alts(tokens)
    if len(alts) < 2:
        return
    firsts = [first_of_seq(alt, prods, cache, set()) for alt in alts]
    labels = [alt_label(alt, prods) for alt in alts]
    for i in range(len(alts)):
        for j in range(i + 1, len(alts)):
            overlap = (firsts[i] & firsts[j]) - {"ε"}
            if not overlap:
                continue
            a, b = labels[i], labels[j]
            ok = (name, a) in hooks or (name, b) in hooks
            msg = f"FIRST overlap in `{name}`: {sorted(overlap)} ({a} vs {b})"
            if ok:
                warnings.append("hook (expected): " + msg)
            else:
                errors.append(msg + " — add a named hook in grammar_hooks.txt or change the grammar")


def main() -> int:
    errors: list[str] = []
    warnings: list[str] = []

    prods = parse_ebnf(EBNF.read_text())
    if "file" not in prods:
        errors.append("grammar.ebnf: missing start production `file`")

    names = set(prods)
    used: set[str] = {"file"}
    mentioned: set[str] = set()
    for toks in prods.values():
        for t in toks:
            if t in names:
                used.add(t)
            if re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*", t):
                mentioned.add(t)

    undefined = sorted(
        n for n in mentioned if n not in names and n not in TERMINALS
    )
    for u in undefined:
        errors.append(f"undefined name `{u}`")
    for u in sorted(names - used):
        warnings.append(f"unused production `{u}`")

    hooks = {(a, b) for a, b in load_tab_map(HOOKS, 2)}
    cache: dict[str, set[str]] = {}
    for name, toks in prods.items():
        if left_recursive(name, prods):
            errors.append(f"left-recursive production `{name}`")
        check_overlaps(name, toks, prods, cache, hooks, errors, warnings)
        for g in inner_groups(toks):
            check_overlaps(name, g, prods, cache, hooks, errors, warnings)

    used_hooks = set()
    # mark hooks that actually fired via warning text is awkward; check presence
    for prod, label in hooks:
        if prod not in prods:
            errors.append(f"grammar_hooks.txt: unknown production `{prod}`")
        used_hooks.add((prod, label))

    word_lits = quoted_words(prods)
    kws = lexer_keywords(LEXER_CPP)
    for w in sorted(word_lits):
        if w.lower() in {x.lower() for x in PUNCT}:
            continue
        if w.lower() in CONTEXTUAL:
            warnings.append(f"contextual keyword `{w}` is not a TokKind (hook)")
            continue
        if w.lower() not in kws:
            errors.append(f"quoted keyword `{w}` is not in lexer keyword_or_ident")

    mmap = {row[0]: (row[1], row[2]) for row in load_tab_map(METHOD_MAP, 3)}
    symbols = parser_symbols([PARSER_CPP, PARSER_HPP])
    for prod in sorted(names):
        if prod not in mmap:
            errors.append(f"production `{prod}` missing from method_map.txt")
            continue
        method, role = mmap[prod]
        if role not in {"generate", "hook"}:
            errors.append(f"method_map `{prod}`: role must be generate|hook")
        if method == "(lexer)":
            continue
        if method not in symbols:
            errors.append(
                f"method_map `{prod}` → `{method}` not found in parser.cpp/.hpp"
            )

    extra = sorted(set(mmap) - names)
    for e in extra:
        warnings.append(f"method_map extra production `{e}` (not in ebnf)")

    stmt_kws = table_quoted(STMT_HPP, "RegionStmtHook")
    section_kws = table_quoted(STMT_HPP, "SectionHook")
    region_first = first_quoted_of("region-stmt", prods, cache)
    section_first = first_quoted_of("section", prods, cache)

    for kw in sorted(region_first - stmt_kws - CONTEXTUAL):
        errors.append(
            f"region-stmt keyword `{kw}` is in grammar.ebnf FIRST but not stmt_dispatch.hpp"
        )
    for kw in sorted(stmt_kws - region_first):
        errors.append(
            f"stmt_dispatch.hpp keyword `{kw}` is not in grammar.ebnf region-stmt FIRST"
        )
    for kw in sorted(section_first - section_kws):
        errors.append(
            f"section keyword `{kw}` is in grammar.ebnf FIRST but not stmt_dispatch.hpp"
        )
    for kw in sorted(section_kws - section_first):
        errors.append(
            f"stmt_dispatch.hpp section keyword `{kw}` is not in grammar.ebnf section FIRST"
        )

    print(f"productions: {len(prods)}")
    print(f"quoted keywords: {sorted(word_lits)}")
    print(f"section table: {sorted(section_kws)}")
    print(f"stmt table: {sorted(stmt_kws)}")
    for w in warnings:
        print("warning:", w)
    for e in errors:
        print("error:", e)
    if errors:
        print(f"FAIL: {len(errors)} error(s), {len(warnings)} warning(s)")
        return 1
    print(f"PASS: {len(warnings)} warning(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
