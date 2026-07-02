//! Exact-rational Fourier–Motzkin elimination for a single conjunctive leaf.
//!
//! **Untrusted search.** This module *finds* Farkas multipliers or decides a
//! leaf is feasible; nothing here is trusted, because whatever multipliers it
//! returns are re-checked from scratch by [`crate::Certificate::replay`] (via
//! `farkas_refutes`). A bug here can only ever cause a missed proof
//! (`Uncertified`), never a wrong `Certified`.
//!
//! Each working row carries a provenance vector — the nonnegative combination
//! of the *original* leaf constraints that produced it — so the multipliers we
//! hand back are already expressed against the leaf atoms replay will see.

use crate::constraint::Constraint;
use adl_sema::{QuantityId, Rat};
use std::collections::BTreeMap;

/// Outcome of eliminating a leaf's variables.
pub(crate) enum LeafResult {
    /// Nonnegative Farkas multipliers over the original leaf constraints, in
    /// order — a witness of real-infeasibility.
    Refuted(Vec<Rat>),
    /// The leaf is real-feasible (so the whole branch, hence the whole input,
    /// is satisfiable).
    Feasible,
    /// Elimination exceeded the fill-in cap; give up (fail closed).
    TooBig,
}

/// A working constraint `Σ coeffs·q ⋈ b`, plus the nonnegative combination
/// `prov` of original constraints that derived it.
struct Row {
    coeffs: BTreeMap<QuantityId, Rat>,
    b: Rat,
    strict: bool,
    prov: Vec<Rat>,
}

/// The status of a working row: a ground row (`0 ⋈ b`, no variables) is either
/// an unsatisfiable contradiction or trivially satisfiable; a row that still has
/// variables is [`RowStatus::Open`].
enum RowStatus {
    Contradiction,
    Satisfiable,
    Open,
}

/// Classify a row: a ground row (`0 ⋈ b`, no variables) is a contradiction when
/// `0 ⋈ b` is false, satisfiable otherwise; a row with variables is open.
fn classify(row: &Row) -> RowStatus {
    if !row.coeffs.is_empty() {
        return RowStatus::Open;
    }
    let contradiction = if row.strict {
        // 0 < b  is false ⇔ b ≤ 0
        !row.b.is_positive()
    } else {
        // 0 ≤ b  is false ⇔ b < 0
        row.b.is_negative()
    };
    if contradiction {
        RowStatus::Contradiction
    } else {
        RowStatus::Satisfiable
    }
}

/// Smallest quantity still present in any row (deterministic elimination order).
fn pick_var(rows: &[Row]) -> Option<QuantityId> {
    rows.iter().flat_map(|r| r.coeffs.keys().copied()).min()
}

/// Combine a positive-`v` row and a negative-`v` row to eliminate `v`:
/// `mp·p + mn·n` with `mp, mn > 0` chosen so the `v` terms cancel.
fn combine(mp: &Rat, p: &Row, mn: &Rat, n: &Row) -> Row {
    let mut coeffs: BTreeMap<QuantityId, Rat> = BTreeMap::new();
    for (q, c) in &p.coeffs {
        coeffs.insert(*q, mp * c);
    }
    for (q, c) in &n.coeffs {
        let e = coeffs.entry(*q).or_insert_with(Rat::zero);
        *e = &*e + &(mn * c);
    }
    coeffs.retain(|_, c| !c.is_zero());

    let b = &(mp * &p.b) + &(mn * &n.b);
    let prov = p
        .prov
        .iter()
        .zip(&n.prov)
        .map(|(pp, np)| &(mp * pp) + &(mn * np))
        .collect();

    Row {
        coeffs,
        b,
        strict: p.strict || n.strict,
        prov,
    }
}

