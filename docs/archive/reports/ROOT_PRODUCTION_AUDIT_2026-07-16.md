# out.root production-readiness audit

**Date:** 2026-07-16 · **Question:** is `smash2 run --histos` out.root
production truly production-grade — usable for real physics analysis?
**Method:** format-level code review of the `rootfile` crate + empirical
adversarial stress battery against the release binary. (The planned
subagent audit hit the account spend limit; this audit was performed
directly and covers the same ground.)

## Verdict: **ready-with-caveats**

The file format layer is production-grade for its declared scope —
histogram + cutflow + provenance files. The *analysis-workflow* layer
around it is where real-physics gaps remain. Concretely: you can trust
the numbers in out.root; whether `smash2 run` alone carries a full
analysis depends on the gaps below.

## Format layer — what was verified

| Property | Status | Evidence |
|---|---|---|
| Container correctness | ✅ | small-format TFile, TKey v4, vendored uproot StreamerInfo; opens in ROOT/TBrowser/hadd/PyROOT and uproot (CI oracle: byte-diff of every object payload vs uproot-written references + strict in-crate re-parse) |
| Semantic exactness | ✅ | Sumw2 incl. flow bins, fEntries vs raw fills, GetStats moments (in-range convention), varbin TAxis fXbins, TH2D global-bin order, fLabels THashList — all pinned by tests green in the 71-suite gate |
| 2 GB small-format cap | ✅ guarded | `Error::TooLarge` raised BEFORE any u32 cast (file.rs:309) — fail-closed, no silent truncation. Large-file (64-bit) format not implemented; irrelevant for histogram files in practice |
| Compression | ⚠️ none | uncompressed records only (declared; header advertises ZLIB(0)). Correctness-neutral; files are larger than ROOT defaults. A zlib option is the single most visible polish gap |
| Determinism | ✅ | byte-identical across runs and any `--jobs` (pinned) |

## Empirical stress battery (all against the release binary)

| Scenario | Behavior |
|---|---|
| baseline ex02 over fixture | valid `root` magic, all 5 outputs |
| duplicate histo name in one region | diagnosed, first declaration wins, file intact |
| variable-bin: 2 edges (1 bin) | accepted correctly |
| variable-bin: non-increasing edges | rejected with diagnostic, histogram skipped |
| always-erroring fill expression | fills counted + diagnosed ("5 fill(s) skipped"), no entry recorded |
| `weight w0 0.0` | accepted (mathematically fine) |
| nbins at the 1e6 cap | works (16 MB file, correct payload size) |
| nbins over the cap | rejected with clear message, rest of file written |
| empty input (0 events) | rc=0, valid outputs with empty histograms |
| NaN in event JSON | loader refuses at parse with line number (fail-closed) |

No corruption or silent mangling path was found: every failure is a
diagnostic plus a safe degradation.

## The real-physics gaps (workflow layer, not format layer)

These are what a grad student doing a real analysis would hit — none are
out.root defects, all are `run` feature scope:

1. **Negative weights (NLO samples).** Weight machinery multiplies
   per-region numeric weights and the event weight; Sumw2 handles sign
   correctly by construction, but negative-weight semantics have no
   dedicated tests or documentation. Must be pinned before NLO use.
2. **No systematic variations** — no weight-branch vectors, no shape
   variations, no per-histogram syst mirrors. The single biggest gap for
   a paper-grade workflow.
3. **No lumi/σ scaling** — normalization happens downstream today
   (defensible: histos.json + provenance carry everything needed, and
   ROOT-side scaling is one line — but it should be documented).
4. **Mid-selection histoList fill points** — diagnosed honestly rather
   than implemented (fills happen on full region acceptance only).
5. **No TTree/ntuple output** — histogram-only by design; event-list
   export would widen applicability.
6. **[DECIDE-I4] weight branch** — `Event.Weight` vs `Weight.Weight[0]`
   still unverified against a weighted Delphes sample.

## Answer to "can it be used for real physics analysis?"

**Yes, within its scope, with more independent validation behind it than
most private analysis code has:** cutflows and LO/unweighted (or
uniformly-weighted) histogram production straight off Delphes/NanoAOD,
validated end-to-end on a real 20k-event sample against independent
uproot/numpy oracles, deterministic and provenance-stamped. The out.root
files themselves are trustworthy artifacts.

**Not yet, standalone, for:** an NLO-weighted, systematics-carrying,
paper-ready measurement — items 1–3 above are the blockers, and they are
workflow features, not correctness risks. Recommended order: pin
negative-weight semantics (S), document the normalization contract (S),
then systematic weight vectors (M–L) if smash2 should carry that stage
rather than hand off to the collaboration's fitting stack.
