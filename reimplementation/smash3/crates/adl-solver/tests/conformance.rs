//! Backend conformance battery (SPEC_ARCHITECTURE §7, TESTING.md §2):
//! the native and subprocess backends must answer the same fixed query
//! battery identically — sat/unsat, models, unsat cores, push/pop
//! discipline, integer sorts, timeout behavior — and the subprocess
//! backend must turn solver `(error …)` output into `Unknown` (legacy
//! audit Bug 5: never silently weaker).

use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{QuantityId, Rat};

fn r(v: f64) -> Rat {
    Rat::from_decimal_f64(v).unwrap()
}
use adl_solver::{AssertName, QSort, SatResult, Solver, SubprocessSolver, subprocess_available};
use std::time::Duration;

const T: Duration = Duration::from_secs(10);

fn q(n: u32) -> QuantityId {
    QuantityId(n)
}

fn atom(qid: u32, rel: Rel, k: f64) -> QFormula {
    QFormula::Atom(LinAtom::single(q(qid), rel, r(k)))
}

fn atom2(c0: f64, q0: u32, c1: f64, q1: u32, rel: Rel, k: f64) -> QFormula {
    QFormula::Atom(LinAtom::new([(r(c0), q(q0)), (r(c1), q(q1))], rel, r(k)))
}

fn name(s: &str) -> Option<AssertName> {
    Some(AssertName::new(s))
}

/// The shared battery: every assertion here must hold for BOTH backends.
fn battery(s: &mut dyn Solver) {
    // --- sat + model -----------------------------------------------------
    s.push();
    s.assert(&atom(0, Rel::Gt, 1.0), None);
    s.assert(&atom(0, Rel::Lt, 3.0), None);
    assert_eq!(
        s.check(T),
        SatResult::Sat,
        "{}: open interval",
        s.backend_name()
    );
    let m = s.model().expect("model after sat");
    let v = m.get(q(0)).expect("q0 valued");
    assert!(
        v > r(1.0) && v < r(3.0),
        "{}: model in (1,3), got {v:?}",
        s.backend_name()
    );
    s.pop();

    // --- unsat + core ----------------------------------------------------
    s.push();
    s.assert(&atom(0, Rel::Gt, 3.0), name("hi"));
    s.assert(&atom(0, Rel::Lt, 1.0), name("lo"));
    s.assert(&atom(1, Rel::Ge, 0.0), name("irrelevant"));
    assert_eq!(s.check(T), SatResult::Unsat, "{}", s.backend_name());
    let core = s.unsat_core().expect("core after unsat");
    assert!(core.contains(&AssertName::new("hi")), "{core:?}");
    assert!(core.contains(&AssertName::new("lo")), "{core:?}");
    assert!(
        !core.contains(&AssertName::new("irrelevant")),
        "{}: core must name only the conflict: {core:?}",
        s.backend_name()
    );
    s.pop();

    // --- push/pop discipline ----------------------------------------------
    s.push();
    s.assert(&atom(2, Rel::Ge, 5.0), None);
    s.push();
    s.assert(&atom(2, Rel::Lt, 5.0), None);
    assert_eq!(s.check(T), SatResult::Unsat);
    s.pop();
    assert_eq!(
        s.check(T),
        SatResult::Sat,
        "{}: pop must restore sat",
        s.backend_name()
    );
    s.pop();

    // --- integer sorts (sizes) ---------------------------------------------
    s.declare(q(7), QSort::Int);
    s.push();
    // size > 0.5 with an Int sort can only mean size >= 1.
    s.assert(&atom(7, Rel::Gt, 0.5), None);
    s.assert(&atom(7, Rel::Lt, 1.5), None);
    assert_eq!(s.check(T), SatResult::Sat);
    let m = s.model().expect("model");
    assert_eq!(
        m.get(q(7)),
        Some(Rat::from_i64(1)),
        "{}: int var forced to 1",
        s.backend_name()
    );
    s.pop();

    // --- multi-term atoms, Eq and Ne --------------------------------------
    s.push();
    s.assert(&atom2(1.0, 3, -1.0, 4, Rel::Eq, 0.0), None); // x = y
    s.assert(&atom(3, Rel::Eq, 2.5), None);
    assert_eq!(s.check(T), SatResult::Sat);
    let m = s.model().expect("model");
    assert_eq!(
        m.get(q(4)),
        Some(r(2.5)),
        "{}: equality propagates",
        s.backend_name()
    );
    s.assert(&atom(4, Rel::Ne, 2.5), None);
    assert_eq!(
        s.check(T),
        SatResult::Unsat,
        "{}: Ne contradicts",
        s.backend_name()
    );
    s.pop();

    // --- rational coefficients are exact, not float-fuzzy -------------------
    s.push();
    // 0.1 * x >= 1  and  x < 10  is UNSAT iff 0.1 is treated as 1/10.
    s.assert(
        &QFormula::Atom(LinAtom::new([(r(0.1), q(5))], Rel::Ge, r(1.0))),
        None,
    );
    s.assert(&atom(5, Rel::Lt, 10.0), None);
    assert_eq!(s.check(T), SatResult::Unsat, "{}", s.backend_name());
    s.pop();

    // --- timeout behavior: returns, never hangs -----------------------------
    s.push();
    s.assert(&atom(6, Rel::Ge, 0.0), None);
    let r = s.check(Duration::from_millis(1));
    assert!(
        matches!(r, SatResult::Sat | SatResult::Unsat | SatResult::Unknown(_)),
        "{}: tiny timeout must still return a SatResult",
        s.backend_name()
    );
    s.pop();

    // --- model completion: declared-but-unconstrained quantities get values --
    s.declare(q(9), QSort::Real);
    s.push();
    s.assert(&atom(0, Rel::Ge, 0.0), None);
    assert_eq!(s.check(T), SatResult::Sat);
    let m = s.model().expect("model");
    assert!(
        m.get(q(9)).is_some(),
        "{}: completion values q9",
        s.backend_name()
    );
    s.pop();
}

