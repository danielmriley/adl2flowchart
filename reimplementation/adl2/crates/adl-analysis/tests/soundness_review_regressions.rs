//! Regression pins for the 2026-07-01 soundness review of the disjointness /
//! overlap / subset / vacuity verdict path (see
//! docs/SOUNDNESS_REVIEW_2026-07-01_VALIDATION_SYSTEM.md §2). Each test names
//! its finding id and encodes the review's reproduced ground-truth
//! counterexample — the physical event that inhabits BOTH regions (or the
//! crash / missing diagnostic) — and asserts the FIXED behavior. Fixes land
//! concurrently, so a test here may fail until its fix integrates; every
//! failure is a still-live soundness escape, never a spurious red.

use adl_analysis::{
    AnalysisOptions, EmptyStatus, FailOn, PairReport, SolverChoice, VerdictKind, analyze_source,
};
use adl_sema::{ExtDecls, analyze_str};
use std::time::Duration;

fn opts(solver: SolverChoice) -> AnalysisOptions {
    AnalysisOptions {
        solver,
        timeout: Duration::from_secs(20),
        fail_on: FailOn::default(),
        reconcile: false,
    }
}

/// The pair over the two named regions, order-independent.
fn find_pair<'a>(pairs: &'a [PairReport], x: &str, y: &str) -> &'a PairReport {
    pairs
        .iter()
        .find(|p| (p.a == x && p.b == y) || (p.a == y && p.b == x))
        .unwrap_or_else(|| panic!("no pair {x} vs {y} in {pairs:?}"))
}

/// Does the report claim `sub` ⊆ `sup`? Resolves the subset flag against
/// which region the pair actually named `a`/`b`.
fn claims_within(p: &PairReport, sub: &str, sup: &str) -> bool {
    if p.a == sub && p.b == sup {
        p.subset_a_in_b
    } else if p.a == sup && p.b == sub {
        p.subset_b_in_a
    } else {
        panic!("pair {} vs {} is not over {sub}/{sup}", p.a, p.b)
    }
}

/// S1 (CRITICAL, RC-A): `<unsupported: reason>` render masks the differing
/// sub-expression, so two reducer-reject bodies over different parents intern
/// to one ElemPredId → one Size variable → contradictory size cuts prove a
/// false DISJOINT. Ground truth (review §2 S1): a 5-jet event far from every
/// electron but near a muon keeps all 5 in `cleanjetsA` (>=4) and drops all
/// from `cleanjetsB` (0 <= 1) — it inhabits BOTH regions, so the pair is not
/// disjoint.
#[test]
fn s1_unsupported_render_must_not_unify() {
    let src = "\
object eles
  take Ele
object muons
  take Muo
object cleanjetsA
  take Jet
  reject any(dR(this, eles) < 0.2 and pt(eles) > 10)
object cleanjetsB
  take Jet
  reject any(dR(this, muons) < 0.4 and pt(muons) > 20)
region RA
  select size(cleanjetsA) >= 4
region RB
  select size(cleanjetsB) <= 1
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s1.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let p = find_pair(&r.pairwise, "RA", "RB");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "cleanjetsA/cleanjetsB collapse to one Size var; a 5-jets-near-muon \
         event is in both regions: {}",
        p.reason
    );
}

/// S2 (CRITICAL, RC-A): a function-wrapped element property (`sqrt(pt)`) on
/// different parent blocks degenerates to one context-free opaque key, so
/// `sqrt(pt)` over Jet and over Muo share a QuantityId and EPRED fabricates a
/// false DISJOINT. Ground truth (review §2 S2, shape 1): an event with a jet
/// (pt 100 → sqrt > 5, `bigA[0].pt > 0`) and a muon (pt 1 → sqrt < 2,
/// `bigB[0].pt > 0`) inhabits BOTH regions.
#[test]
fn s2_elem_self_external_identity() {
    let src = "\
object bigA
  take Jet
  select sqrt(pt) > 5
object bigB
  take Muo
  select sqrt(pt) < 2
region RA
  select pT(bigA[0]) > 0
region RB
  select pT(bigB[0]) > 0
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s2_elem.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let p = find_pair(&r.pairwise, "RA", "RB");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "sqrt(pt) over Jet and over Muo are different quantities; a pt=100 \
         jet + pt=1 muon event is in both regions: {}",
        p.reason
    );
}

