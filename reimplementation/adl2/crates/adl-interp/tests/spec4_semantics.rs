//! SPEC_LANGUAGE §4 clause battery (PLAN Phase-3 exit criterion: every
//! §4 clause has a unit test). Tests are grouped by spec subsection and
//! each names the clause it locks.

use adl_interp::{BinOutcome, Event, Interp, NumOutcome, assign_bin};
use adl_sema::{ExtDecls, Hir, analyze_str};
use std::sync::OnceLock;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

fn hir(src: &str) -> Hir {
    let h = analyze_str(src, "test.adl", ext());
    assert!(
        !adl_syntax::diag::has_errors(&h.diags),
        "unexpected sema/parse errors: {:#?}",
        h.diags
    );
    h
}

fn event(json: &str) -> Event {
    adl_interp::parse_event(json, ext()).expect("test event must parse")
}

/// Membership of `region` over `json`, for the given ADL source.
fn passes(adl: &str, region: &str, json: &str) -> bool {
    let h = hir(adl);
    Interp::new(&h, ext())
        .eval_region_by_name(region, &event(json))
        .expect("region must evaluate")
}

/// The diagnosed evaluation-error reason for `region` over `json`.
fn region_err(adl: &str, region: &str, json: &str) -> String {
    let h = hir(adl);
    Interp::new(&h, ext())
        .eval_region_by_name(region, &event(json))
        .expect_err("region evaluation must fail")
        .reason
}

/// pT values of a named collection materialized over `json`.
fn coll_pts(adl: &str, name: &str, json: &str) -> Vec<f64> {
    let h = hir(adl);
    let pt_key = ext().prop_canon("pt").0;
    Interp::new(&h, ext())
        .collection(name, &event(json))
        .expect("collection must materialize")
        .iter()
        .map(|o| o.get(&pt_key).expect("pt present"))
        .collect()
}

/// The standard event used across the battery: four jets (pT-descending),
/// two electrons, one muon, MET, HT and two trigger flags.
const STD: &str = r#"{
  "Jet": [
    {"pt": 100, "eta":  1.0, "phi":  0.5, "m": 10, "btag": 1},
    {"pt":  50, "eta": -2.0, "phi": -2.5, "m":  8, "btag": 0},
    {"pt":  40, "eta":  0.3, "phi":  1.0, "m":  5, "btag": 1},
    {"pt":  20, "eta":  3.0, "phi":  3.0, "m":  3, "btag": 0}
  ],
  "Electron": [
    {"pt": 60, "eta":  0.1, "phi": 0.2, "charge": -1},
    {"pt": 25, "eta": -1.0, "phi": 2.0, "charge":  1}
  ],
  "Muon": [{"pt": 45, "eta": 0.5, "phi": -1.0, "charge": 1}],
  "MET": {"pt": 80, "phi": 0.4},
  "HT": 210,
  "triggers": {"mu_trig": 1, "el_trig": 0}
}"#;

// =========================================================================
// §4.1 Event model
// =========================================================================

/// §4.1: per-collection ordered object lists with real-valued properties.
#[test]
fn event_collections_are_ordered_object_lists() {
    let ev = event(STD);
    assert_eq!(ev.collections["jet"].len(), 4);
    assert_eq!(ev.collections["electron"].len(), 2);
    let pt = ext().prop_canon("pt").0;
    let jets: Vec<f64> = ev.collections["jet"]
        .iter()
        .map(|o| o.get(&pt).unwrap())
        .collect();
    assert_eq!(jets, vec![100.0, 50.0, 40.0, 20.0]);
}

/// §4.1: event scalars — MET vector → MET.pt / MET.phi, scalar HT.
#[test]
fn event_scalars_met_pt_phi_and_ht() {
    let adl = "region r\n  select MET == 80\n  select MET.pt == 80\n  select MET.phi [] 0.39 0.41\n  select HT == 210\n";
    assert!(passes(adl, "r", STD));
}

