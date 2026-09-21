//! DPLL(Farkas) search that builds a [`crate::Certificate`].
//!
//! **Untrusted.** The tree this produces is validated by
//! [`crate::Certificate::replay`]; a bug here can only under-prove
//! (`Uncertified`), never over-prove.

use crate::certificate::{CertNode, QRat};
use crate::fm::{LeafResult, solve_leaf};
use crate::saturate::{
    build_child, collect_constraints, disjuncts, leftmost_or_index, saturate,
};
use crate::{Budget, MAX_DEPTH};
use adl_formula::QFormula;

/// Carries the budget and the running case-split counter through the recursion.
pub(crate) struct Searcher<'a> {
    budget: &'a Budget,
    branches: usize,
    fill_cap: usize,
}

impl<'a> Searcher<'a> {
    pub(crate) fn new(budget: &'a Budget) -> Self {
        // Fill-in cap for Fourier–Motzkin, derived from the leaf-atom budget:
        // a leaf of m atoms can, in the worst case, fan out quadratically per
        // elimination; this keeps it bounded and predictable.
        let fill_cap = budget
            .max_atoms
            .saturating_mul(budget.max_atoms)
            .saturating_add(64);
        Self {
            budget,
            branches: 0,
            fill_cap,
        }
    }

    /// Refute the conjunction of `conj`, returning a certificate node or an
    /// `Uncertified` reason. `depth` is the case-split nesting depth.
    pub(crate) fn refute(&mut self, conj: &[QFormula], depth: usize) -> Result<CertNode, String> {
        if depth > MAX_DEPTH {
            return Err(format!("budget: case-split depth exceeded {MAX_DEPTH}"));
        }

        let sat = saturate(conj);
        if sat.has_false {
            return Ok(CertNode::Contradiction);
        }

        match leftmost_or_index(&sat.items) {
            None => {
                // A pure conjunction of inequalities — a Farkas leaf.
                let cons = collect_constraints(&sat.items);
                if cons.len() > self.budget.max_atoms {
                    return Err(format!(
                        "budget: leaf has {} atoms (max {})",
                        cons.len(),
                        self.budget.max_atoms
                    ));
                }
                match solve_leaf(&cons, self.fill_cap) {
                    LeafResult::Refuted(prov) => Ok(CertNode::Farkas {
                        multipliers: prov.into_iter().map(QRat).collect(),
                    }),
                    LeafResult::Feasible => {
                        Err("branch satisfiable: real-feasible leaf".to_string())
                    }
                    LeafResult::TooBig => Err(format!(
                        "shape: fourier-motzkin fill-in exceeded {}",
                        self.fill_cap
                    )),
                }
            }
            Some(j) => {
                // Case-split the leftmost `Or`: refute every disjunct branch.
                let ds = disjuncts(&sat.items[j]);
                let mut branches = Vec::with_capacity(ds.len());
                for d in ds {
                    self.branches += 1;
                    if self.branches > self.budget.max_branches {
                        return Err(format!(
                            "budget: case-split count exceeded {}",
                            self.budget.max_branches
                        ));
                    }
                    let child = build_child(&sat.items, j, d);
                    branches.push(self.refute(&child, depth + 1)?);
                }
                Ok(CertNode::Split { branches })
            }
        }
    }
}