/// S2 (CRITICAL, RC-A): a binder used as an external argument (`dR(j, eles)`)
/// collapses to a context-free opaque key shared by every block, so the
/// Jet-binder `dR` and the Pho-binder `dR` intern as one quantity → false
/// DISJOINT. Ground truth (review §2 S2, shape 2): an event with a jet far
/// from electrons (`dR > 0.4`, `cleanA[0].pt > 0`) and a photon near an
/// electron (`dR < 0.1`, `cleanB[0].pt > 0`) inhabits BOTH regions.
#[test]
fn s2_binder_dr_identity() {
    let src = "\
object eles
  take Ele
object cleanA
  take Jet j
  select dR(j, eles) > 0.4
object cleanB
  take Pho p
  select dR(p, eles) < 0.1
region RA
  select pT(cleanA[0]) > 0
region RB
  select pT(cleanB[0]) > 0
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s2_binder.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let p = find_pair(&r.pairwise, "RA", "RB");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "dR(j, eles) on Jet and dR(p, eles) on Pho are per-element distinct; \
         a far-jet + near-photon event is in both regions: {}",
        p.reason
    );
}

/// S2 (CRITICAL, RC-A): the field-standard lepton-cleaning / overlap-removal
/// idiom — `reject dR(j, leptons) < 0.4` vs `select dR(k, leptons) < 0.3` —
/// collapses onto one shared per-element `dR` quantity and EPRED proves a
/// false DISJOINT. Ground truth (review §2 S2, shape 3): an event with one jet
/// far from all leptons (kept by `cleanjets`, size >= 1) AND one jet near a
/// lepton (kept by `lepjets`, size >= 1) inhabits BOTH regions.
#[test]
fn s2_epred_corpus_shape() {
    let src = "\
object leptons
  take Ele
object cleanjets
  take Jet j
  reject dR(j, leptons) < 0.4
object lepjets
  take Jet k
  select dR(k, leptons) < 0.3
region RA
  select size(cleanjets) >= 1
region RB
  select size(lepjets) >= 1
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s2_epred.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let p = find_pair(&r.pairwise, "RA", "RB");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "clean vs lepton-overlapping jets can both be non-empty in one event; \
         the shared dR quantity must not fabricate DISJOINT: {}",
        p.reason
    );
}

/// S3 (HIGH, RC-B): front-to-back ORD (`k==1, i>=1`) lower-bounds an absent
/// front element while IDOM upper-bounds it, breaching the pad-with-0 contract
/// → the base frame is unsatisfiable and a false DISJOINT is derived. Region C
/// exists only to pull `pt(goodjets[2])` into the (unioned) axiom set. Ground
/// truth (review §2 S3): Jet pts [100, 40, 10] with etas [0, 0, 3] →
/// `goodjets` = {100, 40}; A passes (size 2, goodjets[-1]=40 >= 30) and B
/// passes (Jet[2]=10 <= 15). The interpreter passes both; the pair is not
/// disjoint.
#[test]
fn s3_f2b_ord_idom_joint() {
    let src = "\
object goodjets
  take Jet
  select eta < 2
region A
  select size(goodjets) >= 2
  select pT(goodjets[-1]) >= 30
region B
  select pT(Jet[2]) <= 15
region C
  select pT(goodjets[2]) > 0
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s3.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let p = find_pair(&r.pairwise, "A", "B");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "F2B-ORD × IDOM must not fabricate DISJOINT; pts [100,40,10] etas \
         [0,0,3] pass both regions: {}",
        p.reason
    );
}

/// S4 (HIGH): a region-level `sort` is an environment mutation, not a
/// membership hedge — encoding it as pure Unknown (over→True) while ORD still
/// binds the pT-descending element quantities makes the over-projection no
/// longer a superset, fabricating a false EMPTY / DISJOINT. Ground truth
/// (review §2 S4): under `sort pt(jets) ascend`, a `[150, 25]` event re-sorts
/// to `[25, 150]`, so `jets[0].pt = 25 < 30` and `jets[1].pt = 150 > 100`
/// both hold — SR is inhabited and overlaps CR (`MET > 0`).
#[test]
fn s4_region_sort_must_not_prove_empty() {
    let src = "\
object jets
  take Jet
  select pt > 20
region SR
  select MET > 0
  sort pt(jets) ascend
  select jets[0].pt < 30
  select jets[1].pt > 100
region CR
  select MET > 0
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s4.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let sr = r.regions.iter().find(|x| x.name == "SR").expect("SR present");
    assert_ne!(
        sr.empty,
        EmptyStatus::Proven,
        "a [150,25]→[25,150] ascending event inhabits SR; it is not empty",
    );
    let p = find_pair(&r.pairwise, "SR", "CR");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "SR is inhabited and shares MET > 0 with CR: {}",
        p.reason
    );
}

