//! Per-region cutflows (SPEC_EVENT_PIPELINE §2 — our own design,
//! explicitly not CutLang-compatible).
//!
//! Per selection region, an ordered list of **steps**: step 0 is `all`
//! (every processed event), then exactly one step per membership-
//! affecting statement in declaration order — `select` / `reject` /
//! `trigger`, and inheritance (a bare prior-region name) as **one** step
//! carrying the parent's whole predicate (the parent's own table holds
//! its breakdown). `weight`/`histo`/`bin`/`save`/`print`/`counts`/`type`
//! and histoList references contribute no step.
//!
//! Per step: `raw` (events surviving steps ≤ i), `sumw` (Σ effective
//! weight over survivors), `sumw2` (Σw²), `errors`. The effective weight
//! is `Event::weight × Π` of the numeric ADL `weight` statements declared
//! at earlier positions ([DECIDE-W1] positional; see `weights.rs`). A
//! hard [`EvalError`] at step i counts the event as **failing** step i
//! and increments `errors` — a faithful diagnostic, never a guessed pass.
//!
//! `bin` statements get an appendix entry, filled only from events
//! passing the whole region: per-bin `raw`/`sumw`/`sumw2`, an `out`
//! bucket for below-`b0`/non-value, and a `failed` count; boolean bins
//! get `true`/`false` buckets.
//!
//! Step labels are the verbatim source text of the statement (keyword +
//! the expression's source span; `cut` spells as `select`). A `select`
//! that names a boolean define shows the inlined define body (resolution
//! replaces the reference; the HIR carries the body's span). A region
//! containing an out-of-fragment statement (`sort`, unresolved
//! reference) cannot be evaluated and is **skipped with a diagnostic** —
//! same honesty rule as histograms.
//!
//! Honesty on weights: a non-numeric or malformed `weight` poisons later
//! positions; affected steps and bins carry `"weighted_incomplete": true`
//! in the JSON (the value is *not* guessed into the sums). Weight
//! diagnostics themselves are emitted once by [`crate::HistoSet::new`].
//!
//! Emissions (all from this one accumulator): canonical `cutflow.json`
//! ([`CutflowSet::to_json`]; schema `version: 1`, byte-deterministic) and
//! the fixed-width stdout table ([`CutflowSet::text_table`], columns
//! `step | raw | abs% | rel% | errors | sumw +- err`, ASCII). The §6
//! provenance
//! object and the TH1D pair in `out.root` are deferred (Phase 10c and
//! the rootfile `fLabels` extension respectively).

use crate::eval::{BinOutcome, RegionResult, StepEval};
use crate::event::Event;
use crate::json::JsonWriter;
use crate::provenance::Provenance;
use crate::weights::stmt_weights;
use adl_sema::{Fragment, Hir, HirRegionStmt};
use adl_syntax::span::Span;
use std::collections::HashMap;
use std::fmt::Write as _;

/// A raw count with its weighted companions (no output ever shows a
/// weighted number without its raw companion — SPEC_EVENT_PIPELINE §4).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Counts {
    pub raw: u64,
    pub sumw: f64,
    pub sumw2: f64,
}

impl Counts {
    fn add(&mut self, w: f64) {
        self.raw += 1;
        self.sumw += w;
        self.sumw2 += w * w;
    }

    /// Field-wise merge (SPEC_EVENT_PIPELINE §5 deterministic reduction).
    /// `0.0 + v == v`, so merging a single partial into a fresh `Counts`
    /// reproduces it bit-for-bit.
    fn merge(&mut self, other: &Counts) {
        self.raw += other.raw;
        self.sumw += other.sumw;
        self.sumw2 += other.sumw2;
    }
}

/// One cutflow step of one region.
#[derive(Debug, Clone, PartialEq)]
pub struct CutStep {
    /// `"all"`, `"select"`, `"reject"`, `"inherit"`, or `"trigger"`.
    pub kind: &'static str,
    /// Verbatim statement text (module docs).
    pub label: String,
    /// ADL weight product in effect at this position ([DECIDE-W1]).
    factor: f64,
    /// A non-numeric/malformed `weight` precedes this step ([DECIDE-W2]):
    /// the weighted columns are incomplete, flagged — never guessed.
    pub weighted_incomplete: bool,
    pub counts: Counts,
    /// Events whose evaluation hard-errored at this step (they count as
    /// failing it).
    pub errors: u64,
}

