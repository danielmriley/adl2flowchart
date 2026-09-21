//! The presence-model structural invariants (SPEC_PRESENCE_MODEL §11).
//!
//! These are solver-free: they check the SHAPE the encoder produces, which
//! is what Lemma E ("the junk value of an absent quantity is unconstrained
//! by any asserted formula") rests on. A violation is not a lost proof — it
//! is a formula that constrains a value the event does not have, which is
//! how the absent-property class fabricated PROVEN verdicts in the first
//! place.

use adl_formula::{Formula, Rel, encode_regions};
use adl_sema::{ExtDecls, Hir, Quantity, QuantityId, QuantityTable, Rat, analyze_str};
use std::collections::BTreeSet;
use std::path::PathBuf;

fn corpus_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../examples");
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "adl") {
                out.push(p);
            }
        }
    }
    out.sort();
    assert!(out.len() > 100, "corpus not found (got {})", out.len());
    out
}

/// Is this atom exactly `p_q ≥ 1` (present) or `p_q < 1` (absent) for some
/// `q`? Returns `(q, is_present)`.
fn presence_literal(table: &QuantityTable, f: &Formula) -> Option<(QuantityId, bool)> {
    let Formula::Atom(a) = f else { return None };
    let [(c, p)] = a.terms() else { return None };
    if !c.is_one() || !a.constant().is_one() {
        return None;
    }
    let Quantity::Present(inner) = table.quantity(*p) else {
        return None;
    };
    match a.rel() {
        Rel::Ge => Some((*inner, true)),
        Rel::Lt => Some((*inner, false)),
        _ => None,
    }
}

/// Invariant **E-i**: every atom over a possibly-absent quantity `q` is
/// either conjoined (somewhere up the And-spine) with `p_q ≥ 1`, or is a
/// disjunct of an `Or` that also offers `p_q < 1`.
///
/// `present` accumulates through `And` nodes, `absent` through `Or` nodes —
/// which is what lets the encoder HOIST a presence guard above the `And`/`Or`
/// an `abs`, `ratio` or band-`Out` expands into, instead of repeating it on
/// every branch (the hoist is what keeps the bound on the interval layer's
/// spine).
fn check_ei(
    table: &QuantityTable,
    f: &Formula,
    present: &BTreeSet<QuantityId>,
    absent: &BTreeSet<QuantityId>,
    bad: &mut Vec<String>,
) {
    match f {
        Formula::True | Formula::False | Formula::Unknown(_) => {}
        Formula::Atom(a) => {
            if presence_literal(table, f).is_some() {
                return;
            }
            for (_, q) in a.terms() {
                if table.may_be_absent(*q) && !present.contains(q) && !absent.contains(q) {
                    bad.push(format!(
                        "unguarded atom over possibly-absent {q}: {:?}",
                        adl_sema::Quantity::clone(table.quantity(*q))
                    ));
                }
            }
        }
        Formula::And(v) => {
            let mut present = present.clone();
            for p in v {
                if let Some((q, true)) = presence_literal(table, p) {
                    present.insert(q);
                }
            }
            for p in v {
                check_ei(table, p, &present, absent, bad);
            }
        }
        Formula::Or(v) => {
            let mut absent = absent.clone();
            for p in v {
                if let Some((q, false)) = presence_literal(table, p) {
                    absent.insert(q);
                }
            }
            for p in v {
                check_ei(table, p, present, &absent, bad);
            }
        }
        Formula::Dual { plus, minus, .. } => {
            check_ei(table, plus, present, absent, bad);
            check_ei(table, minus, present, absent, bad);
        }
    }
}

fn encode_file(path: &PathBuf) -> (Hir, Vec<adl_formula::EncodedRegion>) {
    let src = std::fs::read_to_string(path).expect("readable");
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut hir = analyze_str(&src, &name, &ExtDecls::legacy());
    let regions = encode_regions(&mut hir);
    (hir, regions)
}

