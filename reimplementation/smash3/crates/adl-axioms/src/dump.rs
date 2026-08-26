//! Canonical axiom-instance dump (P3 oracle form). Byte-for-byte vs
//! smash2_cpp `check --dump-axioms`.

use crate::AxiomSet;
use adl_formula::dump_qformula;
use adl_sema::Hir;

/// Canonical per-unit dump: `unit:` header, count, then id/description/formula.
#[must_use]
pub fn dump_axioms(hir: &Hir, set: &AxiomSet) -> String {
    let mut s = format!("unit: {}\naxioms {}\n", hir.unit, set.instances.len());
    for inst in &set.instances {
        s.push_str(&format!(
            "{} {}\n  {}\n",
            inst.id,
            inst.description,
            dump_qformula(&inst.formula)
        ));
    }
    s
}
