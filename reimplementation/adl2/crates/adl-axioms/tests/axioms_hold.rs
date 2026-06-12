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

region SR
  select size(jets) >= 0
  select size(bjets) >= 0
  select size(leptons) >= 0
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
                .map(|&(c, q)| c * vals.get(&q).copied().unwrap_or(0.0))
                .sum();
            // The axioms are statements over REAL-valued event quantities;
            // the interpreter computes a rounded f64 image (e.g. wrap(-d)
            // is not bit-exactly -wrap(d)). Equalities therefore get an
            // epsilon; inequalities stay exact.
            if a.rel() == adl_formula::Rel::Eq {
                let scale = 1.0_f64.max(lhs.abs()).max(a.constant().abs());
                (lhs - a.constant()).abs() <= 1e-9 * scale
            } else {
                a.rel().eval(lhs, a.constant())
            }
        }
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

    // The vocabulary must exercise the FULL catalog.
    let used = axioms.ids_used();
    for id in AxiomId::ALL {
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
