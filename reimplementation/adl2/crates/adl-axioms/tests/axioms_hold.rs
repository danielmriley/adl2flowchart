//! Per-axiom tests (SPEC_ANALYSIS §4, TESTING.md §2 "Axiom tests"):
//!
//! (a) every emitted instance of every catalog axiom HOLDS on every
//!     adl-difftest generated physical event (under the canonical
//!     pad-with-0 extension for out-of-range element variables — the
//!     same extension that justifies asserting axioms in UNSAT proofs);
//! (b) the prohibited-axiom regressions stay rejected:
//!     - "referencing C[i] implies size(C) > i" (false under guards —
//!       removed in legacy after a false empty-region proof): all
//!       instances must hold on an event with EMPTY collections;
//!     - substring tag matching (audit Bug 6): `btagDeepB` must get NO
//!       {0,1} TAG instance and a 0.5 discriminant value must violate
//!       nothing.

use adl_axioms::{AxiomId, emit_axioms, quantity_label, twin_pairs};
use adl_formula::QFormula;
use adl_interp::{Interp, NumOutcome, parse_event};
use adl_sema::{ExtDecls, HKind, HNode, Hir, QuantityId, analyze_str};
use adl_syntax::span::Span;
use std::collections::{BTreeMap, BTreeSet};

/// A vocabulary file that exercises every catalog axiom: filtered chain
/// (Jet -> jets -> bjets), union (leptons), tags, triggers, MET/HT,
/// oriented angular twins, dR.
const VOCAB: &str = "\
object jets
  take Jet
  select pT > 30

object bjets
  take jets
  select btag == 1

object eles
  take Ele

object muons
  take Muo

object leptons
  take union(eles, muons)

object sortedLeptons
  take sort(leptons, pt(leptons), descend)

composite dijet
  take disjoint(jets j1, jets j2)

composite emu
  take cartesian(eles e, muons m)

composite emuD
  take disjoint(eles e2, muons m2)

region SR
  select size(jets) >= 0
  select size(jets[1:3]) >= 0
  select size(bjets) >= 0
  select size(leptons) >= 0
  select size(sortedLeptons) >= 0
  select size(dijet) >= 0
  select size(emu) >= 0
  select size(emuD) >= 0
  select size(emu->e) >= 0
  select size(eles) >= 0
  select size(muons) >= 0
  select pT(jets[0]) >= 0
  select pT(jets[2]) >= 0
  select pT(bjets[0]) >= 0
  select pT(bjets[1]) >= 0
  select jets[0].btag >= 0
  select jets[1].m >= 0
  select MET.pT >= 0
  select HT >= 0
  select dPhi(jets[0], jets[1]) [] -4 4
  select dPhi(jets[1], jets[0]) [] -4 4
  select dEta(jets[0], muons[0]) [] -10 10
  select dEta(muons[0], jets[0]) [] -10 10
  select dR(jets[0], eles[0]) >= 0
  select cos(dPhi(jets[0], jets[1])) >= -1
  select sin(dPhi(jets[0], jets[1])) <= 1
  trigger mu_trig
";

fn analyzed(src: &str) -> (Hir, ExtDecls) {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, "axiom_vocab.adl", &ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "vocabulary must resolve cleanly: {:#?}",
        hir.diags
    );
    (hir, ext)
}

fn all_quantities(hir: &Hir) -> BTreeSet<QuantityId> {
    (0..hir.table.quantities().len())
        .map(|i| QuantityId(u32::try_from(i).unwrap()))
        .collect()
}

