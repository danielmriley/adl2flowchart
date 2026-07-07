//! Integration smoke test (TESTING.md "encoder vs interpreter" layer,
//! Phase-5 integration): encode the 3-region golden file
//! `collection_quant.adl`, project each region both ways
//! (`Over`/`Under`), and check — for 5 hand-written events — that the
//! interpreter's verdict is consistent with evaluating the projected
//! `QFormula`s on the event's quantity values:
//!
//! - sandwich: `under ⇒ over` (always);
//! - interpreter `Ok(true)` ⇒ `over` is true; `Ok(false)` ⇒ `under` is
//!   false (i.e. `under ⊆ R ⊆ over` on the evaluated event);
//! - exact regions (no `Unknown`/`Dual`): `over == under == verdict`;
//! - OPEN-1 regions, where the interpreter honestly refuses a quantifier
//!   reading: the diagnosed error names OPEN-1, the encoding is
//!   non-exact, and *both* candidate readings (∀ with vacuous truth, ∃)
//!   sit inside the `[under, over]` sandwich — the Dual contract.
//!
//! Quantity values are produced by the interpreter itself
//! (`Interp::eval_num` on `HKind::Quantity` nodes), so this test has no
//! private re-interpretation of semantics; a soft non-value makes the
//! enclosing atom false, the SPEC_LANGUAGE §4.4 rule.

use adl_formula::{QFormula, encode_regions};
use adl_interp::{Event, Interp, NumOutcome, parse_event};
use adl_sema::{ExtDecls, HKind, HNode, QuantityId, analyze_str};
use adl_syntax::span::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Expected interpreter outcome for one region on one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    /// `Ok(verdict)`.
    Pass(bool),
    /// A diagnosed evaluation error naming OPEN-1.
    Open1,
}

/// One hand-written event with its hand-computed expectations,
/// per region in declaration order
/// (`SR_allhard`, `SR_unbounded`, `SR_softlead`).
struct Case {
    name: &'static str,
    json: &'static str,
    /// Jet pTs of the event (the hand-computation input for the ∀/∃
    /// readings of the OPEN-1 cut `pT(jets) > 100`).
    jet_pts: &'static [f64],
    want: [Want; 3],
    /// Hand-computed (over, under) evaluations per region.
    proj: [(bool, bool); 3],
}

const CASES: [Case; 5] = [
    // size = 0: the empty-collection case the legacy ∀-plus dropped
    // (audit Bug 1) — plus must admit it (`size = 0` disjunct).
    Case {
        name: "E1 no jets",
        json: r#"{"Jet": []}"#,
        jet_pts: &[],
        want: [Want::Pass(false), Want::Open1, Want::Pass(false)],
        proj: [(false, false), (true, false), (false, false)],
    },
    // size = 1, the only jet passes the cut: ∀ and ∃ agree (true),
    // so over == under == true for the pure OPEN-1 region.
    Case {
        name: "E2 one hard jet",
        json: r#"{"Jet": [{"pt": 150, "eta": 0.1, "phi": 0.3, "m": 5}]}"#,
        jet_pts: &[150.0],
        want: [Want::Open1, Want::Open1, Want::Pass(false)],
        proj: [(true, true), (true, true), (false, false)],
    },
    // size = 2, readings disagree (∃ true, ∀ false): the Dual gap —
    // over true, under false.
    Case {
        name: "E3 hard + soft jet",
        json: r#"{"Jet": [
            {"pt": 120, "eta": -0.5, "phi": 1.0, "m": 7},
            {"pt":  30, "eta":  2.0, "phi": -2.0, "m": 4}
        ]}"#,
        jet_pts: &[120.0, 30.0],
        want: [Want::Open1, Want::Open1, Want::Pass(false)],
        proj: [(true, false), (true, false), (false, false)],
    },
    // size = 1, the jet fails the cut: ∀ and ∃ agree (false) — over
    // itself is false. SR_softlead passes (40 < 50).
    Case {
        name: "E4 one soft jet",
        json: r#"{"Jet": [{"pt": 40, "eta": 0.7, "phi": -1.2, "m": 3}]}"#,
        jet_pts: &[40.0],
        want: [Want::Open1, Want::Open1, Want::Pass(true)],
        proj: [(false, false), (false, false), (true, true)],
    },
    // size = 4 > k = 3: beyond the OPEN-1 expansion bound (`size > 3`
    // disjunct in plus, `size ≤ 3` bound in minus). SR_allhard
    // short-circuits to Ok(false) on `size <= 2` before the OPEN-1 cut.
    Case {
        name: "E5 four hard jets",
        json: r#"{"Jet": [
            {"pt": 200, "eta": 0.0, "phi": 0.1, "m": 10},
            {"pt": 150, "eta": 1.1, "phi": 2.2, "m": 9},
            {"pt": 120, "eta": -1.3, "phi": -0.7, "m": 8},
            {"pt": 110, "eta": 0.4, "phi": 3.0, "m": 7}
        ]}"#,
        jet_pts: &[200.0, 150.0, 120.0, 110.0],
        want: [Want::Pass(false), Want::Open1, Want::Pass(false)],
        proj: [(false, false), (true, false), (false, false)],
    },
];