/// S5 (HIGH): `guard_existence` early-returns unless the formula is exact, so a
/// mixed-exactness `min(...)` (one opaque ternary arg) drops the `size > 0`
/// guard on the under side, leaving a bare element atom → `¬(B⁻)` too small →
/// false SUBSET. Ground truth (review §2 S5): one Jet at pt 10 → it is in A
/// (`Jet[0].pt = 10 < 50`) but the pt>20 `jets` collection is empty, so B's
/// `min(jets[0].pt, …)` comparison is false and the event is NOT in B —
/// therefore A ⊄ B and the report must not claim it.
#[test]
fn s5_min_guard_must_not_prove_subset() {
    let src = "\
object jets
  take Jet
  select pt > 20
region A
  select Jet[0].pt < 50
region B
  select min(jets[0].pt, MET > 100 ? MET : 7) < 50
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s5.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let p = find_pair(&r.pairwise, "A", "B");
    assert!(
        !claims_within(p, "A", "B"),
        "a single pt=10 jet is in A but not in B; SUBSET A⊆B is false: {}",
        p.reason
    );
}

/// S6 (MEDIUM): `subst` has no `ScalarMinMax` arm, so the OPEN-1 leaf path
/// recurses forever and stack-overflows. Ground truth (review §2 S6):
/// `select min(jets.pt, MET) < 50` crashed the analyzer (core dump). The fix
/// is that analysis simply RETURNS — the test is that `analyze_source` does
/// not crash and produces the single region pair. (Solver-independent: the
/// crash is in encoding, so no no-solver skip.)
#[test]
fn s6_minmax_collprop_must_not_crash() {
    let src = "\
object jets
  take Jet
  select pt > 20
region A
  select min(jets.pt, MET) < 50
region B
  select MET > 100
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s6.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    assert_eq!(
        r.pairwise.len(),
        1,
        "min(jets.pt, MET) must encode without a stack overflow: {:?}",
        r.pairwise
    );
}

/// S7 (MEDIUM): a sort-direction token other than literally `ascend` fails open
/// to Descend, and the descend+pt alias gate then unifies the "sorted"
/// collection with its pT-descending source — ORD proves a DISJOINT false
/// under the ascending intent. Ground truth (review §2 S7): with `ascending`,
/// `upjets` is pt-ascending, so an event `[10, 200]` gives `upjets[0] = 10 <
/// 50` (RB) and `upjets[1] = 200 > 100` (RA) — both hold, the pair is not
/// disjoint. The fix fails closed to an opaque `Sorted` (no alias).
#[test]
fn s7_sort_direction_token_fails_closed() {
    let src = "\
object jets
  take Jet
object upjets
  take sort(jets, pt(jets), ascending)
region RA
  select pT(upjets[1]) > 100
region RB
  select pT(upjets[0]) < 50
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "s7.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver");
        return;
    }
    let p = find_pair(&r.pairwise, "RA", "RB");
    assert_ne!(
        p.kind,
        VerdictKind::ProvenDisjoint,
        "`ascending` must not alias to pt-descending; a [10,200] ascending \
         event inhabits both regions: {}",
        p.reason
    );
}

/// S8 (MEDIUM): duplicate `object` names are silently first-binding-wins with
/// no diagnostic, despite a code comment claiming a duplicate is diagnosed.
/// Ground truth (review §2 S8): two `object jets` blocks — every reference
/// binds to the first, `check` exits 0 with no signal. The fix emits a
/// duplicate-name diagnostic. (Uses `analyze_str` directly: a duplicate error
/// would make `analyze_source` return Err, and we want to inspect the diag.)
#[test]
fn s8_duplicate_object_name_diagnosed() {
    let src = "\
object jets
  take Jet
  select pt > 20
object jets
  take Jet
  select pt > 50
region R
  select size(jets) >= 1
";
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, "s8.adl", &ext);
    assert!(
        hir.diags
            .iter()
            .any(|d| d.message.to_ascii_lowercase().contains("duplicate")),
        "duplicate `object jets` must be diagnosed: {:?}",
        hir.diags
    );
}