/// Evaluate an instance formula with interpreter-supplied values;
/// missing element data takes the canonical pad value 0.
fn eval_formula(f: &QFormula, vals: &BTreeMap<QuantityId, f64>) -> bool {
    match f {
        QFormula::True => true,
        QFormula::False => false,
        QFormula::And(v) => v.iter().all(|p| eval_formula(p, vals)),
        QFormula::Or(v) => v.iter().any(|p| eval_formula(p, vals)),
        QFormula::Atom(a) => {
            let lhs: f64 = a
                .terms()
                .iter()
                .map(|(c, q)| c.to_f64() * vals.get(q).copied().unwrap_or(0.0))
                .sum();
            let k = a.constant().to_f64();
            // The axioms are statements over REAL-valued event quantities;
            // the interpreter computes a rounded f64 image (e.g. wrap(-d)
            // is not bit-exactly -wrap(d)). Equalities therefore get an
            // epsilon; inequalities stay exact.
            if a.rel() == adl_formula::Rel::Eq {
                let scale = 1.0_f64.max(lhs.abs()).max(k.abs());
                (lhs - k).abs() <= 1e-9 * scale
            } else {
                rel_eval_f64(a.rel(), lhs, k)
            }
        }
    }
}

/// f64 evaluation of a relation (the `Rel::eval` API is exact-rational now).
fn rel_eval_f64(rel: adl_formula::Rel, a: f64, b: f64) -> bool {
    use adl_formula::Rel;
    match rel {
        Rel::Lt => a < b,
        Rel::Le => a <= b,
        Rel::Gt => a > b,
        Rel::Ge => a >= b,
        Rel::Eq => a == b,
        Rel::Ne => a != b,
    }
}

fn quantity_values(
    interp: &Interp<'_>,
    nodes: &[(QuantityId, HNode)],
    event: &adl_interp::Event,
) -> BTreeMap<QuantityId, f64> {
    let mut vals = BTreeMap::new();
    for (q, node) in nodes {
        let v = match interp.eval_num(node, event) {
            Ok(NumOutcome::Value(v)) => v,
            // Canonical pad-with-0 extension (missing elements/properties).
            Ok(NumOutcome::NonValue(_)) => 0.0,
            Err(e) => panic!("quantity {q:?} must evaluate or pad: {e}"),
        };
        vals.insert(*q, v);
    }
    vals
}

fn check_all_hold(
    hir: &Hir,
    ext: &ExtDecls,
    axioms: &adl_axioms::AxiomSet,
    events: &[adl_interp::Event],
) {
    let nodes: Vec<(QuantityId, HNode)> = axioms
        .quantities()
        .into_iter()
        .map(|q| (q, HNode::new(HKind::Quantity(q), Span::default())))
        .collect();
    let interp = Interp::new(hir, ext);
    for (n, event) in events.iter().enumerate() {
        let vals = quantity_values(&interp, &nodes, event);
        for inst in &axioms.instances {
            assert!(
                eval_formula(&inst.formula, &vals),
                "event {n}: {} instance violated: {}\nvalues: {:?}",
                inst.id,
                inst.description,
                inst.formula
            );
        }
    }
}

#[test]
fn every_axiom_holds_on_generated_physical_events() {
    let (mut hir, ext) = analyzed(VOCAB);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);

    // The vocabulary must exercise the FULL emitter catalog. XSUB/XEQ are the
    // exception: they are not produced by `emit_axioms` at all but DERIVED by
    // the analysis engine (`Engine::reconcile`) when it proves a cross/intra
    // collection refinement, so no vocabulary can make `emit_axioms` yield
    // them — their soundness is covered by the cross-file reconciliation tests.
    let used = axioms.ids_used();
    for id in AxiomId::ALL {
        if matches!(id, AxiomId::Xsub | AxiomId::Xeq) {
            continue;
        }
        assert!(used.contains(&id), "vocabulary must emit at least one {id}");
    }

    let events = adl_difftest::toy_events(0xAD1_0001, 300, &ext).expect("generator events load");
    check_all_hold(&hir, &ext, &axioms, &events);
}