/// Appendix accumulator for one `bin` statement (filled from events
/// passing the whole region).
#[derive(Debug, Clone, PartialEq)]
pub enum BinFlow {
    /// `bin v b0 … bn` ⇒ bins `[b0,b1), …, [bn,∞)`.
    Boundary {
        label: Option<String>,
        /// Canonical edge texts (as resolved).
        edges: Vec<String>,
        factor: f64,
        weighted_incomplete: bool,
        bins: Vec<Counts>,
        /// Below `b0`, or the bin expression had no value.
        out: Counts,
        /// The bin expression hard-errored.
        failed: u64,
    },
    /// Boolean bin: membership of the condition.
    Cond {
        label: Option<String>,
        factor: f64,
        weighted_incomplete: bool,
        yes: Counts,
        no: Counts,
        failed: u64,
    },
}

impl BinFlow {
    /// Merge a same-shape partial (SPEC_EVENT_PIPELINE §5). Both sides come
    /// from the same `bin` statement, so the variant and bucket count
    /// match (debug-asserted); the merge adds counts field-wise.
    fn merge(&mut self, other: &BinFlow) {
        match (self, other) {
            (
                BinFlow::Boundary {
                    bins, out, failed, ..
                },
                BinFlow::Boundary {
                    bins: ob,
                    out: oo,
                    failed: of,
                    ..
                },
            ) => {
                debug_assert_eq!(bins.len(), ob.len(), "BinFlow::merge bin count mismatch");
                for (a, b) in bins.iter_mut().zip(ob) {
                    a.merge(b);
                }
                out.merge(oo);
                *failed += of;
            }
            (
                BinFlow::Cond {
                    yes, no, failed, ..
                },
                BinFlow::Cond {
                    yes: oy,
                    no: ono,
                    failed: of,
                    ..
                },
            ) => {
                yes.merge(oy);
                no.merge(ono);
                *failed += of;
            }
            _ => debug_assert!(false, "BinFlow::merge variant mismatch"),
        }
    }
}

/// The cutflow of one selection region.
#[derive(Debug, Clone, PartialEq)]
pub struct RegionFlow {
    /// Region name (first-seen spelling).
    pub name: String,
    region_idx: usize,
    /// Step 0 is `all`; then declaration order.
    pub steps: Vec<CutStep>,
    /// Statement index → step index (membership statements only).
    step_of_stmt: HashMap<usize, usize>,
    pub bins: Vec<BinFlow>,
}

impl RegionFlow {
    /// Merge a same-shape partial (SPEC_EVENT_PIPELINE §5). Both sides come
    /// from the same HIR region, so steps and bins align by index
    /// (debug-asserted); counts/errors add field-wise.
    fn merge(&mut self, other: &RegionFlow) {
        debug_assert_eq!(
            self.region_idx, other.region_idx,
            "RegionFlow::merge region mismatch"
        );
        debug_assert_eq!(
            self.steps.len(),
            other.steps.len(),
            "RegionFlow::merge step count mismatch"
        );
        for (a, b) in self.steps.iter_mut().zip(&other.steps) {
            a.counts.merge(&b.counts);
            a.errors += b.errors;
        }
        debug_assert_eq!(
            self.bins.len(),
            other.bins.len(),
            "RegionFlow::merge bin count mismatch"
        );
        for (a, b) in self.bins.iter_mut().zip(&other.bins) {
            a.merge(b);
        }
    }
}

/// All region cutflows of one resolved analysis unit (single source of
/// truth for `cutflow.json`, the stdout table, and the `--json` section).
pub struct CutflowSet {
    regions: Vec<RegionFlow>,
    total: Counts,
    setup_diags: Vec<String>,
}

impl CutflowSet {
    /// Build the step structure from `hir`; `src` is the unit's source
    /// text (labels are verbatim source slices). Unevaluable regions
    /// become setup diagnostics, never silent drops.
    #[must_use]
    pub fn new(hir: &Hir, src: &str) -> Self {
        let mut diags = Vec::new();
        let mut regions = Vec::new();
        for (ridx, region) in hir.regions.iter().enumerate() {
            if hir.histolist_regions.get(ridx).copied().unwrap_or(false) {
                continue; // histogram template block, not a selection region
            }
            let name = hir.symbols.display(region.name).to_owned();
            if let Some(reason) = region.stmts.iter().find_map(|s| match s {
                HirRegionStmt::NonMembership {
                    tag: Fragment::Unsupported(reason),
                    ..
                } => Some(reason.as_str()),
                _ => None,
            }) {
                diags.push(format!(
                    "region `{name}`: cannot evaluate ({reason}); cutflow skipped"
                ));
                continue;
            }
            regions.push(Self::region_flow(hir, src, ridx, name));
        }
        Self {
            regions,
            total: Counts::default(),
            setup_diags: diags,
        }
    }

