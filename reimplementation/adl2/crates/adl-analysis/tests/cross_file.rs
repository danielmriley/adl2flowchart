//! Cross-file (merged-unit) analysis soundness: `merge_hirs` re-interns
//! several units into one structural identity space, and the engine then
//! proves region relations ACROSS files. The load-bearing property is that
//! quantities unify *iff* structurally identical — so a cross-file PROVEN
//! verdict fires only on genuinely-shared quantities, never on same-named
//! but differently-cut ones (which would be a fabricated proof).

use adl_analysis::report::{PairReport, Report};
use adl_analysis::{AnalysisOptions, FailOn, SolverChoice, VerdictKind, analyze_hir};
use adl_sema::{ExtDecls, Hir, analyze_str, merge_hirs};
use std::time::Duration;

fn opts() -> AnalysisOptions {
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
        reconcile: false,
    }
}

/// Cross options with reconciliation enabled — mirrors what `verify --cross`
/// sets, so the integration tests exercise the derived size facts.
fn opts_reconcile() -> AnalysisOptions {
    AnalysisOptions {
        reconcile: true,
        ..opts()
    }
}

/// Resolve `(name, src)` units, merge them, and run the cross-file analysis.
fn cross(units: &[(&str, &str)]) -> Report {
    let ext = ExtDecls::legacy();
    let hirs: Vec<Hir> = units.iter().map(|(n, s)| analyze_str(s, n, &ext)).collect();
    for h in &hirs {
        assert!(
            !adl_syntax::diag::has_errors(&h.diags),
            "unit {} has resolve errors: {:#?}",
            h.unit,
            h.diags
        );
    }
    let refs: Vec<&Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    analyze_hir(&mut merged, "", &ext, &opts())
}

/// The pair whose region names contain the two substrings (either order).
fn pair<'a>(r: &'a Report, x: &str, y: &str) -> &'a PairReport {
    r.pairwise
        .iter()
        .find(|p| {
            (p.a.contains(x) && p.b.contains(y)) || (p.a.contains(y) && p.b.contains(x))
        })
        .unwrap_or_else(|| panic!("no pair {x}/{y} in {:?}", r.pairwise.iter().map(|p| (&p.a, &p.b)).collect::<Vec<_>>()))
}

#[test]
fn cross_proves_subset_and_disjoint_on_a_shared_quantity() {
    // MET.pt is a shared base quantity, so it unifies across the three units.
    let r = cross(&[
        ("hi", "region SRhi\n  select MET.pt > 200\n"),
        ("lo", "region SRlo\n  select MET.pt > 100\n"),
        ("veto", "region SRveto\n  select MET.pt < 50\n"),
    ]);

    // MET>200 ⊆ MET>100 → overlapping with a subset relation pointing at hi.
    let p = pair(&r, "SRhi", "SRlo");
    assert_eq!(p.kind, VerdictKind::ProvenOverlapping, "{p:?}");
    let hi_in_lo = (p.a.contains("SRhi") && p.subset_a_in_b)
        || (p.b.contains("SRhi") && p.subset_b_in_a);
    assert!(hi_in_lo, "SRhi must be proven a subset of SRlo: {p:?}");

    // MET>200 vs MET<50 cannot both hold → proven disjoint across files.
    assert_eq!(
        pair(&r, "SRhi", "SRveto").kind,
        VerdictKind::ProvenDisjoint,
        "disjoint across files on the shared MET.pt"
    );
}

#[test]
fn cross_does_not_falsely_unify_same_name_different_cut() {
    // Both files define `goodjets`, but with DIFFERENT cuts, so their sizes
    // are DIFFERENT quantities. `size>=2` and `size<=1` must NOT be proven
    // disjoint across the files (that would be a fabricated proof).
    let r = cross(&[
        (
            "d",
            "object goodjets\n  take Jet\n  select pt > 30\nregion Rd\n  select size(goodjets) >= 2\n",
        ),
        (
            "e",
            "object goodjets\n  take Jet\n  select pt > 100\nregion Re\n  select size(goodjets) <= 1\n",
        ),
    ]);
    assert_ne!(
        pair(&r, "Rd", "Re").kind,
        VerdictKind::ProvenDisjoint,
        "same-named-but-differently-cut collections must not unify (no false PROVEN DISJOINT)"
    );

    // Control: IDENTICAL cuts → the sizes ARE the same quantity, so
    // `size>=2` vs `size<=1` is correctly proven disjoint.
    let r = cross(&[
        (
            "d",
            "object goodjets\n  take Jet\n  select pt > 30\nregion Rd\n  select size(goodjets) >= 2\n",
        ),
        (
            "f",
            "object goodjets\n  take Jet\n  select pt > 30\nregion Rf\n  select size(goodjets) <= 1\n",
        ),
    ]);
    assert_eq!(
        pair(&r, "Rd", "Rf").kind,
        VerdictKind::ProvenDisjoint,
        "identical cuts → shared size quantity → correctly disjoint"
    );
}

