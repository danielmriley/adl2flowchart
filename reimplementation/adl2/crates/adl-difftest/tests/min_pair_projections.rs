//! K14 kill-case sweep — the OPEN-1 min-pair separation `dR(A, B)`, every
//! relation the pair fold reads, under every negation site, on every shape
//! the pair product can be in (SOUNDNESS_PROOF §8 item 1c).
//!
//! This checks L1′ where it is actually stated — on the PROJECTIONS at the
//! event's presence-extended valuation — rather than through a verdict, so
//! every cell is load-bearing on its own:
//!
//! - `region3` says **In** ⇒ the OVER must be true (an excluded member
//!   fabricates DISJOINT / a subset OUTER);
//! - `region3` says **Out** ⇒ the UNDER must be false (an admitted
//!   non-member fabricates a subset INNER — the K14 failure);
//! - and `under ⇒ over` always.
//!
//! Quantity values come from the interpreter itself (`Interp::eval_quantity`),
//! including the `defined(...)` indicators, so nothing here re-interprets the
//! semantics. Junk values are irrelevant by invariant E-i: every atom over a
//! possibly-absent quantity travels with its presence literal, which settles
//! the branch before the junk is read.
//!
//! No solver, so this runs in every configuration — which matters, because
//! the projections are where the defect lives; the solver query is only how
//! it becomes visible.

use adl_formula::{QFormula, encode_regions};
use adl_interp::{Interp, NumOutcome, parse_event};
use adl_sema::{ExtDecls, QuantityId, analyze_str};
use std::collections::{BTreeMap, BTreeSet};

/// The four shapes the pair product can be in, on the same two collections.
/// `min` is the minimum `dR` over the pairs that HAVE a value; the fold reads
/// `+∞` when there is none.
///
/// | name | product | valued pairs | `min` |
/// |---|---|---|---|
/// | `ALL_VALID` | 2×1 | both | 1.0 |
/// | `NONE_VALID` | 2×1 | none (jets carry no `eta`) | `+∞` |
/// | `EMPTY_PROD` | 0×1 | none (no jets at all) | `+∞` |
/// | `SOME_VALID` | 2×1 | one (`jets[1]` carries no `eta`) | 1.0 |
///
/// `NONE_VALID` and `EMPTY_PROD` differ in exactly one place — the `∀`
/// relations are vacuously TRUE on an empty product and FALSE when a pair
/// exists but has no value — and that difference is what no single presence
/// indicator can carry, which is why `>`/`>=` are the two inexact relations.
const EVENTS: &[(&str, &str)] = &[
    (
        "ALL_VALID",
        r#"{"Jet":[{"pt":200.0,"eta":0.0,"phi":0.5,"m":1.0},{"pt":50.0,"eta":0.0,"phi":-1.5,"m":1.0}],"Electron":[{"pt":80.0,"eta":0.0,"phi":1.5,"m":1.0}],"MET":{"pt":150.0,"phi":0.5}}"#,
    ),
    (
        "NONE_VALID",
        r#"{"Jet":[{"pt":200.0,"phi":0.5,"m":1.0},{"pt":50.0,"phi":-1.5,"m":1.0}],"Electron":[{"pt":80.0,"eta":0.0,"phi":1.5,"m":1.0}],"MET":{"pt":150.0,"phi":0.5}}"#,
    ),
    (
        "EMPTY_PROD",
        r#"{"Jet":[],"Electron":[{"pt":80.0,"eta":0.0,"phi":1.5,"m":1.0}],"MET":{"pt":150.0,"phi":0.5}}"#,
    ),
    (
        "SOME_VALID",
        r#"{"Jet":[{"pt":200.0,"eta":0.0,"phi":0.5,"m":1.0},{"pt":50.0,"phi":-1.5,"m":1.0}],"Electron":[{"pt":80.0,"eta":0.0,"phi":1.5,"m":1.0}],"MET":{"pt":150.0,"phi":0.5}}"#,
    ),
];

/// Evaluate a projected formula at a valuation. A quantity with no value
/// makes the atom false — the SPEC_LANGUAGE §4.4 rule, and (by E-i) never the
/// deciding factor: the presence literal beside it has already settled the
/// branch.
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

