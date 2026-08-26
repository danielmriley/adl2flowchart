//! Histogram accumulation (PLAN Phase 9; weights per SPEC_EVENT_PIPELINE §4).
//!
//! ADL `histo h, "title", n, lo, hi, expr` statements fill during
//! `smash2 run` when the declaring region accepts the event, weighted by
//! `Event::weight × Π` of the region's *numeric* `weight` statements
//! declared **before the fill point** ([DECIDE-W1] positional
//! composition — equivalent to the former whole-region product whenever
//! all `weight` statements precede all fill points; a file where the two
//! differ gets a diagnostic). Semantics follow ROOT's `TH1` with `Sumw2`
//! (SPEC_ROOT_WRITER §4):
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
//! Accumulator forms (SPEC_EVENT_PIPELINE §3): uniform 1-D ([`Hist1D`]),
//! variable-bin 1-D ([`Hist1DVar`], binary-search binning, ROOT histogram
//! semantics — by design different from the `bin` statement's open last
//! bin), and uniform 2-D ([`Hist2D`], flow-inclusive cells in ROOT
//! global-bin order with the seven fill-time moments). Sema resolves all
//! three `histo` forms (`HistoSpec::Uniform1D`/`Var1D`/`Uniform2D`);
//! [`HistoSet::new`] instantiates each into the matching [`HistAcc`] arm
//! and everything downstream (JSON v2, bridges, out.root) renders it.
//!
//! Honesty rules: a histogram whose expression is out of fragment (or
//! whose form sema does not resolve yet) produces **one**
//! diagnostic and is skipped — it never appears in the output. A
//! non-numeric `weight` argument produces a diagnostic, contributes 1.0,
//! and flags every later fill point `weighted_incomplete` ([DECIDE-W2]
//! deferred — unfaithful values are flagged, never guessed). Histograms
//! declared in `histoList` blocks are templates,
//! instantiated into each selection region that references the list;
//! repeated references from one region fill once on full region
//! acceptance (mid-selection fill points are deferred).
//!
//! The canonical output is `histos.json` v2 ([`HistoSet::to_json`]):
//! top-level `version: 2`, each entry typed `"h1" | "h1var" | "h2"` with
//! deterministic field order — `h1`: `name, title, region, type, nbins,
//! lo, hi, sumw, sumw2, underflow, overflow, entries, tsumw, tsumw2,
//! tsumwx, tsumwx2`; `h1var` replaces `lo, hi` with `edges`; `h2`
//! carries both axes (`nx, xlo, xhi, ny, ylo, yhi`), flat flow-inclusive
//! `contents`/`sumw2` in ROOT global-bin order, and the seven moments —
//! plus `weighted_incomplete` (emitted only when `true`). The v1 → v2
//! bump is additive on `h1` entries (`type` key only).

