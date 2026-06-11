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
CutLang integration is ever wanted.
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
**Trade-offs.** The interpreter itself can be wrong — hence CutLang
differential anchoring for [VERIFY] items and the rule that
interpreter/verifier disagreement is release-blocking either way.

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
