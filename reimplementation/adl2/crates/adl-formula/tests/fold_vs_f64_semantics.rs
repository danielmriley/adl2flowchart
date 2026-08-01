//! **Differential oracle**: the encoded atom and the reference interpreter
//! must agree on membership, event by event, at and around every cut
//! boundary (trustworthy-verify M4 / soundness L0).
//!
//! This is the property the whole disjointness proof rests on. `UNSAT(A⁺ ∧
//! B⁺)` only means `A ∩ B = ∅` if every real member of a region satisfies its
//! encoding — so wherever the encoder flattens a cut, the flattened atom has
//! to be *exactly* the predicate the interpreter evaluates. One boundary off
//! by a half ulp is a silent false PROVEN DISJOINT (that is what audit
//! C1–C6 and CE-14 were).
//!
//! The interesting events are always at the boundary, so each case is
//! checked at the literal, one ulp below, and one ulp above — plus the
//! encoder's own folded boundary, read back out of the atom.
//!
//! Fast and deterministic — no solver, no RNG.

use adl_formula::{Formula, LinAtom, encode_region};
use adl_interp::{Interp, parse_event};
use adl_sema::{ExtDecls, Hir, Quantity, QuantityId, Rat, ScalarSource, analyze_str};
use adl_syntax::diag::Severity;

fn build_hir(src: &str) -> Hir {
    let ext = ExtDecls::legacy();
    let hir = analyze_str(src, "fold_vs_f64.adl", &ext);
    assert!(
        !hir.diags.iter().any(|d| d.severity == Severity::Error),
        "fixture has sema/parse errors: {:?}",
        hir.diags
    );
    hir
}

fn met_q(hir: &Hir) -> QuantityId {
    let hits: Vec<usize> = hir
        .table
        .quantities()
        .iter()
        .enumerate()
        .filter(|(_, q)| matches!(q, Quantity::EventScalar(ScalarSource::MetProp(_))))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(hits.len(), 1, "expected one MET quantity, got {hits:?}");
    QuantityId(u32::try_from(hits[0]).unwrap())
}

fn collect_atoms(f: &Formula, out: &mut Vec<LinAtom>) {
    match f {
        Formula::Atom(a) => out.push(a.clone()),
        Formula::And(v) | Formula::Or(v) => v.iter().for_each(|p| collect_atoms(p, out)),
        Formula::Dual { plus, minus, .. } => {
            collect_atoms(plus, out);
            collect_atoms(minus, out);
        }
        Formula::True | Formula::False | Formula::Unknown(_) => {}
    }
}

/// An event carrying only `MET.pt` (and the empty collections the loader
/// wants). Values must be non-negative — the loader enforces the NNEG domain.
fn met_event(v: f64) -> String {
    format!(
        r#"{{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{{"pt":{v},"phi":0.0}},"HT":0.0,"triggers":{{"mu_trig":0,"el_trig":0}}}}"#
    )
}

/// Encode `select <cut>` as a single MET atom, then check the atom and the
/// interpreter agree at every probe value.
fn assert_agrees_on_met(cut: &str, probes: &[f64]) {
    let ext = ExtDecls::legacy();
    let src = format!("region SR\n  select {cut}\n");
    let mut hir = build_hir(&src);
    let mut enc = encode_region(&mut hir, 0);
    // Presence bookkeeping stripped (see `tests/presence_invariants.rs`).
    enc.formula = enc.formula.without_presence(&hir.table);
    let met = met_q(&hir);

    let mut atoms = Vec::new();
    collect_atoms(&enc.formula, &mut atoms);
    assert_eq!(
        atoms.len(),
        1,
        "`{cut}` must encode as one atom (it is exact-valued linear arithmetic): {:?}",
        enc.formula
    );
    let a = &atoms[0];
    assert_eq!(a.terms().len(), 1, "`{cut}`: one term");
    assert_eq!(a.terms()[0].1, met, "`{cut}`: the term is MET");
    let coeff = a.terms()[0].0.clone();

    // The folded boundary, as an event value: k/c.
    let mut values: Vec<f64> = probes.to_vec();
    let edge = a.constant().checked_div(&coeff).unwrap().to_f64();
    for v in [edge, edge.next_up(), edge.next_down()] {
        values.push(v);
    }

    let interp = Interp::new(&hir, &ext);
    for v in values {
        if v < 0.0 {
            continue; // outside the loader's domain
        }
        let line = met_event(v);
        let event = parse_event(&line, &ext).unwrap_or_else(|e| panic!("{cut}: {e}\n{line}"));
        let by_interp = interp
            .eval_region_membership("SR", &event)
            .unwrap_or_else(|e| panic!("{cut}: interpreter: {e}"));
        let met_val = event.met.get(&ext.prop_canon("pt").0).unwrap().clone();
        let by_atom = a.rel().eval(&(&coeff * &met_val), a.constant());
        assert_eq!(
            by_atom, by_interp,
            "`{cut}` at MET={v:?}: encoder says {by_atom}, interpreter says {by_interp} \
             (atom {a:?})"
        );
    }
}

