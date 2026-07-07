//! P1 part B: sort / slice / composite (disjoint & cartesian) interpreter
//! semantics. These are interpret-only constructs; the analyzer keeps them
//! Unknown (verified separately by the corpus sweep staying green). Here we
//! pin the concrete event semantics: a descending-pt sort is the identity on
//! pt-ordered input, a slice rebases indices, a disjoint composite dedups by
//! kinematic value, and a cartesian composite counts the full product.

use adl_interp::{Event, EventObject, Interp};
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

fn passes(adl: &str, region: &str, json: &str) -> bool {
    let h = hir(adl);
    Interp::new(&h, ext())
        .eval_region_by_name(region, &event(json))
        .expect("region must evaluate")
}

fn pts(objs: &[EventObject]) -> Vec<f64> {
    // Properties are stored under the canonical key (`pt` → `ptof`).
    let key = ext().prop_canon("pt").0;
    objs.iter().map(|o| o.get(&key).unwrap()).collect()
}

/// Electrons, no cut. Events below keep them pT-descending (loader requires it).
const ELES: &str = "object eles\n  take Electron\n";

/// Three pt-descending electrons.
const EV3E: &str = r#"{"Electron":[
  {"pt":100,"eta":0.5,"phi":0.0,"mass":0.0},
  {"pt":60,"eta":-0.5,"phi":1.0,"mass":0.0},
  {"pt":30,"eta":0.2,"phi":2.0,"mass":0.0}
]}"#;

// ---- sort --------------------------------------------------------------

#[test]
fn sort_pt_descend_is_identity_on_ordered_input() {
    // `sort(eles, pt(eles), descend)` over a pt-descending source is the
    // identity permutation: sorted[i] ≡ src[i] for every index.
    let adl = format!("{ELES}object s\n  take sort(eles, pt(eles), descend)\n");
    let h = hir(&adl);
    let it = Interp::new(&h, ext());
    let ev = event(EV3E);
    let src = it.collection("eles", &ev).unwrap();
    let sorted = it.collection("s", &ev).unwrap();
    assert_eq!(pts(&src), pts(&sorted));
    assert_eq!(pts(&sorted), vec![100.0, 60.0, 30.0]);
}

#[test]
fn sort_pt_ascend_reverses_a_descending_source() {
    let adl = format!("{ELES}object s\n  take sort(eles, pt(eles), ascend)\n");
    let h = hir(&adl);
    let it = Interp::new(&h, ext());
    let ev = event(EV3E);
    let sorted = it.collection("s", &ev).unwrap();
    assert_eq!(pts(&sorted), vec![30.0, 60.0, 100.0]);
}

#[test]
fn sort_indexed_pt_in_region() {
    // pt(s[0]) is the leading electron (100); s[2] the trailing (30).
    let adl = format!("{ELES}object s\n  take sort(eles, pt(eles), descend)\nregion R\n  select pt(s[0]) == 100\n  select pt(s[2]) == 30\n");
    assert!(passes(&adl, "R", EV3E));
}

// ---- slice -------------------------------------------------------------

#[test]
fn slice_rebases_indices() {
    // `eles[1:]` drops the first; element i ≡ src[1+i].
    let adl = format!("{ELES}object tail\n  take eles[1:]\n");
    let h = hir(&adl);
    let it = Interp::new(&h, ext());
    let ev = event(EV3E);
    let tail = it.collection("tail", &ev).unwrap();
    assert_eq!(pts(&tail), vec![60.0, 30.0]);
}

#[test]
fn slice_prefix_clamps() {
    // `eles[:2]` keeps the first two; `eles[:10]` clamps to the whole list.
    let adl = format!("{ELES}object pre\n  take eles[:2]\nobject big\n  take eles[:10]\n");
    let h = hir(&adl);
    let it = Interp::new(&h, ext());
    let ev = event(EV3E);
    assert_eq!(pts(&it.collection("pre", &ev).unwrap()), vec![100.0, 60.0]);
    assert_eq!(
        pts(&it.collection("big", &ev).unwrap()),
        vec![100.0, 60.0, 30.0]
    );
}

#[test]
fn slice_inside_reducer() {
    // `min(pt(eles[:2]))` iterates the slice {100,60} ⇒ min 60.
    let adl = format!("{ELES}region R\n  select min(pt(eles[:2])) > 50\n");
    assert!(passes(&adl, "R", EV3E));
    let adl2 = format!("{ELES}region R\n  select min(pt(eles[:2])) > 80\n");
    assert!(!passes(&adl2, "R", EV3E)); // min is 60, not > 80
}

// ---- composite: disjoint -----------------------------------------------

