//! Histogram accumulation (PLAN Phase 9).
//!
//! ADL `histo h, "title", n, lo, hi, expr` statements fill during
//! `smash2 run` when the declaring region accepts the event, weighted by
//! the product of the region's *numeric* `weight` statements. Semantics
//! follow ROOT's `TH1` with `Sumw2` (SPEC_ROOT_WRITER §4):
//!
//! - per-bin `sumw`/`sumw2`, plus underflow/overflow with their own
//!   `sumw2`; `x < lo` underflows, `x >= hi` overflows (ROOT
//!   `TAxis::FindBin` convention);
//! - `entries` is the raw fill count (ROOT `fEntries` semantics), it
//!   counts flow-bin fills too;
//! - the stats array `tsumw`/`tsumw2`/`tsumwx`/`tsumwx2` accumulates **at
//!   fill time, in-range fills only** (ROOT `GetStats` excludes flow
//!   bins) — `GetMean` and merged stats under `hadd` stay exact.
//!
//! Honesty rules: a histogram whose expression is out of fragment (or
//! whose form is 2-D / variable-bin, both deferred) produces **one**
//! diagnostic and is skipped — it never appears in the output. A
//! non-numeric `weight` argument produces a diagnostic and contributes
//! 1.0. Histograms declared in `histoList` blocks are templates,
//! instantiated into each selection region that references the list;
//! repeated references from one region fill once on full region
//! acceptance (mid-selection fill points are deferred).
//!
//! The canonical output is `histos.json` ([`HistoSet::to_json`]):
//! deterministic field order `name, title, region, nbins, lo, hi, sumw,
//! sumw2, underflow, overflow, entries, tsumw, tsumw2, tsumwx, tsumwx2`.

use crate::eval::{Interp, NumOutcome, RegionResult};
use crate::event::Event;
use adl_sema::{HNode, Hir, HirWeightValue, HistoSpec};
use std::fmt::Write as _;

/// A 1-D uniform-binning weighted histogram with ROOT `TH1`/`Sumw2`
/// accumulation semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct Hist1D {
    pub nbins: u32,
    pub lo: f64,
    pub hi: f64,
    /// Per-bin Σw, length `nbins` (flow bins kept separately).
    pub sumw: Vec<f64>,
    /// Per-bin Σw², length `nbins`.
    pub sumw2: Vec<f64>,
    pub underflow_w: f64,
    pub underflow_w2: f64,
    pub overflow_w: f64,
    pub overflow_w2: f64,
    /// Raw fill count (ROOT `fEntries`), including flow-bin fills.
    pub entries: u64,
    /// Fill-time stats, in-range fills only (ROOT `GetStats` semantics).
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
}

impl Hist1D {
    /// New empty histogram. `nbins >= 1` and `lo < hi` are the caller's
    /// contract ([`HistoSet::new`] validates and skips violators).
    #[must_use]
    pub fn new(nbins: u32, lo: f64, hi: f64) -> Self {
        Self {
            nbins,
            lo,
            hi,
            sumw: vec![0.0; nbins as usize],
            sumw2: vec![0.0; nbins as usize],
            underflow_w: 0.0,
            underflow_w2: 0.0,
            overflow_w: 0.0,
            overflow_w2: 0.0,
            entries: 0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
        }
    }

    /// One fill: ROOT `TH1::Fill(x, w)` semantics (see module docs).
    /// `x` is finite by construction (the evaluator never yields
    /// non-finite values).
    pub fn fill(&mut self, x: f64, w: f64) {
        self.entries += 1;
        if x < self.lo {
            self.underflow_w += w;
            self.underflow_w2 += w * w;
            return;
        }
        if x >= self.hi {
            self.overflow_w += w;
            self.overflow_w2 += w * w;
            return;
        }
        let frac = (x - self.lo) / (self.hi - self.lo);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // frac ∈ [0, 1) here, so the product is a small non-negative index.
        let mut idx = (frac * f64::from(self.nbins)) as usize;
        // Floating-point guard: x just below `hi` can round up to `nbins`.
        idx = idx.min(self.nbins as usize - 1);
        self.sumw[idx] += w;
        self.sumw2[idx] += w * w;
        self.tsumw += w;
        self.tsumw2 += w * w;
        self.tsumwx += w * x;
        self.tsumwx2 += w * x * x;
    }
}

