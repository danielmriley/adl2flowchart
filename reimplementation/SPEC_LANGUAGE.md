# ADL2 language specification

Status: DRAFT v0.1 — to be frozen at the end of Phase 0 (see PLAN.md).
ADL has no official formal spec; this document *is* the spec for ADL2's
checked fragment, validated two ways in Phase 0: (a) the grammar must
parse the full `examples/` corpus (68 files), (b) every semantic claim
marked **[DECIDE]** must be ratified as a project decision (Daniel +
collaborators) and recorded here before freeze. The ADL2 reference
interpreter (adl-interp) is the authoritative semantics of this fragment;
where the spec is ambiguous, the project decides and records the choice
rather than probing an external tool.

## 1. Design stance

ADL2 parses a **declared fragment** of ADL precisely rather than
best-effort-parsing everything. Constructs outside the fragment produce a
structured diagnostic and an `Unsupported` AST node — never a silent
guess. The legacy lexer's accommodations (hyphens inside identifiers,
signed-number lexing, dotted-identifier tokens) are replaced by grammar-
level constructs, with explicit compatibility notes below.

## 2. Lexical structure

- **Encoding/whitespace**: UTF-8; space/tab separate tokens. Newlines
  are insignificant *except* as terminators for the four greedy
  token-sequence productions (`info-line`, `boundary-list`, `counts-stmt`
  tails, table rows), which end at end-of-line or at the next statement
  keyword, whichever comes first. The lexer emits a NEWLINE token that
  only those productions consult; all spans carry line/column.
- **Comments**: `#` to end of line.
- **Identifiers**: `[A-Za-z][A-Za-z0-9]*` segments joined by `_` only
  when the next character is a letter:
  `ident = [A-Za-z][A-Za-z0-9]* { "_" [A-Za-z][A-Za-z0-9]* }`.
  An `_` followed by a digit is NOT part of the identifier — it is the
  underscore-indexing operator (`goodJets_1` lexes as `goodJets` `_` `1`;
  live in the corpus: ex04, ex10, CMS-SUS-16-033). `m_T2`, `HLT_iso_mu`
  remain single identifiers. The lexer emits a note when an identifier is
  split this way so ambiguous intent is visible. Case is preserved in
  diagnostics; resolution is case-insensitive **[DECIDE]** (legacy corpus
  mixes `Size`/`size`, `pT`/`pt`).
- **Keywords** (reserved, case-insensitive): `define def object obj
  composite take using select cut cmd command reject region algo bin
  histo histoList weight trigger info table tabletype nvars errors union
  process counts countsformat print save sort all none and or not true
  false`.
- **Numbers**: unsigned `[0-9]+` (int) and `[0-9]+ "." [0-9]+` (real).
  No scientific notation in v1 (corpus-checked: none used); `1e6` is a
  lexical error with a "write 1000000.0" help.
  **No signed-literal lexing**: negation is the grammar's unary minus,
  valid in every numeric position (expressions, range bounds, bin
  boundaries, table cells). This removes the legacy `5-3`/table-cell
  ambiguity class wholesale.
- **Strings**: `"..."`, no escapes needed in v1; used for descriptions
  and for **file-path arguments** (e.g. BDT weight files). *Compat note*:
  legacy files pass weight files as bare hyphenated tokens
  (`TMVA_BDT.weights-2016-....xml`); the lexer accepts a bare
  `path-like` token only as a function argument, with a deprecation
  warning suggesting quotes.
- **Operators/punctuation**: `> < >= <= == != ~= + - * / ^ = ? : ( ) [ ]
  { } | , . _` and the range operators `[]` (inclusive band) and `][`
  (excluded band).
- Any other character is a lexical error with span; the lexer recovers by
  skipping it and continues (multi-error reporting).

## 3. Grammar (EBNF)

Hand-written recursive descent (see SPEC_ARCHITECTURE §3 for why). The
grammar below is the contract; the parser is structured one function per
nonterminal so the code audits against this document line by line.