/// Evaluate a projected formula on concrete quantity values.
/// `None` is a soft non-value: the enclosing atom is **false**
/// (SPEC_LANGUAGE §4.4, same rule the interpreter applies).
fn qeval(f: &QFormula, vals: &BTreeMap<QuantityId, Option<f64>>) -> bool {
    match f {
        QFormula::True => true,
        QFormula::False => false,
        QFormula::And(v) => v.iter().all(|p| qeval(p, vals)),
        QFormula::Or(v) => v.iter().any(|p| qeval(p, vals)),
        QFormula::Atom(a) => {
            let mut lhs = 0.0;
            for (c, q) in a.terms() {
                let Some(Some(v)) = vals.get(q).copied() else {
                    return false;
                };
                lhs += c.to_f64() * v;
            }
            let k = a.constant().to_f64();
            use adl_formula::Rel;
            lhs.is_finite()
                && match a.rel() {
                    Rel::Lt => lhs < k,
                    Rel::Le => lhs <= k,
                    Rel::Gt => lhs > k,
                    Rel::Ge => lhs >= k,
                    Rel::Eq => lhs == k,
                    Rel::Ne => lhs != k,
                }
        }
    }
}

/// Every quantity referenced by a projected formula.
fn collect_qids(f: &QFormula, out: &mut BTreeSet<QuantityId>) {
    match f {
        QFormula::Atom(a) => out.extend(a.terms().iter().map(|&(_, q)| q)),
        QFormula::And(v) | QFormula::Or(v) => {
            for p in v {
                collect_qids(p, out);
            }
        }
        QFormula::True | QFormula::False => {}
    }
}

fn golden_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../legacy_parser/tests/golden/collection_quant.adl");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