#[test]
fn cross_single_unit_matches_normal_analysis() {
    // Merging one unit must reproduce its single-file verdicts exactly
    // (the structural remap is identity-preserving). Compare verdict-kind
    // multisets between merged-of-one and a direct analysis.
    let src = "\
object jets
  take Jet
  select pT > 30
region SR1
  select size(jets) >= 2
  select MET.pt > 200
region SR2
  select size(jets) >= 2
  select MET.pt < 100
region SR3
  select MET.pt > 50
";
    let ext = ExtDecls::legacy();
    let direct =
        adl_analysis::analyze_source(src, "u", &ext, &opts()).expect("direct analysis");
    let merged = cross(&[("u", src)]);

    let counts = |r: &Report| -> [usize; 5] {
        let mut c = [0usize; 5];
        for p in &r.pairwise {
            let i = match p.kind {
                VerdictKind::ProvenDisjoint => 0,
                VerdictKind::ProvenOverlapping => 1,
                VerdictKind::CandidateOverlapping => 2,
                VerdictKind::PossiblyOverlapping => 3,
                VerdictKind::Unknown => 4,
            };
            c[i] += 1;
        }
        c
    };
    assert_eq!(
        counts(&direct),
        counts(&merged),
        "merged-of-one must match the direct single-file verdict counts"
    );
    assert_eq!(direct.pairwise.len(), merged.pairwise.len());
}

#[test]
fn cross_opaque_external_does_not_falsely_unify() {
    // REGRESSION (critical false-unify): an opaque external/aggregate over an
    // arithmetic arg renders to a QuantityArg::Opaque embedding a SOURCE-LOCAL
    // collection id. Two units whose objects land at the same local id used to
    // intern these structurally-DIFFERENT sums to one shared quantity → a
    // fabricated PROVEN DISJOINT. After per-unit namespacing they stay distinct.
    let r = cross(&[
        (
            "ua",
            "object goodjets\n  take Jet\n  select pt > 30\nregion RA\n  select sum(goodjets.pt + 1) > 500\n",
        ),
        (
            "ub",
            "object goodjets\n  take Jet\n  select pt > 100\nregion RB\n  select sum(goodjets.pt + 1) < 50\n",
        ),
    ]);
    assert_ne!(
        pair(&r, "RA", "RB").kind,
        VerdictKind::ProvenDisjoint,
        "opaque externals over differently-cut collections must not unify across units"
    );
}

#[test]
fn cross_opaque_external_same_unit_name_attack() {
    // SAME unit name as the only difference from the passing regression above.
    // `unit_name(file)` is the file BASENAME, so two files named cuts.adl in
    // different dirs share src.unit -> the namespacing prefix is identical ->
    // the opaque args collide -> fabricated PROVEN DISJOINT.
    let r = cross(&[
        (
            "cuts.adl",
            "object goodjets\n  take Jet\n  select pt > 30\nregion RA\n  select sum(goodjets.pt + 1) > 500\n",
        ),
        (
            "cuts.adl",
            "object goodjets\n  take Muon\n  select pt > 40\nregion RB\n  select sum(goodjets.pt + 1) < 50\n",
        ),
    ]);
    assert_ne!(
        pair(&r, "RA", "RB").kind,
        VerdictKind::ProvenDisjoint,
        "same-basename units must not let opaque externals over different physics collapse"
    );
}

#[test]
fn cross_dr_unifies_regardless_of_object_declaration_order() {
    // REGRESSION (AngularSep canonicalization): unoriented dR must canonicalize
    // its operands through intern_angular under the SHARED ids, so the same
    // physical dR cut unifies across units even when the two base objects are
    // declared in opposite order — yielding the genuine PROVEN DISJOINT.
    let r = cross(&[
        (
            "o1",
            "object j\n  take Jet\nobject e\n  take Electron\nregion R1\n  select dR(j[0], e[0]) > 0.4\n",
        ),
        (
            "o2",
            "object e\n  take Electron\nobject j\n  take Jet\nregion R2\n  select dR(j[0], e[0]) < 0.2\n",
        ),
    ]);
    assert_eq!(
        pair(&r, "R1", "R2").kind,
        VerdictKind::ProvenDisjoint,
        "the same dR cut must unify across units despite opposite object declaration order"
    );
    assert!(
        r.internal_diagnostics.is_empty(),
        "no INTERNAL witness-validation diagnostic expected: {:?}",
        r.internal_diagnostics
    );
}

