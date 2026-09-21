---
name: adl-authoring
description: Write or edit ADL using tutorial object/region style. Use when drafting, revising, or restyling ADL for smash2 or CutLang, especially object/take/select/reject/region blocks.
---

# ADL authoring

Voice follows the short goldens in `fixtures/` and `examples/tutorials/`. Grammar may move. When it does, record the change in `GRAMMAR_NOTES.md` at this plugin root. Until then, follow those examples.

Cheatsheet: `references/adl-cheatsheet.md`

## Prefer this idiom

```
object goodJets
  take Jet
  select pT(Jet) > 50
  select abs(eta(Jet)) < 2.4

region baseline
  select ALL
  select size(goodJets) >= 3

region singleelectron
  baseline
  select size(goodEles) == 1
```

- Function-on-object: `pT(Jet)`, `eta(Ele)`, `size(goodJets)`, `m(Muo[0] Muo[1])`
- Inclusive range: `m(Zeecand) [] 70 110`
- Exclusive / veto window: `][`
- Object input: `take Jet` (not `{obj}Pt` or `obj_i` unless the source already uses that dialect)
- Region inheritance: a bare parent name as the first statement
- Union: `object leptons : Union(goodEles, goodMuos)`
- Comments: `#` lines. Commented-out cuts become `# TODO:` or `*.unresolved.md`, never active selects

`ex04_syntaxes.adl` shows legal alternatives (`: Ele`, `{Ele}Pt`, `goodJets_1`). Do not mix them in new files.

## Block order

1. `info analysis` (and `info adl` when datatier / author is known)
2. `table` blocks
3. `object` blocks, parents before children
4. `define` aliases used more than once
5. `region` blocks, parents before children
6. `histo` only when the source books histograms. Histos are execution auxiliaries, not the selection.

## Emit from a draft

For each draft field, write the matching keyword. Do not add a cut that is not in `selects` / `rejects`. Put `unresolved` items after the block they belong to:

```
# TODO: photon ID -- source comments POGcutbasedlooseID; no working expression
```

If the draft `info` is empty, omit the info block or comment it. Do not invent a title.

## Grammar pointer

Keywords seen in the legacy scanner (read-only): `define`/`def`, `region`/`algo`, `object`/`obj`, `take`/`using`, `select`/`cut`, `reject`, `trigger`, `weight`, `bin`, `histo`, `info`, `table`, `Union`. Smash2 may tighten this. If a file in `examples/tutorials/` disagrees with an old keyword, the tutorial wins. Log the delta in `GRAMMAR_NOTES.md`.