#[test]
fn axioms_hold_on_the_empty_event_no_existence_from_mention() {
    // Prohibited-axiom regression: Q mentions jets[2].pt, bjets[1].pt and
    // the sizes; if any emitter derived "size > i" from mere mention, the
    // all-empty event would violate it.
    let (mut hir, ext) = analyzed(VOCAB);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);
    let empty = parse_event(
        r#"{"Jet": [], "Electron": [], "Muon": [],
            "MET": {"pt": 0.0, "phi": 0.0}, "HT": 0.0,
            "triggers": {"mu_trig": 0, "el_trig": 0}}"#,
        &ext,
    )
    .expect("empty event parses");
    check_all_hold(&hir, &ext, &axioms, &[empty]);
}

#[test]
fn continuous_btag_discriminant_gets_no_tag_axiom() {
    // Audit Bug 6 regression: exact-name rule only — `btagDeepB` is a
    // continuous discriminant and must NOT be forced into {0,1}.
    let src = "\
object jets
  take Jet

region SR
  select jets[0].btagDeepB > 0.2
  select jets[0].btag >= 0
";
    let (mut hir, ext) = analyzed(src);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);

    let tag_labels: Vec<String> = axioms
        .instances
        .iter()
        .filter(|i| i.id == AxiomId::Tag)
        .map(|i| i.description.clone())
        .collect();
    assert!(
        tag_labels
            .iter()
            .any(|d| d.contains(".btag ") || d.contains(".btag in")),
        "exact-name btag must still get its TAG instance: {tag_labels:?}"
    );
    assert!(
        !tag_labels
            .iter()
            .any(|d| d.to_lowercase().contains("deepb")),
        "btagDeepB must get NO TAG instance: {tag_labels:?}"
    );

    // And a 0.5 discriminant violates nothing.
    let event = parse_event(
        r#"{"Jet": [{"pt": 100.0, "eta": 0.0, "phi": 0.0, "m": 5.0,
                     "btag": 1, "btagDeepB": 0.5}]}"#,
        &ext,
    )
    .expect("event parses");
    check_all_hold(&hir, &ext, &axioms, &[event]);
}

#[test]
fn trig_axiom_bounds_cos_sin_only_not_tan() {
    // P3: opaque cos/sin calls get -1 <= . <= 1; tan (unbounded) gets NONE.
    let src = "\
object jets
  take Jet

region SR
  select cos(dPhi(jets[0], jets[1])) >= -2
  select sin(dPhi(jets[0], jets[1])) <= 2
  select tan(dPhi(jets[0], jets[1])) >= -1000000
";
    let (mut hir, ext) = analyzed(src);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);

    let trig: Vec<&str> = axioms
        .instances
        .iter()
        .filter(|i| i.id == AxiomId::Trig)
        .map(|i| i.description.as_str())
        .collect();
    assert!(
        trig.iter().any(|d| d.contains("cos")),
        "cos must get a TRIG [-1,1] bound: {trig:?}"
    );
    assert!(
        trig.iter().any(|d| d.contains("sin")),
        "sin must get a TRIG [-1,1] bound: {trig:?}"
    );
    assert!(
        !trig.iter().any(|d| d.contains("tan")),
        "tan must get NO TRIG bound (unbounded): {trig:?}"
    );

    // The bound holds on a concrete event (the interpreter computes cos/sin
    // of the realized dPhi, which is in [-1,1] by definition).
    let event = parse_event(
        r#"{"Jet": [{"pt": 100.0, "eta": 0.0, "phi": 1.0, "m": 5.0},
                    {"pt": 90.0, "eta": 0.5, "phi": -2.0, "m": 5.0}]}"#,
        &ext,
    )
    .expect("event parses");
    check_all_hold(&hir, &ext, &axioms, &[event]);
}