#[test]
fn golden_three_region_formula_vs_interpreter_on_hand_written_events() {
    let ext = ExtDecls::legacy();
    let src = golden_source();
    let mut hir = analyze_str(&src, "collection_quant.adl", &ext);
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "golden file must resolve cleanly: {:#?}",
        hir.diags
    );

    // Encode all three regions and project both ways.
    let encs = encode_regions(&mut hir);
    assert_eq!(encs.len(), 3, "collection_quant.adl declares 3 regions");
    assert_eq!(encs[0].name, "SR_allhard");
    assert_eq!(encs[1].name, "SR_unbounded");
    assert_eq!(encs[2].name, "SR_softlead");
    // OPEN-1 regions carry a Dual; the indexed region is exact.
    assert!(!encs[0].is_exact(), "SR_allhard has an OPEN-1 Dual");
    assert!(!encs[1].is_exact(), "SR_unbounded has an OPEN-1 Dual");
    assert!(encs[2].is_exact(), "SR_softlead is exact");

    let projections: Vec<(QFormula, QFormula)> = encs
        .iter()
        .map(|e| {
            (
                e.formula.over().into_qformula(),
                e.formula.under().into_qformula(),
            )
        })
        .collect();

    // Quantity nodes for everything the projections mention; values come
    // from the interpreter itself (no semantics re-implemented here).
    // Built after `encode_regions`: the OPEN-1 expansion interns
    // `jets[i].pt` / `size(jets)` quantities into the table.
    let mut qids = BTreeSet::new();
    for (over, under) in &projections {
        collect_qids(over, &mut qids);
        collect_qids(under, &mut qids);
    }
    let qnodes: Vec<(QuantityId, HNode)> = qids
        .into_iter()
        .map(|q| (q, HNode::new(HKind::Quantity(q), Span::default())))
        .collect();

    let interp = Interp::new(&hir, &ext);

    for case in &CASES {
        let event: Event = parse_event(case.json, &ext)
            .unwrap_or_else(|e| panic!("{}: event must parse: {e}", case.name));

        let vals: BTreeMap<QuantityId, Option<f64>> = qnodes
            .iter()
            .map(|(q, node)| {
                let out = interp
                    .eval_num(node, &event)
                    .unwrap_or_else(|e| panic!("{}: quantity {q:?} must evaluate: {e}", case.name));
                let v = match out {
                    NumOutcome::Value(v) => Some(v),
                    NumOutcome::NonValue(_) => None,
                };
                (*q, v)
            })
            .collect();

        // Hand-computed candidate readings of the OPEN-1 cut
        // `pT(jets) > 100` (computed from the event definition alone).
        let forall = case.jet_pts.iter().all(|&p| p > 100.0); // vacuously true
        let exists = case.jet_pts.iter().any(|&p| p > 100.0);
        let size_window = (1..=2).contains(&case.jet_pts.len());
        // Region-level readings: SR_allhard = size cuts ∧ OPEN-1 cut;
        // SR_unbounded = OPEN-1 cut alone; None for the exact region.
        let readings: [Option<(bool, bool)>; 3] = [
            Some((size_window && forall, size_window && exists)),
            Some((forall, exists)),
            None,
        ];

        for (r, enc) in encs.iter().enumerate() {
            let ctx = format!("{} / {}", case.name, enc.name);
            let (over_f, under_f) = &projections[r];
            let over = qeval(over_f, &vals);
            let under = qeval(under_f, &vals);
            assert_eq!(
                (over, under),
                case.proj[r],
                "{ctx}: hand-computed (over, under) projection values"
            );
            // Sandwich: an under-approximation can never accept an event
            // the over-approximation rejects.
            assert!(!under || over, "{ctx}: under ⇒ over violated");

            let verdict = interp.eval_region_by_name(&enc.name, &event);
            match case.want[r] {
                Want::Pass(expected) => {
                    let got = verdict.unwrap_or_else(|e| {
                        panic!("{ctx}: interpreter must produce a verdict: {e}")
                    });
                    assert_eq!(got, expected, "{ctx}: hand-computed interpreter verdict");
                    // Consistency: under ⊆ R ⊆ over on this event.
                    assert!(!got || over, "{ctx}: interpreter passes but over rejects");
                    assert!(
                        got || !under,
                        "{ctx}: interpreter rejects but under accepts"
                    );
                    if enc.is_exact() {
                        assert_eq!(over, got, "{ctx}: exact region must match over exactly");
                        assert_eq!(under, got, "{ctx}: exact region must match under exactly");
                    }
                }
                Want::Open1 => {
                    let err = verdict.expect_err("OPEN-1 region must be a diagnosed error");
                    assert!(
                        err.reason.contains("OPEN-1"),
                        "{ctx}: error must name OPEN-1, got: {}",
                        err.reason
                    );
                    assert!(!enc.is_exact(), "{ctx}: refused region must be non-exact");
                    // Dual contract: both candidate readings sit inside
                    // the [under, over] sandwich.
                    let (f_all, f_any) =
                        readings[r].expect("OPEN-1 expectations only for Dual regions");
                    for (label, reading) in [("∀", f_all), ("∃", f_any)] {
                        assert!(
                            !under || reading,
                            "{ctx}: under accepts but the {label} reading rejects"
                        );
                        assert!(
                            !reading || over,
                            "{ctx}: the {label} reading accepts but over rejects"
                        );
                    }
                }
            }
        }
    }
}