    fn region_flow(hir: &Hir, src: &str, ridx: usize, name: String) -> RegionFlow {
        let region = &hir.regions[ridx];
        let weights = stmt_weights(hir, ridx);
        let mut steps = vec![CutStep {
            kind: "all",
            label: "all".to_owned(),
            factor: 1.0,
            weighted_incomplete: false,
            counts: Counts::default(),
            errors: 0,
        }];
        let mut step_of_stmt = HashMap::new();
        let mut bins = Vec::new();
        for (i, stmt) in region.stmts.iter().enumerate() {
            let (factor, weighted_incomplete) = weights.at(i);
            let (kind, label) = match stmt {
                HirRegionStmt::Select(n) => ("select", labeled(src, "select", n.span, i)),
                HirRegionStmt::Reject(n) => ("reject", labeled(src, "reject", n.span, i)),
                HirRegionStmt::Trigger(n) => ("trigger", labeled(src, "trigger", n.span, i)),
                HirRegionStmt::Inherit {
                    region: target,
                    span,
                } => {
                    if hir.histolist_regions.get(*target).copied().unwrap_or(false) {
                        continue; // histoList reference: a fill point, not a step
                    }
                    let label = snippet(src, *span).map_or_else(
                        || {
                            hir.region_name_order.get(*target).map_or_else(
                                || format!("<region {target}>"),
                                |&s| hir.symbols.display(s).to_owned(),
                            )
                        },
                        str::to_owned,
                    );
                    ("inherit", label)
                }
                HirRegionStmt::Bin {
                    label, edges, span, ..
                } => {
                    bins.push(BinFlow::Boundary {
                        label: bin_label(src, label.as_ref(), *span),
                        edges: edges.clone(),
                        factor,
                        weighted_incomplete,
                        bins: vec![Counts::default(); edges.len()],
                        out: Counts::default(),
                        failed: 0,
                    });
                    continue;
                }
                HirRegionStmt::BinCond { label, span, .. } => {
                    bins.push(BinFlow::Cond {
                        label: bin_label(src, label.as_ref(), *span),
                        factor,
                        weighted_incomplete,
                        yes: Counts::default(),
                        no: Counts::default(),
                        failed: 0,
                    });
                    continue;
                }
                HirRegionStmt::NonMembership { .. } => continue,
            };
            step_of_stmt.insert(i, steps.len());
            steps.push(CutStep {
                kind,
                label,
                factor,
                weighted_incomplete,
                counts: Counts::default(),
                errors: 0,
            });
        }
        RegionFlow {
            name,
            region_idx: ridx,
            steps,
            step_of_stmt,
            bins,
        }
    }

    /// Accumulate one event. `results`/`traces` must be the
    /// [`crate::Interp::run_event_traced`] output for the same event
    /// (one entry per HIR region, declaration order).
    pub fn record_event(
        &mut self,
        event: &Event,
        results: &[RegionResult],
        traces: &[Vec<StepEval>],
    ) {
        let w_in = event.weight;
        self.total.add(w_in);
        for flow in &mut self.regions {
            let f0 = flow.steps[0].factor;
            flow.steps[0].counts.add(w_in * f0);
            let Some(trace) = traces.get(flow.region_idx) else {
                continue;
            };
            for se in trace {
                // Statements with no step (histoList references) always
                // evaluate `Ok(true)`; nothing to record.
                let Some(&si) = flow.step_of_stmt.get(&se.stmt) else {
                    continue;
                };
                let step = &mut flow.steps[si];
                match &se.outcome {
                    Ok(true) => step.counts.add(w_in * step.factor),
                    Ok(false) => {}
                    Err(_) => step.errors += 1,
                }
            }
            let Some(result) = results.get(flow.region_idx) else {
                continue;
            };
            if result.pass == Ok(true) {
                for (acc, outcome) in flow.bins.iter_mut().zip(&result.bins) {
                    record_bin(acc, outcome, w_in);
                }
            }
        }
    }

    /// Merge a partial [`CutflowSet`] of the same analysis unit into this
    /// one (SPEC_EVENT_PIPELINE §5). Both sets come from the same HIR, so
    /// `regions[i]` aligns by index; `setup_diags` is HIR-derived and
    /// identical, so the master's copy is kept. Folding partials in
    /// ascending chunk order makes the result byte-identical to a serial
    /// run that processes those same chunks in order.
    pub fn merge(&mut self, other: &CutflowSet) {
        self.total.merge(&other.total);
        debug_assert_eq!(
            self.regions.len(),
            other.regions.len(),
            "CutflowSet::merge region count mismatch"
        );
        for (a, b) in self.regions.iter_mut().zip(&other.regions) {
            a.merge(b);
        }
    }