/// One instantiated histogram: accumulator + fill expression + the
/// selection region that gates it.
pub struct HistoFill<'h> {
    pub name: String,
    pub title: String,
    /// Selection region the histogram fills under (first-seen spelling).
    pub region: String,
    region_idx: usize,
    expr: &'h HNode,
    weight: f64,
    pub hist: Hist1D,
    /// Fills skipped because the expression had no value (soft
    /// non-value: missing element/property, non-finite arithmetic).
    nonvalue_skips: u64,
    /// Fills skipped on a hard evaluation error (missing event-level data).
    error_skips: u64,
    first_error: Option<String>,
}

/// All histograms of one resolved analysis unit, ready to fill.
pub struct HistoSet<'h> {
    pub histos: Vec<HistoFill<'h>>,
    setup_diags: Vec<String>,
}

impl<'h> HistoSet<'h> {
    /// Instantiate every accumulable histogram of `hir`; problems become
    /// setup diagnostics (one line each), never silent drops.
    #[must_use]
    pub fn new(hir: &'h Hir) -> Self {
        let mut diags = Vec::new();
        let region_weights = Self::region_weights(hir, &mut diags);
        let mut histos: Vec<HistoFill<'h>> = Vec::new();

        for (ridx, region) in hir.regions.iter().enumerate() {
            if hir.histolist_regions.get(ridx).copied().unwrap_or(false) {
                continue; // template block, instantiated at reference sites
            }
            let region_name = hir.symbols.display(region.name).to_owned();
            let mut seen_lists: Vec<usize> = Vec::new();
            // Own histos first (declaration order), then each referenced
            // histoList's histos at its (first) reference site.
            let mut candidates: Vec<&'h adl_sema::HirHisto> =
                hir.histos.iter().filter(|h| h.region == ridx).collect();
            for stmt in &region.stmts {
                let adl_sema::HirRegionStmt::Inherit { region: target, .. } = stmt else {
                    continue;
                };
                if !hir.histolist_regions.get(*target).copied().unwrap_or(false) {
                    continue; // ordinary region inheritance, not a histoList
                }
                if seen_lists.contains(target) {
                    let list = hir
                        .region_name_order
                        .get(*target)
                        .map_or("?", |&s| hir.symbols.display(s));
                    diags.push(format!(
                        "region `{region_name}`: histoList `{list}` referenced more than \
                         once; mid-selection fill points are not supported — its histograms \
                         fill once on full region acceptance"
                    ));
                    continue;
                }
                seen_lists.push(*target);
                candidates.extend(hir.histos.iter().filter(|h| h.region == *target));
            }

            for h in candidates {
                if histos
                    .iter()
                    .any(|f| f.region_idx == ridx && f.name.eq_ignore_ascii_case(&h.name))
                {
                    diags.push(format!(
                        "histo `{}` in region `{region_name}`: duplicate histogram name; \
                         only the first declaration fills",
                        h.name
                    ));
                    continue;
                }
                if let Some(fill) =
                    Self::instantiate(h, ridx, &region_name, region_weights[ridx], &mut diags)
                {
                    histos.push(fill);
                }
            }
        }

        Self {
            histos,
            setup_diags: diags,
        }
    }

