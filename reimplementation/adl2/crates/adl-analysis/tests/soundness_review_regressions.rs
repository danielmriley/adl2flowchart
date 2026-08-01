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
        sample_gate: 64,
        refute_gate: true,
        certify: true,
        combine: false,
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

    // The identity collapse this pins fabricated its PROVEN through the
    // INTERVAL fast path, which runs without any solver — so this assertion
    // must hold solver-less too. A skip here would make the sole verdict-
    // level pin for the identity class vacuous in exactly the configuration
    // (no solver → interval path) where the bug is reachable.
    let r0 =
        analyze_source(src, "s1.adl", &ext, &opts(SolverChoice::NoSolver)).expect("resolves");
    let p0 = find_pair(&r0.pairwise, "RA", "RB");
    assert_ne!(
        p0.kind,
        VerdictKind::ProvenDisjoint,
        "identity collapse must not prove via the solver-free interval path: {}",
        p0.reason
    );

    let r = analyze_source(src, "s1.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver (interval-path assertion above still ran)");
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

/// ABSENT-PROPERTY seam (CRITICAL, 2026-07-25): the interpreter evaluates a
/// comparison over an ABSENT property as a decidable soft `false` (NaN
/// convention), so a `reject` over it HOLDS precisely because the data is
/// missing — while the classical encoding `¬(q ⋈ k)` constrains a total
/// valuation. Two complementary rejects over the same possibly-absent
/// property therefore fabricated a PROVEN DISJOINT through the interval
/// path (no axiom, no solver needed): a btag-less-jet event is accepted by
/// BOTH regions (neither veto fires) — verified via `run` on a real JSONL
/// event. The battery's jets carry btag, so the sampling gate was blind and
/// the false verdict SHIPPED. Ground truth: NOT disjoint over loader-valid
/// events. The encoder's `guarded_not` now degrades such negations to
/// Unknown (fail-closed) pending definedness modeling.
#[test]
fn absent_property_complementary_rejects_must_not_prove_disjoint() {
    let src = "\
object jets
  take Jet
region RA
  select size(jets) >= 1
  reject BTag(jets[0]) > 0.5
region RB
  select size(jets) >= 1
  reject BTag(jets[0]) <= 0.5
";
    let ext = ExtDecls::legacy();
    // The fabrication fired through the solver-free interval path, so the
    // pin must hold without a solver too (same rationale as S1 above).
    let r0 =
        analyze_source(src, "absent.adl", &ext, &opts(SolverChoice::NoSolver)).expect("resolves");
    let p0 = find_pair(&r0.pairwise, "RA", "RB");
    assert_ne!(
        p0.kind,
        VerdictKind::ProvenDisjoint,
        "complementary rejects over a possibly-absent property must not prove \
         via the interval path: {}",
        p0.reason
    );

    let r = analyze_source(src, "absent.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver != "none" {
        let p = find_pair(&r.pairwise, "RA", "RB");
        assert_ne!(p.kind, VerdictKind::ProvenDisjoint, "{}", p.reason);
        // And the guard must not have traded the false proof for a gate
        // refutation — the claim should never be DERIVED at all now.
        assert_eq!(
            r.sampling.as_ref().map(|s| s.refutations),
            Some(0),
            "the guard should prevent the derivation, not rely on the gate"
        );
    }
}

