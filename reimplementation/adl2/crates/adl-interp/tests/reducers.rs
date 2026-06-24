//! P1 part A: reducer (`any`/`all`/`sum`/`min`/`max`) and 4-vector-sum
//! interpreter semantics, plus the empty-collection conventions that are the
//! load-bearing metamorphic contract for the P2 encoder.

use adl_interp::{Event, Interp};
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

/// `jets` filtered to `pt > 20` over a small event.
const JETS: &str = "object jets\n  take Jet\n  select pt > 20\n";

/// Three jets passing the filter (pT 120/40/30) plus a softer one dropped.
const EV3: &str = r#"{"Jet":[
  {"pt":120,"eta":0.5,"phi":0.3,"mass":10},
  {"pt":40,"eta":-0.5,"phi":1.3,"mass":8},
  {"pt":30,"eta":0.1,"phi":2.3,"mass":5},
  {"pt":5,"eta":0.0,"phi":0.0,"mass":1}
],"MET":{"pt":80,"phi":-1.9}}"#;

/// Event with NO jet passing `pt > 20` (the empty-collection case).
const EV_EMPTY: &str = r#"{"Jet":[{"pt":5,"eta":0,"phi":0,"mass":1}],"MET":{"pt":80,"phi":0}}"#;

// ---- boolean reducers --------------------------------------------------

#[test]
fn any_already_boolean_body() {
    let adl = format!("{JETS}region R\n  select any(pt(jets) > 100)\n");
    assert!(passes(&adl, "R", EV3)); // jet0 pt=120 > 100
    let adl2 = format!("{JETS}region R\n  select any(pt(jets) > 200)\n");
    assert!(!passes(&adl2, "R", EV3));
}

#[test]
fn all_already_boolean_body() {
    let adl = format!("{JETS}region R\n  select all(pt(jets) > 5)\n");
    assert!(passes(&adl, "R", EV3));
    let adl2 = format!("{JETS}region R\n  select all(pt(jets) > 50)\n");
    assert!(!passes(&adl2, "R", EV3)); // jet1 pt=40 fails
}

#[test]
fn any_comparison_hoist_in_object_filter() {
    // `reject any(dR(this, electrons)) < 0.4`: the outer element is the jet,
    // the iteration collection is electrons. Jet0 (eta 0.5, phi 0.3) is within
    // dR 0.4 of the electron, so it is rejected; the other jets survive.
    let adl = "\
object electrons
  take Electron
  select pt > 10

object jets
  take Jet
  select pt > 20
  reject any(dR(this, electrons)) < 0.4

region R
  select size(jets) == 2
";
    let ev = r#"{"Electron":[{"pt":50,"eta":0.5,"phi":0.30,"mass":0.0005}],
                 "Jet":[
                   {"pt":120,"eta":0.51,"phi":0.31,"mass":10},
                   {"pt":40,"eta":-2.0,"phi":2.5,"mass":8},
                   {"pt":30,"eta":1.0,"phi":-1.0,"mass":5}
                 ],"MET":{"pt":80,"phi":0}}"#;
    assert!(passes(adl, "R", ev));
}

// ---- empty-collection conventions (metamorphic contract) ----------------

#[test]
fn any_over_empty_is_false() {
    let adl = format!("{JETS}region R\n  select any(pt(jets) > 0)\n");
    assert!(!passes(&adl, "R", EV_EMPTY), "any over empty must be false");
}

#[test]
fn all_over_empty_is_true() {
    let adl = format!("{JETS}region R\n  select all(pt(jets) > 999999)\n");
    assert!(passes(&adl, "R", EV_EMPTY), "all over empty must be vacuously true");
}

#[test]
fn min_max_over_empty_is_cut_false() {
    let min = format!("{JETS}region R\n  select min(pt(jets)) < 999999\n");
    let max = format!("{JETS}region R\n  select max(pt(jets)) > -1\n");
    assert!(!passes(&min, "R", EV_EMPTY), "min over empty has no value ⇒ cut-false");
    assert!(!passes(&max, "R", EV_EMPTY), "max over empty has no value ⇒ cut-false");
}

#[test]
fn sum_over_empty_is_zero() {
    let adl = format!("{JETS}region R\n  select sum(pt(jets)) < 1\n");
    assert!(passes(&adl, "R", EV_EMPTY), "sum over empty must be 0");
}

// ---- numeric reducers (non-empty) --------------------------------------

#[test]
fn sum_min_max_values() {
    // pts 120, 40, 30 ⇒ sum 190, min 30, max 120.
    assert!(passes(&format!("{JETS}region R\n  select sum(pt(jets)) > 189\n"), "R", EV3));
    assert!(passes(&format!("{JETS}region R\n  select sum(pt(jets)) < 191\n"), "R", EV3));
    assert!(passes(&format!("{JETS}region R\n  select min(pt(jets)) > 29\n"), "R", EV3));
    assert!(passes(&format!("{JETS}region R\n  select min(pt(jets)) < 31\n"), "R", EV3));
    assert!(passes(&format!("{JETS}region R\n  select max(pt(jets)) > 119\n"), "R", EV3));
    assert!(passes(&format!("{JETS}region R\n  select max(pt(jets)) < 121\n"), "R", EV3));
}

// ---- self-cross / multi-collection gating (Unsupported, sound) ---------