    fn instantiate(
        h: &'h adl_sema::HirHisto,
        region_idx: usize,
        region_name: &str,
        weight: f64,
        diags: &mut Vec<String>,
    ) -> Option<HistoFill<'h>> {
        let skip = |diags: &mut Vec<String>, reason: &str| {
            diags.push(format!(
                "histo `{}` in region `{region_name}`: {reason}; histogram skipped",
                h.name
            ));
            None
        };
        match &h.spec {
            HistoSpec::Unsupported(reason) => skip(diags, reason),
            HistoSpec::Uniform1D {
                nbins,
                lo,
                hi,
                expr,
            } => {
                if expr.has_unsupported() {
                    return skip(diags, "fill expression is outside the checked fragment");
                }
                let (Ok(lo), Ok(hi)) = (lo.parse::<f64>(), hi.parse::<f64>()) else {
                    return skip(diags, "malformed axis bound");
                };
                if lo >= hi {
                    return skip(diags, &format!("empty axis range [{lo}, {hi})"));
                }
                Some(HistoFill {
                    name: h.name.clone(),
                    title: h.title.clone(),
                    region: region_name.to_owned(),
                    region_idx,
                    expr,
                    weight,
                    hist: Hist1D::new(*nbins, lo, hi),
                    nonvalue_skips: 0,
                    error_skips: 0,
                    first_error: None,
                })
            }
        }
    }

    /// Per-region fill weight: the product of the region's numeric
    /// `weight` statements; non-numeric arguments contribute 1.0 with a
    /// diagnostic.
    fn region_weights(hir: &Hir, diags: &mut Vec<String>) -> Vec<f64> {
        let mut weights = vec![1.0; hir.regions.len()];
        for w in &hir.weights {
            let region_name = hir
                .region_name_order
                .get(w.region)
                .map_or("?", |&s| hir.symbols.display(s));
            match &w.value {
                HirWeightValue::Num(text) => match text.parse::<f64>() {
                    Ok(v) => weights[w.region] *= v,
                    Err(_) => diags.push(format!(
                        "weight `{}` in region `{region_name}`: malformed numeric literal \
                         `{text}`; treated as 1.0",
                        w.name
                    )),
                },
                HirWeightValue::Other(desc) => diags.push(format!(
                    "weight `{}` in region `{region_name}`: non-numeric argument ({desc}); \
                     treated as 1.0",
                    w.name
                )),
            }
        }
        weights
    }

    /// Fill every histogram whose region accepted `event`. `results`
    /// must be the [`Interp::run_event`] output for the same event (one
    /// entry per HIR region, in declaration order).
    pub fn fill_event(&mut self, interp: &Interp<'h>, event: &Event, results: &[RegionResult]) {
        for f in &mut self.histos {
            let accepted = results
                .get(f.region_idx)
                .is_some_and(|r| r.pass == Ok(true));
            if !accepted {
                continue;
            }
            match interp.eval_num(f.expr, event) {
                Ok(NumOutcome::Value(x)) => f.hist.fill(x, f.weight),
                Ok(NumOutcome::NonValue(_)) => f.nonvalue_skips += 1,
                Err(e) => {
                    f.error_skips += 1;
                    if f.first_error.is_none() {
                        f.first_error = Some(e.reason);
                    }
                }
            }
        }
    }

    /// All diagnostics, deterministic: setup lines first, then per-
    /// histogram skipped-fill summaries (declaration order).
    #[must_use]
    pub fn diagnostics(&self) -> Vec<String> {
        let mut out = self.setup_diags.clone();
        for f in &self.histos {
            if f.nonvalue_skips > 0 {
                out.push(format!(
                    "histo `{}` (region `{}`): {} fill(s) skipped: expression had no value",
                    f.name, f.region, f.nonvalue_skips
                ));
            }
            if f.error_skips > 0 {
                let reason = f.first_error.as_deref().unwrap_or("evaluation error");
                out.push(format!(
                    "histo `{}` (region `{}`): {} fill(s) skipped: {reason}",
                    f.name, f.region, f.error_skips
                ));
            }
        }
        out
    }

    /// Canonical `histos.json` content. Field order is fixed (module
    /// docs); `pretty` selects 2-space indentation (the file form) vs a
    /// single line (the `run --json` form). Both are byte-deterministic.
    #[must_use]
    pub fn to_json(&self, pretty: bool) -> String {
        let mut w = JsonWriter::new(pretty);
        w.open('{');
        w.key("histograms");
        w.open('[');
        for f in &self.histos {
            w.open('{');
            w.key("name");
            w.str_val(&f.name);
            w.key("title");
            w.str_val(&f.title);
            w.key("region");
            w.str_val(&f.region);
            w.key("nbins");
            w.raw(&f.hist.nbins.to_string());
            w.key("lo");
            w.num(f.hist.lo);
            w.key("hi");
            w.num(f.hist.hi);
            w.key("sumw");
            w.num_array(&f.hist.sumw);
            w.key("sumw2");
            w.num_array(&f.hist.sumw2);
            w.key("underflow");
            w.flow(f.hist.underflow_w, f.hist.underflow_w2);
            w.key("overflow");
            w.flow(f.hist.overflow_w, f.hist.overflow_w2);
            w.key("entries");
            w.raw(&f.hist.entries.to_string());
            w.key("tsumw");
            w.num(f.hist.tsumw);
            w.key("tsumw2");
            w.num(f.hist.tsumw2);
            w.key("tsumwx");
            w.num(f.hist.tsumwx);
            w.key("tsumwx2");
            w.num(f.hist.tsumwx2);
            w.close('}');
        }
        w.close(']');
        w.close('}');
        w.finish()
    }
}