/// Same seam, positive/negative split: `select q ⋈ k` (positive) is still
/// exactly encodable — a decided-true cut implies the property was present —
/// so sizes-and-selects proofs must survive the guard (soundness must not
/// cost all precision). The b-veto idiom via sizes is the canonical case.
#[test]
fn size_based_vetoes_still_prove_after_the_absent_guard() {
    let src = "\
object jets
  take Jet
object bjets
  take jets
  select btag == 1
region ZeroB
  select size(bjets) == 0
region MultiB
  select size(bjets) > 0
";
    let ext = ExtDecls::legacy();
    let r0 =
        analyze_source(src, "veto.adl", &ext, &opts(SolverChoice::NoSolver)).expect("resolves");
    let p0 = find_pair(&r0.pairwise, "ZeroB", "MultiB");
    assert_eq!(
        p0.kind,
        VerdictKind::ProvenDisjoint,
        "size-complement vetoes are negation-free and must keep proving: {}",
        p0.reason
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

/// Vacuous-reducer hard-presence (CRITICAL, 2026-08-01, found during the
/// Phase B landing review): `negate` conjoined `p_MET >= 1` onto the
/// over-side of `reject any(pT(jets) + MET > 50)` — but a reducer over an
/// EMPTY collection decides vacuously without ever reading its body, so the
/// empty-jets/no-MET event IS in the region while the encoded superset
/// excluded it. Combined with NNEG(MET), that shipped a false PROVEN SUBSET
/// against `select MET >= 0` (the interpreter errors that region on the
/// same event). Fix: `hard_quantities` must not descend into `Dual` — a
/// bounded-expansion hedge marks CONDITIONAL evaluation, and inside it the
/// positive encoding's own presence literal negates by plain De Morgan to
/// the sound `p < 1 ∨ ¬atom`. This is the shape the difftest oracle
/// plausibly tripped on in the un-persisted failing run.
#[test]
fn vacuous_reducer_must_not_claim_hard_presence_under_negation() {
    let src = "\
object jets
  take Jet
region A
  reject any(pT(jets) + MET > 50)
region B
  select MET >= 0
";
    let ext = ExtDecls::legacy();
    let r = analyze_source(src, "vacred.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        eprintln!("SKIP: no solver (encoding-side pin below still meaningful via NoSolver)");
    } else {
        let p = find_pair(&r.pairwise, "A", "B");
        assert!(
            !p.subset_a_in_b,
            "the empty-jets/no-MET event is in A but errors in B — A ⊄ B: {}",
            p.reason
        );
        assert_eq!(
            r.sampling.as_ref().map(|s| s.refutations),
            Some(0),
            "prevention, not gate reliance"
        );
    }
    // Solver-free arm: the claim must be underivable via intervals too.
    let r0 = analyze_source(src, "vacred.adl", &ext, &opts(SolverChoice::NoSolver))
        .expect("resolves");
    let p0 = find_pair(&r0.pairwise, "A", "B");
    assert!(!p0.subset_a_in_b, "{}", p0.reason);
}

/// The other two shapes of the same class, found while fixing the one above
/// (2026-08-01). `In(¬f)` requires a hard-absent datum only when EVERY route
/// to `f` being decidably FALSE evaluates it. Three routes do not, and the
/// first version of `Encoder::negate` conjoined presence on the over side
/// regardless — excluding a genuine member, which fabricates a subset rather
/// than losing a proof:
///
/// - a `Dual` (the vacuous reducer, pinned above);
/// - an `And`, where one decidably-false conjunct settles it while a sibling
///   stays Unknown;
/// - an atom that ALSO mentions a soft-absent quantity, whose soft non-value
///   beats the blocking hard error in the same comparison.
///
/// A fix that instead drops the conjunct from BOTH projections is not
/// sufficient: the under side would then admit the `p < 1` disjunct De
/// Morgan produced, claiming membership on an event the interpreter answers
/// Unknown on. Hence over-classical / under-guarded, and hence these pins.
#[test]
fn negation_over_a_conditional_hard_datum_must_not_claim_presence() {
    let ext = ExtDecls::legacy();
    // (name, RA, RB) — RA ⊄ RB must stay underivable in every row.
    let cases: &[(&str, &str, &str)] = &[
        (
            "and-absorbs-unknown",
            "  reject (MET > 1 and size(jets) > 99)",
            "  select MET >= 0",
        ),
        (
            "soft-beats-hard-in-one-cmp",
            "  reject BTag(jets[0]) - MET > 0",
            "  select MET >= 0",
        ),
        (
            "vacuous-reducer-as-subset-inner",
            "  select size(jets) >= 1",
            "  reject any(pT(jets) + MET > 50)",
        ),
    ];
    for (name, ra, rb) in cases {
        let src = format!("object jets\n  take Jet\nregion A\n{ra}\nregion B\n{rb}\n");
        let r = analyze_source(&src, "condhard.adl", &ext, &opts(SolverChoice::Auto))
            .unwrap_or_else(|e| panic!("{name} must resolve: {e}"));
        if r.solver == "none" {
            continue;
        }
        let p = find_pair(&r.pairwise, "A", "B");
        assert!(
            !p.subset_a_in_b,
            "{name}: A ⊄ B must be underivable — {}",
            p.reason
        );
        assert_eq!(
            r.sampling.as_ref().map(|s| s.refutations),
            Some(0),
            "{name}: prevention, not gate reliance"
        );
    }
}

/// …and the precision the fix must NOT cost: a `reject` whose scope is a
/// bare comparison (or a disjunction of them) over event-level data still
/// pins the datum present on BOTH projections, so the bound stays on the
/// And-spine where the interval layer reads it and the pair still proves.
#[test]
fn reject_of_a_bare_event_scalar_cut_keeps_its_spine_bound() {
    let ext = ExtDecls::legacy();
    let src = "\
object jets
  take Jet
region A
  reject MET > 100
region B
  select MET > 200
";
    let r = analyze_source(src, "spine.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        return;
    }
    let p = find_pair(&r.pairwise, "A", "B");
    assert_eq!(p.kind, VerdictKind::ProvenDisjoint, "{}", p.reason);
}

/// P1/P2 — the two adversarial probes from the landing review, pinned
/// against the KLEENE layer rather than `smash2 run`.
///
/// Both were first checked with `smash2 run`, which reported ERROR for each
/// — but that is the TWO-VALUED path, and the soundness contract is defined
/// over `region3` (proof §1). Under `region3` both regions are genuinely
/// `In` on their event, which is what makes them the exact vacuity shapes
/// they were designed to probe:
///
/// - **P1** (`Or` over an `And` that absorbs the Unknown): the `And` is
///   decidably FALSE via `size(jets) > 99`, the other disjunct is FALSE, so
///   the `Or` is FALSE and the `reject` HOLDS — with MET never read.
/// - **P2** (cancellation): `pT(jets[0])` is a MISSING ELEMENT on the empty
///   event, a soft non-value, and `eval.rs`'s `Cmp` arm tests `Ok(Err(_))`
///   before `Err(_)` — so the comparison is a decidable FALSE and the
///   `reject` HOLDS, again with MET never read. `pt` is cancelled out of the
///   atom's `terms()`, so only the definedness footprint keeps it visible.
///
/// Membership being `In` is exactly what makes `subset A within B` false
/// (B errors on the same event), so both assertions below are load-bearing:
/// the region3 pin says the counterexample is real, the subset pin says the
/// engine does not claim otherwise.
#[test]
fn p1_p2_vacuity_probes_are_members_under_kleene_and_derive_no_subset() {
    use adl_interp::{Interp, parse_event};
    let ext = ExtDecls::legacy();
    // (name, RA body, event JSON)
    let cases: &[(&str, &str, &str)] = &[
        (
            "P1-or-over-absorbing-and",
            "  reject ((MET > 1 and size(jets) > 99) or pT(jets[0]) > 5)",
            r#"{"Jet":[{"pt":1.0,"eta":0.0,"phi":0.0,"m":0.0,"btag":0.0,"ctag":0.0}],"Electron":[]}"#,
        ),
        (
            "P2-cancelled-soft-operand",
            "  reject (MET + pT(jets[0]) - pT(jets[0]) > 5)",
            r#"{"Jet":[],"Electron":[]}"#,
        ),
    ];
    for (name, ra, event_json) in cases {
        let src = format!("object jets\n  take Jet\nregion A\n{ra}\nregion B\n  select MET >= 0\n");
        let hir = analyze_str(&src, "probe.adl", &ext);
        let interp = Interp::new(&hir, &ext);
        let e = parse_event(event_json, &ext).expect("loader-valid");

        // region3: A is In (the datum is never read), B is Unknown.
        assert_eq!(
            interp.eval_region_membership("A", &e).ok(),
            Some(true),
            "{name}: A must be In under the Kleene layer — that is what makes \
             it a counterexample"
        );
        assert_ne!(
            interp.eval_region_membership("B", &e).ok(),
            Some(true),
            "{name}: B reads MET, which the event lacks, so B is not In"
        );

        // …so the engine must not claim A ⊆ B.
        let r = analyze_source(&src, "probe.adl", &ext, &opts(SolverChoice::Auto))
            .unwrap_or_else(|err| panic!("{name} must resolve: {err}"));
        if r.solver == "none" {
            continue;
        }
        let p = find_pair(&r.pairwise, "A", "B");
        assert!(!p.subset_a_in_b, "{name}: {}", p.reason);
        assert_eq!(
            r.sampling.as_ref().map(|s| s.refutations),
            Some(0),
            "{name}: prevention, not gate reliance"
        );
    }
}

/// K13 (2026-08-01) — the `And`-INTERSECTION case, and the one that showed
/// the rule had to be set-valued rather than a boolean "does this shape
/// qualify".
///
/// ```text
/// A: reject (MET > 1 and HT > 2)
/// B: select HT >= 0 or HT < 0
/// ```
///
/// An `and` is decidably FALSE as soon as ONE member is, so it forces only
/// the quantities read on EVERY false-route — the INTERSECTION over its
/// members. Here that is empty: a present `MET = 1` settles the conjunction
/// without `HT` ever being read. The predecessor rule qualified the whole
/// `And` because both members were pure-hard atoms, and `negate` then
/// conjoined the UNION (`p_MET ≥ 1 ∧ p_HT ≥ 1`) onto A's over side —
/// excluding the very event that witnesses `A ⊄ B` and shipping the subset.
///
/// This is K12 with the sides swapped, which is why a shape-based rule kept
/// missing one of them.
#[test]
fn k13_and_forces_only_the_intersection_of_its_members() {
    use adl_interp::{Interp, parse_event};
    let ext = ExtDecls::legacy();
    let src = "\
object jets
  take Jet
region A
  reject (MET > 1 and HT > 2)
region B
  select HT >= 0 or HT < 0
";
    // MET present at exactly 1 (so `MET > 1` is decidably FALSE), no HT key.
    let event_json = r#"{"Jet":[{"pt":50.0,"eta":0.0,"phi":0.0,"m":0.0,"btag":0.0,"ctag":0.0}],"Electron":[],"MET":{"pt":1.0,"phi":0.0}}"#;
    let hir = analyze_str(src, "k13.adl", &ext);
    let interp = Interp::new(&hir, &ext);
    let e = parse_event(event_json, &ext).expect("loader-valid");
    assert_eq!(
        interp.eval_region_membership("A", &e).ok(),
        Some(true),
        "A is In: `MET > 1` is false, and false absorbs the hard error on `HT > 2`"
    );
    assert_ne!(
        interp.eval_region_membership("B", &e).ok(),
        Some(true),
        "B reads HT in both disjuncts, so B is not In"
    );

    let r = analyze_source(src, "k13.adl", &ext, &opts(SolverChoice::Auto)).expect("resolves");
    if r.solver == "none" {
        return;
    }
    let p = find_pair(&r.pairwise, "A", "B");
    assert!(!p.subset_a_in_b, "A ⊄ B must be underivable: {}", p.reason);
    assert!(!p.subset_b_in_a, "{}", p.reason);
    assert_eq!(
        r.sampling.as_ref().map(|s| s.refutations),
        Some(0),
        "prevention, not gate reliance"
    );

    // The SELECT-side mirror must be the same region: the negation
    // placement peepholes make `select not X` and `reject X` encode
    // identically, so a presence split that depended on spelling would be a
    // bug of its own.
    let mirrored = src.replace(
        "  reject (MET > 1 and HT > 2)",
        "  select not (MET > 1 and HT > 2)",
    );
    let rm = analyze_source(&mirrored, "k13.adl", &ext, &opts(SolverChoice::Auto))
        .expect("resolves");
    let pm = find_pair(&rm.pairwise, "A", "B");
    assert_eq!(
        (pm.kind, pm.subset_a_in_b, pm.subset_b_in_a),
        (p.kind, p.subset_a_in_b, p.subset_b_in_a),
        "`select not X` must behave identically to `reject X`"
    );
}