#[test]
fn cross_colliding_region_names_do_not_mask_witness_validation() {
    // REGRESSION (false-proven via name collision): two units share the unit
    // label AND a region name, so the merged regions collide on `g::R`. Witness
    // re-validation must resolve by INDEX, not name — otherwise both regions
    // resolve to the first match, region B's decidable dR cut is never checked,
    // and a true POSSIBLY is promoted to a fabricated PROVEN OVERLAPPING.
    // Region A: just MET>50. Region B: MET>50 AND dR(jet[0],MET)>0.4 — a
    // DECIDABLE cut the witness builder cannot realize (MET has no
    // pseudorapidity, so there is no eta to separate jet[0] from), so B
    // genuinely fails re-validation. (Object-vs-object dR IS realizable now, so
    // the unrealizable anchor must be MET.)
    let r = cross(&[
        ("g", "object jet\n  take Jet\nregion R\n  select MET.pt > 50\n"),
        (
            "g",
            "object jet\n  take Jet\nregion R\n  select MET.pt > 50\n  select dR(jet[0], MET) > 0.4\n",
        ),
    ]);
    assert_eq!(r.pairwise.len(), 1, "two regions → one pair");
    assert_ne!(
        r.pairwise[0].kind,
        VerdictKind::ProvenOverlapping,
        "region B's unrealizable dR cut must not be masked by the name collision: {:?}",
        r.pairwise[0]
    );
}

#[test]
fn cross_diagnostics_render_from_hir_not_empty_src() {
    // REGRESSION (round-3 diagnostics defect): a merged unit has no single
    // `src`, so cut text / bin labels must render from the HIR, not slice an
    // empty string (which produced blank cut text and `[?]` bin labels).
    let r = cross(&[
        ("u", "region S\n  select MET.pt > 50\n  bin MET.pt 100 200\n"),
        ("v", "region T\n  select MET.pt > 50\n  bin MET.pt 100 200\n"),
    ]);
    let out = r.human_default(false);
    assert!(!out.contains("[?]"), "bin label must render from HIR, not '?':\n{out}");
    assert!(out.contains("[MET.pt]"), "bin variable label expected:\n{out}");
}

/// Cross-file analysis with reconciliation ENABLED, mirroring `verify --cross`.
fn cross_reconcile(units: &[(&str, &str)]) -> Report {
    let ext = ExtDecls::legacy();
    let hirs: Vec<Hir> = units.iter().map(|(n, s)| analyze_str(s, n, &ext)).collect();
    for h in &hirs {
        assert!(
            !adl_syntax::diag::has_errors(&h.diags),
            "unit {} has resolve errors: {:#?}",
            h.unit,
            h.diags
        );
    }
    let refs: Vec<&Hir> = hirs.iter().collect();
    let mut merged = merge_hirs(&refs);
    analyze_hir(&mut merged, "", &ext, &opts_reconcile())
}

#[test]
fn reconcile_unlocks_disjoint_via_refinement() {
    // The keystone: two files filter the SAME base `Jet` with DIFFERENT pt
    // cuts. A's `pt > 100` is a REFINEMENT of B's `pt > 30`, so
    // size(A_jets) <= size(B_jets). A needs >= 3 tight jets, B allows <= 2
    // loose jets: 3 <= size(A) <= size(B) <= 2 is UNSAT -> PROVEN DISJOINT.
    let units = &[
        ("a", "object jets\n  take Jet\n  select pt > 100\nregion RA\n  select size(jets) >= 3\n"),
        ("b", "object jets\n  take Jet\n  select pt > 30\nregion RB\n  select size(jets) <= 2\n"),
    ];
    // Without reconciliation the two sizes are independent -> POSSIBLY.
    assert_eq!(
        pair(&cross(units), "RA", "RB").kind,
        VerdictKind::PossiblyOverlapping,
        "control: sizes independent without reconciliation"
    );
    // With reconciliation the derived size fact makes it PROVEN DISJOINT.
    let rr = cross_reconcile(units);
    let p = pair(&rr, "RA", "RB");
    assert_eq!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "reconciliation must relate the two jet sizes and prove disjoint: {p:?}"
    );
}

