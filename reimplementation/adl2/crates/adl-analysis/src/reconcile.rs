//! Cross/intra-collection reconciliation encoding (Track C keystone).
//!
//! Two analyses (or two object blocks) rarely spell a shared collection the
//! same way — `jets` cut `pt > 30` in one file and `pt > 25` in another are
//! byte-distinct filter DAGs, so structural interning never relates them and
//! their sizes look independent. This module lowers each pure `Base`/`Filtered`
//! chain's membership predicate onto ONE shared generic base element
//! (`base[GENERIC_INDEX]`), so that two predicates over the same base symbol
//! live in the same quantity space and the existing UNSAT-side subset prover
//! can decide refinement.
//!
//! It is PURELY an encoding step: it interns helper quantities and produces a
//! [`Formula`] per candidate collection, but emits no solver fact and reads no
//! verdict. The engine ([`crate::engine`]) consumes the [`ReconEnc`], proves
//! each candidate pair's refinement on the subset side, and only then asserts a
//! derived `size(A) <= size(B)` fact — so nothing here can, by itself, change a
//! verdict. Soundness of the derived facts is argued at the XSUB/XEQ catalog
//! rows and in [`crate::engine::Engine::reconcile`].
//!
//! Fail-closed guards (soundness): a candidate is DROPPED when a filter
//! predicate references a composite binder/reduce element OR a CONCRETE peer
//! element (`Jet[1]`, an angular separation, a collection size) — see
//! [`adl_axioms::encode_elem_pred_generic`]; and a candidate whose base is not
//! a known ext detector object is skipped (private base names collide by
//! spelling). Both were confirmed false-PROVEN paths in adversarial review.
//!
//! DOCUMENTED RESIDUAL (property-alias class, same status as base identity and
//! the float-vs-real gap): distinct source property spellings that canonicalise
//! to ONE key (e.g. `constituents`/`daughters` → `ccountof` in
//! property_vars.txt) ground onto one generic quantity, so a refinement can be
//! proven between them. This is SELF-CONSISTENT — the interpreter and witness
//! realizer read both through the same key, so re-validation agrees — and is a
//! tool-wide modeling choice inherited from the legacy property table, not a
//! reconciliation-specific contradiction.

use adl_axioms::{GENERIC_INDEX, encode_elem_pred_generic};
use adl_formula::{DiagTable, Formula};
use adl_sema::{Collection, ElemPredId, ExtDecls, Hir, Quantity, QuantityId};
use std::collections::BTreeSet;

/// One reconciliation candidate: two distinct filtered collections over the
/// same base, with their membership predicates lowered onto the shared generic
/// element. `phi_a`/`phi_b` are three-valued (opaque leaves are `Unknown`,
/// never dropped) so the engine can honour the FULL superset predicate on the
/// `.under()` side.
pub(crate) struct ReconCandidate {
    pub size_a: QuantityId,
    pub size_b: QuantityId,
    pub phi_a: Formula,
    pub phi_b: Formula,
}

/// The reconciliation encoding for one (merged) unit.
pub(crate) struct ReconEnc {
    pub candidates: Vec<ReconCandidate>,
}

impl ReconEnc {
    /// Every helper quantity the candidate formulas mention (generic-element
    /// properties) plus the two collection sizes per candidate — the engine
    /// must declare these to the solver before asserting anything.
    pub(crate) fn quantities(&self) -> BTreeSet<QuantityId> {
        let mut out = BTreeSet::new();
        for c in &self.candidates {
            out.insert(c.size_a);
            out.insert(c.size_b);
            crate::encode::formula_quantities(&c.phi_a, &mut out);
            crate::encode::formula_quantities(&c.phi_b, &mut out);
        }
        out
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }
}

/// Build the reconciliation encoding: for every candidate pair, lower both
/// filter chains' predicates onto the shared base's generic element. A pair is
/// dropped (fail-closed, NO relation) when a predicate references a residual
/// composite/reduce binder that cannot ground onto one element.
///
/// Mutates `hir.table` (interns the shared base collection, the generic
/// element quantities, and the two collection sizes). MUST run after
/// `emit_axioms` so these helper quantities receive no base axioms of their own
/// — they exist only inside the transient subset frames the engine pushes.
pub(crate) fn build(hir: &mut Hir, ext: &ExtDecls) -> ReconEnc {
    let cands = hir.table.reconciliation_candidates();
    let mut candidates = Vec::new();
    for (a, b) in cands {
        // Both flatten to the same base symbol (guaranteed by the candidate
        // enumeration); re-read to obtain the predicate lists.
        let Some((base_sym, preds_a)) = hir.table.filter_chain(a) else {
            continue;
        };
        let Some((_, preds_b)) = hir.table.filter_chain(b) else {
            continue;
        };
        // Only reconcile over a genuine DETECTOR base (a known ext object:
        // Jet/Electron/Muon/MET/...). An UNKNOWN base name is interned
        // verbatim as a PRIVATE base (resolve_base_name), so two analyses can
        // reuse the same spelling for physically different inputs and collide
        // to one base Symbol — the "same base name = same input" residual is
        // safe only for real shared detector objects, not arbitrary names.
        if ext.base_collection(hir.symbols.display(base_sym)).is_none() {
            continue;
        }
        let base = hir.table.intern_collection(Collection::Base(base_sym));
        let Some(phi_a) = lower(hir, &preds_a, base) else {
            continue;
        };
        let Some(phi_b) = lower(hir, &preds_b, base) else {
            continue;
        };
        let size_a = hir.table.intern_quantity(Quantity::Size(a));
        let size_b = hir.table.intern_quantity(Quantity::Size(b));
        candidates.push(ReconCandidate {
            size_a,
            size_b,
            phi_a,
            phi_b,
        });
    }
    ReconEnc { candidates }
}

/// Conjoin a filter chain's predicates, each lowered onto `base[GENERIC_INDEX]`.
/// Returns `None` if any predicate fails to ground (residual binder/reduce) —
/// the whole pair is then excluded.
fn lower(hir: &mut Hir, preds: &[ElemPredId], base: adl_sema::CollectionId) -> Option<Formula> {
    // Reconciliation diagnostics are never surfaced (opaque leaves resolve to
    // true/false under .over()/.under()), so a throwaway table is correct.
    let mut diags = DiagTable::default();
    let mut parts = Vec::with_capacity(preds.len());
    for &pid in preds {
        let node = hir.elem_pred(pid).node.clone();
        let f = encode_elem_pred_generic(&mut hir.table, &node, base, GENERIC_INDEX, &mut diags)?;
        parts.push(f);
    }
    Some(match parts.len() {
        // A bare filter with no lowered conjuncts keeps every base element.
        0 => Formula::True,
        1 => parts.pop().expect("len == 1"),
        _ => Formula::And(parts),
    })
}
