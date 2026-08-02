---
name: adl2-landing-discipline
description: The campaign process for landing soundness-critical ADL2/smash2 changes — orchestrator/agent roles, acceptance gates, adversarial verification rounds, and the operational hazards that have actually burned sessions. Use this whenever coordinating subagents on this repo, verifying an agent's completion report, deciding whether a multi-commit line is ready to push, running the acceptance batteries, or adding files anywhere under examples/. Trigger on background agents, worktrees, gate/battery runs, corpus A/B comparisons, and push decisions — even if the user only says "land this" or "check the agent's work". Companion to adl2-soundness (the contract); this skill is HOW the work gets verified and landed without shipping a false claim or destroying concurrent work.
allowed-tools: Read Edit Write Bash Grep Glob
---

# Landing discipline: gates, agents, and the hazards that actually happened

Every rule here was paid for. The presence-model campaign (2026-07/08)
shipped a false PROVEN SUBSET to a committed tree, had an agent report a
known-failing gate as "pending", had a resumed agent overwrite the
orchestrator's fix mid-verification, and had a fail-fast run hide 48
failures — all in one week, all caught before push by this process.

## Roles: who implements, who verifies, who pushes

- **Agents implement; the orchestrator verifies; only the orchestrator
  merges and pushes.** An agent's "all green" report is a claim to check,
  not a result to relay. Re-run the gates yourself, on the exact commit the
  agent produced — a whitespace-equivalent tree is NOT the committed tree.
- **One merge queue.** Parallel agents work in isolated worktrees
  (`.claude/worktrees/…`); nothing lands except through the orchestrator's
  fast-forward of a reviewed, gate-green branch.
- **"Pending" is not a status for a gate you have seen fail.** Two separate
  agents reported in-flight batteries as a formality after observing a
  failure earlier in their session. If a check has ever failed in this
  session, it is a known-failing gate until a full clean re-run says
  otherwise, and reports must say so.
- **Never push with a red acceptance gate**, no matter how unrelated the
  failure looks or how solid the corpus ledger is. Hold, diagnose, fix,
  re-run everything.

## The acceptance gates (all of them, on the final commit)

1. `cargo test --workspace --release --no-fail-fast` — and **CONFIRM THE
   TARGET COUNT** (baseline 2026-08-02: **87 targets / 976 tests**). A
   fail-fast + tool-timeout run once reported 33 targets clean while 80
   existed and 48 were failing. Count `test result:` lines; sum failures
   across ALL of them, not the tail.
2. **The batteries**: 3× `prop_encoder_vs_interp` at `PROPTEST_CASES=3000`,
   3× `metamorphic` at 2000, plus `cross_oracle`, `merge_identity`,
   `prop_reconcile_oracle`. Three runs is not ritual: the §8 1c false
   subset failed the oracle only ~2 runs in 3 at 3000 cases and passed
   most 2000-case runs. One green run of a probabilistic gate proves
   nothing.
3. **Corpus A/B** (`scripts/verify_corpus_gate.sh`, 146 files / 1900
   pairs; ledger baseline: 813 PD 100% certified · 76 PO · 45 cand ·
   966 possibly · 248 subset · 13 empty · 0/0 refutations). Justify EVERY
   changed pair in one line. PROVEN DISJOINT must never RISE unexplained
   (unsound-axiom red flag); subset PROMOTIONS are red flags; demotions
   need a reason each.
4. Final gates run on a **clean, serial** build when agents built
   concurrently — a stale-linked test binary once reproduced pre-fix
   behavior from a post-fix tree.

**The corpus ledger is NOT a correctness signal.** It stayed bit-identical
through every repair while a live false subset shipped — the failing shape
simply wasn't in the corpus. Only the oracle batteries cover the space.

## Adversarial verification: budget at least two rounds

The negation-presence fix took three counterexample rounds between two
independent reviewers: the orchestrator's fix was killed by the agent's
K12, the agent's refinement was killed by the orchestrator's K13, and one
regression was caught only by the snapshot-acceptance guard refusing a
non-count diff. Protocol:

- Reviewer and implementer each try to construct a kill case against the
  other's rule before accepting it. A fix is done when the skeptic fails.