#[test]
fn reconcile_is_directional_no_false_proven() {
    // SOUNDNESS: the derived fact is size(tight) <= size(loose) ONLY. Here the
    // TIGHT collection is bounded ABOVE (<= 2) and the LOOSE one below (>= 3):
    // size(tight) <= size(loose), size(tight) <= 2, size(loose) >= 3 is
    // perfectly consistent (e.g. 0 and 5). It must NOT be proven disjoint.
    let units = &[
        ("a", "object jets\n  take Jet\n  select pt > 100\nregion RA\n  select size(jets) <= 2\n"),
        ("b", "object jets\n  take Jet\n  select pt > 30\nregion RB\n  select size(jets) >= 3\n"),
    ];
    assert_ne!(
        pair(&cross_reconcile(units), "RA", "RB").kind,
        VerdictKind::ProvenDisjoint,
        "the refinement bounds size(tight) <= size(loose); the reverse must not be assumed"
    );
}

#[test]
fn reconcile_opaque_superset_conjunct_fails_closed() {
    // B's predicate has an OPAQUE conjunct (btag) that under-approximates to
    // false, so A `pt > 100` cannot be proven a subset of B `pt > 30 && btag`.
    // No size fact is derived, so nothing is proven disjoint (fail-closed).
    let units = &[
        ("a", "object jets\n  take Jet\n  select pt > 100\nregion RA\n  select size(jets) >= 3\n"),
        ("b", "object jets\n  take Jet\n  select pt > 30\n  select btag == 1\nregion RB\n  select size(jets) <= 2\n"),
    ];
    assert_ne!(
        pair(&cross_reconcile(units), "RA", "RB").kind,
        VerdictKind::ProvenDisjoint,
        "an opaque superset conjunct must block the refinement (no fabricated PROVEN)"
    );
}

#[test]
fn reconcile_fails_closed_on_concrete_peer_index() {
    // SOUNDNESS REGRESSION (adversarial review, confirmed false PROVEN): a
    // filter predicate that references a CONCRETE peer element `pt(Jet[k])`
    // keeps that peer's SHARED analysis quantity id. The base-frame ORD axiom
    // (pt(Jet[1]) >= pt(Jet[2]), asserted unconditionally) would leak into the
    // reconciliation subset frame and "prove" size(A) <= size(B) — FALSE in a
    // 2-jet event where Jet[2] does not exist. reconcile must fail closed:
    // NO derived size fact, so SR (size(A)>=1 AND size(B)<1) is NOT empty.
    use adl_analysis::report::EmptyStatus;
    let r = cross_reconcile(&[(
        "u",
        "object A\n  take Jet\n  select pT(Jet) > pT(Jet[1])\n\
         object B\n  take Jet\n  select pT(Jet) > pT(Jet[2])\n\
         region CARRIER\n  select pT(Jet[1]) > 0\n  select pT(Jet[2]) > 0\n\
         region SR\n  select size(A) >= 1\n  select size(B) < 1\n",
    )]);
    let sr = r.regions.iter().find(|x| x.name.contains("SR")).expect("SR region");
    assert_ne!(
        sr.empty,
        EmptyStatus::Proven,
        "concrete-peer predicate must fail closed; SR must not be fabricated-empty: {sr:?}"
    );
}

#[test]
fn reconcile_skips_private_base_name_collision() {
    // SOUNDNESS (adversarial review): two files reuse a NON-builtin base name
    // (`PuppiJet`) for what may be physically different inputs. It interns as a
    // private base and collides by spelling across files. Reconciliation must
    // NOT relate their sizes (no XSUB, no subset claim) — the "same base name =
    // same input" residual is honoured only for genuine ext detector objects.
    let r = cross_reconcile(&[
        ("a", "object cleanjets\n  take PuppiJet\n  select pt > 30\nregion SRA\n  select size(cleanjets) >= 1\n"),
        ("b", "object hardjets\n  take PuppiJet\n  select pt > 30\n  select pt < 200\nregion SRB\n  select size(hardjets) >= 1\n"),
    ]);
    let p = pair(&r, "SRA", "SRB");
    assert!(
        !p.subset_a_in_b && !p.subset_b_in_a,
        "private-base name collision must not yield a subset claim: {p:?}"
    );
    assert_ne!(p.kind, VerdictKind::ProvenDisjoint, "{p:?}");

    // Control: the SAME shapes over the builtin `Jet` base DO reconcile
    // (hardjets pt in (30,200) subset of cleanjets pt>30 -> subset claim).
    let r = cross_reconcile(&[
        ("a", "object cleanjets\n  take Jet\n  select pt > 30\nregion SRA\n  select size(cleanjets) >= 1\n"),
        ("b", "object hardjets\n  take Jet\n  select pt > 30\n  select pt < 200\nregion SRB\n  select size(hardjets) >= 1\n"),
    ]);
    let p = pair(&r, "SRA", "SRB");
    assert!(
        p.subset_a_in_b || p.subset_b_in_a,
        "builtin Jet base must reconcile hardjets subset cleanjets: {p:?}"
    );
}
