# Grammar notes

Follow `examples/tutorials/` in this repository and the short goldens under `fixtures/`.

Grammar changes land here. Until a note is added, tutorial files win over memory, blog posts, and older CutLang slides.

Read-only keyword aliases seen in `legacy_parser/adl/scanner.l` (may differ in smash2):

- `define` / `def`
- `region` / `algo`
- `object` / `obj` / `composite`
- `take` / `using`
- `select` / `command` / `cut` / `cmd`
- `reject`
- `histo`, `histoList`, `bin`, `info`, `table`, `trigger`, `weight`
- `Union` / `union`

Preferred spelling for new files is the left-hand keyword in each line, matching `ex01_selection.adl`.
