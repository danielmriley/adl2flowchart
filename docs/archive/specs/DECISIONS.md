# ADL2 architecture decision records

Each ADR cites the concrete legacy failure that motivates it. Legacy
references: `../docs/REVIEW_NOTES.md` (original audit, findings 1a–1h,
C1–C13), `../docs/DUAL_ENCODING_REPORT.md` (rewrite + roadmap), and the
June-2026 adversarial audit (Bugs 1–6, fixed in the legacy tree).

## ADR-001: Rust (2024 edition)

**Decision.** Implement ADL2 as a Rust cargo workspace.
**Motivated by.** Legacy C10: the AST mixed deep-clone-and-delete with
shallow-copy-never-free nodes — double-free minefield, leaks, UB in
`fMR`-style uninitialized returns; an entire bug class that ownership
typing removes. Also: first-class enums/pattern matching for AST/Formula
IRs, proptest/insta/cargo-fuzz for the testing strategy, mature `z3`
bindings.
**Trade-offs.** HEP toolchains are C++-centric; mitigations: the CLI is a
standalone binary with no runtime deps beyond optional z3; the subprocess
solver backend avoids linking requirements; C FFI layer possible later if
integration with a C/C++ toolchain is ever wanted.
**Rejected.** Modern C++ (would re-fight ownership and tooling for every
crate-equivalent); keeping/extending the legacy codebase (it now works,
but its parser and identity layers are load-bearing string heuristics).

## ADR-002: Hand-written recursive-descent parser, spec first

**Decision.** EBNF spec frozen before code; one parser function per
nonterminal; no parser generator.
**Motivated by.** Legacy C1–C4: 87 grammar conflicts at peak (silent
wrong-AST parses), a NOT token the lexer never produced (finding 1d),
hyphen-eating identifiers, signed-literal lexing, AST-counter "line
numbers". LALR hid all of this; RD makes precedence, recovery, and
context-sensitivity explicit and unit-testable.
**Trade-offs.** More code than a grammar file; mitigated by the
EBNF-to-function structural correspondence and snapshot tests.
**Amended.** ADR-011 allows an in-tree recursive-descent *emitter*
(`adl2_rdgen`) that reads the frozen EBNF. LALR / Bison / Flex remain
rejected.

## ADR-003: Typed Quantity/Collection identity model

**Decision.** Event quantities are interned typed values
(SPEC_ARCHITECTURE §4); identity is structural; relations between
non-identical quantities are proven facts.
**Motivated by.** The single largest legacy bug family: string-key
synthesis lost indices (1e), over-merged lineage and aliases (1g,
scalarHT→MET), needed case/MET.pt normalization hacks, merged oriented
angular pairs (audit Bug 3), and dropped the define↔body link (audit
Bug 2). Cross-file disjointness — the project's destination — is an
identity problem; strings don't scale to it.
**Trade-offs.** More up-front modeling than emitting strings; pays for
itself the first time two files are loaded.

## ADR-004: Soundness polarity in the type system

**Decision.** `Formula` (with Unknown/Dual) projects to distinct `Over`
and `Under` types; proof functions accept only the correct polarity;
SAT-direction witnesses are re-validated through the interpreter.
**Motivated by.** Original findings 1a–1c/1f (silent strengthening ⇒
false PROVEN DISJOINT) and audit Bugs 1–2 (polarity holes in *new* code:
the empty-∀ plus-branch, opaque defines). Convention enforced by review
failed twice; types don't get tired.
**Trade-offs.** Slightly more ceremony at call sites — which is the point.

## ADR-005: Reference interpreter as the executable spec and oracle

**Decision.** adl-interp implements SPEC_LANGUAGE §4 and is shipped as a
user feature (`smash2 run`); the verifier is property-tested against it
and re-validates every witness through it at runtime.
**Motivated by.** Legacy had no ground truth; all six audit bugs were
found by hand-crafted attack files. Sampling against an interpreter finds
the same class mechanically and continuously.
**Trade-offs.** The interpreter itself can be wrong — mitigated by it
being the authoritative semantics defined in SPEC_LANGUAGE, the
property-test battery that exercises it against the encoder, collaborator
review of [DECIDE] items, and the rule that interpreter/verifier
disagreement is release-blocking either way.

## ADR-006: libz3 native bindings primary, SMT-LIB subprocess secondary

**Decision.** `Solver` trait with two conformance-tested backends.
**Motivated by.** Audit Bug 5: the text protocol let an invalid literal
drop an assert and z3's `(error)` line slipped past the parser — a false
PROVEN OVERLAPPING. Native terms make malformed input unrepresentable and
give incremental solving, models, and unsat cores (the explanations
feature) without string parsing.
**Trade-offs.** Linking burden in exotic environments — covered by the
subprocess backend as a supported, CI-tested configuration.

## ADR-007: Declared checked fragment

**Decision.** The spec names exactly what ADL2 interprets/verifies;
everything else is `Unsupported` with one shared diagnostic consumed by
both tools.
**Motivated by.** The legacy extractor's best-effort posture is where
silent wrongness lived (the six-fallback `extractSimpleConstraint`
cascade). "Honest refusal + visible coverage" proved more useful to
physicists than optimistic guessing.

## ADR-008: Axioms as an audited catalog

**Decision.** One table, one emitter each, justification + assumption tag
+ test required; prohibited-axiom list is permanent.
**Motivated by.** Two real incidents: "C[i] referenced ⇒ size>i" (false
under guards — produced a false empty-region proof) and the substring
btag {0,1} axiom hitting continuous discriminants (audit Bug 6). Axioms
are the one place where physics claims enter the math; they deserve the
same review surface as the encoder.

## ADR-009: Legacy tool retained as transitional oracle