#[cfg(feature = "native")]
#[test]
fn native_backend_passes_battery() {
    let mut s = adl_solver::NativeSolver::new();
    battery(&mut s);
}

#[test]
fn subprocess_backend_passes_battery() {
    if !subprocess_available("z3") {
        eprintln!("SKIP: no z3 binary on PATH (subprocess conformance)");
        return;
    }
    let mut s = SubprocessSolver::z3();
    battery(&mut s);
}

/// Audit Bug 5 regression: an `(error …)` line in the solver output must
/// make the check `Unknown` — never Sat/Unsat from a partial script.
#[test]
fn subprocess_error_output_is_unknown_not_weaker() {
    if !subprocess_available("z3") {
        eprintln!("SKIP: no z3 binary on PATH (error injection)");
        return;
    }
    let mut s = SubprocessSolver::z3();
    // A query that would be plain `sat` if the malformed assert were
    // silently dropped (exactly the legacy failure shape).
    s.assert(&atom(0, Rel::Ge, 0.0), None);
    s.inject_raw("(assert (this_is_not_a_function q0))");
    let r = s.check(T);
    assert!(
        matches!(r, SatResult::Unknown(_)),
        "(error) output must be Unknown, got {r:?}"
    );
}

/// The Bug-5 error verdict is **sticky**: every check made while the
/// offending command is still on the frame stack comes back `Unknown`, not
/// just the first one. This is the property that would break under a
/// stream-the-deltas backend, where the solver reports the bad command once
/// and every later check silently answers a script it quietly dropped a
/// command from (the legacy false-PROVEN shape).
#[test]
fn subprocess_error_stays_unknown_until_the_frame_pops() {
    if !subprocess_available("z3") {
        eprintln!("SKIP: no z3 binary on PATH (sticky error)");
        return;
    }
    let mut s = SubprocessSolver::z3();
    s.assert(&atom(0, Rel::Ge, 0.0), None);
    s.push();
    s.inject_raw("(assert (this_is_not_a_function q0))");
    for i in 0..3 {
        let r = s.check(T);
        assert!(matches!(r, SatResult::Unknown(_)), "check {i}: {r:?}");
        assert!(
            r.is_solver_error(),
            "check {i} must count as a solver error: {r:?}"
        );
    }
    s.pop();
    assert_eq!(
        s.check(T),
        SatResult::Sat,
        "popping the frame removes the bad command"
    );
}

