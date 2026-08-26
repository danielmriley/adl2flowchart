//! Positional ADL `weight` composition (SPEC_EVENT_PIPELINE §4,
//! [DECIDE-W1] resolved positional): within a region, the weight in
//! effect at statement `k` is the product of the *numeric* `weight`
//! statements declared at positions `< k`. The input event weight
//! (`Event::weight`) multiplies on top at accumulation time.
//!
//! A non-numeric `weight` argument ([DECIDE-W2] deferred) or a malformed
//! numeric literal contributes 1.0 and poisons every later position with
//! `incomplete = true` — downstream outputs flag the affected cutflow
//! steps and histograms `weighted_incomplete` instead of guessing.
//!
//! Diagnostics for bad weight values are emitted once, by
//! [`crate::histo::HistoSet::new`] (which runs for every analysis,
//! histograms or not); this walker is silent by design so the cutflow
//! accumulator never duplicates them.

use adl_sema::{Hir, HirRegionStmt, HirWeightValue};
use std::collections::HashMap;

/// Per-statement weight state of one region.
pub(crate) struct StmtWeights {
    /// `(ADL weight product, weighted_incomplete)` in effect **before**
    /// each statement; index = position in the region's `stmts`.
    pub eff: Vec<(f64, bool)>,
}

impl StmtWeights {
    /// Effect at statement `i` (defensive: out of range = neutral).
    pub(crate) fn at(&self, i: usize) -> (f64, bool) {
        self.eff.get(i).copied().unwrap_or((1.0, false))
    }
}

/// Walk region `ridx`'s statements computing the positional weight state.
pub(crate) fn stmt_weights(hir: &Hir, ridx: usize) -> StmtWeights {
    let region = &hir.regions[ridx];
    // `weight` payloads live in `hir.weights`; the region statement is a
    // `NonMembership { kind: "weight" }` marker with the same span.
    let by_span: HashMap<(u32, u32), &HirWeightValue> = hir
        .weights
        .iter()
        .filter(|w| w.region == ridx)
        .map(|w| ((w.span.start, w.span.end), &w.value))
        .collect();

    let mut factor = 1.0_f64;
    let mut incomplete = false;
    let mut eff = Vec::with_capacity(region.stmts.len());
    for stmt in &region.stmts {
        eff.push((factor, incomplete));
        if let HirRegionStmt::NonMembership {
            kind: "weight",
            span,
            ..
        } = stmt
        {
            match by_span.get(&(span.start, span.end)) {
                Some(HirWeightValue::Num(text)) => match text.parse::<f64>() {
                    Ok(v) => factor *= v,
                    Err(_) => incomplete = true,
                },
                Some(HirWeightValue::Other(_)) | None => incomplete = true,
            }
        }
    }
    StmtWeights { eff }
}
