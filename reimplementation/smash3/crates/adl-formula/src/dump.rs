//! Canonical formula dump (P3 oracle form). Byte-for-byte vs smash2_cpp
//! `check --dump-formula`.
//!
//! Rat formatting uses existing `to_parts()` only — this module must not
//! change `adl_sema::Rat` or the encoder.

use crate::encode::EncodedRegion;
use crate::formula::{Formula, QFormula};
use crate::lin::LinAtom;
use adl_sema::{Hir, Rat};

fn dump_rat(r: &Rat) -> String {
    let p = r.to_parts();
    let sign = if p.negative { "-" } else { "" };
    if p.denominator == "1" {
        format!("{sign}{}", p.numerator)
    } else {
        format!("{sign}{}/{}", p.numerator, p.denominator)
    }
}

fn dump_atom(a: &LinAtom) -> String {
    let mut s = String::from("(atom [");
    for (i, (c, q)) in a.terms().iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("({} {q})", dump_rat(c)));
    }
    s.push_str(&format!(
        "] {} {})",
        a.rel().as_str(),
        dump_rat(a.constant())
    ));
    s
}

fn dump_f(f: &Formula) -> String {
    match f {
        Formula::True => "true".into(),
        Formula::False => "false".into(),
        Formula::Atom(a) => dump_atom(a),
        Formula::And(v) => {
            let mut s = String::from("(and");
            for x in v {
                s.push(' ');
                s.push_str(&dump_f(x));
            }
            s.push(')');
            s
        }
        Formula::Or(v) => {
            let mut s = String::from("(or");
            for x in v {
                s.push(' ');
                s.push_str(&dump_f(x));
            }
            s.push(')');
            s
        }
        Formula::Unknown(d) => format!("(unknown {d})"),
        Formula::Dual { plus, minus, why } => {
            format!("(dual {why} {} {})", dump_f(plus), dump_f(minus))
        }
    }
}

fn dump_q(f: &QFormula) -> String {
    match f {
        QFormula::True => "true".into(),
        QFormula::False => "false".into(),
        QFormula::Atom(a) => dump_atom(a),
        QFormula::And(v) => {
            let mut s = String::from("(and");
            for x in v {
                s.push(' ');
                s.push_str(&dump_q(x));
            }
            s.push(')');
            s
        }
        QFormula::Or(v) => {
            let mut s = String::from("(or");
            for x in v {
                s.push(' ');
                s.push_str(&dump_q(x));
            }
            s.push(')');
            s
        }
    }
}

/// Canonical dump of one formula (sexpr).
#[must_use]
pub fn dump_formula(f: &Formula) -> String {
    dump_f(f)
}

/// Canonical dump of a projected QFormula.
#[must_use]
pub fn dump_qformula(f: &QFormula) -> String {
    dump_q(f)
}

/// Canonical per-unit dump: `unit:` header, then per-region formula/over/under/diag.
#[must_use]
pub fn dump_encoded(hir: &Hir, regions: &[EncodedRegion]) -> String {
    let mut s = format!("unit: {}\n", hir.unit);
    for r in regions {
        s.push_str(&format!(
            "region {} {} exact={}\n",
            r.region,
            r.name,
            if r.is_exact() { "true" } else { "false" }
        ));
        s.push_str(&format!("formula {}\n", dump_f(&r.formula)));
        s.push_str(&format!("over {}\n", dump_q(r.formula.over().qformula())));
        s.push_str(&format!("under {}\n", dump_q(r.formula.under().qformula())));
        for (i, d) in r.diags.iter() {
            s.push_str(&format!(
                "diag {i} {}..{} {}\n",
                d.span.start, d.span.end, d.reason
            ));
        }
    }
    s
}