/// Minimal ordered-field JSON emitter. `serde_json`'s object model
/// reorders keys (BTreeMap) and the canonical schema fixes field order,
/// so the few forms needed here are written directly.
struct JsonWriter {
    out: String,
    pretty: bool,
    depth: usize,
    /// Does the current container already have an item?
    has_item: Vec<bool>,
    /// A key was just written; the next emit is its value (no separator).
    pending_value: bool,
}

impl JsonWriter {
    fn new(pretty: bool) -> Self {
        Self {
            out: String::new(),
            pretty,
            depth: 0,
            has_item: Vec::new(),
            pending_value: false,
        }
    }

    fn newline_indent(&mut self) {
        if self.pretty {
            self.out.push('\n');
            for _ in 0..self.depth {
                self.out.push_str("  ");
            }
        }
    }

    /// Separator before the next item; a no-op in value position.
    fn item(&mut self) {
        if self.pending_value {
            self.pending_value = false;
            return;
        }
        if let Some(has) = self.has_item.last_mut() {
            if *has {
                self.out.push(',');
            }
            *has = true;
            self.newline_indent();
        }
    }

    fn open(&mut self, c: char) {
        self.item();
        self.out.push(c);
        self.depth += 1;
        self.has_item.push(false);
    }

    fn close(&mut self, c: char) {
        self.depth -= 1;
        let had_items = self.has_item.pop() == Some(true);
        if had_items {
            self.newline_indent();
        }
        self.out.push(c);
    }

    fn key(&mut self, k: &str) {
        self.item();
        let _ = write!(self.out, "\"{k}\":");
        if self.pretty {
            self.out.push(' ');
        }
        self.pending_value = true;
    }

    fn raw(&mut self, v: &str) {
        self.item();
        self.out.push_str(v);
    }

    fn str_val(&mut self, s: &str) {
        self.item();
        let quoted = serde_json::to_string(s).expect("string serializes");
        self.out.push_str(&quoted);
    }

    fn num(&mut self, v: f64) {
        self.item();
        self.push_num(v);
    }

    /// serde_json/ryu shortest round-trip text; finite by construction.
    fn push_num(&mut self, v: f64) {
        let text = serde_json::to_string(&v).expect("finite f64 serializes");
        self.out.push_str(&text);
    }

    fn num_array(&mut self, vs: &[f64]) {
        self.item();
        self.out.push('[');
        for (i, &v) in vs.iter().enumerate() {
            if i > 0 {
                self.out.push(',');
                if self.pretty {
                    self.out.push(' ');
                }
            }
            self.push_num(v);
        }
        self.out.push(']');
    }

    /// `{"w": ..., "w2": ...}` — flow-bin pair, always inline.
    fn flow(&mut self, w: f64, w2: f64) {
        self.item();
        let sp = if self.pretty { " " } else { "" };
        self.out.push_str("{\"w\":");
        self.out.push_str(sp);
        self.push_num(w);
        self.out.push(',');
        self.out.push_str(sp);
        self.out.push_str("\"w2\":");
        self.out.push_str(sp);
        self.push_num(w2);
        self.out.push('}');
    }

    fn finish(mut self) -> String {
        if self.pretty {
            self.out.push('\n');
        }
        self.out
    }
}