#[test]
fn sub_axiom_is_single_source_only_unions_get_uni() {
    let (mut hir, ext) = analyzed(VOCAB);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);

    // No SUB instance may mention the union collection (the audit
    // union-size regression: a union take must not get the subset axiom).
    for inst in axioms.instances.iter().filter(|i| i.id == AxiomId::Sub) {
        assert!(
            !inst.description.contains("leptons)") || !inst.description.starts_with("size(leptons"),
            "SUB must never constrain a union: {}",
            inst.description
        );
    }
    // The union gets UNI bounds instead.
    let uni: Vec<&str> = axioms
        .instances
        .iter()
        .filter(|i| i.id == AxiomId::Uni)
        .map(|i| i.description.as_str())
        .collect();
    assert!(
        uni.iter()
            .any(|d| d.contains("size(leptons) >= size(eles)")),
        "{uni:?}"
    );
    assert!(
        uni.iter()
            .any(|d| d.contains("size(leptons) <= size(eles) + size(muons)")),
        "{uni:?}"
    );
}

#[test]
fn ord_axiom_skips_filtered_union_but_keeps_base_chain() {
    // ORD/IDOM ride on pT-descending order. A base-rooted filtered chain
    // (jets <- Jet) preserves it; a filter OVER A UNION does not — the
    // interpreter concatenates union parts without a pT-merge, so
    // `goodleptons = [eles..] ++ [muons..]` can have element 1 with higher pT
    // than element 0. Emitting ORD there asserts a false fact into every
    // UNSAT-direction proof (false PROVEN DISJOINT/EMPTY/SUBSET).
    let src = "\
object jets
  take Jet
  select pT > 30

object eles
  take Ele

object muons
  take Muo

object goodleptons
  take union(eles, muons)
  select pT > 20

region SR
  select pT(jets[0]) >= 0
  select pT(jets[1]) >= 0
  select pT(goodleptons[0]) >= 0
  select pT(goodleptons[1]) >= 0
";
    let (mut hir, ext) = analyzed(src);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);
    let ord: Vec<&str> = axioms
        .instances
        .iter()
        .filter(|i| i.id == AxiomId::Ord)
        .map(|i| i.description.as_str())
        .collect();

    // Base-rooted filtered collection: ORD MUST still fire (no regression).
    assert!(
        ord.iter().any(|d| d.contains("jets[0]") && d.contains("jets[1]")),
        "ORD must still constrain a base-rooted filtered collection: {ord:?}"
    );
    // Filtered-over-union: ORD MUST NOT fire.
    assert!(
        !ord.iter().any(|d| d.contains("goodleptons")),
        "ORD must never constrain a filtered-union collection: {ord:?}"
    );
}

#[test]
fn twin_pair_detection_is_orientation_exact() {
    let (mut hir, ext) = analyzed(VOCAB);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);
    let pairs = twin_pairs(&hir.table, &axioms.quantities());
    // dPhi(jets[0], jets[1]) / dPhi(jets[1], jets[0]) and the dEta pair.
    assert_eq!(pairs.len(), 2, "{pairs:?}");
    for (a, b) in &pairs {
        let la = quantity_label(&hir, *a);
        let lb = quantity_label(&hir, *b);
        assert_ne!(la, lb);
    }
    // dR is unoriented by interning: dR(a,b) == dR(b,a) is one quantity,
    // so it can never appear as a twin pair.
    assert!(
        pairs
            .iter()
            .all(|(a, b)| !quantity_label(&hir, *a).starts_with("dR")
                && !quantity_label(&hir, *b).starts_with("dR"))
    );
}

#[test]
fn epred_guard_keeps_filtered_facts_vacuous_when_absent() {
    // size(jets) <= i  OR  pred(jets[i]) — directly check the shape: on
    // an event with no jets the guard arm must already satisfy it.
    let (mut hir, ext) = analyzed(VOCAB);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);
    let epreds: Vec<_> = axioms
        .instances
        .iter()
        .filter(|i| i.id == AxiomId::Epred)
        .collect();
    assert!(!epreds.is_empty(), "vocabulary has filtered element refs");
    for inst in &epreds {
        let QFormula::Or(arms) = &inst.formula else {
            panic!("EPRED must be guard OR predicate: {:?}", inst.formula);
        };
        assert!(arms.len() >= 2, "{:?}", inst.formula);
    }
    let _ = ext;
}