use crate::eval::{Interp, NumOutcome, RegionResult};
use crate::event::Event;
use crate::json::JsonWriter;
use crate::provenance::Provenance;
use crate::weights::stmt_weights;
use adl_sema::{HNode, Hir, HirRegionStmt, HirWeightValue, HistoSpec};

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

    /// Merge a partial accumulator of the **same shape** into this one
    /// (SPEC_EVENT_PIPELINE §5 deterministic reduction). Field-wise
    /// addition in fixed order; with `self` starting from a fresh zero
    /// accumulator, `merge` of a single partial reproduces it bit-for-bit
    /// (`0.0 + v == v`). `nbins`/`lo`/`hi` come from the same HIR and must
    /// match (debug-asserted).
    pub fn merge(&mut self, other: &Hist1D) {
        debug_assert_eq!(
            (self.nbins, self.lo, self.hi),
            (other.nbins, other.lo, other.hi),
            "Hist1D::merge shape mismatch"
        );
        for (a, b) in self.sumw.iter_mut().zip(&other.sumw) {
            *a += *b;
        }
        for (a, b) in self.sumw2.iter_mut().zip(&other.sumw2) {
            *a += *b;
        }
        self.underflow_w += other.underflow_w;
        self.underflow_w2 += other.underflow_w2;
        self.overflow_w += other.overflow_w;
        self.overflow_w2 += other.overflow_w2;
        self.entries += other.entries;
        self.tsumw += other.tsumw;
        self.tsumw2 += other.tsumw2;
        self.tsumwx += other.tsumwx;
        self.tsumwx2 += other.tsumwx2;
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

/// A 1-D variable-bin weighted histogram (SPEC_EVENT_PIPELINE §3):
/// `edges` holds the `n + 1` strictly increasing bin edges; binning is by
/// binary search with ROOT histogram semantics (`x < edges[0]`
/// underflows, `x >= edges[n]` overflows — deliberately different from
/// the `bin` statement's open last bin, SPEC_LANGUAGE §4.3). Everything
/// else matches [`Hist1D`].
#[derive(Debug, Clone, PartialEq)]
pub struct Hist1DVar {
    /// `nbins + 1` strictly increasing edges.
    pub edges: Vec<f64>,
    /// Per-bin Σw, length `nbins`.
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

impl Hist1DVar {
    /// New empty histogram. At least 2 finite, strictly increasing edges
    /// are the caller's contract ([`HistoSet::new`] validates and skips
    /// violators).
    #[must_use]
    pub fn new(edges: Vec<f64>) -> Self {
        let n = edges.len().saturating_sub(1);
        Self {
            edges,
            sumw: vec![0.0; n],
            sumw2: vec![0.0; n],
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

    /// Merge a same-shape partial (SPEC_EVENT_PIPELINE §5); see
    /// [`Hist1D::merge`].
    pub fn merge(&mut self, other: &Hist1DVar) {
        debug_assert_eq!(self.edges, other.edges, "Hist1DVar::merge edge mismatch");
        for (a, b) in self.sumw.iter_mut().zip(&other.sumw) {
            *a += *b;
        }
        for (a, b) in self.sumw2.iter_mut().zip(&other.sumw2) {
            *a += *b;
        }
        self.underflow_w += other.underflow_w;
        self.underflow_w2 += other.underflow_w2;
        self.overflow_w += other.overflow_w;
        self.overflow_w2 += other.overflow_w2;
        self.entries += other.entries;
        self.tsumw += other.tsumw;
        self.tsumw2 += other.tsumw2;
        self.tsumwx += other.tsumwx;
        self.tsumwx2 += other.tsumwx2;
    }

    /// One fill: ROOT `TH1::Fill(x, w)` semantics over variable edges.
    pub fn fill(&mut self, x: f64, w: f64) {
        self.entries += 1;
        if x < self.edges[0] {
            self.underflow_w += w;
            self.underflow_w2 += w * w;
            return;
        }
        if x >= self.edges[self.edges.len() - 1] {
            self.overflow_w += w;
            self.overflow_w2 += w * w;
            return;
        }
        // x ∈ [edges[idx], edges[idx + 1]) ⇔ idx = #edges <= x, minus one.
        let idx = self.edges.partition_point(|e| *e <= x) - 1;
        self.sumw[idx] += w;
        self.sumw2[idx] += w * w;
        self.tsumw += w;
        self.tsumw2 += w * w;
        self.tsumwx += w * x;
        self.tsumwx2 += w * x * x;
    }
}

/// A uniform 2-D weighted histogram (SPEC_EVENT_PIPELINE §3): flat
/// flow-inclusive `(nx+2)·(ny+2)` cells in ROOT global-bin order
/// (`gbin = bx + (nx+2)·by`, x fastest), plus the seven fill-time
/// moments (`Σw, Σw², Σwx, Σwx², Σwy, Σwy², Σwxy` — in-range fills only,
/// both axes).
#[derive(Debug, Clone, PartialEq)]
pub struct Hist2D {
    pub nx: u32,
    pub xlo: f64,
    pub xhi: f64,
    pub ny: u32,
    pub ylo: f64,
    pub yhi: f64,
    /// Flow-inclusive Σw cells, ROOT global-bin order.
    pub sumw: Vec<f64>,
    /// Flow-inclusive Σw² cells, same order.
    pub sumw2: Vec<f64>,
    /// Raw fill count (ROOT `fEntries`), including flow-cell fills.
    pub entries: u64,
    pub tsumw: f64,
    pub tsumw2: f64,
    pub tsumwx: f64,
    pub tsumwx2: f64,
    pub tsumwy: f64,
    pub tsumwy2: f64,
    pub tsumwxy: f64,
}

impl Hist2D {
    /// New empty histogram. `nx, ny >= 1` and ordered finite axis ranges
    /// are the caller's contract.
    #[must_use]
    pub fn new(nx: u32, xlo: f64, xhi: f64, ny: u32, ylo: f64, yhi: f64) -> Self {
        let cells = (nx as usize + 2) * (ny as usize + 2);
        Self {
            nx,
            xlo,
            xhi,
            ny,
            ylo,
            yhi,
            sumw: vec![0.0; cells],
            sumw2: vec![0.0; cells],
            entries: 0,
            tsumw: 0.0,
            tsumw2: 0.0,
            tsumwx: 0.0,
            tsumwx2: 0.0,
            tsumwy: 0.0,
            tsumwy2: 0.0,
            tsumwxy: 0.0,
        }
    }

    /// Per-axis cell index in `0 ..= n + 1` (0 = underflow, n + 1 =
    /// overflow), ROOT `TAxis::FindBin` convention.
    fn axis_cell(x: f64, lo: f64, hi: f64, n: u32) -> usize {
        if x < lo {
            return 0;
        }
        if x >= hi {
            return n as usize + 1;
        }
        let frac = (x - lo) / (hi - lo);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // frac ∈ [0, 1) here, so the product is a small non-negative index.
        let idx = (frac * f64::from(n)) as usize;
        // Floating-point guard: x just below `hi` can round up to `n`.
        1 + idx.min(n as usize - 1)
    }

    /// Merge a same-shape partial (SPEC_EVENT_PIPELINE §5); see
    /// [`Hist1D::merge`]. All seven moments add field-wise.
    pub fn merge(&mut self, other: &Hist2D) {
        debug_assert_eq!(
            (self.nx, self.xlo, self.xhi, self.ny, self.ylo, self.yhi),
            (
                other.nx, other.xlo, other.xhi, other.ny, other.ylo, other.yhi
            ),
            "Hist2D::merge shape mismatch"
        );
        for (a, b) in self.sumw.iter_mut().zip(&other.sumw) {
            *a += *b;
        }
        for (a, b) in self.sumw2.iter_mut().zip(&other.sumw2) {
            *a += *b;
        }
        self.entries += other.entries;
        self.tsumw += other.tsumw;
        self.tsumw2 += other.tsumw2;
        self.tsumwx += other.tsumwx;
        self.tsumwx2 += other.tsumwx2;
        self.tsumwy += other.tsumwy;
        self.tsumwy2 += other.tsumwy2;
        self.tsumwxy += other.tsumwxy;
    }

    /// One fill: ROOT `TH2::Fill(x, y, w)` semantics. Stats accumulate
    /// only when **both** coordinates are in range (ROOT `GetStats`
    /// excludes flow cells).
    pub fn fill(&mut self, x: f64, y: f64, w: f64) {
        self.entries += 1;
        let bx = Self::axis_cell(x, self.xlo, self.xhi, self.nx);
        let by = Self::axis_cell(y, self.ylo, self.yhi, self.ny);
        let gbin = bx + (self.nx as usize + 2) * by;
        self.sumw[gbin] += w;
        self.sumw2[gbin] += w * w;
        if bx >= 1 && bx <= self.nx as usize && by >= 1 && by <= self.ny as usize {
            self.tsumw += w;
            self.tsumw2 += w * w;
            self.tsumwx += w * x;
            self.tsumwx2 += w * x * x;
            self.tsumwy += w * y;
            self.tsumwy2 += w * y * y;
            self.tsumwxy += w * x * y;
        }
    }
}

/// The accumulator behind one fill point — one variant per
/// SPEC_EVENT_PIPELINE §3 histogram form.
#[derive(Debug, Clone, PartialEq)]
pub enum HistAcc {
    H1(Hist1D),
    H1Var(Hist1DVar),
    H2(Hist2D),
}

impl HistAcc {
    /// Raw fill count (ROOT `fEntries`), whatever the form.
    #[must_use]
    pub fn entries(&self) -> u64 {
        match self {
            HistAcc::H1(h) => h.entries,
            HistAcc::H1Var(h) => h.entries,
            HistAcc::H2(h) => h.entries,
        }
    }

    /// Merge a same-form partial (SPEC_EVENT_PIPELINE §5). A form mismatch
    /// cannot occur — both sides instantiate from the same HIR — and is a
    /// debug-assert; release builds skip the mismatched merge rather than
    /// corrupt either accumulator.
    fn merge(&mut self, other: &HistAcc) {
        match (self, other) {
            (HistAcc::H1(a), HistAcc::H1(b)) => a.merge(b),
            (HistAcc::H1Var(a), HistAcc::H1Var(b)) => a.merge(b),
            (HistAcc::H2(a), HistAcc::H2(b)) => a.merge(b),
            _ => debug_assert!(false, "HistAcc::merge form mismatch"),
        }
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
    /// Fill expression(s): x, plus y for the 2-D form.
    expr: &'h HNode,
    expr_y: Option<&'h HNode>,
    /// ADL weight product in effect at the fill point ([DECIDE-W1]
    /// positional); the input event weight multiplies at fill time.
    factor: f64,
    /// A non-numeric/malformed `weight` precedes the fill point: the
    /// weighted contents are incomplete ([DECIDE-W2] — flagged, never
    /// guessed).
    pub weighted_incomplete: bool,
    pub hist: HistAcc,
    /// Fills skipped because the expression had no value (soft
    /// non-value: missing element/property, non-finite arithmetic).
    nonvalue_skips: u64,
    /// Fills skipped on a hard evaluation error (missing event-level data).
    error_skips: u64,
    first_error: Option<String>,
}

impl<'h> HistoFill<'h> {
    /// The uniform 1-D accumulator, when this fill point is that form.
    #[must_use]
    pub fn h1(&self) -> Option<&Hist1D> {
        match &self.hist {
            HistAcc::H1(h) => Some(h),
            _ => None,
        }
    }

    /// One accepted-event fill: evaluate the expression(s) and route to
    /// the accumulator. A 2-D fill needs **both** coordinates: a
    /// non-value or error on either skips the whole fill (counted once).
    fn fill_from(&mut self, interp: &Interp<'h>, event: &Event) {
        let w = self.factor * event.weight;
        let x = match interp.eval_num(self.expr, event) {
            Ok(NumOutcome::Value(x)) => x,
            Ok(NumOutcome::NonValue(_)) => {
                self.nonvalue_skips += 1;
                return;
            }
            Err(e) => {
                self.error_skips += 1;
                if self.first_error.is_none() {
                    self.first_error = Some(e.reason);
                }
                return;
            }
        };
        let y = match self.expr_y {
            None => None,
            Some(ey) => match interp.eval_num(ey, event) {
                Ok(NumOutcome::Value(y)) => Some(y),
                Ok(NumOutcome::NonValue(_)) => {
                    self.nonvalue_skips += 1;
                    return;
                }
                Err(e) => {
                    self.error_skips += 1;
                    if self.first_error.is_none() {
                        self.first_error = Some(e.reason);
                    }
                    return;
                }
            },
        };
        match (&mut self.hist, y) {
            (HistAcc::H1(h), _) => h.fill(x, w),
            (HistAcc::H1Var(h), _) => h.fill(x, w),
            (HistAcc::H2(h), Some(y)) => h.fill(x, y, w),
            // Unreachable by construction: instantiation pairs every H2
            // accumulator with a y expression. Counted, never silent.
            (HistAcc::H2(_), None) => {
                debug_assert!(false, "2-D accumulator without a y expression");
                self.error_skips += 1;
                if self.first_error.is_none() {
                    self.first_error = Some("internal: 2-D fill without y expression".into());
                }
            }
        }
    }

    /// Merge a partial of the **same fill point** (same HIR ⇒ same name,
    /// region, accumulator form). The skip counters add; the first error
    /// message follows chunk order (the §5 fold visits chunks ascending,
    /// so `first_error` is the earliest-chunk first error — input order).
    fn merge(&mut self, other: &HistoFill<'h>) {
        debug_assert_eq!(self.name, other.name, "HistoFill::merge name mismatch");
        debug_assert_eq!(
            self.region_idx, other.region_idx,
            "HistoFill::merge region mismatch"
        );
        self.hist.merge(&other.hist);
        self.nonvalue_skips += other.nonvalue_skips;
        self.error_skips += other.error_skips;
        if self.first_error.is_none() {
            self.first_error.clone_from(&other.first_error);
        }
    }
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
        Self::weight_diags(hir, &mut diags);
        let mut histos: Vec<HistoFill<'h>> = Vec::new();

        for (ridx, region) in hir.regions.iter().enumerate() {
            if hir.histolist_regions.get(ridx).copied().unwrap_or(false) {
                continue; // template block, instantiated at reference sites
            }
            let region_name = hir.symbols.display(region.name).to_owned();
            let weights = stmt_weights(hir, ridx);
            // Fill-point position of each own `histo` statement, by span.
            let own_pos: std::collections::HashMap<(u32, u32), usize> = region
                .stmts
                .iter()
                .enumerate()
                .filter_map(|(i, s)| match s {
                    HirRegionStmt::NonMembership {
                        kind: "histo",
                        span,
                        ..
                    } => Some(((span.start, span.end), i)),
                    _ => None,
                })
                .collect();
            let mut seen_lists: Vec<usize> = Vec::new();
            // Own histos first (declaration order), then each referenced
            // histoList's histos at its (first) reference site — each
            // candidate carries the weight state at its fill point.
            let mut candidates: Vec<(&'h adl_sema::HirHisto, (f64, bool))> = hir
                .histos
                .iter()
                .filter(|h| h.region == ridx)
                .map(|h| {
                    let eff = own_pos
                        .get(&(h.span.start, h.span.end))
                        .map_or((1.0, false), |&i| weights.at(i));
                    (h, eff)
                })
                .collect();
            let mut first_fill_stmt: Option<usize> = own_pos.values().copied().min();
            for (i, stmt) in region.stmts.iter().enumerate() {
                let HirRegionStmt::Inherit { region: target, .. } = stmt else {
                    continue;
                };
                if !hir.histolist_regions.get(*target).copied().unwrap_or(false) {
                    continue; // ordinary region inheritance, not a histoList
                }
                first_fill_stmt = Some(first_fill_stmt.map_or(i, |f| f.min(i)));
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
                candidates.extend(
                    hir.histos
                        .iter()
                        .filter(|h| h.region == *target)
                        .map(|h| (h, weights.at(i))),
                );
            }

            // [DECIDE-W1] lint: a `weight` after a fill point makes the
            // positional product differ from the former whole-region one.
            if let Some(first) = first_fill_stmt
                && region.stmts.iter().enumerate().any(|(i, s)| {
                    i > first && matches!(s, HirRegionStmt::NonMembership { kind: "weight", .. })
                })
            {
                diags.push(format!(
                    "region `{region_name}`: a `weight` statement follows a histogram fill \
                     point; weights compose positionally ([DECIDE-W1]) — earlier fill points \
                     exclude the later weight"
                ));
            }

            for (h, eff) in candidates {
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
                if let Some(fill) = Self::instantiate(h, ridx, &region_name, eff, &mut diags) {
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
        (factor, weighted_incomplete): (f64, bool),
        diags: &mut Vec<String>,
    ) -> Option<HistoFill<'h>> {
        let skip = |diags: &mut Vec<String>, reason: &str| {
            diags.push(format!(
                "histo `{}` in region `{region_name}`: {reason}; histogram skipped",
                h.name
            ));
            None
        };
        let mk = |expr: &'h HNode, expr_y, hist| {
            Some(HistoFill {
                name: h.name.clone(),
                title: h.title.clone(),
                region: region_name.to_owned(),
                region_idx,
                expr,
                expr_y,
                factor,
                weighted_incomplete,
                hist,
                nonvalue_skips: 0,
                error_skips: 0,
                first_error: None,
            })
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
                mk(expr, None, HistAcc::H1(Hist1D::new(*nbins, lo, hi)))
            }
            HistoSpec::Var1D { edges, expr } => {
                if expr.has_unsupported() {
                    return skip(diags, "fill expression is outside the checked fragment");
                }
                let mut parsed = Vec::with_capacity(edges.len());
                for e in edges {
                    let Ok(v) = e.parse::<f64>() else {
                        return skip(diags, &format!("malformed bin edge `{e}`"));
                    };
                    parsed.push(v);
                }
                // Sema guarantees ≥ 2 strictly increasing edges; re-check so
                // a future caller cannot smuggle a degenerate axis past us.
                if parsed.len() < 2 || parsed.windows(2).any(|w| w[0] >= w[1]) {
                    return skip(diags, "bin edges must be strictly increasing");
                }
                mk(expr, None, HistAcc::H1Var(Hist1DVar::new(parsed)))
            }
            HistoSpec::Uniform2D {
                nx,
                xlo,
                xhi,
                ny,
                ylo,
                yhi,
                xexpr,
                yexpr,
            } => {
                if xexpr.has_unsupported() || yexpr.has_unsupported() {
                    return skip(diags, "fill expression is outside the checked fragment");
                }
                let (Ok(xlo), Ok(xhi), Ok(ylo), Ok(yhi)) = (
                    xlo.parse::<f64>(),
                    xhi.parse::<f64>(),
                    ylo.parse::<f64>(),
                    yhi.parse::<f64>(),
                ) else {
                    return skip(diags, "malformed axis bound");
                };
                if xlo >= xhi {
                    return skip(diags, &format!("empty x axis range [{xlo}, {xhi})"));
                }
                if ylo >= yhi {
                    return skip(diags, &format!("empty y axis range [{ylo}, {yhi})"));
                }
                mk(
                    xexpr,
                    Some(yexpr),
                    HistAcc::H2(Hist2D::new(*nx, xlo, xhi, *ny, ylo, yhi)),
                )
            }
        }
    }

    /// One diagnostic per bad `weight` value (declaration order); the
    /// 1.0 fallback and the `weighted_incomplete` flagging live in
    /// `weights.rs` (shared with the cutflow accumulator).
    fn weight_diags(hir: &Hir, diags: &mut Vec<String>) {
        for w in &hir.weights {
            let region_name = hir
                .region_name_order
                .get(w.region)
                .map_or("?", |&s| hir.symbols.display(s));
            match &w.value {
                HirWeightValue::Num(text) => {
                    if text.parse::<f64>().is_err() {
                        diags.push(format!(
                            "weight `{}` in region `{region_name}`: malformed numeric literal \
                             `{text}`; treated as 1.0",
                            w.name
                        ));
                    }
                }
                HirWeightValue::Other(desc) => diags.push(format!(
                    "weight `{}` in region `{region_name}`: non-numeric argument ({desc}); \
                     treated as 1.0",
                    w.name
                )),
            }
        }
    }

    /// Fill every histogram whose region accepted `event`, weighted by
    /// `event.weight ×` the fill point's ADL weight product
    /// (SPEC_EVENT_PIPELINE §4). `results` must be the
    /// [`Interp::run_event`] output for the same event (one entry per
    /// HIR region, in declaration order).
    pub fn fill_event(&mut self, interp: &Interp<'h>, event: &Event, results: &[RegionResult]) {
        for f in &mut self.histos {
            let accepted = results
                .get(f.region_idx)
                .is_some_and(|r| r.pass == Ok(true));
            if !accepted {
                continue;
            }
            f.fill_from(interp, event);
        }
    }

    /// Merge a partial [`HistoSet`] of the same analysis unit into this one
    /// (SPEC_EVENT_PIPELINE §5). Both sets come from the same HIR, so
    /// `histos[i]` aligns by index; `setup_diags` is HIR-derived and
    /// identical, so the master's copy is kept. Folding partials in
    /// ascending chunk order makes the result byte-identical to a serial
    /// run that processes those same chunks in order.
    pub fn merge(&mut self, other: &HistoSet<'h>) {
        debug_assert_eq!(
            self.histos.len(),
            other.histos.len(),
            "HistoSet::merge length mismatch"
        );
        for (a, b) in self.histos.iter_mut().zip(&other.histos) {
            a.merge(b);
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
        self.to_json_with(pretty, None)
    }

    /// `histos.json` with the SPEC_EVENT_PIPELINE §6 `provenance` object
    /// embedded as a top-level key (after `version`, before
    /// `histograms`) when supplied. The embedded object is the same
    /// canonical bytes carried by `cutflow.json`/`out.root`.
    #[must_use]
    pub fn to_json_with(&self, pretty: bool, provenance: Option<&Provenance>) -> String {
        let mut w = JsonWriter::new(pretty);
        w.open('{');
        w.key("version");
        w.raw("2");
        if let Some(p) = provenance {
            w.key("provenance");
            p.write(&mut w);
        }
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
            w.key("type");
            match &f.hist {
                HistAcc::H1(h) => {
                    w.str_val("h1");
                    w.key("nbins");
                    w.raw(&h.nbins.to_string());
                    w.key("lo");
                    w.num(h.lo);
                    w.key("hi");
                    w.num(h.hi);
                    h1_tail_json(
                        &mut w,
                        &h.sumw,
                        &h.sumw2,
                        h.underflow_w,
                        h.underflow_w2,
                        h.overflow_w,
                        h.overflow_w2,
                        h.entries,
                        h.tsumw,
                        h.tsumw2,
                        h.tsumwx,
                        h.tsumwx2,
                    );
                }
                HistAcc::H1Var(h) => {
                    w.str_val("h1var");
                    w.key("nbins");
                    w.raw(&h.sumw.len().to_string());
                    w.key("edges");
                    w.num_array(&h.edges);
                    h1_tail_json(
                        &mut w,
                        &h.sumw,
                        &h.sumw2,
                        h.underflow_w,
                        h.underflow_w2,
                        h.overflow_w,
                        h.overflow_w2,
                        h.entries,
                        h.tsumw,
                        h.tsumw2,
                        h.tsumwx,
                        h.tsumwx2,
                    );
                }
                HistAcc::H2(h) => {
                    w.str_val("h2");
                    w.key("nx");
                    w.raw(&h.nx.to_string());
                    w.key("xlo");
                    w.num(h.xlo);
                    w.key("xhi");
                    w.num(h.xhi);
                    w.key("ny");
                    w.raw(&h.ny.to_string());
                    w.key("ylo");
                    w.num(h.ylo);
                    w.key("yhi");
                    w.num(h.yhi);
                    w.key("contents");
                    w.num_array(&h.sumw);
                    w.key("sumw2");
                    w.num_array(&h.sumw2);
                    w.key("entries");
                    w.raw(&h.entries.to_string());
                    w.key("tsumw");
                    w.num(h.tsumw);
                    w.key("tsumw2");
                    w.num(h.tsumw2);
                    w.key("tsumwx");
                    w.num(h.tsumwx);
                    w.key("tsumwx2");
                    w.num(h.tsumwx2);
                    w.key("tsumwy");
                    w.num(h.tsumwy);
                    w.key("tsumwy2");
                    w.num(h.tsumwy2);
                    w.key("tsumwxy");
                    w.num(h.tsumwxy);
                }
            }
            if f.weighted_incomplete {
                w.key("weighted_incomplete");
                w.raw("true");
            }
            w.close('}');
        }
        w.close(']');
        w.close('}');
        w.finish()
    }
}

/// The shared 1-D entry tail: `sumw, sumw2, underflow, overflow,
/// entries, tsumw, tsumw2, tsumwx, tsumwx2`.
#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the fixed canonical field order"
)]
fn h1_tail_json(
    w: &mut JsonWriter,
    sumw: &[f64],
    sumw2: &[f64],
    under_w: f64,
    under_w2: f64,
    over_w: f64,
    over_w2: f64,
    entries: u64,
    tsumw: f64,
    tsumw2: f64,
    tsumwx: f64,
    tsumwx2: f64,
) {
    w.key("sumw");
    w.num_array(sumw);
    w.key("sumw2");
    w.num_array(sumw2);
    w.key("underflow");
    w.flow(under_w, under_w2);
    w.key("overflow");
    w.flow(over_w, over_w2);
    w.key("entries");
    w.raw(&entries.to_string());
    w.key("tsumw");
    w.num(tsumw);
    w.key("tsumw2");
    w.num(tsumw2);
    w.key("tsumwx");
    w.num(tsumwx);
    w.key("tsumwx2");
    w.num(tsumwx2);
}

#[cfg(test)]
mod tests {
    use super::*;
    use adl_sema::ExtDecls;

    fn ext() -> ExtDecls {
        ExtDecls::legacy()
    }

    /// Two 1-D histos give us two independent fill expressions (`MET`,
    /// `MET / 10`) to assemble the not-yet-sema-reachable forms directly —
    /// the wiring point coverage for `fill_from` and the v2 JSON arms.
    const TWO_EXPR_ADL: &str = "region SR\n  select MET > 10\n  \
                                histo hx, \"x\", 2, 0, 100, MET\n  \
                                histo hy, \"y\", 2, 0, 10, MET / 10\n";

    fn met_event(ext: &ExtDecls, met: f64) -> Event {
        crate::parse_event(
            &format!("{{\"MET\": {{\"pt\": {met}, \"phi\": 0.0}}}}"),
            ext,
        )
        .expect("event parses")
    }

    /// Rebuild the set with the H1 fills replaced by one H2 (x = MET,
    /// y = MET / 10) and one H1Var fill sharing the x expression.
    fn synthetic_set<'h>(hir: &'h Hir) -> HistoSet<'h> {
        let base = HistoSet::new(hir);
        assert_eq!(base.histos.len(), 2, "fixture declares two 1-D histos");
        let [hx, hy] = match &base.histos[..] {
            [a, b] => [a.expr, b.expr],
            _ => unreachable!(),
        };
        let mk = |name: &str, expr_y, hist| HistoFill {
            name: name.to_owned(),
            title: name.to_owned(),
            region: "SR".to_owned(),
            region_idx: base.histos[0].region_idx,
            expr: hx,
            expr_y,
            factor: 2.0,
            weighted_incomplete: false,
            hist,
            nonvalue_skips: 0,
            error_skips: 0,
            first_error: None,
        };
        HistoSet {
            histos: vec![
                mk(
                    "h2d",
                    Some(hy),
                    HistAcc::H2(Hist2D::new(2, 0.0, 100.0, 2, 0.0, 10.0)),
                ),
                mk(
                    "hvar",
                    None,
                    HistAcc::H1Var(Hist1DVar::new(vec![0.0, 30.0, 70.0, 150.0])),
                ),
            ],
            setup_diags: Vec::new(),
        }
    }

    #[test]
    fn h2_and_h1var_fill_through_the_set_and_render_v2_json() {
        let ext = ext();
        let hir = adl_sema::analyze_str(TWO_EXPR_ADL, "t.adl", &ext);
        assert!(!adl_syntax::diag::has_errors(&hir.diags), "{:?}", hir.diags);
        let interp = Interp::new(&hir, &ext);
        let mut set = synthetic_set(&hir);
        // MET = 25 → x bin 1, y = 2.5 → y bin 1; MET = 75 → x bin 2, y bin 2;
        // MET = 250 → both overflow; MET = 5 fails the region (no fill).
        for met in [25.0, 75.0, 250.0, 5.0] {
            let ev = met_event(&ext, met);
            let results = interp.run_event(&ev);
            set.fill_event(&interp, &ev, &results);
        }

        let HistAcc::H2(h2) = &set.histos[0].hist else {
            panic!("h2 form")
        };
        assert_eq!(h2.entries, 3);
        // gbin = bx + (nx+2)*by with nx = 2: (1,1) → 5, (2,2) → 10,
        // overflow (3,3) → 15; weight factor 2.0 each.
        assert_eq!(h2.sumw[5], 2.0);
        assert_eq!(h2.sumw[10], 2.0);
        assert_eq!(h2.sumw[15], 2.0);
        assert_eq!(h2.sumw.iter().sum::<f64>(), 6.0);
        assert_eq!(h2.sumw2[5], 4.0);
        // In-range moments exclude the overflow fill.
        assert_eq!(h2.tsumw, 4.0);
        assert_eq!(h2.tsumwx, 2.0 * 25.0 + 2.0 * 75.0);
        assert_eq!(h2.tsumwy, 2.0 * 2.5 + 2.0 * 7.5);
        assert_eq!(h2.tsumwxy, 2.0 * 25.0 * 2.5 + 2.0 * 75.0 * 7.5);

        let HistAcc::H1Var(hv) = &set.histos[1].hist else {
            panic!("h1var form")
        };
        assert_eq!(hv.entries, 3);
        assert_eq!(hv.sumw, vec![2.0, 0.0, 2.0]); // 25 → [0,30), 75 → [70,150)
        assert_eq!(hv.overflow_w, 2.0); // 250 ≥ 150
        assert_eq!(hv.tsumw, 4.0);

        let json = set.to_json(false);
        assert!(json.starts_with("{\"version\":2,"));
        assert!(json.contains(
            "\"type\":\"h2\",\"nx\":2,\"xlo\":0.0,\"xhi\":100.0,\"ny\":2,\"ylo\":0.0,\"yhi\":10.0,\
             \"contents\":["
        ));
        assert!(json.contains("\"tsumwxy\":"));
        assert!(json.contains(
            "\"type\":\"h1var\",\"nbins\":3,\"edges\":[0.0,30.0,70.0,150.0],\"sumw\":[2.0,0.0,2.0]"
        ));
        // Determinism of the new arms.
        assert_eq!(json, set.to_json(false));
    }

    #[test]
    fn h2_skips_fill_when_either_coordinate_is_unavailable() {
        let ext = ext();
        // y expression indexes a collection that can be empty.
        let adl = "object goodJets\n  take Jet\n  select pt > 30\n\
                   region SR\n  select MET > 10\n  \
                   histo hx, \"x\", 2, 0, 100, MET\n  \
                   histo hy, \"y\", 2, 0, 1000, goodJets[0].pt\n";
        let hir = adl_sema::analyze_str(adl, "t.adl", &ext);
        assert!(!adl_syntax::diag::has_errors(&hir.diags), "{:?}", hir.diags);
        let interp = Interp::new(&hir, &ext);
        let base = HistoSet::new(&hir);
        let mut set = HistoSet {
            histos: vec![HistoFill {
                name: "h2d".to_owned(),
                title: "t".to_owned(),
                region: "SR".to_owned(),
                region_idx: base.histos[0].region_idx,
                expr: base.histos[0].expr,
                expr_y: Some(base.histos[1].expr),
                factor: 1.0,
                weighted_incomplete: false,
                hist: HistAcc::H2(Hist2D::new(2, 0.0, 100.0, 2, 0.0, 1000.0)),
                nonvalue_skips: 0,
                error_skips: 0,
                first_error: None,
            }],
            setup_diags: Vec::new(),
        };
        // No jets: y has no value → the whole fill is skipped, counted once.
        let ev = crate::parse_event("{\"MET\": {\"pt\": 50, \"phi\": 0.0}, \"Jet\": []}", &ext)
            .expect("event parses");
        let results = interp.run_event(&ev);
        set.fill_event(&interp, &ev, &results);
        let HistAcc::H2(h2) = &set.histos[0].hist else {
            panic!("h2 form")
        };
        assert_eq!(h2.entries, 0, "no partial fill from x alone");
        assert_eq!(set.histos[0].nonvalue_skips, 1);
        assert_eq!(
            set.diagnostics(),
            vec![
                "histo `h2d` (region `SR`): 1 fill(s) skipped: expression had no value".to_owned()
            ]
        );
    }
}