/// **I-1 — the chokepoint holds over the whole corpus.**
#[test]
fn i1_no_unguarded_possibly_absent_atom_in_the_corpus() {
    let mut failures = Vec::new();
    let mut checked = 0usize;
    for path in corpus_files() {
        let (hir, regions) = encode_file(&path);
        for r in &regions {
            checked += 1;
            let mut bad = Vec::new();
            check_ei(
                &hir.table,
                &r.formula,
                &BTreeSet::new(),
                &BTreeSet::new(),
                &mut bad,
            );
            for b in bad {
                failures.push(format!("{}::{}: {b}", path.display(), r.name));
            }
        }
    }
    assert!(checked > 100, "encoded too few regions ({checked})");
    assert!(
        failures.is_empty(),
        "{} E-i violation(s) — an atom constrains a value the event may not have:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
    eprintln!("I-1: {checked} corpus regions satisfy E-i");
}

/// **I-4 — negation is exact where the restoration claim needs it.**
///
/// The Phase-A hedge made `reject c` non-exact whenever `c` mentioned ANY
/// possibly-absent quantity, which poisoned every subset inner and region
/// emptiness that ran through a `reject` — 18 corpus proofs and three
/// complement pins. What restores them is that a SOFT-absent scope negates
/// exactly: absence is expressible, so `p < 1 ∨ ¬φ` is the interpreter's own
/// rule and no `Dual` is needed.
///
/// A HARD-absent scope is different and the difference is not a hedge, it is
/// the semantics: a missing event-level datum makes the operand Unknown, and
/// `In(¬c)` requires the datum only when EVERY route to `c` being decidably
/// FALSE reads it. Where it does — a bare comparison, a disjunction of them,
/// or either wrapped in this encoder's own presence guards — the literal is
/// conjoined on both projections and the encoding stays exact (that is what
/// keeps `reject MET > 100`'s bound on the And-spine). Where it does not —
/// an `And` that can absorb, a bounded-expansion `Dual`, or a comparison
/// that ALSO mentions a soft-absent quantity — the literal is under-only and
/// the encoding is a `Dual`. Kill cases K10-K12: conjoining it on the over
/// side there excluded a genuine member and fabricated a subset.
#[test]
fn i4a_soft_absent_scopes_negate_exactly() {
    let ext = ExtDecls::legacy();
    for c in [
        "BTag(jets[0]) >= 1",
        "pT(jets[0]) > 30",
        "dR(jets, eles) < 0.4",
        "abs(Eta(jets[0])) < 2.4",
        "Eta(jets[0]) [] 0 2",
        "min(pT(jets[0]), pT(jets[1])) > 30",
        "size(jets) >= 2",
    ] {
        let (sel, rej) = encode_select_reject(&ext, c);
        assert_eq!(
            sel, rej,
            "`reject {c}` exactness ({rej}) must match `select {c}` ({sel}) — \
             this is what restores subset inners through a reject"
        );
    }
}

#[test]
fn i4b_hard_absent_scopes_stay_exact_where_the_datum_is_forced() {
    let ext = ExtDecls::legacy();
    for c in ["MET > 100", "HT > 50", "MET > 100 or HT > 50"] {
        let (sel, rej) = encode_select_reject(&ext, c);
        assert!(sel, "`select {c}` should encode exactly");
        assert!(
            rej,
            "`reject {c}` must stay EXACT: every route to `{c}` being false \
             reads the datum, so the presence literal is conjoined on both \
             projections and the And-spine bound survives"
        );
    }
}

#[test]
fn i4c_hard_absent_scopes_split_polarity_where_the_datum_is_conditional() {
    let ext = ExtDecls::legacy();
    for c in [
        // soft co-mention: the soft non-value beats the hard error
        "pT(jets[0]) + MET > 200",
        // `And` absorption: one false conjunct settles it
        "MET > 1 and size(jets) > 99",
    ] {
        let (sel, rej) = encode_select_reject(&ext, c);
        assert!(sel, "`select {c}` should encode exactly");
        assert!(
            !rej,
            "`reject {c}` must be a polarity-split Dual: the datum may never \
             be read, so demanding it on the OVER side would exclude a \
             genuine member (K10-K12)"
        );
    }
}

/// `(select c exact, reject c exact)`.
fn encode_select_reject(ext: &ExtDecls, c: &str) -> (bool, bool) {
    let src = format!(
        "object jets\n  take Jet\n  select pt > 30\n\
         object eles\n  take Ele\n  select pt > 20\n\
         region RSel\n  select {c}\n\
         region RRej\n  reject {c}\n"
    );
    let mut hir = analyze_str(&src, "i4.adl", ext);
    let regions = encode_regions(&mut hir);
    let sel = regions.iter().find(|r| r.name == "RSel").expect("RSel");
    let rej = regions.iter().find(|r| r.name == "RRej").expect("RRej");
    (sel.is_exact(), rej.is_exact())
}

/// **Rewrite invariance under the presence model.** `reject c`,
/// `select not c` and `select not not not c` must encode IDENTICALLY: the
/// presence handling must not depend on where the author put the negation.
/// (The metamorphic battery found the first, blunt version of the Phase-A
/// guard flipping verdicts across exactly these rewrites.)
#[test]
fn negation_placement_is_rewrite_invariant() {
    let ext = ExtDecls::legacy();
    let head = "object jets\n  take Jet\n  select pt > 30\n";
    for c in [
        "BTag(jets[0]) >= 1",
        "MET > 100",
        "pT(jets[0]) > 30",
        // K13's shape: the presence split must not depend on where the
        // author put the negation either.
        "MET > 1 and HT > 2",
    ] {
        let f = |stmt: String| {
            let src = format!("{head}region R\n  {stmt}\n");
            let mut hir = analyze_str(&src, "rw.adl", &ext);
            encode_regions(&mut hir).remove(0).formula
        };
        let a = f(format!("reject {c}"));
        let b = f(format!("select not ({c})"));
        let d = f(format!("reject not not ({c})"));
        assert_eq!(a, b, "`reject {c}` != `select not {c}`");
        assert_eq!(a, d, "`reject {c}` != `reject not not {c}`");
    }
}

/// A presence literal is a plain INEQUALITY over a real — never an equality,
/// never `!=`, never integrality-dependent (SPEC_PRESENCE_MODEL §3.2). The
/// interval layer reads it as an ordinary single-quantity bound and the
/// Farkas certifier as an ordinary row; an equality would need
/// equality-splitting in both.
#[test]
fn presence_literals_are_plain_inequalities_at_one() {
    let ext = ExtDecls::legacy();
    let src = "object jets\n  take Jet\n  select pt > 30\n\
               region R\n  select BTag(jets[0]) >= 1\n  reject pT(jets[1]) > 50\n";
    let mut hir = analyze_str(src, "lit.adl", &ext);
    let regions = encode_regions(&mut hir);
    let mut seen = 0usize;
    fn walk(table: &QuantityTable, f: &Formula, seen: &mut usize) {
        match f {
            Formula::Atom(a) => {
                let [(c, p)] = a.terms() else { return };
                if !matches!(table.quantity(*p), Quantity::Present(_)) {
                    return;
                }
                *seen += 1;
                assert!(c.is_one(), "presence literal must have coefficient 1");
                assert_eq!(a.constant(), &Rat::one(), "presence literal cuts at 1");
                assert!(
                    matches!(a.rel(), Rel::Ge | Rel::Lt),
                    "presence literal must be >= or <, got {:?}",
                    a.rel()
                );
            }
            Formula::And(v) | Formula::Or(v) => {
                for x in v {
                    walk(table, x, seen);
                }
            }
            Formula::Dual { plus, minus, .. } => {
                walk(table, plus, seen);
                walk(table, minus, seen);
            }
            Formula::True | Formula::False | Formula::Unknown(_) => {}
        }
    }
    walk(&hir.table, &regions[0].formula, &mut seen);
    assert!(seen >= 2, "expected presence literals, found {seen}");
}

/// **Cost pin.** SPEC_PRESENCE_MODEL §10 R-c estimated "roughly a 1.8x
/// growth in declared variables" for the largest realistic corpus file.
/// This measures it, and pins an upper bound so a future encoder change
/// that starts interning presence indicators indiscriminately is visible in
/// the diff rather than only in the solver's wall clock.
#[test]
fn presence_indicator_cost_on_the_largest_corpus_file() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../examples/CMS/CMS-SUS-16-033_Delphes.adl");
    let (hir, regions) = encode_file(&path);
    assert!(regions.len() >= 13, "expected the 13-region file");
    let total = hir.table.quantities().len();
    let present = hir
        .table
        .quantities()
        .iter()
        .filter(|q| matches!(q, Quantity::Present(_)))
        .count();
    let base = total - present;
    let ratio = total as f64 / base as f64;
    eprintln!(
        "CMS-SUS-16-033: {base} quantities + {present} presence indicators = {total} ({ratio:.2}x)"
    );
    assert!(present > 0, "the file must intern presence indicators");
    assert!(
        ratio < 2.0,
        "presence indicators must not more than double the declared set: {ratio:.2}x \
         ({base} + {present})"
    );
}