/// A composite dilepton over a same-source disjoint pairing, with a candidate
/// 4-vector and a mass cut.
fn dilepton_adl(mass_cut: &str) -> String {
    format!(
        "{ELES}composite dilepton\n  take disjoint(eles l1, eles l2)\n  candidate ll = l1 + l2\n  select mass(ll) > {mass_cut}\n"
    )
}

#[test]
fn disjoint_pair_count_is_n_choose_2() {
    // Three distinct electrons ⇒ C(3,2) = 3 unordered value-distinct pairs.
    let adl = format!("{}region R\n  select size(dilepton->ll) == 3\n", dilepton_adl("0"));
    assert!(passes(&adl, "R", EV3E));
}

#[test]
fn disjoint_dedups_value_equal_members() {
    // Two electrons share identical (pt,eta,phi,mass): the (a,a)-value pair is
    // dropped, so only the one cross pair with the third remains... here both
    // duplicates pair with the third (2 pairs) but NOT with each other (0).
    let ev = r#"{"Electron":[
      {"pt":50,"eta":0.0,"phi":0.0,"mass":0.0},
      {"pt":50,"eta":0.0,"phi":0.0,"mass":0.0},
      {"pt":30,"eta":1.0,"phi":1.0,"mass":0.0}
    ]}"#;
    // Pairs: (0,1) value-equal ⇒ dropped; (0,2),(1,2) kept ⇒ 2.
    let adl = format!("{}region R\n  select size(dilepton->ll) == 2\n", dilepton_adl("0"));
    assert!(passes(&adl, "R", ev));
    // The same value-equal members never form a self-pair: with only the two
    // duplicates present, zero pairs survive.
    let ev2 = r#"{"Electron":[
      {"pt":50,"eta":0.0,"phi":0.0,"mass":0.0},
      {"pt":50,"eta":0.0,"phi":0.0,"mass":0.0}
    ]}"#;
    let adl2 = format!("{}region R\n  select size(dilepton->ll) == 0\n", dilepton_adl("0"));
    assert!(passes(&adl2, "R", ev2));
}

#[test]
fn disjoint_candidate_mass_window() {
    // Two back-to-back electrons pt=45, eta=0, opposite phi ⇒ invariant mass 90.
    let ev = r#"{"Electron":[
      {"pt":45,"eta":0.0,"phi":0.0,"mass":0.0},
      {"pt":45,"eta":0.0,"phi":3.141592653589793,"mass":0.0}
    ]}"#;
    let adl = format!(
        "{ELES}composite dilepton\n  take disjoint(eles l1, eles l2)\n  candidate ll = l1 + l2\nregion R\n  select size(dilepton->ll) == 1\n  select 80 < mass(dilepton->ll[0]) < 100\n"
    );
    assert!(passes(&adl, "R", ev));
}

#[test]
fn disjoint_per_tuple_cut_filters_candidates() {
    // C(3,2)=3 pairs, but the per-tuple `mass(ll) > 1000` drops them all.
    let adl = format!("{}region R\n  select size(dilepton->ll) == 0\n", dilepton_adl("1000"));
    assert!(passes(&adl, "R", EV3E));
}

// ---- composite: cartesian ----------------------------------------------

#[test]
fn cartesian_counts_full_cross_product() {
    // `cartesian(eles a, jets b)`: |eles| * |jets| ordered tuples.
    let adl = "object eles\n  take Electron\nobject jets\n  take Jet\ncomposite mix\n  take cartesian(eles a, jets b)\nregion R\n  select size(mix->a) == 6\n";
    let ev = r#"{"Electron":[
      {"pt":100,"eta":0.0,"phi":0.0,"mass":0.0},
      {"pt":50,"eta":0.0,"phi":0.5,"mass":0.0}
    ],"Jet":[
      {"pt":120,"eta":0.0,"phi":1.0,"mass":5.0},
      {"pt":80,"eta":0.0,"phi":1.5,"mass":5.0},
      {"pt":40,"eta":0.0,"phi":2.0,"mass":5.0}
    ]}"#;
    assert!(passes(adl, "R", ev)); // 2 * 3 = 6
}

#[test]
fn member_axis_projects_binder() {
    // `mix->a[0].pt` reads the first tuple's `a` member.
    let adl = "object eles\n  take Electron\nobject jets\n  take Jet\ncomposite mix\n  take cartesian(eles a, jets b)\nregion R\n  select pt(mix->a[0]) == 100\n";
    let ev = r#"{"Electron":[{"pt":100,"eta":0.0,"phi":0.0,"mass":0.0}],"Jet":[{"pt":120,"eta":0.0,"phi":1.0,"mass":5.0}]}"#;
    assert!(passes(adl, "R", ev));
}