/// §4.1: trigger flags are part of the event and live in {0,1}.
#[test]
fn event_trigger_flags_are_zero_or_one() {
    let ev = event(STD);
    assert_eq!(ev.triggers["mu_trig"], 1.0);
    assert_eq!(ev.triggers["el_trig"], 0.0);
    let bad = adl_interp::parse_event(r#"{"triggers": {"x": 0.5}}"#, ext());
    assert!(matches!(
        bad,
        Err(adl_interp::EventError::BadTriggerFlag { .. })
    ));
}

/// §4.1 + PHASE0: collections are pT-descending; the loader validates
/// and refuses unordered input — re-sort is OFF.
#[test]
fn pt_descending_is_validated_never_resorted() {
    let bad = adl_interp::parse_event(r#"{"Jet": [{"pt": 10}, {"pt": 50}]}"#, ext());
    assert!(matches!(
        bad,
        Err(adl_interp::EventError::NotPtDescending { index: 1, .. })
    ));
    // Equal pTs are still non-increasing: fine.
    assert!(adl_interp::parse_event(r#"{"Jet": [{"pt": 50}, {"pt": 50}]}"#, ext()).is_ok());
}

/// §4.1 + PHASE0 OPEN-3: indices are 0-based; `C_n` is the same index
/// operator as `C[n]`.
#[test]
fn indices_are_zero_based() {
    assert!(passes(
        "region r\n  select Jet[0].pt == 100\n  select Jet[1].pt == 50\n  select Jet_1.pt == 50\n",
        "r",
        STD
    ));
}

/// §4.1 (JSONL form): one event per non-blank line.
#[test]
fn read_jsonl_one_event_per_line() {
    let text = "{\"HT\": 10}\n\n{\"HT\": 20}\n";
    let evs = adl_interp::read_jsonl(text, ext()).unwrap();
    assert_eq!(evs.len(), 2);
    assert_eq!(evs[0].scalars["ht"], 10.0);
    assert_eq!(evs[1].scalars["ht"], 20.0);
}

/// PHASE0 case rule: resolution is case-insensitive — for event keys too.
#[test]
fn event_keys_resolve_case_insensitively() {
    let json = r#"{"JET": [{"pt": 90}], "met": 33, "ht": 5}"#;
    let adl = "region r\n  select Jet.size == 1\n  select MET == 33\n";
    assert!(passes(adl, "r", json));
    // Region lookup is case-insensitive as well.
    let h = hir(adl);
    assert!(
        Interp::new(&h, ext())
            .eval_region_by_name("R", &event(json))
            .unwrap()
    );
}

/// §4.1: an absent collection is an empty one.
#[test]
fn missing_collection_is_empty() {
    assert!(passes(
        "region r\n  select Jet.size == 0\n",
        "r",
        r#"{"HT": 10}"#
    ));
}

/// Loader hygiene: properties that collide after canonicalization are a
/// data error, not a silent overwrite.
#[test]
fn duplicate_canonical_property_is_rejected() {
    let bad = adl_interp::parse_event(r#"{"Jet": [{"pt": 10, "pT": 11}]}"#, ext());
    assert!(matches!(bad, Err(adl_interp::EventError::Shape { .. })));
}

// =========================================================================
// §4.2 Objects
// =========================================================================

const FILTER_ADL: &str = "object goodjets\n  take Jet\n  select pt > 30\n";

/// §4.2: `object D take S <cuts>` = elements of S passing all cuts,
/// order preserved; cuts are per-element with the element as the
/// implicit subject.
#[test]
fn object_filtering_preserves_order() {
    assert_eq!(
        coll_pts(FILTER_ADL, "goodjets", STD),
        vec![100.0, 50.0, 40.0]
    );
}

/// §4.2: order is preserved even when survivors are non-contiguous.
#[test]
fn object_filtering_keeps_noncontiguous_order() {
    let adl = "object centraljets\n  take Jet\n  select |eta| < 1.5\n";
    assert_eq!(coll_pts(adl, "centraljets", STD), vec![100.0, 40.0]);
}

/// §4.2: multiple cuts conjoin; `reject` inside an object block negates
/// its per-element predicate.
#[test]
fn object_cuts_conjoin_and_reject_negates() {
    let adl = "object sel\n  take Jet\n  select pt > 30\n  reject btag == 1\n";
    assert_eq!(coll_pts(adl, "sel", STD), vec![50.0]);
}

/// §4.2: `take union(A,B)` concatenates in order (no re-sort, no dedup).
#[test]
fn union_concatenates_in_order() {
    let adl = "object leps\n  take union(Electron, Muon)\n";
    assert_eq!(coll_pts(adl, "leps", STD), vec![60.0, 25.0, 45.0]);
    assert!(passes(
        &format!("{adl}region r\n  select leps.size == 3\n"),
        "r",
        STD
    ));
}

/// §4.2: an object block with a single take and no cuts is a pure
/// rename — identical to its source by construction.
#[test]
fn pure_rename_is_identity_with_source() {
    let adl = "object myjets\n  take Jet\n";
    assert_eq!(coll_pts(adl, "myjets", STD), coll_pts(adl, "Jet", STD));
    assert!(passes(
        &format!("{adl}region r\n  select myjets.size == 4\n  select myjets[0].pt == 100\n"),
        "r",
        STD
    ));
}

/// §4.2: filtering composes — a filtered collection can itself be a
/// take source.
#[test]
fn filtered_of_filtered_composes() {
    let adl = "object hard\n  take Jet\n  select pt > 30\nobject tagged\n  take hard\n  select btag == 1\n";
    assert_eq!(coll_pts(adl, "tagged", STD), vec![100.0, 40.0]);
}

// =========================================================================
// §4.3 Regions
// =========================================================================

/// §4.3: a region is the conjunction, in order, of its statements.
#[test]
fn region_is_conjunction_of_statements() {
    let adl = "region r\n  select HT > 100\n  select MET > 50\n";
    assert!(passes(adl, "r", STD));
    assert!(!passes(adl, "r", r#"{"HT": 210, "MET": 10}"#));
    assert!(!passes(adl, "r", r#"{"HT": 50, "MET": 80}"#));
}

/// §4.3: `select c` contributes `c`; `reject c` contributes `¬c`.
#[test]
fn select_contributes_c_reject_contributes_not_c() {
    assert!(passes("region r\n  select MET > 50\n", "r", STD));
    assert!(!passes("region r\n  reject MET > 50\n", "r", STD));
    assert!(passes("region r\n  reject MET > 200\n", "r", STD));
}

/// §4.3: a bare region name inlines that region's predicate
/// (inheritance) — equivalent to textually pasting it.
#[test]
fn inheritance_inlines_prior_region_predicate() {
    let adl = "region base\n  select HT > 100\nregion child\n  base\n  select MET > 50\nregion pasted\n  select HT > 100\n  select MET > 50\n";
    for json in [
        STD,
        r#"{"HT": 50, "MET": 80}"#,
        r#"{"HT": 210, "MET": 10}"#,
        r#"{"HT": 99, "MET": 10}"#,
    ] {
        assert_eq!(
            passes(adl, "child", json),
            passes(adl, "pasted", json),
            "inherit ≢ paste on {json}"
        );
    }
    assert!(passes(adl, "child", STD));
    assert!(!passes(adl, "child", r#"{"HT": 50, "MET": 80}"#));
}

/// §4.3: `trigger t` contributes the trigger flag.
#[test]
fn trigger_contributes_the_flag() {
    let adl = "region r\n  trigger mu_trig\n";
    assert!(passes(adl, "r", STD));
    assert!(!passes("region r\n  trigger el_trig\n", "r", STD));
    // A missing flag is a data mismatch, not physics: diagnosed error.
    let err = region_err("region r\n  trigger zz_trig\n", "r", STD);
    assert!(err.contains("trigger"), "got: {err}");
}

/// §4.3: `weight`/`histo`/`save` contribute nothing to membership.
#[test]
fn weight_histo_save_do_not_affect_membership() {
    let plain = "region r\n  select MET > 50\n";
    let decorated = "region r\n  weight w 0.5\n  select MET > 50\n  histo hmet , \"met\" , 10 , 0 , 200 , MET\n  save snap csv MET\n";
    for json in [STD, r#"{"MET": 10}"#] {
        assert_eq!(passes(plain, "r", json), passes(decorated, "r", json));
    }
}

/// §4.3: `bin` statements partition the region's events and do NOT
/// constrain membership — even when the value falls below every bin.
#[test]
fn bins_do_not_constrain_membership() {
    let adl = "region r\n  select MET > 0\n  bin HT 100 200 300\n";
    assert!(passes(adl, "r", r#"{"MET": 5, "HT": 50}"#)); // HT under b0
}

/// §4.3: boundary-list `bin v b0 … bn` denotes `[b0,b1), …, [bn,∞)` —
/// half-open bins, open last bin (checked end to end via run_event).
#[test]
fn boundary_bin_assignment_end_to_end() {
    let adl = "region r\n  select MET > 0\n  bin HT 100 200 300\n";
    let h = hir(adl);
    let it = Interp::new(&h, ext());
    let bin_of = |ht: f64| {
        let results = it.run_event(&event(&format!(r#"{{"MET": 5, "HT": {ht}}}"#)));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].pass, Ok(true));
        match &results[0].bins[..] {
            [BinOutcome::Boundary { bin, .. }] => *bin,
            other => panic!("expected one boundary bin, got {other:?}"),
        }
    };
    assert_eq!(bin_of(50.0), None); // below b0: no bin
    assert_eq!(bin_of(100.0), Some(0)); // left edge closed
    assert_eq!(bin_of(199.875), Some(0)); // just under b1
    assert_eq!(bin_of(200.0), Some(1)); // right edge open ⇒ next bin
    assert_eq!(bin_of(300.0), Some(2)); // last edge starts the open bin
    assert_eq!(bin_of(987654.0), Some(2)); // last bin is [bn, ∞)
}

/// §4.3 (divergence 5): bin boundaries are reals — fractional edges work.
#[test]
fn boundary_bin_fractional_edges() {
    assert_eq!(assign_bin(100.5, &[100.5, 200.25]), Some(0));
    assert_eq!(assign_bin(200.25, &[100.5, 200.25]), Some(1));
    assert_eq!(assign_bin(100.4, &[100.5, 200.25]), None);
}

/// §4.3: boundary-bin edge cases as a pure-function battery.
#[test]
fn assign_bin_edge_cases() {
    let edges = [100.0, 200.0, 300.0];
    assert_eq!(assign_bin(99.999, &edges), None);
    assert_eq!(assign_bin(100.0, &edges), Some(0));
    assert_eq!(assign_bin(150.0, &edges), Some(0));
    assert_eq!(assign_bin(200.0, &edges), Some(1));
    assert_eq!(assign_bin(299.999, &edges), Some(1));
    assert_eq!(assign_bin(300.0, &edges), Some(2));
    assert_eq!(assign_bin(1.0e15, &edges), Some(2));
    // Negative edges behave identically.
    assert_eq!(assign_bin(-150.0, &[-200.0, -100.0]), Some(0));
    assert_eq!(assign_bin(-100.0, &[-200.0, -100.0]), Some(1));
    assert_eq!(assign_bin(-201.0, &[-200.0, -100.0]), None);
    // Non-finite values are never binned.
    assert_eq!(assign_bin(f64::NAN, &edges), None);
    assert_eq!(assign_bin(f64::INFINITY, &edges), None);
}

/// §4.3: boolean bins record membership of the condition and leave
/// region membership untouched.
#[test]
fn boolean_bin_membership() {
    let adl = "region r\n  select MET > 0\n  bin HT > 250\n";
    let h = hir(adl);
    let it = Interp::new(&h, ext());
    let run = |json: &str| it.run_event(&event(json));
    let low = run(r#"{"MET": 5, "HT": 100}"#);
    assert_eq!(low[0].pass, Ok(true));
    assert_eq!(
        low[0].bins,
        vec![BinOutcome::Cond {
            label: None,
            member: false
        }]
    );
    let high = run(r#"{"MET": 5, "HT": 400}"#);
    assert_eq!(
        high[0].bins,
        vec![BinOutcome::Cond {
            label: None,
            member: true
        }]
    );
}

/// §4.3 + §3: a bare name resolving to a boolean define is sugar for
/// `select <define>`.
#[test]
fn bare_boolean_define_is_select_sugar() {
    let adl = "define hard : HT > 100\nregion r\n  hard\nregion rs\n  select hard\n";
    assert!(passes(adl, "r", STD));
    assert!(passes(adl, "rs", STD));
    assert!(!passes(adl, "r", r#"{"HT": 50}"#));
}

/// §5: out-of-fragment region statements (`sort`) raise a diagnosed
/// evaluation error — never a silent no-op.
#[test]
fn sort_statement_is_a_diagnosed_eval_error() {
    let err = region_err("region r\n  select HT > 100\n  sort Jet.pt\n", "r", STD);
    assert!(err.contains("sort"), "got: {err}");
}

// =========================================================================
// §4.4 Expressions
// =========================================================================

/// §4.4: ternary `g ? a : b` ≡ `(g∧a) ∨ (¬g∧b)` — full truth table
/// against the explicit formula.
#[test]
fn ternary_equals_its_boolean_expansion() {
    let tern = "region r\n  select HT > 100 ? MET > 50 : MET > 10\n";
    let formula = "region r\n  select (HT > 100 and MET > 50) or ((not (HT > 100)) and MET > 10)\n";
    let cases = [
        (r#"{"HT": 150, "MET": 60}"#, true),  // g ∧ a
        (r#"{"HT": 150, "MET": 20}"#, false), // g ∧ ¬a
        (r#"{"HT": 50, "MET": 30}"#, true),   // ¬g ∧ b
        (r#"{"HT": 50, "MET": 5}"#, false),   // ¬g ∧ ¬b
    ];
    for (json, expected) in cases {
        assert_eq!(passes(tern, "r", json), expected, "ternary on {json}");
        assert_eq!(passes(formula, "r", json), expected, "formula on {json}");
    }
}

/// §4.4: a missing else-branch is `true`.
#[test]
fn ternary_missing_else_is_true() {
    let adl = "region r\n  select HT > 100 ? MET > 50\n";
    assert!(passes(adl, "r", r#"{"HT": 50, "MET": 5}"#)); // guard false ⇒ true
    assert!(passes(adl, "r", STD));
    assert!(!passes(adl, "r", r#"{"HT": 150, "MET": 5}"#));
}

/// §4.4: an `ALL` branch is `true`.
#[test]
fn ternary_all_branch_is_true() {
    let adl = "region r\n  select HT > 100 ? MET > 50 : ALL\n";
    assert!(passes(adl, "r", r#"{"HT": 50, "MET": 5}"#));
    assert!(!passes(adl, "r", r#"{"HT": 150, "MET": 5}"#));
}

/// §4.4: `x [] lo hi` ≡ `lo ≤ x ≤ hi` (both edges inclusive).
#[test]
fn inclusive_band() {
    let adl = "region r\n  select MET [] 50 100\n";
    assert!(passes(adl, "r", r#"{"MET": 50}"#));
    assert!(passes(adl, "r", r#"{"MET": 100}"#));
    assert!(passes(adl, "r", r#"{"MET": 75}"#));
    assert!(!passes(adl, "r", r#"{"MET": 49.999}"#));
    assert!(!passes(adl, "r", r#"{"MET": 100.001}"#));
}

/// §4.4: `x ][ lo hi` ≡ `x ≤ lo ∨ x ≥ hi` (excluded band; edges in).
#[test]
fn excluded_band() {
    let adl = "region r\n  select MET ][ 50 100\n";
    assert!(passes(adl, "r", r#"{"MET": 50}"#));
    assert!(passes(adl, "r", r#"{"MET": 100}"#));
    assert!(passes(adl, "r", r#"{"MET": 30}"#));
    assert!(passes(adl, "r", r#"{"MET": 120}"#));
    assert!(!passes(adl, "r", r#"{"MET": 75}"#));
}

/// §4.4: bands accept negative bounds (unary minus, divergence 4).
#[test]
fn band_with_negative_bounds() {
    let adl = "region r\n  select Jet[1].eta [] -2.5 -1.5\n";
    assert!(passes(adl, "r", STD)); // eta = -2.0
}

/// §4.4: numeric defines are event scalars.
#[test]
fn numeric_define_is_an_event_scalar() {
    let adl = "define meff = HT + MET\nregion r\n  select meff == 290\n  select meff / 2 == 145\n";
    assert!(passes(adl, "r", STD));
}

/// §4.4: boolean defines are predicates, and references inline the body
/// — a named define and its textual inline are equivalent.
#[test]
fn define_inline_equals_textual_inline() {
    let named = "define lowmet : MET < 100\nregion r\n  select lowmet and HT > 100\n";
    let inline = "region r\n  select MET < 100 and HT > 100\n";
    for json in [
        STD,
        r#"{"MET": 150, "HT": 210}"#,
        r#"{"MET": 80, "HT": 50}"#,
    ] {
        assert_eq!(passes(named, "r", json), passes(inline, "r", json));
    }
}

/// §4.4: division by zero ⇒ the enclosing comparison is FALSE, in every
/// direction of the comparison.
#[test]
fn division_by_zero_makes_comparison_false() {
    let gt = "region r\n  select MET / (HT - HT) > 0\n";
    let lt = "region r\n  select MET / (HT - HT) < 999999\n";
    assert!(!passes(gt, "r", STD));
    assert!(!passes(lt, "r", STD));
    // 0/0 (NaN) is not even equal to itself.
    let nan = "region r\n  select (HT - HT) / (HT - HT) == (HT - HT) / (HT - HT)\n";
    assert!(!passes(nan, "r", STD));
}

/// §4.4: `reject` of a div-by-zero comparison passes (the comparison is
/// false; reject contributes its negation).
#[test]
fn reject_of_division_by_zero_passes() {
    assert!(passes("region r\n  reject MET / (HT - HT) > 0\n", "r", STD));
}

/// §4.4: non-finite arithmetic generally (not just division) fails the
/// enclosing comparison — e.g. overflow via `^`.
#[test]
fn nonfinite_arithmetic_fails_comparison() {
    assert!(!passes("region r\n  select HT ^ HT > 0\n", "r", STD)); // 210^210 = inf
    assert!(!passes("region r\n  select HT ^ HT < 999999\n", "r", STD));
}

/// §4.4 band with a non-finite subject: both band forms are false.
#[test]
fn nonfinite_value_fails_bands() {
    assert!(!passes(
        "region r\n  select MET / (HT - HT) [] 0 99\n",
        "r",
        STD
    ));
    assert!(!passes(
        "region r\n  select MET / (HT - HT) ][ 0 99\n",
        "r",
        STD
    ));
}

/// Guarded references: an out-of-range element makes the enclosing
/// comparison false (referencing `C[i]` does not imply `size > i`).
#[test]
fn out_of_range_element_fails_comparison() {
    assert!(!passes("region r\n  select Jet[9].pt > 0\n", "r", STD));
    assert!(!passes("region r\n  select Jet[9].pt <= 0\n", "r", STD));
    assert!(passes("region r\n  reject Jet[9].pt > 0\n", "r", STD));
}

/// A missing object property behaves like the guarded-reference case.
#[test]
fn missing_property_fails_comparison() {
    assert!(!passes(
        "region r\n  select Electron[0].btag == 0\n",
        "r",
        STD
    ));
    assert!(!passes(
        "region r\n  select Electron[0].btag == 1\n",
        "r",
        STD
    ));
}

/// §4.4 + PHASE0 OPEN-2: dPhi is oriented (order-sensitive, sign flips),
/// wrapped into [-π, π); dEta is oriented and signed; dR is unoriented.
#[test]
fn angular_separations() {
    let json = r#"{"Jet": [
        {"pt": 100, "eta": 1.5, "phi": 3.0},
        {"pt": 90, "eta": -0.5, "phi": -3.0}
    ]}"#;
    // φ difference 6.0 wraps to 6.0 − 2π ≈ −0.28319.
    assert!(passes(
        "region r\n  select dPhi(Jet[0], Jet[1]) [] -0.284 -0.283\n",
        "r",
        json
    ));
    assert!(passes(
        "region r\n  select dPhi(Jet[1], Jet[0]) [] 0.283 0.284\n",
        "r",
        json
    ));
    assert!(passes(
        "region r\n  select dEta(Jet[0], Jet[1]) == 2\n  select dEta(Jet[1], Jet[0]) == -2\n",
        "r",
        json
    ));
    // dR = √(2² + 0.28319²) ≈ 2.01996; symmetric by construction.
    assert!(passes(
        "region r\n  select dR(Jet[0], Jet[1]) [] 2.019 2.021\n  select dR(Jet[0], Jet[1]) == dR(Jet[1], Jet[0])\n",
        "r",
        json
    ));
}

/// dPhi against the MET vector uses the event MET φ.
#[test]
fn dphi_with_met_vector() {
    let json = r#"{"Jet": [{"pt": 100, "eta": 0.0, "phi": 1.0}], "MET": {"pt": 50, "phi": 0.25}}"#;
    assert!(passes(
        "region r\n  select dPhi(Jet[0], MET) [] 0.749 0.751\n",
        "r",
        json
    ));
}

/// `sqrt` is the one external function with a fixed reference
/// interpretation; a negative argument is NaN ⇒ comparison false.
#[test]
fn sqrt_external_function() {
    assert!(passes(
        "region r\n  select sqrt(HT) [] 14.49 14.5\n",
        "r",
        STD
    )); // √210 ≈ 14.4914
    assert!(!passes(
        "region r\n  select sqrt(Jet[1].eta) > -999999\n",
        "r",
        STD
    )); // √(−2) = NaN
}

/// Public numeric API exposes the soft/hard split.
#[test]
fn eval_num_exposes_soft_nonvalues() {
    let adl = "define ratio = MET / (HT - HT)\nregion r\n  select ratio > 0\n";
    let h = hir(adl);
    let it = Interp::new(&h, ext());
    let body = &h.define("ratio").unwrap().body;
    match it.eval_num(body, &event(STD)).unwrap() {
        NumOutcome::NonValue(adl_interp::NonValue::NonFinite) => {}
        other => panic!("expected NonFinite, got {other:?}"),
    }
}

// =========================================================================
// §4.5 / §5 Fragment honesty
// =========================================================================

/// §4.5 OPEN-1 (PHASE0): an unindexed collection cut at region level is
/// ambiguous — the interpreter refuses with a diagnosed error.
#[test]
fn unindexed_collection_cut_is_diagnosed() {
    for adl in [
        "region r\n  select pt(Jet) > 30\n",
        "region r\n  select Jet.pt > 30\n",
    ] {
        let err = region_err(adl, "r", STD);
        assert!(err.contains("OPEN-1"), "got: {err}");
    }
}

/// §5: an undeclared function is outside the fragment — diagnosed error.
#[test]
fn undeclared_function_is_diagnosed() {
    let err = region_err("region r\n  select aplanarity(Jet) > 0.1\n", "r", STD);
    assert!(err.contains("aplanarity"), "got: {err}");
}

/// §5: a declared-but-opaque external function evaluates to a diagnosed
/// error (the verifier's Unknown leaf tells the same story).
#[test]
fn declared_opaque_function_is_diagnosed() {
    let err = region_err("region r\n  select fMT2(Jet[0], Jet[1]) > 0\n", "r", STD);
    assert!(err.contains("no reference interpretation"), "got: {err}");
}

/// §4.1: missing event-level data (here: a referenced event scalar) is a
/// hard, diagnosed error — not a silent pass or fail.
#[test]
fn missing_event_scalar_is_diagnosed() {
    let err = region_err("region r\n  select HT > 100\n", "r", r#"{"MET": 5}"#);
    assert!(err.contains("scalar"), "got: {err}");
}

/// Determinism: two evaluations of the same file over the same events
/// produce identical results.
#[test]
fn evaluation_is_deterministic() {
    let adl = "object goodjets\n  take Jet\n  select pt > 30\nregion r\n  select goodjets.size >= 2\n  select MET [] 50 100\n  bin HT 100 200 300\n";
    let h1 = hir(adl);
    let h2 = hir(adl);
    let i1 = Interp::new(&h1, ext());
    let i2 = Interp::new(&h2, ext());
    let ev = event(STD);
    assert_eq!(i1.run_event(&ev), i2.run_event(&ev));
}