- Sweep the shape matrix, not the instance: operators × negation sites ×
  presence patterns (the 1c fix's pin suite is 288 cells, found a second
  bug in the fix itself).
- Pin every kill case through **`region3`, both directions** — never
  through `smash2 run` (cutflow evidence, not membership — see
  adl2-soundness).
- When a fix candidate is "obviously minimal", check what it costs: the
  blanket-`And` refusal was sound and silently destroyed `reject_or_band`
  exactness. Strictly-weakening is not the same as acceptable.

## Instrument, don't derive (the two-strike rule)

If your hand-derivation of what the encoder emits has disagreed with the
tool's observed behavior **twice**, stop deriving. Both k14 and §8 1c were
resolved in one shot by dumping the real encoding after multiple failed
whiteboard rounds (four, in k14's case — the existence-guard conjunct was
invisible to derivation). The pattern:

```rust
// throwaway test in crates/adl-formula/tests/, delete after use
use adl_formula::encode_regions;
use adl_sema::{analyze_str, ExtDecls};
#[test]
fn dump() {
    let mut hir = analyze_str(SRC, "probe.adl", &ExtDecls::legacy());
    for r in encode_regions(&mut hir) {
        eprintln!("== {} exact={} ==\n{:#?}", r.name, r.is_exact(), r);
    }
    panic!("dump only");
}
```

Run it in YOUR OWN worktree with a scratch `CARGO_TARGET_DIR`. Diagnosis
before design: no fix is proposed until the offending projection, evaluated
by hand at the falsifying event, is demonstrably wrong and the emitting
code path is named.

## Operational hazards (each of these fired at least once)

- **Stale-waiter resumes.** A "completed" background agent can resume on a
  late monitor and become a surprise concurrent writer — one overwrote the
  orchestrator's in-tree fix mid-verification. Rules: new work goes to a
  FRESH agent in a FRESH worktree; send superseded agents an explicit
  stand-down ("revert diagnosis-only edits, let monitors lapse, stop if
  woken"); check `git status` ownership before editing a tree an agent has
  ever held.
- **Never `pkill`/`killall` by process name** — an agent's cleanup pkill
  killed the orchestrator's verification gate. Kill by PID from a pidfile,
  or match a full path with an anchor that cannot match the caller.
- **Don't build in a tree whose agent is mid-gate.** Orchestrator
  verification runs in its own worktree with its own
  `CARGO_TARGET_DIR` (scratchpad), `nice -n 19`. Note
  `verify_corpus_gate.sh` hardcodes `target/release/smash2` — symlink
  `target` → your scratch target dir inside the worktree.
- **Never add `.adl` files under `examples/`** unless you mean to change
  the corpus: `verify_corpus_gate.sh` (146-file count + ledger baseline)
  and the legacy CI sweep (`legacy_parser/scripts/validate_corpus.sh`)
  both glob it. New ADL2-only goldens MUST be added to the legacy SKIP
  list in the same commit — two presence goldens broke CI this way.
  Demos and probes live in `demos/` or the scratchpad.
- **Queued gates die under the user's own runs.** If the user is running
  cargo/nextest in the repo, queue behind them; don't fight for the lock.
- Commit messages via `git commit -F <file>` — backticks in `-m` execute.
  Trailers: `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` +
  the `Claude-Session:` line. `PATH=/home/daniel/bin` for z3 4.12.2;
  never the `native` cargo feature.

## Report format that survived review

Numbers, not adjectives: "87 targets / 976 / 0", "oracle 3/3 at 3000
(378s, 330s, 366s)", "corpus Δ: −2 subsets, justified per-pair". Separate
"verified" from "pending" from "not sure of" explicitly — the best agent
report of the campaign had a six-item "things I am NOT sure of" section,
and item #1 (batteries ran pre-reformat) was real and needed discharging.

## Cross-references

- **adl2-soundness** — the contract this process defends; membership is
  `region3`; the meet-lattice rule; the bug taxonomy.
- **adl2-build-test** — solver backends, build workarounds.
- **adl2-corpus-sweep** — running and diffing the corpus sweep.