#[test]
fn opaque_pt_named_external_gets_nneg_but_bdt_stays_free() {
    // CORPUS gap 1 (CMS-SUS-16-032): the opaque `pT(jets[0] jets[1])`
    // scalar is the pT magnitude of SOME particle combination, hence
    // >= 0 on every physical event; the NNEG emitter must cover
    // exact-name pt/m/mass/e/energy/dr external calls. Unrelated opaque
    // functions (bdt, aplanarity, ...) must stay free — the exact-name
    // rule, same discipline as TAG. (No interpreter cross-check here:
    // opaque calls have no reference interpretation by design.)
    let src = "\
object jets
  take Jet

region SR
  select pT(jets[0] jets[1]) > 100
  select bdt(jets[0] jets[1]) > 0.5
  select aplanarity(jets) > 0.1
";
    let (mut hir, ext) = analyzed(src);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);

    let nneg: Vec<&str> = axioms
        .instances
        .iter()
        .filter(|i| i.id == AxiomId::Nneg)
        .map(|i| i.description.as_str())
        .collect();
    assert!(
        nneg.iter().any(|d| d.starts_with("pT(...)")),
        "opaque pt-named external must get an NNEG instance: {nneg:?}"
    );
    assert!(
        !nneg
            .iter()
            .any(|d| d.to_lowercase().contains("bdt") || d.to_lowercase().contains("aplanarity")),
        "bdt/aplanarity externals must get NO NNEG instance: {nneg:?}"
    );
}

#[test]
fn comb_size_lower_bound_is_cartesian_only_not_disjoint() {
    // SOUNDNESS GUARD (USER ANSWER 4): the positive existence lower bound
    // `all-parts-nonempty => size(K) >= 1` may fire ONLY for a bare CARTESIAN.
    // A same-source OR cross-source DISJOINT must NOT get it — value-equal
    // elements can form zero pairs even with non-empty sources.
    let src = "\
object eles
  take Ele
object muons
  take Muo
object jets
  take Jet

composite emuCart
  take cartesian(eles e, muons m)

composite emuDisj
  take disjoint(eles e2, muons m2)

composite jjDisj
  take disjoint(jets j1, jets j2)

region SR
  select size(emuCart) >= 0
  select size(emuDisj) >= 0
  select size(jjDisj) >= 0
";
    let (mut hir, ext) = analyzed(src);
    let qs = all_quantities(&hir);
    let axioms = emit_axioms(&mut hir, &ext, &qs);
    let combsize: Vec<&str> = axioms
        .instances
        .iter()
        .filter(|i| i.id == AxiomId::CombSize)
        .map(|i| i.description.as_str())
        .collect();

    // The cartesian gets the `>= 1` lower bound.
    assert!(
        combsize.iter().any(|d| d.contains(">= 1") && d.contains("emuCart")),
        "cartesian must get the all-nonempty lower bound: {combsize:?}"
    );
    // NEITHER disjoint composite gets a `>= 1` lower bound (only the
    // `< 2 => = 0` / `= 0 => = 0` zero facts).
    assert!(
        !combsize
            .iter()
            .any(|d| d.contains(">= 1") && (d.contains("emuDisj") || d.contains("jjDisj"))),
        "disjoint composites must NOT get a positive lower bound: {combsize:?}"
    );
    // The same-source disjoint gets its `size(C) < 2 => size(K) = 0` fact.
    assert!(
        combsize.iter().any(|d| d.contains("< 2 =>") && d.contains("jjDisj")),
        "same-source disjoint must get the size<2 zero fact: {combsize:?}"
    );
}