/// A child that dies mid-run degrades that one query to `Unknown` (accounted
/// as a process failure — never a verdict), then the next query respawns and
/// replays the recorded frame stack: same declarations, same named asserts,
/// same answers.
#[test]
fn subprocess_survives_child_death_and_replays_state() {
    if !subprocess_available("z3") {
        eprintln!("SKIP: no z3 binary on PATH (child death)");
        return;
    }
    let mut s = SubprocessSolver::z3();
    s.declare(q(7), QSort::Int);
    s.push();
    s.assert(&atom(0, Rel::Gt, 1.0), name("lo"));
    s.assert(&atom(0, Rel::Lt, 3.0), name("hi"));
    assert_eq!(s.check(T), SatResult::Sat);

    assert!(s.kill_child_for_test(), "there was a live child");
    let dead = s.check(T);
    assert!(
        dead.is_process_failure(),
        "a dead child is a process failure, not a verdict: {dead:?}"
    );
    assert!(s.model().is_none(), "no model to fetch from a dead child");

    // Respawn + replay: the state is exactly what it was before the death.
    assert_eq!(s.check(T), SatResult::Sat, "replayed state is still sat");
    let m = s.model().expect("model after replay");
    let v = m.get(q(0)).expect("q0 valued");
    assert!(v > r(1.0) && v < r(3.0), "replayed model in (1,3): {v:?}");
    assert!(
        m.get(q(7)).is_some(),
        "a quantity declared before the death is declared again"
    );
    s.assert(&atom(0, Rel::Gt, 5.0), None);
    assert_eq!(s.check(T), SatResult::Unsat, "replayed asserts still bite");
    let core = s.unsat_core().expect("core after replay");
    assert!(core.contains(&AssertName::new("hi")), "{core:?}");
    s.pop();
}

/// `pop` drops a frame's assertions but never its declarations: the backend's
/// declaration set is global and monotone, and it is what `model()` asks for.
/// A quantity first mentioned inside a popped frame must therefore still be
/// valued afterwards — otherwise the next `(get-value …)` hits `unknown
/// constant` and every model silently disappears.
#[test]
fn subprocess_keeps_declarations_from_popped_frames() {
    if !subprocess_available("z3") {
        eprintln!("SKIP: no z3 binary on PATH (declaration scope)");
        return;
    }
    let mut s = SubprocessSolver::z3();
    s.push();
    s.assert(&atom(4, Rel::Gt, 2.0), None); // q4 first mentioned inside the frame
    assert_eq!(s.check(T), SatResult::Sat);
    s.pop();
    s.assert(&atom(0, Rel::Ge, 0.0), None);
    assert_eq!(s.check(T), SatResult::Sat);
    let m = s.model().expect("model after pop");
    assert!(
        m.get(q(4)).is_some(),
        "q4 must survive the pop that removed its frame"
    );
}

/// A missing solver binary degrades to `Unknown`, never panics.
#[test]
fn subprocess_missing_binary_is_unknown() {
    let mut s = SubprocessSolver::with_command("definitely-not-a-solver-binary-xyz");
    s.assert(&atom(0, Rel::Ge, 0.0), None);
    let r = s.check(T);
    assert!(matches!(r, SatResult::Unknown(_)), "{r:?}");
    assert!(s.model().is_none());
    assert!(s.unsat_core().is_none());
}

/// Both backends answer the deterministic battery identically; spot-check
/// the answers side by side on a fixed query list.
#[cfg(feature = "native")]
#[test]
fn backends_agree_on_fixed_queries() {
    if !subprocess_available("z3") {
        eprintln!("SKIP: no z3 binary on PATH (agreement)");
        return;
    }
    let queries: Vec<(Vec<QFormula>, SatResult)> = vec![
        (
            vec![atom(0, Rel::Gt, 0.0), atom(0, Rel::Lt, 1.0)],
            SatResult::Sat,
        ),
        (
            vec![atom(0, Rel::Gt, 1.0), atom(0, Rel::Lt, 1.0)],
            SatResult::Unsat,
        ),
        (
            vec![
                atom2(1.0, 0, 1.0, 1, Rel::Le, 1.0),
                atom(0, Rel::Ge, 2.0),
                atom(1, Rel::Ge, 0.0),
            ],
            SatResult::Unsat,
        ),
        (
            vec![
                QFormula::Or(vec![atom(0, Rel::Lt, 0.0), atom(0, Rel::Gt, 5.0)]),
                atom(0, Rel::Eq, 6.0),
            ],
            SatResult::Sat,
        ),
    ];
    let mut native = adl_solver::NativeSolver::new();
    let mut sub = SubprocessSolver::z3();
    for (i, (asserts, expected)) in queries.iter().enumerate() {
        for s in [&mut native as &mut dyn Solver, &mut sub as &mut dyn Solver] {
            s.push();
            for a in asserts {
                s.assert(a, None);
            }
            let got = s.check(T);
            assert_eq!(&got, expected, "query {i} on {}", s.backend_name());
            s.pop();
        }
    }
}