    /// Setup diagnostics (skipped regions), deterministic order.
    #[must_use]
    pub fn diagnostics(&self) -> Vec<String> {
        self.setup_diags.clone()
    }

    /// No region produced a cutflow (no regions declared, or all skipped).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    #[must_use]
    pub fn regions(&self) -> &[RegionFlow] {
        &self.regions
    }

    #[must_use]
    pub fn total(&self) -> Counts {
        self.total
    }

    /// Canonical `cutflow.json` content (schema `version: 1`,
    /// SPEC_EVENT_PIPELINE §2). `pretty` selects the file form; both forms
    /// are byte-deterministic.
    #[must_use]
    pub fn to_json(&self, pretty: bool) -> String {
        self.to_json_with(pretty, None)
    }

    /// `cutflow.json` with the SPEC_EVENT_PIPELINE §6 `provenance` object
    /// embedded as a top-level key (after `version`, before `total`) when
    /// supplied — the same canonical bytes carried by
    /// `histos.json`/`out.root`.
    #[must_use]
    pub fn to_json_with(&self, pretty: bool, provenance: Option<&Provenance>) -> String {
        let mut w = JsonWriter::new(pretty);
        w.open('{');
        w.key("version");
        w.raw("1");
        if let Some(p) = provenance {
            w.key("provenance");
            p.write(&mut w);
        }
        w.key("total");
        counts_json(&mut w, self.total);
        w.key("regions");
        w.open('[');
        for flow in &self.regions {
            w.open('{');
            w.key("name");
            w.str_val(&flow.name);
            w.key("steps");
            w.open('[');
            for step in &flow.steps {
                w.open('{');
                w.key("kind");
                w.str_val(step.kind);
                w.key("label");
                w.str_val(&step.label);
                w.key("raw");
                w.raw(&step.counts.raw.to_string());
                w.key("sumw");
                w.num(step.counts.sumw);
                w.key("sumw2");
                w.num(step.counts.sumw2);
                w.key("errors");
                w.raw(&step.errors.to_string());
                if step.weighted_incomplete {
                    w.key("weighted_incomplete");
                    w.raw("true");
                }
                w.close('}');
            }
            w.close(']');
            w.key("bins");
            w.open('[');
            for bin in &flow.bins {
                bin_json(&mut w, bin);
            }
            w.close(']');
            w.close('}');
        }
        w.close(']');
        w.close('}');
        w.finish()
    }

    /// Fixed-width per-region stdout tables (deterministic formatting):
    /// `step | raw | abs% | rel% | errors | sumw +- err` — abs vs `all`,
    /// rel vs the previous step, err = √sumw2. Weighted columns of
    /// flagged steps carry a `(weighted incomplete)` marker.
    #[must_use]
    pub fn text_table(&self) -> String {
        let mut out = String::new();
        for (ri, flow) in self.regions.iter().enumerate() {
            if ri > 0 {
                out.push('\n');
            }
            let _ = writeln!(out, "cutflow: {}", flow.name);
            let label_w = flow
                .steps
                .iter()
                .map(|s| s.label.len())
                .max()
                .unwrap_or(4)
                .max(4);
            let raw_w = flow
                .steps
                .iter()
                .map(|s| s.counts.raw.to_string().len())
                .max()
                .unwrap_or(3)
                .max(3);
            let _ = writeln!(
                out,
                "  {:<label_w$}  {:>raw_w$}  {:>8}  {:>8}  {:>6}  sumw +- err",
                "step", "raw", "abs%", "rel%", "errors"
            );
            let all_raw = flow.steps[0].counts.raw;
            let mut prev_raw = all_raw;
            for (i, step) in flow.steps.iter().enumerate() {
                let abs = pct(step.counts.raw, all_raw);
                let rel = if i == 0 {
                    "-".to_owned()
                } else {
                    pct(step.counts.raw, prev_raw)
                };
                let mut wcol = format!(
                    "{} +- {}",
                    fnum(step.counts.sumw),
                    fnum(step.counts.sumw2.sqrt())
                );
                if step.weighted_incomplete {
                    wcol.push_str(" (weighted incomplete)");
                }
                let _ = writeln!(
                    out,
                    "  {:<label_w$}  {:>raw_w$}  {:>8}  {:>8}  {:>6}  {}",
                    step.label, step.counts.raw, abs, rel, step.errors, wcol
                );
                prev_raw = step.counts.raw;
            }
        }
        out
    }
}

