//! Property tests for the trusted kernel.
//!
//! * `certified_replays` — every `Certified` result replays true, before and
//!   after a serde round-trip (2000 random systems).
//! * `sat_by_construction_never_certified` — systems built to hold at a chosen
//!   rational point are never certified (2000 systems).
//! * `z3_agreement` — a bounded cross-check against `/home/daniel/bin/z3`:
//!   `Certified ⇒ z3 unsat`, and each satisfiable-by-construction system is
//!   `z3 sat` and uncertified. Skipped (with a note) when z3 is absent.

use adl_certify::{Budget, Certificate, certify_unsat};
use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::TestRunner;
use std::collections::BTreeSet;

const N_QUANTS: u32 = 3;

/// A generated formula skeleton. `k` drives the free (random) build; `delta`
/// drives the satisfiable-by-construction build.
#[derive(Debug, Clone)]
enum Shape {
    Atom {
        terms: Vec<(i64, u32)>,
        rel: u8,
        k: i64,
        delta: i64,
    },
    And(Vec<Shape>),
    Or(Vec<Shape>),
}

fn rel_of(t: u8) -> Rel {
    match t % 6 {
        0 => Rel::Lt,
        1 => Rel::Le,
        2 => Rel::Gt,
        3 => Rel::Ge,
        4 => Rel::Eq,
        _ => Rel::Ne,
    }
}

fn atom_strat() -> impl Strategy<Value = Shape> {
    (
        prop::collection::vec((-3i64..=3, 0u32..N_QUANTS), 1..3),
        0u8..6,
        -5i64..=5,
        1i64..=4,
    )
        .prop_map(|(terms, rel, k, delta)| Shape::Atom {
            terms,
            rel,
            k,
            delta,
        })
}

fn shape_strat() -> impl Strategy<Value = Shape> {
    atom_strat().prop_recursive(3, 16, 3, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 1..3).prop_map(Shape::And),
            prop::collection::vec(inner, 1..3).prop_map(Shape::Or),
        ]
    })
}

fn ri(n: i64) -> Rat {
    Rat::from_i64(n)
}

fn terms_to_atom(terms: &[(i64, u32)], rel: Rel, k: i64) -> QFormula {
    let ts: Vec<(Rat, QuantityId)> = terms.iter().map(|(c, q)| (ri(*c), QuantityId(*q))).collect();
    QFormula::Atom(LinAtom::new(ts, rel, ri(k)))
}

/// Build the formula with the skeleton's own (random) constants.
fn build_free(sh: &Shape) -> QFormula {
    match sh {
        Shape::Atom { terms, rel, k, .. } => terms_to_atom(terms, rel_of(*rel), *k),
        Shape::And(v) => QFormula::And(v.iter().map(build_free).collect()),
        Shape::Or(v) => QFormula::Or(v.iter().map(build_free).collect()),
    }
}

/// Build a formula that is *true at* `point`: every atom's constant is chosen so
/// the relation holds at the point, so the whole boolean combination holds too.
fn build_true(sh: &Shape, point: [i64; 3]) -> QFormula {
    match sh {
        Shape::Atom {
            terms, rel, delta, ..
        } => {
            let rel = rel_of(*rel);
            let lhs: i64 = terms.iter().map(|(c, q)| c * point[*q as usize]).sum();
            let d = (*delta).max(1);
            let k = match rel {
                Rel::Lt => lhs + d, // lhs < lhs+d
                Rel::Gt => lhs - d, // lhs > lhs-d
                Rel::Ne => lhs + d, // lhs != lhs+d
                Rel::Le | Rel::Ge | Rel::Eq => lhs, // lhs <=/>=/== lhs
            };
            terms_to_atom(terms, rel, k)
        }
        Shape::And(v) => QFormula::And(v.iter().map(|c| build_true(c, point)).collect()),
        Shape::Or(v) => QFormula::Or(v.iter().map(|c| build_true(c, point)).collect()),
    }
}