/// **Cancellation must not lose definedness.** `MET + HT − HT > 50` folds
/// to an atom over MET alone — the HT coefficient cancels to zero and
/// `LinAtom::new` drops zero-coefficient terms — but the INTERPRETER still
/// evaluates HT, and a missing HT scalar makes the whole comparison a
/// non-value. This shipped as a false `subset` in the golden corpus
/// (`features-num_09.adl`) until the presence guard started reading the
/// pre-construction footprint.
#[test]
fn cancelled_operands_keep_their_definedness() {
    let ext = ExtDecls::legacy();
    let src = "region R\n  select MET + HT - HT > 50\n";
    let mut hir = analyze_str(src, "cancel.adl", &ext);
    let regions = encode_regions(&mut hir);

    // The ATOM lost HT …
    let mut atom_qs = BTreeSet::new();
    fn atoms(f: &Formula, out: &mut BTreeSet<QuantityId>, table: &QuantityTable) {
        match f {
            Formula::Atom(a) => {
                if presence_literal(table, f).is_none() {
                    out.extend(a.terms().iter().map(|&(_, q)| q));
                }
            }
            Formula::And(v) | Formula::Or(v) => v.iter().for_each(|p| atoms(p, out, table)),
            Formula::Dual { plus, minus, .. } => {
                atoms(plus, out, table);
                atoms(minus, out, table);
            }
            Formula::True | Formula::False | Formula::Unknown(_) => {}
        }
    }
    atoms(&regions[0].formula, &mut atom_qs, &hir.table);
    assert_eq!(atom_qs.len(), 1, "the atom should mention MET alone: {atom_qs:?}");

    // … but the formula still REQUIRES both operands defined.
    let mut guarded = BTreeSet::new();
    fn presences(f: &Formula, out: &mut BTreeSet<QuantityId>, table: &QuantityTable) {
        if let Some((q, true)) = presence_literal(table, f) {
            out.insert(q);
        }
        match f {
            Formula::And(v) | Formula::Or(v) => v.iter().for_each(|p| presences(p, out, table)),
            Formula::Dual { plus, minus, .. } => {
                presences(plus, out, table);
                presences(minus, out, table);
            }
            _ => {}
        }
    }
    presences(&regions[0].formula, &mut guarded, &hir.table);
    assert_eq!(
        guarded.len(),
        2,
        "both MET and the CANCELLED HT must be required present: {guarded:?}"
    );
    assert!(guarded.is_superset(&atom_qs));
}