fn record_bin(acc: &mut BinFlow, outcome: &BinOutcome, w_in: f64) {
    match (acc, outcome) {
        (
            BinFlow::Boundary {
                factor, bins, out, ..
            },
            BinOutcome::Boundary { bin, .. },
        ) => {
            let w = w_in * *factor;
            match bin {
                Some(i) => {
                    if let Some(c) = bins.get_mut(*i) {
                        c.add(w);
                    }
                }
                None => out.add(w),
            }
        }
        (
            BinFlow::Cond {
                factor, yes, no, ..
            },
            BinOutcome::Cond { member, .. },
        ) => {
            let w = w_in * *factor;
            if *member {
                yes.add(w);
            } else {
                no.add(w);
            }
        }
        (
            BinFlow::Boundary { failed, .. } | BinFlow::Cond { failed, .. },
            BinOutcome::Failed { .. },
        ) => *failed += 1,
        // Kind mismatch cannot happen: both sides derive from the same
        // statement list in the same order. Defensive: count as failed.
        (BinFlow::Boundary { failed, .. } | BinFlow::Cond { failed, .. }, _) => *failed += 1,
    }
}

fn counts_json(w: &mut JsonWriter, c: Counts) {
    w.open('{');
    w.key("raw");
    w.raw(&c.raw.to_string());
    w.key("sumw");
    w.num(c.sumw);
    w.key("sumw2");
    w.num(c.sumw2);
    w.close('}');
}

fn bin_json(w: &mut JsonWriter, bin: &BinFlow) {
    w.open('{');
    match bin {
        BinFlow::Boundary {
            label,
            edges,
            weighted_incomplete,
            bins,
            out,
            failed,
            ..
        } => {
            w.key("kind");
            w.str_val("boundary");
            w.key("label");
            opt_str(w, label.as_deref());
            w.key("edges");
            w.open('[');
            for e in edges {
                w.raw(e);
            }
            w.close(']');
            w.key("bins");
            w.open('[');
            for c in bins {
                counts_json(w, *c);
            }
            w.close(']');
            w.key("out");
            counts_json(w, *out);
            w.key("failed");
            w.raw(&failed.to_string());
            if *weighted_incomplete {
                w.key("weighted_incomplete");
                w.raw("true");
            }
        }
        BinFlow::Cond {
            label,
            weighted_incomplete,
            yes,
            no,
            failed,
            ..
        } => {
            w.key("kind");
            w.str_val("cond");
            w.key("label");
            opt_str(w, label.as_deref());
            w.key("true");
            counts_json(w, *yes);
            w.key("false");
            counts_json(w, *no);
            w.key("failed");
            w.raw(&failed.to_string());
            if *weighted_incomplete {
                w.key("weighted_incomplete");
                w.raw("true");
            }
        }
    }
    w.close('}');
}

fn opt_str(w: &mut JsonWriter, s: Option<&str>) {
    match s {
        Some(s) => w.str_val(s),
        None => w.null(),
    }
}

/// Verbatim source slice of `span`, trimmed; `None` if empty/invalid.
fn snippet(src: &str, span: Span) -> Option<&str> {
    let s = src.get(span.start as usize..span.end as usize)?;
    let t = s.trim();
    (!t.is_empty()).then_some(t)
}

fn labeled(src: &str, kw: &str, span: Span, stmt_idx: usize) -> String {
    match snippet(src, span) {
        Some(t) => format!("{kw} {t}"),
        None => format!("{kw} <statement {stmt_idx}>"),
    }
}

/// Bin appendix label: the declared label if any, else the verbatim
/// statement text.
fn bin_label(src: &str, label: Option<&String>, span: Span) -> Option<String> {
    label
        .cloned()
        .or_else(|| snippet(src, span).map(str::to_owned))
}

/// `xx.xx%` or `-` when the denominator is zero.
fn pct(num: u64, den: u64) -> String {
    if den == 0 {
        return "-".to_owned();
    }
    #[allow(clippy::cast_precision_loss)] // event counts
    let v = 100.0 * num as f64 / den as f64;
    format!("{v:.2}%")
}

/// Shortest round-trip float text (same digits as the JSON output).
fn fnum(v: f64) -> String {
    serde_json::to_string(&v).expect("finite f64 serializes")
}