fn config() -> ProptestConfig {
    // No source tree next to `tests/` to persist regressions into; disable it
    // to silence proptest's "failed to find lib.rs or main.rs" note.
    ProptestConfig {
        cases: 2000,
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

proptest! {
    #![proptest_config(config())]

    /// Whenever the certifier certifies, the certificate replays — and keeps
    /// replaying after a JSON serialization round-trip.
    #[test]
    fn certified_replays(shape in shape_strat()) {
        let forms = vec![build_free(&shape)];
        let r = certify_unsat(&forms, &Budget::default());
        if let Some(cert) = r.certificate() {
            prop_assert!(cert.replay(&forms), "certificate failed to replay");
            let js = serde_json::to_string(cert).unwrap();
            let back: Certificate = serde_json::from_str(&js).unwrap();
            prop_assert!(back.replay(&forms), "replay failed after JSON round-trip");
        }
    }
}

proptest! {
    #![proptest_config(config())]

    /// A system built to be satisfiable (holds at a concrete rational point) is
    /// never certified UNSAT.
    #[test]
    fn sat_by_construction_never_certified(
        shape in shape_strat(),
        point in proptest::array::uniform3(-3i64..=3),
    ) {
        let forms = vec![build_true(&shape, point)];
        let r = certify_unsat(&forms, &Budget::default());
        prop_assert!(
            !r.is_certified(),
            "certified a satisfiable set (point={point:?}): {shape:?}"
        );
    }
}

// ---- z3 cross-check ---------------------------------------------------------

const Z3_PATH: &str = "/home/daniel/bin/z3";

fn z3_available() -> bool {
    std::path::Path::new(Z3_PATH).exists()
}

fn collect_qs(f: &QFormula, set: &mut BTreeSet<u32>) {
    match f {
        QFormula::Atom(a) => {
            for (_, q) in a.terms() {
                set.insert(q.0);
            }
        }
        QFormula::And(v) | QFormula::Or(v) => {
            for c in v {
                collect_qs(c, set);
            }
        }
        QFormula::True | QFormula::False => {}
    }
}

fn render_atom(a: &LinAtom) -> String {
    let lhs = if a.terms().is_empty() {
        "0".to_string()
    } else {
        let parts: Vec<String> = a
            .terms()
            .iter()
            .map(|(c, q)| format!("(* {} q{})", c.smt_real(), q.0))
            .collect();
        if parts.len() == 1 {
            parts.into_iter().next().unwrap()
        } else {
            format!("(+ {})", parts.join(" "))
        }
    };
    let rhs = a.constant().smt_real();
    match a.rel() {
        Rel::Lt => format!("(< {lhs} {rhs})"),
        Rel::Le => format!("(<= {lhs} {rhs})"),
        Rel::Gt => format!("(> {lhs} {rhs})"),
        Rel::Ge => format!("(>= {lhs} {rhs})"),
        Rel::Eq => format!("(= {lhs} {rhs})"),
        Rel::Ne => format!("(not (= {lhs} {rhs}))"),
    }
}

fn render(f: &QFormula) -> String {
    match f {
        QFormula::True => "true".to_string(),
        QFormula::False => "false".to_string(),
        QFormula::And(v) if v.is_empty() => "true".to_string(),
        QFormula::Or(v) if v.is_empty() => "false".to_string(),
        QFormula::And(v) => format!(
            "(and {})",
            v.iter().map(render).collect::<Vec<_>>().join(" ")
        ),
        QFormula::Or(v) => format!(
            "(or {})",
            v.iter().map(render).collect::<Vec<_>>().join(" ")
        ),
        QFormula::Atom(a) => render_atom(a),
    }
}

fn to_smt2(forms: &[QFormula]) -> String {
    let mut qs = BTreeSet::new();
    for f in forms {
        collect_qs(f, &mut qs);
    }
    let mut s = String::from("(set-logic QF_LRA)\n");
    for q in &qs {
        s.push_str(&format!("(declare-const q{q} Real)\n"));
    }
    for f in forms {
        s.push_str(&format!("(assert {})\n", render(f)));
    }
    s.push_str("(check-sat)\n");
    s
}

/// `Some(true)` = sat, `Some(false)` = unsat, `None` = unknown / unavailable.
fn z3_check(script: &str) -> Option<bool> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new(Z3_PATH)
        .arg("-smt2")
        .arg("-in")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child
        .stdin
        .take()?
        .write_all(script.as_bytes())
        .ok()?;
    let out = child.wait_with_output().ok()?;
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        match line.trim() {
            "unsat" => return Some(false),
            "sat" => return Some(true),
            _ => {}
        }
    }
    None
}

#[test]
fn z3_agreement() {
    if !z3_available() {
        eprintln!("z3 not found at {Z3_PATH}; skipping z3 cross-check");
        return;
    }

    let mut runner = TestRunner::deterministic();
    let strat = (shape_strat(), proptest::array::uniform3(-3i64..=3));
    let budget = Budget::default();
    let cases = 800;

    for _ in 0..cases {
        let (shape, point) = strat.new_tree(&mut runner).unwrap().current();

        // (a) Random system: if we certify, z3 must agree it is unsat.
        let free = vec![build_free(&shape)];
        if certify_unsat(&free, &budget).is_certified() {
            let ans = z3_check(&to_smt2(&free));
            assert_eq!(ans, Some(false), "certified but z3 != unsat:\n{}", to_smt2(&free));
        }

        // (b) Satisfiable-by-construction: never certified, and z3 says sat.
        let sat = vec![build_true(&shape, point)];
        assert!(
            !certify_unsat(&sat, &budget).is_certified(),
            "certified a satisfiable-by-construction set"
        );
        let ans = z3_check(&to_smt2(&sat));
        assert_eq!(ans, Some(true), "sat-by-construction but z3 != sat:\n{}", to_smt2(&sat));
    }

    eprintln!("z3 cross-check: {cases} random + {cases} sat-by-construction cases agreed");
}