/// Every cut constant, its ulp neighbours, and the f64 evaluation of the
/// constant subtree — the values that used to separate the two semantics.
const BOUNDARY_PROBES: &[f64] = &[
    0.3,
    0.30000000000000004, // fl(0.1+0.2) — the M4 kill-case value
    0.5,
    0.5000000000000001, // C4 half-ulp
    1.0,
    3.333_333_333_333_333_5, // C5 quotient
    50.0,
    150.0,
    0.4,
    0.03,
    0.030_000_000_000_000_002,
];

#[test]
fn exact_fragment_cuts_agree_with_the_interpreter() {
    // Every shape the audit found: the encoder now folds exactly *because*
    // the interpreter does, so agreement is expected everywhere — including
    // the events that used to be counterexamples.
    for cut in [
        "MET > 0.1 + 0.2",
        "MET <= 0.1 * 3",
        "MET + 0.5 <= 1",
        "MET + -0.1 > 0.3",
        "MET * 0.3 <= 1",
        "MET / 3 > 50",
        "MET / 0.3 <= 0.1",
        "MET * 0.2 * 0.3 > 1",
        "MET > 200",
        "MET >= 0.30000000000000004",
    ] {
        assert_agrees_on_met(cut, BOUNDARY_PROBES);
    }
}

#[test]
fn the_kill_case_pair_agrees_event_by_event() {
    // A: `MET > 0.1 + 0.2` (boundary 3/10). B: a point band at the f64 sum.
    // `verify` called these disjoint while `run` accepted MET =
    // 0.30000000000000004 in both, because the encoder folded `0.1 + 0.2` in
    // f64 and the interpreter did not.
    let ext = ExtDecls::legacy();
    let src = "region A\n  select MET > 0.1 + 0.2\n\
               region B\n  select MET [] 0.30000000000000004 0.30000000000000004\n";
    let mut hir = build_hir(src);
    let mut a = encode_region(&mut hir, 0);
    let mut b = encode_region(&mut hir, 1);
    a.formula = a.formula.without_presence(&hir.table);
    b.formula = b.formula.without_presence(&hir.table);
    let met = met_q(&hir);
    let interp = Interp::new(&hir, &ext);

    let value = 0.300_000_000_000_000_04_f64;
    let event = parse_event(&met_event(value), &ext).unwrap();
    let met_val = event.met.get(&ext.prop_canon("pt").0).unwrap().clone();

    let holds = |f: &Formula| {
        let mut atoms = Vec::new();
        collect_atoms(f, &mut atoms);
        atoms.iter().all(|at| {
            assert_eq!(at.terms(), &[(Rat::one(), met)]);
            at.rel().eval(&met_val, at.constant())
        })
    };
    assert!(holds(&a.formula), "the encoding of A must admit the event");
    assert!(holds(&b.formula), "the encoding of B must admit the event");
    for name in ["A", "B"] {
        assert!(
            interp.eval_region_membership(name, &event).unwrap(),
            "the interpreter must accept the event in {name}"
        );
    }
}

/// Complementary cuts over the same expression must stay exactly
/// complementary after folding: no event may satisfy both, and none may
/// satisfy neither.
#[test]
fn complementary_folded_cuts_partition_the_boundary() {
    let ext = ExtDecls::legacy();
    for (a_cut, b_cut) in [
        ("MET + 0.5 <= 1", "MET > 0.5"),
        ("MET*0.3 <= 1", "MET >= 3.3333333333333335"),
        ("MET / 0.3 <= 0.1", "MET > 0.03"),
        ("MET <= 0.1 * 3", "MET > 0.30000000000000004"),
    ] {
        let src = format!("region A\n  select {a_cut}\nregion B\n  select {b_cut}\n");
        let mut hir = build_hir(&src);
        let _ = encode_region(&mut hir, 0);
        let _ = encode_region(&mut hir, 1);
        let interp = Interp::new(&hir, &ext);
        for v in BOUNDARY_PROBES {
            let event = parse_event(&met_event(*v), &ext).unwrap();
            let ma = interp.eval_region_membership("A", &event).unwrap();
            let mb = interp.eval_region_membership("B", &event).unwrap();
            // `MET <= 0.1*3` vs `MET > 0.30000000000000004` is not a
            // partition (there is a gap between 3/10 and the literal), but it
            // must never be BOTH.
            assert!(
                !(ma && mb),
                "{a_cut} / {b_cut} at MET={v:?}: the interpreter accepts both"
            );
        }
    }
}