#[test]
fn min_pair_projections_sandwich_region3_on_every_product_shape() {
    let ext = ExtDecls::legacy();
    let sites: [(&str, &str); 4] = [
        ("select", "  select CUT\n"),
        ("reject", "  reject CUT\n"),
        ("not", "  select not (CUT)\n"),
        ("notnot", "  select not (not (CUT))\n"),
    ];
    let mut checked = 0usize;
    // Collected rather than asserted cell-by-cell: the failure mode is a
    // WHOLE ROW of the operator table being wrong, and seeing one cell tells
    // you far less than seeing which relations and which product shapes.
    let mut bad: Vec<String> = Vec::new();
    // Thresholds: a constant; one that is itself possibly-absent; and one
    // that CANCELS to a constant while still reading a possibly-absent leaf.
    // The fold reads the threshold FIRST and soft-falses the whole cut when
    // it has no value — on `EMPTY_PROD` that is what decides
    // `dR(jets,eles) > pT(jets[0])`, long before the vacuously-true empty
    // product would. The cancelling one pins that the constancy test reads
    // the definedness FOOTPRINT, not the folded term map.
    //
    // `>`/`>=`/`!=` are refused outright against the two non-constant
    // thresholds, so those cells hold trivially. They are here so a future
    // attempt to encode them has to satisfy the sandwich rather than widen
    // the fragment unnoticed.
    let thresholds = ["1", "pT(jets[0])", "pT(jets[0]) - pT(jets[0]) + 1"];
    for (rel, threshold) in [">", ">=", "<", "<=", "==", "!="]
        .into_iter()
        .flat_map(|r| thresholds.map(|t| (r, t)))
    {
        let cut = format!("dR(jets, eles) {rel} {threshold}");
        let rel = format!("{rel} {threshold}");
        let mut src = String::from("object jets   take Jet\nobject eles   take Ele\n");
        for (name, body) in &sites {
            src.push_str(&format!("region R{name}\n{}", body.replace("CUT", &cut)));
        }
        let mut hir = analyze_str(&src, "minpair.adl", &ext);
        let encs = encode_regions(&mut hir);
        let projections: Vec<(QFormula, QFormula)> = encs
            .iter()
            .map(|e| {
                (
                    e.formula.over().into_qformula(),
                    e.formula.under().into_qformula(),
                )
            })
            .collect();

        // Every quantity the projections mention, including the presence
        // indicators the encoder interned while projecting.
        let mut qids = BTreeSet::new();
        for (over, under) in &projections {
            collect_qids(over, &mut qids);
            collect_qids(under, &mut qids);
        }
        let interp = Interp::new(&hir, &ext);

        for (ev_name, json) in EVENTS {
            let event = parse_event(json, &ext).expect("loader-valid");
            let vals: BTreeMap<QuantityId, Option<f64>> = qids
                .iter()
                .map(|&q| {
                    let v = match interp.eval_quantity(q, &event) {
                        Ok(NumOutcome::Value(v)) => Some(v),
                        Ok(NumOutcome::NonValue(_)) | Err(_) => None,
                    };
                    (q, v)
                })
                .collect();

            for (i, (site, _)) in sites.iter().enumerate() {
                let ctx = format!("dR {rel} / {site} / {ev_name}");
                let member = interp
                    .eval_region_membership(&format!("R{site}"), &event)
                    .unwrap_or_else(|e| panic!("{ctx}: region3 must decide: {e}"));
                let over = qeval(&projections[i].0, &vals);
                let under = qeval(&projections[i].1, &vals);
                if under && !over {
                    bad.push(format!("{ctx}: under ⇒ over violated"));
                }
                if member && !over {
                    bad.push(format!(
                        "{ctx}: region3 says In, so the OVER must admit the event \
                         — excluding a member fabricates DISJOINT"
                    ));
                }
                if !member && under {
                    bad.push(format!(
                        "{ctx}: region3 says Out, so the UNDER must reject the \
                         event — admitting a non-member fabricates a SUBSET (K14)"
                    ));
                }
                checked += 1;
            }

            // The negation peepholes and the pair fold must agree about the
            // same cut: `select`/`notnot` coincide, `reject`/`not` coincide,
            // and the two groups are complementary.
            let m = |n: &str| interp.eval_region_membership(&format!("R{n}"), &event).ok();
            assert_eq!(m("select"), m("notnot"), "dR {rel} / {ev_name}");
            assert_eq!(m("reject"), m("not"), "dR {rel} / {ev_name}");
            assert_ne!(m("select"), m("reject"), "dR {rel} / {ev_name}");
        }
    }
    assert_eq!(
        checked,
        6 * 3 * 4 * 4,
        "the full relation × threshold × site × product-shape matrix"
    );
    assert!(
        bad.is_empty(),
        "{} of {checked} cells break the projection sandwich:\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}

/// The min-pair separation OUTSIDE `dR(A, B) <rel> <value>` encodes as
/// `Unknown`, in both projections.
///
/// The interpreter reads those shapes through the plain min value, with no
/// operator scoping — `dR(A,B) + 1 > 2` is `min + 1 > 2`, while the bare
/// `dR(A,B) > 1` is a `∀` over the pair product. The two readings genuinely
/// differ (they part company as soon as one pair has no value), and only the
/// second is what a `p ≥ 1 ∧ atom` leaf says. Rather than encode a third
/// thing, the encoder refuses: an `Unknown` weakens to POSSIBLY, an atom that
/// means the wrong fold is K14 through a different door.
///
/// This pins a CONSERVATISM, not an agreement — the interpreter decides these
/// shapes and the encoder declines to. If a later change makes the encoder
/// read them faithfully, delete the case rather than weaken the assertion.
#[test]
fn min_pair_outside_the_fold_is_unknown_in_both_projections() {
    let ext = ExtDecls::legacy();
    let shapes = [
        ("arithmetic", "dR(jets, eles) + 1 > 2"),
        ("scaled", "2 * dR(jets, eles) > 2"),
        ("both-sides", "dR(jets, eles) > dR(eles, jets)"),
        ("band", "dR(jets, eles) [] 0 1"),
        ("band-out", "dR(jets, eles) ][ 0 1"),
        ("abs", "abs(dR(jets, eles)) > 1"),
    ];
    for (name, cut) in shapes {
        let src =
            format!("object jets   take Jet\nobject eles   take Ele\nregion R\n  select {cut}\n");
        let mut hir = analyze_str(&src, "outside.adl", &ext);
        let encs = encode_regions(&mut hir);
        let f = &encs[0].formula;
        assert!(
            !f.is_exact(),
            "{name}: `{cut}` must not encode as an exact atom — that atom \
             would mean the plain min, not the operator-scoped fold"
        );
        assert!(
            !qeval(&f.under().into_qformula(), &BTreeMap::new()),
            "{name}: the UNDER must claim nothing"
        );
        // The OVER admits everything, so no member can be excluded whatever
        // the interpreter decides.
        assert!(
            qeval(&f.over().into_qformula(), &BTreeMap::new()),
            "{name}: the OVER must admit everything"
        );
    }
}