```ebnf
file            = { section } EOF ;
section         = info-block | table-block | countsformat-block
                | define | object-block | region-block ;

info-block      = "info" ident { info-line } ;
define          = ("define"|"def") ident ("="|":") condition ;

object-block    = ("object"|"obj"|"composite"|"trigger") ident
                  { take-stmt | cut-stmt } ;
take-stmt       = ("take"|"using"|":") take-source ;
take-source     = ident
                | ident "(" arg-list ")"
                | "union" "(" ident { "," ident } ")" ;

region-block    = ("region"|"algo"|"histoList") ident { region-stmt } ;
region-stmt     = cut-stmt | reject-stmt | bin-stmt | weight-stmt
                | trigger-stmt | histo-stmt | save-stmt | counts-stmt
                | region-ref ;
cut-stmt        = ("select"|"cut"|"cmd"|"command") condition ;
reject-stmt     = "reject" condition ;
region-ref      = ident ;                     (* names a prior region *)
bin-stmt        = "bin" [ string ] bin-body ;
bin-body        = postfix boundary-list       (* boundary-list binning *)
                | condition ;                 (* boolean bin *)
boundary-list   = signed-num signed-num { signed-num } ;
trigger-stmt    = "trigger" condition ;
histo-stmt      = "histo" ident "," string { "," histo-arg } ;

condition       = ternary ;
ternary         = or-expr [ "?" ternary [ ":" ternary ] ] ;
or-expr         = and-expr { ("or"|"||") and-expr } ;
and-expr        = not-expr { ("and"|"&&") not-expr } ;
not-expr        = ("not"|"!") not-expr | comparison ;
comparison      = additive [ cmp-op additive
                           | "[]" signed-num signed-num
                           | "][" signed-num signed-num ] ;
cmp-op          = ">" | "<" | ">=" | "<=" | "==" | "!=" | "~=" ;
additive        = multiplicative { ("+"|"-") multiplicative } ;
multiplicative  = unary { ("*"|"/"|"^") unary } ;
unary           = "-" unary | postfix ;
postfix         = primary { "." ident
                          | "[" index [ ":" index ] "]"
                          | "_" index [ ":" index ] } ;
primary         = number | ident | func-call
                | "(" condition ")"
                | "|" additive "|"                  (* abs *)
                | "{" arg-list "}" ident ;          (* braced property *)
func-call       = ident "(" [ arg-list ] ")" ;
arg-list        = arg { "," arg } ;
arg             = particle-list | condition | string | path-token ;
particle-list   = postfix postfix { postfix } ;   (* >=2 adjacent object
                     refs form one ParticleList node: pT(jets[0] jets[1]),
                     comb(...) args — divergence 7 *)
path-token      = (* bare weight-file token containing "-"/"."/"/";
                     only valid as an arg; deprecation warning *) ;
index           = [ "-" ] integer ;               (* negative pending OPEN-3 *)
signed-num      = [ "-" ] number ;

info-line       = ident { ident | string | signed-num } ;
table-block     = "table" ident "tabletype" ident "nvars" integer
                  "errors" ("true"|"false") { signed-num } ;
countsformat-block = "countsformat" ident
                  { "process" ident "," string { "," ident } } ;
weight-stmt     = "weight" ( ident | "trigger" )
                  ( signed-num | ident | func-call ) ;
histo-arg       = signed-num | condition
                | "[" signed-num { signed-num } "]" ;
print-stmt      = "print" arg-list ;
save-stmt       = "save" ident ident arg-list ;
counts-stmt     = "counts" ident { signed-num | ident | "+" | "-" | "+-" } ;
sort-stmt       = "sort" (* consumed to end of statement *) ;
(* Region-level sort semantics (proof-system v2 Phase 5, owner-approved):
   a recognized `sort prop(coll) [ascend|descend]` (or `coll.prop` form)
   RE-BINDS `coll` for the region's SUBSEQUENT statements to the
   re-sorted view — `coll[i]` then means the i-th element of the sorted
   sequence, matching CutLang's operational behavior. Both tools carry
   the same lowering: the verifier gets a distinct Sorted identity (no
   ordering facts unless the sort is provably the identity permutation —
   descending-pT of an already-pT-descending source), the interpreter
   materializes the view by actually re-sorting. Statements BEFORE the
   sort keep the original binding. Any unrecognized sort shape falls
   back to fail-closed: subsequent element-indexed statements of that
   region leave the checked fragment. *)
```
`region-stmt` additionally includes `print-stmt` and `sort-stmt`.
A bare `ident` region statement must resolve (in sema) to a prior region
(inheritance) or to a boolean define (sugar for `select ident`); any
other resolution is a diagnostic — never a silent no-op.

### 3.1 Deliberate divergences from the legacy grammar

| # | Divergence | Rationale | Compat handling |
|---|---|---|---|
| 1 | `or` binds looser than `and` (legacy: same precedence, right-assoc, so `a and b or c` parsed as `a and (b or c)`) | Standard precedence; the legacy reading is a correctness trap | Phase-0 corpus scan for mixed and/or chains without parens; if any parse differently, emit a warning lint and list them in the parity report |
| 2 | `not` is properly recursive (`not not x`, `not (a or b)`, `define x = not y` all parse; all are syntax errors today) | The legacy `chain : not condition` rule was a bolt-on | none needed (strictly more accepted) |
| 3 | Dotted access is grammar (`postfix . ident`), not a lexed token (`a.b.c` one token today) | Lexer-level dots blocked spans, indexing (`jets[0].pt`), and made `NOT.jets`-style garbage possible | identical strings re-parse to identical meaning |
| 4 | Unsigned literals + unary minus everywhere | Kills the `5-3` token bug and table/bin signed-cell hacks | identical meaning; corpus gate |
| 5 | Bin boundaries are reals | Legacy truncates `300.7` to `300` via the int accessor vector | strictly better |
| 6 | Multi-arg `union(a,b,c)` | corpus uses two args; cheap generality | superset |
| 7 | Space-separated function args (`pT(jets[0] jets[1])`, comb args) parse as an explicit **particle-list** argument node, not an identifier mash | legacy folded these into broken VarNodes | same surface syntax, real AST |