/// Decide the leaf `Σ … ⋈ …` system by eliminating every variable, returning a
/// Farkas witness ([`LeafResult::Refuted`]) the moment a ground contradiction
/// appears, [`LeafResult::Feasible`] if all variables eliminate without one, or
/// [`LeafResult::TooBig`] on fill-in blow-up.
pub(crate) fn solve_leaf(cons: &[Constraint], fill_cap: usize) -> LeafResult {
    let n = cons.len();
    let mut rows: Vec<Row> = Vec::with_capacity(n);

    for (i, con) in cons.iter().enumerate() {
        let mut prov = vec![Rat::zero(); n];
        prov[i] = Rat::one();
        let row = Row {
            coeffs: con.coeffs.iter().cloned().collect(),
            b: con.b.clone(),
            strict: con.strict,
            prov,
        };
        match classify(&row) {
            RowStatus::Contradiction => return LeafResult::Refuted(row.prov),
            RowStatus::Satisfiable => {}
            RowStatus::Open => rows.push(row),
        }
    }

    while let Some(v) = pick_var(&rows) {
        let mut pos: Vec<Row> = Vec::new();
        let mut neg: Vec<Row> = Vec::new();
        let mut next: Vec<Row> = Vec::new();
        for row in rows.drain(..) {
            match row.coeffs.get(&v).map(Rat::signum) {
                Some(1) => pos.push(row),
                Some(-1) => neg.push(row),
                // Coefficient of `v` is zero or absent: `v` does not constrain
                // this row — carry it forward untouched.
                _ => next.push(row),
            }
        }

        for p in &pos {
            for nrow in &neg {
                // cp > 0, cn < 0; multiply p by (-cn) > 0 and n by cp > 0.
                let cp = &p.coeffs[&v];
                let cn = &nrow.coeffs[&v];
                let mp = -cn;
                let mn = cp.clone();
                let row = combine(&mp, p, &mn, nrow);
                match classify(&row) {
                    RowStatus::Contradiction => return LeafResult::Refuted(row.prov),
                    RowStatus::Satisfiable => {}
                    RowStatus::Open => {
                        next.push(row);
                        if next.len() > fill_cap {
                            return LeafResult::TooBig;
                        }
                    }
                }
            }
        }
        // If `pos` or `neg` was empty, `v` is unbounded on one side and all its
        // rows are dropped (satisfiable by pushing `v` to ±∞) — exactly what
        // leaving them out of `next` does.
        rows = next;
    }

    LeafResult::Feasible
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::canonicalize;
    use adl_formula::{LinAtom, Rel};

    fn c(qi: u32, rel: Rel, k: i64) -> Constraint {
        canonicalize(&LinAtom::single(QuantityId(qi), rel, Rat::from_i64(k)))
    }

    #[test]
    fn refutes_x_gt_2_and_x_lt_1() {
        let cons = vec![c(0, Rel::Gt, 2), c(0, Rel::Lt, 1)];
        match solve_leaf(&cons, 64) {
            LeafResult::Refuted(m) => {
                assert_eq!(m.len(), 2);
                // Replay-level check that the witness is valid.
                assert!(crate::constraint::farkas_refutes(&cons, &m));
            }
            _ => panic!("expected refutation"),
        }
    }

    #[test]
    fn feasible_equal_bounds() {
        let cons = vec![c(0, Rel::Ge, 1), c(0, Rel::Le, 1)];
        assert!(matches!(solve_leaf(&cons, 64), LeafResult::Feasible));
    }

    #[test]
    fn two_variable_chain_refutes() {
        // x - y < 0, y - z < 0, z - x < 0  ==>  cyclic strict, infeasible.
        let xy = canonicalize(&LinAtom::new(
            [(Rat::from_i64(1), QuantityId(0)), (Rat::from_i64(-1), QuantityId(1))],
            Rel::Lt,
            Rat::zero(),
        ));
        let yz = canonicalize(&LinAtom::new(
            [(Rat::from_i64(1), QuantityId(1)), (Rat::from_i64(-1), QuantityId(2))],
            Rel::Lt,
            Rat::zero(),
        ));
        let zx = canonicalize(&LinAtom::new(
            [(Rat::from_i64(1), QuantityId(2)), (Rat::from_i64(-1), QuantityId(0))],
            Rel::Lt,
            Rat::zero(),
        ));
        let cons = vec![xy, yz, zx];
        match solve_leaf(&cons, 64) {
            LeafResult::Refuted(m) => assert!(crate::constraint::farkas_refutes(&cons, &m)),
            _ => panic!("expected refutation"),
        }
    }
}