#[test]
fn self_cross_body_is_unsupported() {
    let adl = "\
object leptons
  take Electron

region R
  reject any(dR(leptons, leptons) < 0.2)
";
    let h = hir(adl);
    let ev = event(r#"{"Electron":[{"pt":50,"eta":0,"phi":0,"mass":0.0005}],"MET":{"pt":1,"phi":0}}"#);
    let r = Interp::new(&h, ext()).eval_region_by_name("R", &ev);
    assert!(r.is_err(), "self-cross reducer body must be Unsupported, got {r:?}");
}

#[test]
fn two_collection_body_is_unsupported() {
    let adl = "\
object leptons
  take Electron
object bjets
  take Jet

region R
  select any(dR(leptons, bjets) < 0.2)
";
    let h = hir(adl);
    let ev = event(r#"{"Electron":[{"pt":50,"eta":0,"phi":0,"mass":0.0005}],
                       "Jet":[{"pt":50,"eta":0,"phi":0,"mass":5}],"MET":{"pt":1,"phi":0}}"#);
    let r = Interp::new(&h, ext()).eval_region_by_name("R", &ev);
    assert!(r.is_err(), "two-collection reducer body must be Unsupported, got {r:?}");
}

// ---- 4-vector sum (mass / pt of l1 + l2) -------------------------------

/// Reference Lorentz computation matching `LV::from_ptetaphim` + invariant
/// mass, used to pin `mass(l1 + l2)` to an exact value.
fn ref_mass(parts: &[(f64, f64, f64, f64)]) -> f64 {
    let (mut px, mut py, mut pz, mut e) = (0.0, 0.0, 0.0, 0.0);
    for &(pt, eta, phi, m) in parts {
        let (cx, cy, cz) = (pt * phi.cos(), pt * phi.sin(), pt * eta.sinh());
        px += cx;
        py += cy;
        pz += cz;
        e += (cx * cx + cy * cy + cz * cz + m * m).sqrt();
    }
    (e * e - (px * px + py * py + pz * pz)).max(0.0).sqrt()
}

#[test]
fn mass_of_sum_evaluates() {
    use adl_interp::NumOutcome;
    let adl = "\
object leptons
  take Electron

region R
  select mass(leptons[0] + leptons[1]) > 0
";
    let ev_json = r#"{"Electron":[
            {"pt":50,"eta":2.0,"phi":2.9,"mass":0.0005},
            {"pt":30,"eta":-1.0,"phi":-1.5,"mass":0.0005}
        ],"MET":{"pt":1,"phi":0}}"#;
    let h = hir(adl);
    let ev = event(ev_json);
    // It must EVALUATE (no hard error); the value must match the reference
    // Lorentz mass exactly.
    let interp = Interp::new(&h, ext());
    let adl_sema::HirRegionStmt::Select(node) = &h.regions[0].stmts[0] else {
        panic!("expected a select statement");
    };
    let adl_sema::HKind::Cmp { lhs, .. } = &node.kind else {
        panic!("expected a comparison");
    };
    let got = interp.eval_num(lhs, &ev).expect("mass(sum) must evaluate");
    let want = ref_mass(&[(50.0, 2.0, 2.9, 0.0005), (30.0, -1.0, -1.5, 0.0005)]);
    match got {
        NumOutcome::Value(v) => assert!((v - want).abs() < 1e-6, "mass {v} != {want}"),
        NumOutcome::NonValue(nv) => panic!("mass(sum) was a non-value: {nv}"),
    }
    // And the surrounding cut passes (mass > 0).
    assert!(passes(adl, "R", ev_json));
}

#[test]
fn mass_of_sum_is_association_invariant() {
    // `l0 + l1 + l2` interns the same regardless of association/order, so the
    // two regions evaluate identically.
    let adl = "\
object leptons
  take Electron

region A
  select mass(leptons[0] + leptons[1] + leptons[2]) > 0

region B
  select mass(leptons[2] + (leptons[0] + leptons[1])) > 0
";
    let ev = r#"{"Electron":[
        {"pt":50,"eta":1.0,"phi":0.5,"mass":0.0005},
        {"pt":30,"eta":-0.5,"phi":2.0,"mass":0.0005},
        {"pt":20,"eta":0.2,"phi":-1.0,"mass":0.0005}
    ],"MET":{"pt":1,"phi":0}}"#;
    assert_eq!(passes(adl, "A", ev), passes(adl, "B", ev));
    assert!(passes(adl, "A", ev));
}

#[test]
fn mass_of_sum_missing_mass_is_nonvalue() {
    use adl_interp::NumOutcome;
    // No `mass` field ⇒ MissingProperty (we never assume massless).
    let adl = "\
object leptons
  take Electron

region R
  select mass(leptons[0] + leptons[1]) > 0
";
    let h = hir(adl);
    let ev = event(
        r#"{"Electron":[
            {"pt":50,"eta":1.0,"phi":0.5},
            {"pt":30,"eta":-0.5,"phi":2.0}
        ],"MET":{"pt":1,"phi":0}}"#,
    );
    let interp = Interp::new(&h, ext());
    let adl_sema::HirRegionStmt::Select(node) = &h.regions[0].stmts[0] else {
        panic!("select");
    };
    let adl_sema::HKind::Cmp { lhs, .. } = &node.kind else {
        panic!("cmp");
    };
    let got = interp.eval_num(lhs, &ev).expect("evaluates to a (non-)value");
    assert!(
        matches!(got, NumOutcome::NonValue(_)),
        "missing mass must be a non-value, got {got:?}"
    );
}