**Decision.** No big-bang switch: legacy `smash` keeps running in CI
until the Phase-7 parity gate, and nightly for one release after.
**Motivated by.** The legacy tree now embodies ~50 hard-won golden
checks and two audits; throwing that signal away while ADL2 stabilizes
would repeat the original project's mistake of having no oracle.

## ADR-010: Full C++ reimplementation under `cpp/` (Rust forever-oracle)

**Decision.** Implement a second, from-scratch C++ toolchain that
reproduces the **smash2 architecture and behavior** under repo-root
[`cpp/`](../../../cpp/) (working binary name `smash2_cpp` / library
`adl2_cpp`). This is **not** an in-place rewrite of
[`legacy_parser/`](../../../legacy_parser/). Parser technology is the
ADR-002 contract: frozen EBNF (`cpp/grammar.ebnf`), one hand-written
recursive-descent `parse_X` per nonterminal, a collaborator
`BISON_MAP.md` (“if you know bison”), and grammar-shaped diagnostics —
**not** Bison/Flex as the implementation. Rust `smash2`
(`reimplementation/adl2`) remains the **forever oracle** in CI; future
parity gates diff C++ outputs against it. Legacy `smash` stays a
transitional secondary oracle (ADR-009) and is not the port target.

**Soundness non-negotiables (imported).** The C++ port inherits the
contracts already paid for in ADR-003–008 and the certify / viz /
numeric stack — it does not get to re-discover them:

| Import | Source | C++ obligation |
|---|---|---|
| Typed Quantity/Collection identity | ADR-003 | No string-key identity; structural/proven unification only |
| Polarity types (`Over` / `Under` / Unknown·Dual) | ADR-004 | Proof APIs accept only the correct polarity; SAT witnesses re-validated |
| Reference interpreter as executable spec | ADR-005 | Interpreter oracle; verifier/interpreter disagreement is release-blocking |
| Declared checked fragment + honest Unknown | ADR-007 | Outside fragment → `Unsupported` / Unknown with a visible reason; never silent guess |
| Audited axiom catalog | ADR-008 | One table, one emitter, justification + assumption tag + test; prohibited list permanent |
| Independent certification | `adl-certify` | Proofs carry certificates; uncertified is not “proven” |
| HIR-derived visualization | `adl-viz` | Flowchart/AST from semantic IR, not ad-hoc string graphs |
| Exact rationals | rational-numeric / `Rat` | Exact `ℚ` core for checked numeric claims; no float-as-truth |

**Motivated by.** HEP collaborators live in C++ toolchains; a faithful
port lets them read, extend, and integrate the prover without abandoning
the soundness architecture that the Rust campaign established. Porting
*in place* under `legacy_parser/` would re-attach the load-bearing
string heuristics and LALR conflict surface that ADR-001–003 rejected.
Dropping the Rust oracle would repeat the original project's “no ground
truth” failure mode the moment the two trees diverge.

**Trade-offs.** Two implementations to keep aligned — mitigated by
treating Rust smash2 as the forever CI oracle and landing C++ behind
explicit parity gates. More upfront grammar packaging (`grammar.ebnf` +
`parse_X` + `BISON_MAP.md`) than a generator — that packaging *is* the
collaborator onboarding surface (ADR-002).

**Rejected.**

- **Bison/Flex as the C++ implementation** — contradicts ADR-002; LALR
  hid the legacy conflict / NOT-token / hyphen-ident classes.
- **Rewrite-in-place of `legacy_parser/`** — keeps Expr\*/string
  identity and the accreted grammar as the substrate.
- **FFI-only wrapping of Rust smash2** — useful later for embedding, but
  not a C++ reimplementation collaborators can own and extend.
- **Dropping the Rust smash2 oracle** — removes the only executable
  ground truth the soundness campaign depends on.
- **Accreting onto legacy `Expr*` / string identity** — reopens ADR-003
  and ADR-004 bug families by construction.

## ADR-011: In-tree RD emitter (`adl2_rdgen`), not LALR

**Decision.** The C++ parser may be *partially generated* by a host
tool, `adl2_rdgen`, that reads [`cpp/grammar.ebnf`](../../../cpp/grammar.ebnf)
and emits recursive-descent `parse_X` method bodies. CMake builds the
tool first and lists the EBNF as an explicit `DEPENDS` of `adl2_syntax`.
Generated code stays `adl2::syntax::Parser` + `peek`/`advance`/token
vector. The lexer stays hand-written. Productions that the EBNF cannot
state (indent-only `object-define`, contextual `bins`, arg-only
`path-token`, particle-list juxtaposition, comparison chains, sort
absorb-to-EOL) remain named **hooks**.

**Motivated by.** Collaborators already audit a frozen EBNF against
`parser.hpp`. Keeping that mapping as a compile-time check — and
emitting the mechanical expression ladder from the same file — stops
the grammar and the C++ from drifting, without reintroducing LALR
conflict hiding (the reason ADR-002 rejected generators).

**Trade-offs.** A second binary exists in the build graph. It is a
**host tool**, not a product CLI: no `adl2_*` link, no z3, not
installed, never invoked by `smash2_cpp` users. Writing it in C++
avoids adding Python/flex/bison to the published toolchain.

**Rejected.**

- **Bison/Flex / any LALR or PEG table generator** — ADR-002; a stock
  `.y` cannot express the hook constraints above.
- **Generating the lexer** — policy (no hyphen-eating ids, no signed
  literals, case-insensitive keywords) is smaller and safer by hand.
- **A user-facing second smash-shaped tool** — `adl2_rdgen` is
  compile-time only.
- **Silently rewriting ADR-010** — the C++ port, smash2 forever-oracle,
  and “not bison as the implementation” still hold. This ADR only
  amends ADR-002’s “no parser generator” to “no *LALR* generator”.

