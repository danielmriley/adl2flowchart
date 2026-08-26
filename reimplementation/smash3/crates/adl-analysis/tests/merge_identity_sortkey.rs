//! Regression pin: a collided `SortKey::Opaque` must never fabricate a
//! cross-file `PROVEN DISJOINT`.
//!
//! Found 2026-07-25 by the merge-identity invariant work: a take-level
//! `take sort(jets, dR(jets, refs[0]))` with a DIFFERENT `refs` in each
//! unit rendered a byte-identical opaque sort key, the two `Sorted`
//! collections interned to one, `sj[0].pt` became a single variable, and
//! the interval prefilter proved a false DISJOINT — with or without a
//! solver. (A region-level opaque `sort` was already blocked by the
//! fragment gate; the take-level form was not.) Fixed the same day by
//! unit-namespacing `SortKey::Opaque` in `Merger::remap_key`, the same
//! discipline `QuantityArg::Opaque` gets in `remap_arg`.
//!
//! The structural half lives in `adl-sema/tests/merge_identity.rs`; this
//! test pins the verdict level. Solver-optional: the collision fired via
//! the interval prefilter, so the pin is meaningful without z3 on PATH.

use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_hir};
use adl_sema::{ExtDecls, Hir, analyze_str, merge_hirs};
use std::time::Duration;

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
        reconcile: true,
        sample_gate: 64,
        refute_gate: true,
        certify: true,
        combine: false,
    }
}

/// Both files sort the SAME `jets` (Jet, pt>30) by their dR to the leading
/// element of a reference collection — but the reference differs: `pt>50` in
/// one file, `pt>90` in the other. Two different orderings of one set, so
/// `sj[0]` is a different jet in each file, and
/// `sj[0].pt > 100` / `sj[0].pt < 50` are jointly satisfiable.
///
/// The sort key is opaque and its render embeds unit-LOCAL collection ids;
/// `refs` happens to land at local id `C2` in both files, so both keys render
/// `"C1#jets,dR(C2#refs[0], C1#jets[*])"`. `Merger::remap_key` passes that
/// string through verbatim, the two `Sorted` collections intern to one, and
/// `sj[0].pt` becomes a single solver variable — which the interval prefilter
/// then "proves" cannot satisfy both regions.
#[test]
fn colliding_opaque_sort_keys_must_not_fabricate_a_cross_file_proof() {
    let ext = ExtDecls::legacy();
    let unit = |refcut: &str, cut: &str| {
        format!(
            "object jets\n  take Jet\n  select pt > 30\n\
             object refs\n  take Jet\n  {refcut}\n\
             object sj\n  take sort(jets, dR(jets, refs[0]))\n\
             region R\n  select sj[0].pt {cut}\n"
        )
    };
    let a = analyze_str(&unit("select pt > 50", "> 100"), "a.adl", &ext);
    let b = analyze_str(&unit("select pt > 90", "< 50"), "b.adl", &ext);
    for h in [&a, &b] {
        assert!(
            !adl_syntax::diag::has_errors(&h.diags),
            "unit {} must resolve cleanly: {:#?}",
            h.unit,
            h.diags
        );
    }

    let hirs: Vec<&Hir> = vec![&a, &b];
    let mut m = merge_hirs(&hirs);
    let r = analyze_hir(&mut m, "", &ext, &opts());
    let p = r
        .pairwise
        .iter()
        .find(|p| p.a.contains("a.adl") && p.b.contains("b.adl"))
        .expect("the cross-file pair");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "two files' DIFFERENT opaque sort keys were fused into one collection, so \
         sj[0].pt became one shared quantity and the pair was 'proven' disjoint: {}",
        p.reason
    );
}