### 3.2 Error recovery

The parser never aborts on first error: on failure inside a statement it
records a diagnostic with the statement's span and resynchronizes at the
next statement keyword. Exit code is nonzero if any error was recorded.
Diagnostics carry span + label + help text (e.g. "`selct` is not a
keyword; did you mean `select`?").

## 4. Semantics

### 4.1 Event model

An **event** is: for each base collection (Jet, Electron, Muon, Tau,
Photon, Track, ...), a finite ordered list of objects with real-valued
properties (`pt, eta, phi, m, e, charge, btag, ...`); plus event scalars
(MET vector → `MET.pt`, `MET.phi`; scalar HT; trigger flags ∈ {0,1}).
Collections are **pT-descending ordered** **[DECIDE]**; indices are
0-based **[DECIDE: 1-based or `[-1]`-supporting is a possible reading]**.

### 4.2 Objects

`object D take S <cuts>` defines collection `D` = elements of `S`
passing all cuts, **order preserved**. Cuts inside an object block are
per-element predicates; the element is the implicit subject (`select
pt > 30` means "this element's pt"). `take union(A,B)` concatenates
**[DECIDE: dedup/ordering of union]**. An object block with a single
take and no cuts is a pure rename (identity with its source — this is a
*theorem in the semantics*, which is what licenses the analyzer's
pure-alias unification).

### 4.3 Regions

A region is a per-event predicate: the conjunction, in order, of its
statements. `select c` contributes `c`; `reject c` contributes `¬c`;
a bare region name inlines that region's predicate (inheritance);
`trigger t` contributes the trigger flag; `weight`/`histo`/`save`
contribute nothing to membership. `bin` statements partition the
region's events and do not constrain membership; a boundary-list
`bin v b0 b1 … bn` denotes bins `[b0,b1), …, [bn-1,bn), [bn,∞)`
**[DECIDE: last-bin openness]**.

### 4.4 Expressions

Ternary `g ? a : b` = `(g ∧ a) ∨ (¬g ∧ b)`; missing/`ALL` branch is
`true`. `x [] lo hi` = `lo ≤ x ≤ hi`; `x ][ lo hi` = `x ≤ lo ∨ x ≥ hi`.
Defines are textual-scope-free named expressions; boolean defines are
predicates, numeric defines are event scalars; recursion is an error.
Division by zero / non-finite arithmetic: the enclosing comparison is
**false** (the event fails the cut) **[DECIDE]** — the verifier already
assumes this; the interpreter must implement the verified answer.

### 4.5 Open semantic questions (block spec freeze)

| ID | Question | Resolution procedure |
|---|---|---|
| OPEN-1 | `select pt(jets) > 30` at *region* level: per-element ∀, ∃, or error? | DECIDE: project decision with collaborators; if the chosen reading is an error, ADL2 makes it a diagnostic; else encode exactly (no more Dual hedge). Until decided, the convention-neutral strategy stands |
| OPEN-2 | `dPhi`/`dEta` convention: signed or absolute? `dPhi` range? | DECIDE: project decision with collaborators; then either drop the convention-neutral disjunction axiom for the chosen one, or keep it if conventions vary. Until decided, the convention-neutral strategy stands |
| OPEN-3 | Index base and negative indices (`jets[-1]`) | DECIDE: project decision with collaborators. Until decided, the convention-neutral strategy stands |
| OPEN-4 | `~=` exact meaning (legacy lexes as `!=`) | DECIDE: project decision with collaborators; suspect "approximately equal" — if so it is NOT `!=` and legacy is wrong. Until decided, the convention-neutral strategy stands |
| OPEN-5 | `Size`/`count`/`size` aliases and their domains | DECIDE: project decision with collaborators, informed by a corpus scan. Until decided, the convention-neutral strategy stands |

## 5. The checked fragment

ADL2 fully interprets and verifies: comparisons over linear arithmetic of
event scalars, element properties, sizes, angular separations and
declared external functions; full boolean structure incl. ternary and
reject; defines; object filtering/union; region inheritance; bins;
triggers. Outside the fragment (e.g. user-defined functions with unknown
semantics, `sort`, tables/weights as *values*): the interpreter raises a
diagnosed evaluation error; the verifier produces an `Unknown` leaf with
the same diagnostic identity, so the two tools tell one story.
