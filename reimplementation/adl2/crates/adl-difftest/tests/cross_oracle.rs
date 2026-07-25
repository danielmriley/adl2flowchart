//! Property-based oracle over the CROSS-FILE (merged-unit) path — the route
//! `verify --cross` takes: resolve N units, [`merge_hirs`] them into one
//! structural identity space, then prove region relations ACROSS files with
//! reconciliation on.
//!
//! Until now nothing randomized reached here. The single-unit battery
//! (`prop_encoder_vs_interp`) never calls the merger, and `prop_reconcile_oracle`
//! exercises reconciliation only INTRA-unit; the merge path itself was covered
//! by hand-picked scenarios (`adl-analysis/tests/cross_file.rs`) alone — which
//! is how a merge-identity bug survived a year: [`Merger::remap_pred`] interned
//! element predicates by render-string dedup WITHOUT the fail-closed
//! `has_unsupported()` branch, so two units' physically different opaque cuts
//! (whose lossy `<unsupported: …>` renders collide) collapsed into ONE
//! collection and ONE size quantity, and `size(o1) >= 3` vs `size(o2) <= 1`
//! "proved" disjoint.
//!
//! Two independent nets run on every generated pair:
//!
//! 1. **Structural (no solver, always runs).** After the merge, an element
//!    predicate whose node `has_unsupported()` must never be shared by two
//!    Filtered collections — the exact invariant whose absence produced the
//!    bug above. This is the only net that reaches the original instance: a
//!    cut like `sum(pt(Muon) + pt(Electron)) > 5` is unevaluable for the
//!    interpreter too, so no differential check can refute a proof built on it.
//! 2. **Differential (interpreter vs report).** Every PROVEN verdict in the
//!    merged report is refuted against a shared event sample plus per-case
//!    events: PROVEN DISJOINT ⇒ no event decidably in both regions; PROVEN
//!    OVERLAPPING ⇒ `witness_validated == Some(true)`; PROVEN EMPTY ⇒ no
//!    decidable member; PROVEN SUBSET ⇒ no event in A and decidably out of B.
//!    Membership is three-valued exactly as the engine's own sampling gate
//!    treats it (`eval_region_membership_idx(..).ok()`): `None` (opaque) is
//!    skipped, never counted as a pass or a fail.
//!
//! The generator aims at the merge's identity seams: shared bases (`Jet`/`Ele`
//! aliases in both units, so reconciliation has candidates), same-named objects
//! carrying different cuts, same-named regions (so unit prefixing is
//! exercised), exact cut chains that make XSUB/XEQ fire, reducer/undeclared-call
//! cuts that must stay opaque and fail closed, unions and slices. Region
//! statements combine size cuts over those objects — biased "high" in the first
//! unit and "low" in the second, the shape that turns an identity error into a
//! PROVEN DISJOINT — with conditions drawn from the existing single-unit
//! vocabulary ([`adl_difftest::casegen::arb_cond`]), which is
//! interpreter-evaluable by construction.
//!
//! A third check rides along: the engine's OWN sampling gate must never have
//! fired (`report.sampling.refutations == 0`). A gate refutation is the engine
//! saying "an encoder/axiom fact is false on a real event" about itself, and it
//! silently demotes the verdict — so without this check a bug that the gate
//! happens to catch would leave the oracle green. It is what caught the
//! mutation-test of a flipped XSUB direction.
//!
//! # Known event-class mismatch (found by this oracle, reported separately)
//!
//! The interpreter answers a **decidable `false`** for a property the event
//! does not carry: with a `btag`-less electron, `BTag(e) >= 0` and
//! `BTag(e) < 0` are BOTH `Ok(false)` — excluded middle fails, so such an
//! event is not a model of the axioms at all. The analyzer's TAG axiom mean-
//! while asserts `btag ∈ {0,1}` for every element that exists, and the engine's
//! gate battery ([`adl_interp::sample::battery`]) gives tags to `Jet`/`Tau`
//! only. A generated `btag` cut on electrons therefore makes the engine refute
//! its own correct claims, for a reason that has nothing to do with cross-file
//! identity. The generator keeps tag cuts on tagged collections
//! ([`in_event_class`]) — which is also what the physics says — instead of
//! weakening the check.
//!
//! Runs 400 pairs under plain `cargo test`; the `deep` feature raises that to
//! 8000 (`cargo test -p adl-difftest --features deep --test cross_oracle`).
//! `PROPTEST_CASES` overrides both. The case sequence is deterministic (a fixed
//! RNG seed), so a failure reproduces exactly.

use adl_analysis::report::{CoreItem, EmptyStatus, ReconOutcome, Report, VerdictKind};
use adl_analysis::{AnalysisOptions, SolverChoice, analyze_hir};
use adl_difftest::casegen::{
    GCond, GNum, GProp, GQuant, GRel, PT_POOL, RenderCtx, SIZE_POOL, arb_cond, cond_str, fmt_const,
};
use adl_difftest::{SplitMix64, toy_jsonl};
use adl_interp::{Event, Interp, parse_event};
use adl_sema::{Collection, ElemPredId, ExtDecls, Hir, analyze_str, merge_hirs};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use proptest::test_runner::{Config, RngAlgorithm, TestError, TestRng, TestRunner};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{LazyLock, Mutex, OnceLock};
use std::time::Duration;

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

/// The shared event sample, kept with the JSONL line each event came from so a
/// violation can print a runnable repro.
fn shared_events() -> &'static [(String, Event)] {
    static EVENTS: OnceLock<Vec<(String, Event)>> = OnceLock::new();
    EVENTS.get_or_init(|| {
        adl_difftest::oracle::sample_events(ext())
            .into_iter()
            .map(|e| (event_json(&e), e))
            .collect()
    })
}

const DEFAULT_CASES: u32 = if cfg!(feature = "deep") { 8_000 } else { 400 };

fn opts() -> AnalysisOptions {
    // Mirrors `verify --cross`: reconciliation on, certification on, the
    // default 64-event sampling gate, real solver.
    AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        reconcile: true,
        sample_gate: 64,
        certify: true,
        ..AnalysisOptions::default()
    }
}

// ---- generated units ---------------------------------------------------------

/// Object names both units draw from, so "same name, different cuts" — the
/// shape a merge must NOT unify — happens by construction.
const OBJ_NAMES: [&str; 2] = ["g", "h"];
/// Region names both units draw from (merged labels become `<unit>::<name>`).
const REG_NAMES: [&str; 2] = ["SR", "CR"];
/// Base detector types for filtered objects; the alias emitted for each.
const BASES: [(&str, &str); 2] = [("Jet", "jets"), ("Ele", "eles")];
/// Bodies with TWO plural collection references: the resolver's reason keeps
/// only the reducer kind and the reference count, so every one of these renders
/// as the SAME `<unsupported: …>` string while denoting a different cut. This
/// is the render-collision family the fail-closed interner exists for.
const LOSSY_BODIES: [&str; 4] = [
    "pt(Muon) + pt(Electron)",
    "eta(Photon) * eta(Tau)",
    "pt(Electron) - pt(Photon)",
    "eta(Muon) + eta(Tau)",
];
/// Thresholds for the lossy cuts. Small on purpose: the render that has to
/// collide is `<unsupported: …> > k`, so the constant must repeat often enough
/// for two different bodies to produce the SAME interning key.
const LOSSY_K: [f64; 2] = [5.0, 50.0];
/// The name of the twin object planted in both units by [`arb_pair`].
const TWIN_NAME: &str = "z";
/// The name of the planted refinement object (see [`Refine`]).
const REFINE_NAME: &str = "r";
/// Functions absent from the external library (opaque, distinct renders).
const UNDECLARED: [&str; 3] = ["fMTauTau", "fRazor", "fWpt"];
/// Non-negative |eta| acceptance thresholds.
const ABSETA_POOL: [f64; 3] = [1.0, 2.0, 2.4];
/// pT thresholds for OBJECT cuts. Deliberately lower than the region-level
/// [`PT_POOL`]: a `pt > 400` object is empty on almost every sampled event, and
/// an empty collection makes every size cut over it vacuous — the differential
/// net can only refute a disjointness between regions the sample actually
/// ENTERS, so the generator must keep collections populated.
const CUT_PT_POOL: [f64; 4] = [0.0, 25.0, 50.0, 100.0];

#[derive(Debug, Clone, Copy, PartialEq)]
enum ExactCut {
    Pt(GRel, f64),
    AbsEta(GRel, f64),
    PtBand(f64, f64),
    Btag(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ang {
    DR,
    DPhi,
}

#[derive(Debug, Clone, PartialEq)]
enum XCut {
    /// Fully resolved cuts: unify across units iff physically identical, and
    /// drive the reconciliation prover (XSUB/XEQ derived size facts).
    Exact(Vec<ExactCut>),
    /// `reject any|all(<ang>(b, <coll>) ⋈ c)` — the object-cleaning idiom.
    /// Interpreter-evaluable; opaque to the formula layer, so a region over it
    /// must never be falsely PROVEN, and reconciliation must SKIP it.
    Clean {
        all: bool,
        ang: Ang,
        against: u8,
        rel: GRel,
        c: f64,
    },
    /// `select sum(<two-plural-ref body>) > k` — out of fragment with a LOSSY
    /// reason: physically different bodies render identically.
    Lossy { body: u8, k: f64 },
    /// `select <undeclared fn>(pt) > k` — out of fragment, distinct renders.
    Undeclared { f: u8, k: f64 },
}

#[derive(Debug, Clone, PartialEq)]
enum XObj {
    Filtered { name: u8, base: u8, cut: XCut },
    Union { name: u8 },
    Slice { name: u8, base: u8, start: u8, len: u8 },
}

impl XObj {
    fn name(&self) -> &'static str {
        let i = match self {
            XObj::Filtered { name, .. } | XObj::Union { name } | XObj::Slice { name, .. } => *name,
        };
        OBJ_NAMES[i as usize % OBJ_NAMES.len()]
    }
}

#[derive(Debug, Clone, PartialEq)]
enum XStmt {
    /// `size(<obj>) ⋈ k` — what a wrong collection identity corrupts.
    Size { obj: u8, rel: GRel, k: f64 },
    /// A condition over the shared base vocabulary (MET/HT/`jets`/`eles`
    /// element props/sizes/angles), reused from the single-unit generator.
    Cond { reject: bool, cond: GCond },
    /// `any|all(pT(<obj>) ⋈ k)` — a reducer over a generated object.
    Reduce { obj: u8, all: bool, rel: GRel, k: f64 },
}

#[derive(Debug, Clone, PartialEq)]
struct XUnit {
    objs: Vec<XObj>,
    regions: Vec<Vec<XStmt>>,
    /// Which name each region takes from [`REG_NAMES`] (rotated), so the pair
    /// mixes same-named regions across units with distinct ones.
    reg_base: u8,
    /// A planted twin object (see [`arb_pair`]): `(body, k)` of a lossy cut
    /// plus the size relation the region asserts over it.
    twin: Option<(u8, f64, GRel, f64)>,
    /// A planted refinement object (see [`Refine`]).
    refine: Option<Refine>,
    /// A planted cut on a quantity SHARED with the other unit (see
    /// [`SHARED_QUANTS`]): `(quantity, relation, threshold)`.
    scalar: Option<(u8, GRel, f64)>,
}

/// Quantities both units can spell identically, so the merge has to unify them
/// for any cross-file verdict to exist at all. Cheap to satisfy, which is the
/// point: a region carrying one of these stays LIVE on the sample, so a
/// disjointness proof about it is one the interpreter could actually refute
/// (a proof involving a region no event enters is unfalsifiable by sampling).
const SHARED_QUANTS: [(&str, &[f64]); 4] = [
    ("MET.pt", &[50.0, 100.0, 200.0, 400.0]),
    ("HT", &[50.0, 100.0, 200.0, 400.0]),
    ("pT(jets[0])", &[25.0, 50.0, 100.0, 200.0]),
    ("size(jets)", &[1.0, 2.0, 3.0]),
];

/// One side of a planted refinement pair: both units define an object of the
/// same name over `Jet` with a `pt` cut (and sometimes an `abs(eta)` cut), so
/// the reconciliation prover gets a candidate it can actually DECIDE.
///
/// Nothing else in this generator reliably reaches the XSUB/XEQ derived size
/// facts, and those are the cross path's most dangerous inference: the engine
/// asserts `size(A) <= size(B)` into the solver off a proven refinement, so one
/// wrong refinement is a fabricated PROVEN DISJOINT — and, unlike the lossy
/// twin, both regions here stay interpreter-decidable, so the differential net
/// can refute it.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Refine {
    pt: f64,
    eta: Option<f64>,
    rel: GRel,
    k: f64,
}

/// One generated case: the two units that get merged. [`arb_pair`] fills in
/// the planted shapes; the twin plant in particular reproduces the historic
/// bug's own shape (both units define an object of the SAME name whose cut is
/// out of fragment with a *lossy* reason — different bodies, same rendered key
/// — and assert opposite size cuts over it). If the merge ever unifies those
/// two cuts again, `size(z) >= 2` and `size(z) <= 1` collapse onto one
/// quantity and the pair is "proven" disjoint; only the structural invariant
/// catches that, since neither region is interpreter-decidable on a non-empty
/// event.
#[derive(Debug, Clone, PartialEq)]
struct XPair {
    a: XUnit,
    b: XUnit,
}

/// Which side of the pair a unit is: the size cuts are biased in opposite
/// directions so a fabricated collection identity shows up as a contradiction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    High,
    Low,
}

// ---- rendering ---------------------------------------------------------------

fn rel_of(r: GRel) -> &'static str {
    r.as_str()
}

fn render_cut(cut: &XCut, out: &mut String) {
    match cut {
        XCut::Exact(cuts) => {
            for c in cuts {
                match c {
                    ExactCut::Pt(r, k) => {
                        let _ = writeln!(out, "  select pt {} {}", rel_of(*r), fmt_const(*k));
                    }
                    ExactCut::AbsEta(r, k) => {
                        let _ = writeln!(out, "  select abs(eta) {} {}", rel_of(*r), fmt_const(*k));
                    }
                    ExactCut::PtBand(lo, hi) => {
                        let _ = writeln!(out, "  select pt [] {} {}", fmt_const(*lo), fmt_const(*hi));
                    }
                    ExactCut::Btag(k) => {
                        let _ = writeln!(out, "  select btag == {}", fmt_const(*k));
                    }
                }
            }
        }
        XCut::Clean {
            all,
            ang,
            against,
            rel,
            c,
        } => {
            let f = match ang {
                Ang::DR => "dR",
                Ang::DPhi => "dPhi",
            };
            let red = if *all { "all" } else { "any" };
            let coll = BASES[*against as usize % BASES.len()].1;
            let _ = writeln!(
                out,
                "  reject {red}({f}(b, {coll}) {} {})",
                rel_of(*rel),
                fmt_const(*c)
            );
        }
        XCut::Lossy { body, k } => {
            let _ = writeln!(
                out,
                "  select sum({}) > {}",
                LOSSY_BODIES[*body as usize % LOSSY_BODIES.len()],
                fmt_const(*k)
            );
        }
        XCut::Undeclared { f, k } => {
            let _ = writeln!(
                out,
                "  select {}(pt) > {}",
                UNDECLARED[*f as usize % UNDECLARED.len()],
                fmt_const(*k)
            );
        }
    }
}

fn render_obj(o: &XObj, out: &mut String) {
    match o {
        XObj::Filtered { base, cut, .. } => {
            let ty = BASES[*base as usize % BASES.len()].0;
            let _ = writeln!(out, "object {}\n  take {ty} b", o.name());
            render_cut(cut, out);
        }
        XObj::Union { .. } => {
            let _ = writeln!(out, "object {}\n  take union(jets, eles)", o.name());
        }
        XObj::Slice { base, start, len, .. } => {
            let src = BASES[*base as usize % BASES.len()].1;
            let lo = start % 3;
            let hi = lo + 1 + (len % 2);
            let _ = writeln!(out, "object {}\n  take {src}[{lo}:{hi}]", o.name());
        }
    }
    out.push('\n');
}

fn render_stmt(s: &XStmt, objs: &[XObj], ctx: &RenderCtx, out: &mut String) {
    match s {
        XStmt::Size { obj, rel, k } => {
            let name = objs[*obj as usize % objs.len()].name();
            let _ = writeln!(out, "  select size({name}) {} {}", rel_of(*rel), fmt_const(*k));
        }
        XStmt::Cond { reject, cond } => {
            let kw = if *reject { "reject" } else { "select" };
            let _ = writeln!(out, "  {kw} {}", cond_str(cond, &[], &[], ctx));
        }
        XStmt::Reduce { obj, all, rel, k } => {
            let name = objs[*obj as usize % objs.len()].name();
            let red = if *all { "all" } else { "any" };
            let _ = writeln!(
                out,
                "  select {red}(pT({name}) {} {})",
                rel_of(*rel),
                fmt_const(*k)
            );
        }
    }
}

/// Render one unit. Both units always alias the same two detector bases, so
/// their collections live in a shared identity space after the merge.
fn render_unit(u: &XUnit) -> String {
    let ctx = RenderCtx::default();
    let mut out = String::from("object jets\n  take Jet\n\nobject eles\n  take Ele\n\n");
    for o in &u.objs {
        render_obj(o, &mut out);
    }
    if let Some((body, k, _, _)) = u.twin {
        let _ = writeln!(
            out,
            "object {TWIN_NAME}\n  take Jet b\n  select sum({}) > {}\n",
            LOSSY_BODIES[body as usize % LOSSY_BODIES.len()],
            fmt_const(k)
        );
    }
    if let Some(r) = u.refine {
        let _ = writeln!(
            out,
            "object {REFINE_NAME}\n  take Jet\n  select pt > {}",
            fmt_const(r.pt)
        );
        if let Some(e) = r.eta {
            let _ = writeln!(out, "  select abs(eta) < {}", fmt_const(e));
        }
        out.push('\n');
    }
    for (i, stmts) in u.regions.iter().enumerate() {
        // Distinct names within a unit (duplicates would make the merged
        // labels ambiguous and silently weaken the region-index mapping).
        let n = (usize::from(u.reg_base) + i) % REG_NAMES.len();
        let _ = writeln!(out, "region {}", REG_NAMES[n]);
        if i == 0
            && let Some((_, _, rel, k)) = u.twin
        {
            let _ = writeln!(
                out,
                "  select size({TWIN_NAME}) {} {}",
                rel_of(rel),
                fmt_const(k)
            );
        }
        if i == 0
            && let Some(r) = u.refine
        {
            let _ = writeln!(
                out,
                "  select size({REFINE_NAME}) {} {}",
                rel_of(r.rel),
                fmt_const(r.k)
            );
        }
        if i == 0
            && let Some((q, rel, k)) = u.scalar
        {
            let _ = writeln!(
                out,
                "  select {} {} {}",
                SHARED_QUANTS[q as usize % SHARED_QUANTS.len()].0,
                rel_of(rel),
                fmt_const(k)
            );
        }
        for s in stmts {
            render_stmt(s, &u.objs, &ctx, &mut out);
        }
        out.push('\n');
    }
    out
}

// ---- strategies --------------------------------------------------------------

fn arb_rel() -> impl Strategy<Value = GRel> {
    prop_oneof![
        3 => Just(GRel::Gt),
        3 => Just(GRel::Lt),
        2 => Just(GRel::Ge),
        2 => Just(GRel::Le),
        1 => Just(GRel::Eq),
    ]
}

fn arb_exact_cut() -> impl Strategy<Value = ExactCut> {
    prop_oneof![
        5 => (arb_rel(), proptest::sample::select(&CUT_PT_POOL[..]))
            .prop_map(|(r, k)| ExactCut::Pt(r, k)),
        3 => (arb_rel(), proptest::sample::select(&ABSETA_POOL[..]))
            .prop_map(|(r, k)| ExactCut::AbsEta(r, k)),
        2 => (proptest::sample::select(&CUT_PT_POOL[..]), proptest::sample::select(PT_POOL))
            .prop_map(|(a, b)| {
                let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
                ExactCut::PtBand(lo, hi)
            }),
        2 => proptest::sample::select(&[0.0f64, 1.0][..]).prop_map(ExactCut::Btag),
    ]
}

fn arb_cut() -> impl Strategy<Value = XCut> {
    prop_oneof![
        // Exact chains dominate: they are what reconciliation actually proves
        // refinements between, and both sides stay interpreter-decidable.
        10 => proptest::collection::vec(arb_exact_cut(), 1..=2).prop_map(XCut::Exact),
        // Analyzer-opaque but interpreter-DECIDABLE: the only opaque family a
        // differential check can refute, and a reconciliation must skip it.
        5 => (any::<bool>(), prop_oneof![2 => Just(Ang::DR), 1 => Just(Ang::DPhi)], 0u8..2, arb_rel())
            .prop_flat_map(|(all, ang, against, rel)| {
                let pool: &[f64] = match ang {
                    Ang::DR => &[0.0, 0.4, 1.0, 2.0],
                    Ang::DPhi => &[0.0, 1.5, 3.0],
                };
                proptest::sample::select(pool).prop_map(move |c| XCut::Clean {
                    all,
                    ang,
                    against,
                    rel,
                    c,
                })
            }),
        // Lossy-render opaque cuts: the fixed bug's own family.
        3 => (0u8..4, proptest::sample::select(&LOSSY_K[..]))
            .prop_map(|(body, k)| XCut::Lossy { body, k }),
        1 => (0u8..3, proptest::sample::select(PT_POOL))
            .prop_map(|(f, k)| XCut::Undeclared { f, k }),
    ]
}

/// One object. The name is a placeholder — [`arb_unit`] rewrites it to the
/// object's POSITION, so a unit never declares two objects of the same name
/// (which would leave one of them unreferenced) while the two units still
/// collide on `g`/`h` with different cuts.
fn arb_obj() -> impl Strategy<Value = XObj> {
    prop_oneof![
        8 => (0u8..2, arb_cut())
            .prop_map(|(base, cut)| XObj::Filtered { name: 0, base, cut: in_event_class(base, cut) }),
        1 => Just(XObj::Union { name: 0 }),
        1 => (0u8..2, 0u8..3, 0u8..2)
            .prop_map(|(base, start, len)| XObj::Slice { name: 0, base, start, len }),
    ]
}

/// Keep a cut inside the event class BOTH samplers emit. Electrons carry no
/// `btag` in the engine's own gate battery ([`adl_interp::sample::battery`]
/// gives tags to `Jet`/`Tau` only), and the interpreter answers a *decidable*
/// `false` — not `Unknown` — for an absent property, while the analyzer's TAG
/// axiom asserts `btag ∈ {0,1}` for any element that exists. A generated
/// `btag` cut on electrons therefore makes the two disagree for a reason that
/// has nothing to do with cross-file identity (see `missing property` note in
/// the module docs). Physics agrees: an electron has no b-tag.
fn in_event_class(base: u8, cut: XCut) -> XCut {
    if base.is_multiple_of(2) {
        return cut;
    }
    match cut {
        XCut::Exact(cuts) => {
            let kept: Vec<ExactCut> = cuts
                .into_iter()
                .filter(|c| !matches!(c, ExactCut::Btag(_)))
                .collect();
            XCut::Exact(if kept.is_empty() {
                vec![ExactCut::Pt(GRel::Gt, 0.0)]
            } else {
                kept
            })
        }
        other => other,
    }
}

fn set_name(o: &mut XObj, i: u8) {
    match o {
        XObj::Filtered { name, .. } | XObj::Union { name } | XObj::Slice { name, .. } => *name = i,
    }
}

/// A size cut biased by role: `High` units ask for MANY, `Low` units for FEW.
/// A wrong unification of the two units' collections then reads as a
/// contradiction on one quantity — a PROVEN DISJOINT the interpreter refutes.
fn arb_size(role: Role) -> impl Strategy<Value = XStmt> {
    // Thresholds stay small (1–2, not 3+) and `size < 0` is never generated:
    // a sampled event holds a handful of objects, and a size cut nothing can
    // satisfy makes the whole region vacuously empty — which proves disjoint
    // against everything and teaches the oracle nothing.
    let pairs: Vec<(GRel, f64)> = match role {
        Role::High => vec![
            (GRel::Ge, 1.0),
            (GRel::Ge, 1.0),
            (GRel::Ge, 2.0),
            (GRel::Gt, 0.0),
            (GRel::Gt, 1.0),
        ],
        Role::Low => vec![
            (GRel::Le, 0.0),
            (GRel::Le, 1.0),
            (GRel::Le, 1.0),
            (GRel::Lt, 1.0),
            (GRel::Lt, 2.0),
        ],
    };
    (
        0u8..2,
        prop_oneof![
            5 => proptest::sample::select(pairs),
            1 => (arb_rel(), proptest::sample::select(SIZE_POOL)),
        ],
    )
        .prop_map(|(obj, (rel, k))| XStmt::Size { obj, rel, k })
}

/// [`in_event_class`] for a borrowed single-unit condition: rewrite
/// `BTag(eles[i])` (collection slot 1) to `pT(eles[i])`. Same reason —
/// electrons carry no tag in the engine's gate battery.
fn in_class_quant(q: GQuant) -> GQuant {
    match q {
        GQuant::Elem {
            coll,
            idx,
            prop: GProp::Btag,
        } if coll % 2 == 1 => GQuant::Elem {
            coll,
            idx,
            prop: GProp::Pt,
        },
        other => other,
    }
}

fn in_class_num(n: GNum) -> GNum {
    let q = in_class_quant;
    match n {
        GNum::Q(a) => GNum::Q(q(a)),
        GNum::Add(a, b) => GNum::Add(q(a), q(b)),
        GNum::Sub(a, b) => GNum::Sub(q(a), q(b)),
        GNum::Scale(c, a) => GNum::Scale(c, q(a)),
        GNum::Min(a, b) => GNum::Min(q(a), q(b)),
        GNum::Max(a, b) => GNum::Max(q(a), q(b)),
        GNum::Ratio(a, b) => GNum::Ratio(q(a), q(b)),
        GNum::Mul(a, b) => GNum::Mul(q(a), q(b)),
        GNum::Sum3(a, b, c, assoc) => GNum::Sum3(q(a), q(b), q(c), assoc),
        GNum::QConst(a, c) => GNum::QConst(q(a), c),
        GNum::MinMaxTernary {
            is_max,
            elem,
            grel,
            c1,
            c2,
        } => GNum::MinMaxTernary {
            is_max,
            elem: q(elem),
            grel,
            c1,
            c2,
        },
    }
}

fn in_class_cond(c: GCond) -> GCond {
    let rec = |b: Box<GCond>| Box::new(in_class_cond(*b));
    match c {
        GCond::Cmp(n, r, k) => GCond::Cmp(in_class_num(n), r, k),
        GCond::BandIn(n, lo, hi) => GCond::BandIn(in_class_num(n), lo, hi),
        GCond::BandOut(n, lo, hi) => GCond::BandOut(in_class_num(n), lo, hi),
        GCond::And(a, b) => GCond::And(rec(a), rec(b)),
        GCond::Or(a, b) => GCond::Or(rec(a), rec(b)),
        GCond::Not(a) => GCond::Not(rec(a)),
        GCond::Ite(g, t, e) => GCond::Ite(rec(g), rec(t), e.map(rec)),
        // `OrdPair` is pT-only; the rest carry no element property.
        other => other,
    }
}

fn arb_stmt(role: Role) -> impl Strategy<Value = XStmt> {
    prop_oneof![
        4 => arb_size(role),
        4 => (prop_oneof![3 => Just(false), 1 => Just(true)], arb_cond(0, 0))
            .prop_map(|(reject, cond)| XStmt::Cond { reject, cond: in_class_cond(cond) }),
        1 => (0u8..2, any::<bool>(), arb_rel(), proptest::sample::select(PT_POOL))
            .prop_map(|(obj, all, rel, k)| XStmt::Reduce { obj, all, rel, k }),
    ]
}

fn arb_unit(role: Role) -> impl Strategy<Value = XUnit> {
    (
        proptest::collection::vec(arb_obj(), 1..=2),
        proptest::collection::vec(arb_stmt(role), 1..=2),
        prop_oneof![
            3 => Just(None),
            1 => proptest::collection::vec(arb_stmt(role), 1..=2).prop_map(Some),
        ],
        arb_size(role),
        0u8..2,
    )
        .prop_map(move |(mut objs, first, second, lead, reg_base)| {
            for (i, o) in objs.iter_mut().enumerate() {
                set_name(o, u8::try_from(i).unwrap_or(0));
            }
            // Every unit opens with a size cut over its first object, so the
            // merge's collection identity is always load-bearing for a verdict.
            let mut r0 = vec![lead];
            r0.extend(first);
            let mut regions = vec![r0];
            if let Some(r1) = second {
                regions.push(r1);
            }
            XUnit {
                objs,
                regions,
                reg_base,
                twin: None,
                refine: None,
                scalar: None,
            }
        })
}

/// The pair: two random units plus up to three PLANTED cross-file shapes, each
/// aimed at one thing purely random generation reaches too rarely.
///
/// - `twin` (~1 in 5): the historic bug's exact shape — same object name in
///   both units, cuts that are physically different but render to the SAME
///   lossy `<unsupported: …>` key, opposite size cuts over it.
/// - `refine` (~1 in 3): same-named `Jet` objects whose pt/eta cuts the
///   reconciliation prover can actually decide, so the derived XSUB/XEQ size
///   facts — the cross path's most dangerous inference — are exercised, with
///   both regions interpreter-decidable.
/// - `scalar` (~1 in 2): opposite-sense cuts on ONE quantity both units spell
///   identically, which keeps regions LIVE on the sample so a disjointness
///   proof about them is one the interpreter could refute.
fn arb_pair() -> impl Strategy<Value = XPair> {
    (
        arb_unit(Role::High),
        arb_unit(Role::Low),
        prop_oneof![
            4 => Just(None),
            1 => (0u8..4, 0u8..4, proptest::sample::select(&LOSSY_K[..]))
                .prop_map(|(ba, bb, k)| Some((ba, bb, k))),
        ],
        prop_oneof![
            2 => Just(None),
            1 => (
                proptest::sample::select(&CUT_PT_POOL[..]),
                proptest::sample::select(&CUT_PT_POOL[..]),
                proptest::option::of(proptest::sample::select(&ABSETA_POOL[..])),
                proptest::option::of(proptest::sample::select(&ABSETA_POOL[..])),
            )
                .prop_map(Some),
        ],
        prop_oneof![
            1 => Just(None),
            1 => (0u8..4).prop_flat_map(|q| {
                let pool = SHARED_QUANTS[q as usize].1;
                (
                    Just(q),
                    proptest::sample::select(pool),
                    proptest::sample::select(pool),
                )
            })
            .prop_map(Some),
        ],
    )
        .prop_map(|(mut a, mut b, twin, refine, scalar)| {
            if let Some((ba, bb, k)) = twin {
                a.twin = Some((ba, k, GRel::Ge, 2.0));
                b.twin = Some((bb, k, GRel::Le, 1.0));
            }
            if let Some((pa, pb, ea, eb)) = refine {
                // Independent pt/eta cuts on each side: the pair lands on
                // A ⊆ B, B ⊆ A, IDENTICAL, or genuinely unrelated, so the
                // reconciliation prover is exercised in all four outcomes.
                a.refine = Some(Refine {
                    pt: pa,
                    eta: ea,
                    rel: GRel::Ge,
                    k: 2.0,
                });
                // `size(r) >= 2` vs `size(r) <= 1` is the shape a derived
                // `size(A) <= size(B)` fact turns into PROVEN DISJOINT.
                b.refine = Some(Refine {
                    pt: pb,
                    eta: eb,
                    rel: GRel::Le,
                    k: 1.0,
                });
            }
            if let Some((q, hi, lo)) = scalar {
                // Opposite senses on ONE shared quantity: disjoint when
                // hi >= lo, overlapping otherwise — both outcomes wanted.
                a.scalar = Some((q, GRel::Gt, hi));
                b.scalar = Some((q, GRel::Lt, lo));
            }
            // A planted statement is the case's cross-file signal; keeping the
            // random statements alongside it in the same region piles up
            // conjuncts until no sampled event enters, and a region the sample
            // never enters cannot refute anything. Trim to one.
            for u in [&mut a, &mut b] {
                if (u.twin.is_some() || u.refine.is_some() || u.scalar.is_some())
                    && let Some(r0) = u.regions.first_mut()
                    && r0.len() > 2
                {
                    r0.truncate(2);
                }
                // Reference the LAST object, so no generated object is dead —
                // an unreferenced object never reaches the merged table, and
                // the structural invariant would never see its cut. The
                // reference is deliberately TRIVIAL (`>= 0`): it materializes
                // the collection without narrowing the region.
                if u.objs.len() > 1 {
                    let last = u8::try_from(u.objs.len() - 1).unwrap_or(0);
                    u.regions
                        .last_mut()
                        .expect("at least one region")
                        .push(XStmt::Size {
                            obj: last,
                            rel: GRel::Ge,
                            k: 0.0,
                        });
                }
            }
            XPair { a, b }
        })
}

// ---- events ------------------------------------------------------------------

fn json_num(v: f64) -> Value {
    serde_json::Number::from_f64(v).map_or(Value::Null, Value::Number)
}

/// Reconstruct a loader-valid JSONL line from a parsed event, so a violation
/// prints something that can be fed straight back to `smash2 run --events`.
fn event_json(e: &Event) -> String {
    let mut root = Map::new();
    for (name, objs) in &e.collections {
        let arr: Vec<Value> = objs
            .iter()
            .map(|o| {
                Value::Object(
                    o.properties()
                        .map(|(k, v)| (k.to_owned(), json_num(v)))
                        .collect(),
                )
            })
            .collect();
        root.insert(name.clone(), Value::Array(arr));
    }
    if !e.met.is_empty() {
        root.insert(
            "MET".into(),
            Value::Object(e.met.iter().map(|(k, &v)| (k.clone(), json_num(v))).collect()),
        );
    }
    for (k, &v) in &e.scalars {
        root.insert(k.clone(), json_num(v));
    }
    if !e.triggers.is_empty() {
        root.insert(
            "triggers".into(),
            Value::Object(
                e.triggers
                    .iter()
                    .map(|(k, &v)| (k.clone(), json_num(v)))
                    .collect(),
            ),
        );
    }
    Value::Object(root).to_string()
}

/// A deterministic seed for a case's own events (no clock, no entropy).
fn seed_of(a: &str, b: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in a.bytes().chain([0u8]).chain(b.bytes()) {
        h ^= u64::from(byte);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Per-case events: toy-generator records (electrons get the `btag` the
/// vocabulary promises, as in the single-unit sampler) plus forced-empty
/// collection variants, which are what most size cuts turn on.
fn case_events(seed: u64, n: usize) -> Vec<(String, Event)> {
    let mut flag = SplitMix64::new(seed ^ 0xBADC_0FFE);
    let mut out = Vec::new();
    for (i, line) in toy_jsonl(seed, n).lines().enumerate() {
        let mut v: Value = serde_json::from_str(line).expect("toy generator emits valid JSON");
        if let Some(Value::Array(eles)) = v.get_mut("Electron") {
            for e in eles {
                if let Value::Object(o) = e {
                    o.insert("btag".into(), f64::from(flag.flag()).into());
                }
            }
        }
        let push = |v: &Value, out: &mut Vec<(String, Event)>| {
            let line = v.to_string();
            let ev = parse_event(&line, ext())
                .unwrap_or_else(|e| panic!("case sampler emitted a bad event: {e}\n{line}"));
            out.push((line, ev));
        };
        push(&v, &mut out);
        if i % 3 == 0 {
            let mut nj = v.clone();
            nj["Jet"] = Value::Array(Vec::new());
            push(&nj, &mut out);
        }
    }
    out
}

// ---- the checks --------------------------------------------------------------

#[derive(Debug, Default, Clone)]
struct Census {
    cases: usize,
    /// Cut families reaching the merged units.
    exact: usize,
    clean: usize,
    lossy: usize,
    undeclared: usize,
    union_or_slice: usize,
    /// Cases where both units name an object the same but cut it differently.
    same_name_diff_cut: usize,
    /// Cases where both units declare the same region name.
    same_region_name: usize,
    /// Cases carrying the planted twin (lossy-render) object.
    twin_planted: usize,
    /// Cases carrying the planted refinement pair.
    refine_planted: usize,
    /// Cases carrying the planted shared-quantity cut.
    scalar_planted: usize,
    /// Merged units carrying at least one unsupported element predicate, and
    /// those where two DISTINCT unsupported preds share a render (i.e. the
    /// fail-closed branch was load-bearing on this very case).
    with_unsupported_pred: usize,
    lossy_render_collision: usize,
    /// Reconciliation ledger outcomes.
    recon_related: usize,
    recon_skipped: usize,
    recon_unrelated: usize,
    with_recon_rows: usize,
    /// Cases whose verdicts rest on a derived XSUB/XEQ size fact.
    cases_with_derived_fact: usize,
    /// Verdicts observed over all pairs.
    proven_disjoint: usize,
    proven_overlapping: usize,
    candidate_disjoint: usize,
    candidate_overlapping: usize,
    possibly: usize,
    unknown: usize,
    subsets: usize,
    proven_empty: usize,
    /// Region/event membership decidability (the differential net's reach).
    memb_decidable: usize,
    memb_total: usize,
    /// Pairs where BOTH regions were decidable on at least one event.
    pairs_with_reach: usize,
    pairs_total: usize,
    /// Regions the sample actually entered (a "live" region — an emptiness
    /// proof or a disjointness against an empty region is not refutable).
    regions_live: usize,
    regions_total: usize,
    /// PROVEN DISJOINT pairs whose two regions BOTH accepted a sampled event:
    /// the informative subset, where a fabricated proof would have been caught.
    disjoint_live: usize,
    /// Engine-internal diagnostics seen (a witness the interpreter rejected).
    internal_diags: usize,
    diag_samples: Vec<String>,
}

static CENSUS: LazyLock<Mutex<Census>> = LazyLock::new(|| Mutex::new(Census::default()));

/// The census, recovering from poisoning: a panic inside a property case is
/// reported by proptest, and a poison panic here would replace that message.
fn census_lock() -> std::sync::MutexGuard<'static, Census> {
    CENSUS.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn resolve(src: &str, unit: &str) -> Result<Hir, String> {
    let h = analyze_str(src, unit, ext());
    if adl_syntax::diag::has_errors(&h.diags) {
        return Err(format!(
            "generated unit `{unit}` failed the frontend:\n{}",
            adl_syntax::diag::render(src, unit, &h.diags)
        ));
    }
    Ok(h)
}

fn repro(msg: &str, ua: &str, ub: &str, event: Option<&str>) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{msg}");
    let _ = writeln!(s, "--- unit ua.adl ---\n{ua}");
    let _ = writeln!(s, "--- unit ub.adl ---\n{ub}");
    if let Some(e) = event {
        let _ = writeln!(s, "--- event (JSONL) ---\n{e}");
    }
    let _ = writeln!(
        s,
        "--- repro ---\nsmash2 verify --cross ua.adl ub.adl   (+ smash2 run --events ev.jsonl)"
    );
    s
}

/// CHECK (e): the merge-identity invariant — an element predicate that is out
/// of fragment gets a fresh, never-shared id, so two Filtered collections can
/// never rest on one unsupported cut. Structural: no solver, no events.
fn check_merge_identity(m: &Hir, census: &mut Census) -> Result<(), String> {
    let mut users: HashMap<ElemPredId, Vec<usize>> = HashMap::new();
    for (i, c) in m.table.collections().iter().enumerate() {
        if let Collection::Filtered { pred, .. } = c {
            users.entry(*pred).or_default().push(i);
        }
    }
    let mut unsupported: Vec<(ElemPredId, &str)> = Vec::new();
    for (&pred, colls) in &users {
        let p = &m.elem_preds[pred.0 as usize];
        if !p.node.has_unsupported() {
            continue;
        }
        unsupported.push((pred, p.render.as_str()));
        if colls.len() > 1 {
            return Err(format!(
                "MERGE IDENTITY VIOLATION: element predicate {pred:?} is out of fragment \
                 (`{}`) yet backs {} Filtered collections {colls:?} — two physically \
                 different opaque cuts were unified, which collapses their sizes into one \
                 quantity (the false-PROVEN factory fixed in ElemPredInterner)",
                p.render,
                colls.len()
            ));
        }
    }
    if !unsupported.is_empty() {
        census.with_unsupported_pred += 1;
        let mut by_render: HashMap<&str, usize> = HashMap::new();
        for (_, r) in &unsupported {
            *by_render.entry(r).or_default() += 1;
        }
        if by_render.values().any(|&n| n > 1) {
            census.lossy_render_collision += 1;
        }
    }
    Ok(())
}

/// Per-region membership over the sample: `Some(true)` in, `Some(false)` out,
/// `None` undecidable (opaque) — the engine's own three-valued treatment.
fn membership(m: &Hir, events: &[(String, Event)]) -> Vec<Vec<Option<bool>>> {
    let interp = Interp::new(m, ext());
    (0..m.regions.len())
        .map(|idx| {
            events
                .iter()
                .map(|(_, e)| interp.eval_region_membership_idx(idx, e).ok())
                .collect()
        })
        .collect()
}

/// Map a report region label back to its index in the merged unit. Ambiguity
/// is a hard error: a wrong index would silently check the wrong region.
fn region_index(m: &Hir, label: &str) -> Result<usize, String> {
    let mut found = None;
    for (i, s) in m.region_name_order.iter().enumerate() {
        if m.symbols.display(*s) == label {
            if found.is_some() {
                return Err(format!("region label `{label}` is ambiguous in the merged unit"));
            }
            found = Some(i);
        }
    }
    found.ok_or_else(|| format!("region `{label}` is missing from the merged unit"))
}

#[allow(clippy::too_many_lines)]
fn check_case(ua: &str, ub: &str, census: &mut Census) -> Result<(), String> {
    let a = resolve(ua, "ua.adl")?;
    let b = resolve(ub, "ub.adl")?;
    let mut m = merge_hirs(&[&a, &b]);

    // (e) structural, always — independent of the solver.
    check_merge_identity(&m, census).map_err(|e| repro(&e, ua, ub, None))?;

    let report: Report = analyze_hir(&mut m, "", ext(), &opts());
    census.internal_diags += report.internal_diagnostics.len();
    if census.diag_samples.len() < 5 {
        census.diag_samples.extend(
            report
                .internal_diagnostics
                .iter()
                .map(|d| d.chars().take(160).collect::<String>()),
        );
    }
    let mut derived = false;
    for row in &report.reconciliations {
        match row.outcome {
            ReconOutcome::Equivalent | ReconOutcome::ARefinesB | ReconOutcome::BRefinesA => {
                census.recon_related += 1;
                derived = true;
            }
            ReconOutcome::Skipped => census.recon_skipped += 1,
            ReconOutcome::Unrelated => census.recon_unrelated += 1,
        }
    }
    census.with_recon_rows += usize::from(!report.reconciliations.is_empty());
    census.cases_with_derived_fact += usize::from(derived);

    let mut events: Vec<(String, Event)> = shared_events().to_vec();
    events.extend(case_events(seed_of(ua, ub), 12));
    let memb = membership(&m, &events);
    let live: Vec<bool> = memb
        .iter()
        .map(|row| row.contains(&Some(true)))
        .collect();
    for (row, &l) in memb.iter().zip(&live) {
        census.memb_total += row.len();
        census.memb_decidable += row.iter().filter(|x| x.is_some()).count();
        census.regions_total += 1;
        census.regions_live += usize::from(l);
    }

    // (f) the engine's own sampling gate must never have had to fire: a
    // refutation there means an encoder/axiom fact is false on a real event.
    if let Some(s) = report.sampling
        && s.refutations > 0
    {
        return Err(repro(
            &format!(
                "the engine's sampling gate refuted {} of its own verdicts — an \
                 encoder/axiom fact is false on a real event (internal contradiction):\n{}",
                s.refutations,
                report.internal_diagnostics.join("\n")
            ),
            ua,
            ub,
            None,
        ));
    }

    for p in &report.pairwise {
        census.pairs_total += 1;
        let ia = region_index(&m, &p.a)?;
        let ib = region_index(&m, &p.b)?;
        let both_reach = (0..events.len())
            .any(|i| memb[ia][i].is_some() && memb[ib][i].is_some());
        census.pairs_with_reach += usize::from(both_reach);

        match p.kind {
            VerdictKind::ProvenDisjoint => {
                census.proven_disjoint += 1;
                census.disjoint_live += usize::from(live[ia] && live[ib]);
                // (a) no sampled event may be decidably IN both regions.
                for (i, (line, _)) in events.iter().enumerate() {
                    if memb[ia][i] == Some(true) && memb[ib][i] == Some(true) {
                        return Err(repro(
                            &format!(
                                "FALSE PROVEN DISJOINT across files: `{}` and `{}` are reported \
                                 PROVEN DISJOINT, but sampled event {i} is accepted by the \
                                 interpreter in BOTH.\nreason: {}\ncore: {:?}",
                                p.a,
                                p.b,
                                p.reason,
                                p.core.iter().map(CoreItem::human).collect::<Vec<_>>()
                            ),
                            ua,
                            ub,
                            Some(line),
                        ));
                    }
                }
            }
            VerdictKind::ProvenOverlapping => {
                census.proven_overlapping += 1;
                // (b) a PROVEN overlap carries an interpreter-validated witness.
                if p.witness_validated != Some(true) {
                    return Err(repro(
                        &format!(
                            "PROVEN OVERLAPPING for `{}` vs `{}` but witness_validated = {:?} \
                             (a proof requires the interpreter to accept the witness in both \
                             regions; reason: {})",
                            p.a, p.b, p.witness_validated, p.reason
                        ),
                        ua,
                        ub,
                        None,
                    ));
                }
            }
            VerdictKind::CandidateDisjoint => {
                census.candidate_disjoint += 1;
                // The tier exists only when certification RAN and failed.
                if p.certified != Some(false) {
                    return Err(repro(
                        &format!(
                            "CANDIDATE DISJOINT for `{}` vs `{}` but certified = {:?}",
                            p.a, p.b, p.certified
                        ),
                        ua,
                        ub,
                        None,
                    ));
                }
            }
            VerdictKind::CandidateOverlapping => {
                census.candidate_overlapping += 1;
                if p.witness_validated == Some(true) {
                    return Err(repro(
                        &format!(
                            "CANDIDATE OVERLAPPING for `{}` vs `{}` but witness_validated = \
                             Some(true) — a validated overlap must be PROVEN OVERLAPPING",
                            p.a, p.b
                        ),
                        ua,
                        ub,
                        None,
                    ));
                }
            }
            VerdictKind::PossiblyOverlapping => census.possibly += 1,
            VerdictKind::Unknown => census.unknown += 1,
        }

        // (d) subset claims: no event in the subset and decidably out of the
        // superset.
        let mut subset = |sub: usize, sup: usize, label: (&str, &str)| -> Result<(), String> {
            census.subsets += 1;
            for (i, (line, _)) in events.iter().enumerate() {
                if memb[sub][i] == Some(true) && memb[sup][i] == Some(false) {
                    return Err(repro(
                        &format!(
                            "FALSE PROVEN SUBSET across files: `{}` ⊆ `{}`, but sampled event \
                             {i} is accepted in the subset and REJECTED by the superset",
                            label.0, label.1
                        ),
                        ua,
                        ub,
                        Some(line),
                    ));
                }
            }
            Ok(())
        };
        if p.subset_a_in_b {
            subset(ia, ib, (&p.a, &p.b))?;
        }
        if p.subset_b_in_a {
            subset(ib, ia, (&p.b, &p.a))?;
        }
    }

    // (c) a region PROVEN EMPTY has no decidable member.
    for r in &report.regions {
        if r.empty == EmptyStatus::Proven {
            census.proven_empty += 1;
            let idx = region_index(&m, &r.name)?;
            for (i, (line, _)) in events.iter().enumerate() {
                if memb[idx][i] == Some(true) {
                    return Err(repro(
                        &format!(
                            "FALSE PROVEN EMPTY across files: region `{}` is reported empty, \
                             but sampled event {i} is a member",
                            r.name
                        ),
                        ua,
                        ub,
                        Some(line),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Generation-side census (no solver): which axes a rendered pair reaches.
fn census_shapes(p: &XPair, census: &mut Census) {
    let (a, b) = (&p.a, &p.b);
    for u in [a, b] {
        for o in &u.objs {
            match o {
                XObj::Filtered { cut, .. } => match cut {
                    XCut::Exact(_) => census.exact += 1,
                    XCut::Clean { .. } => census.clean += 1,
                    XCut::Lossy { .. } => census.lossy += 1,
                    XCut::Undeclared { .. } => census.undeclared += 1,
                },
                XObj::Union { .. } | XObj::Slice { .. } => census.union_or_slice += 1,
            }
        }
        census.lossy += usize::from(u.twin.is_some());
    }
    let same_name_diff_cut = a.twin.is_some()
        || a.objs.iter().any(|x| {
            b.objs
                .iter()
                .any(|y| x.name() == y.name() && cut_of(x) != cut_of(y))
        });
    census.same_name_diff_cut += usize::from(same_name_diff_cut);
    let names = |u: &XUnit| -> Vec<&'static str> {
        (0..u.regions.len())
            .map(|i| REG_NAMES[(usize::from(u.reg_base) + i) % REG_NAMES.len()])
            .collect()
    };
    let (na, nb) = (names(a), names(b));
    census.same_region_name += usize::from(na.iter().any(|x| nb.contains(x)));
    census.twin_planted += usize::from(a.twin.is_some());
    census.refine_planted += usize::from(a.refine.is_some());
    census.scalar_planted += usize::from(a.scalar.is_some());
    census.exact += usize::from(a.refine.is_some()) + usize::from(b.refine.is_some());
}

fn cut_of(o: &XObj) -> Option<&XCut> {
    match o {
        XObj::Filtered { cut, .. } => Some(cut),
        _ => None,
    }
}

// ---- the property ------------------------------------------------------------

#[test]
fn cross_file_verdicts_hold_on_sampled_events() {
    let mut config = Config::default(); // honors PROPTEST_CASES
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = DEFAULT_CASES;
    }
    config.failure_persistence = None; // counterexamples become explicit tests
    // Every shrink step re-runs a full merge + solver analysis (~0.2 s), so
    // shrinking is budgeted: enough to minimize a repro, not enough to turn a
    // find into a half-hour wait. The unminimized case is printed either way.
    config.max_shrink_iters = 256;
    config.max_shrink_time = 90_000;
    // Deterministic case sequence: same run, same pairs, exact reproduction.
    let mut runner = TestRunner::new_with_rng(
        config,
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    *census_lock() = Census::default();

    // The runner is single-threaded, so the census lock is never contended;
    // it exists only because `run` takes a `Fn`. A panic inside the closure
    // (proptest catches it and reports it as the failure) must not turn into
    // a poison panic here that hides the real message — hence the
    // `into_inner` recovery in `census_lock`.
    let result = runner.run(&arb_pair(), |pair| {
        let sa = render_unit(&pair.a);
        let sb = render_unit(&pair.b);
        let mut census = census_lock();
        census.cases += 1;
        census_shapes(&pair, &mut census);
        check_case(&sa, &sb, &mut census).map_err(TestCaseError::fail)
    });

    let census = census_lock().clone();
    println!("{}", render_census(&census));
    match result {
        Ok(()) => {}
        Err(TestError::Fail(reason, pair)) => {
            panic!(
                "cross-file oracle FAILED\n{reason}\n\n--- minimized case ---\n{}\n{}",
                render_unit(&pair.a),
                render_unit(&pair.b)
            );
        }
        Err(e) => panic!("cross-file oracle could not run: {e}"),
    }
}

fn render_census(c: &Census) -> String {
    let n = c.cases.max(1) as f64;
    let pct = |x: usize| 100.0 * x as f64 / n;
    let mut s = String::new();
    let _ = writeln!(s, "--- cross-oracle census over {} pairs ---", c.cases);
    let _ = writeln!(
        s,
        "cuts: exact={} clean={} lossy={} undeclared={} union/slice={}",
        c.exact, c.clean, c.lossy, c.undeclared, c.union_or_slice
    );
    let _ = writeln!(
        s,
        "axes: same-name-diff-cut {:.1}% | same region name {:.1}% | twin planted {:.1}% | \
         refine planted {:.1}% | shared-quantity planted {:.1}% | unsupported pred present \
         {:.1}% | lossy render COLLISION {:.1}% | recon rows {:.1}%",
        pct(c.same_name_diff_cut),
        pct(c.same_region_name),
        pct(c.twin_planted),
        pct(c.refine_planted),
        pct(c.scalar_planted),
        pct(c.with_unsupported_pred),
        pct(c.lossy_render_collision),
        pct(c.with_recon_rows)
    );
    let _ = writeln!(
        s,
        "recon: related={} unrelated={} skipped={} | cases resting on a derived XSUB/XEQ \
         fact {:.1}%",
        c.recon_related,
        c.recon_unrelated,
        c.recon_skipped,
        pct(c.cases_with_derived_fact)
    );
    let _ = writeln!(
        s,
        "verdicts: PROVEN DISJOINT={} PROVEN OVERLAPPING={} CAND DISJOINT={} \
         CAND OVERLAPPING={} POSSIBLY={} UNKNOWN={} | subsets={} proven-empty={}",
        c.proven_disjoint,
        c.proven_overlapping,
        c.candidate_disjoint,
        c.candidate_overlapping,
        c.possibly,
        c.unknown,
        c.subsets,
        c.proven_empty
    );
    let _ = writeln!(
        s,
        "reach: {:.1}% of (region, event) memberships decidable; {}/{} pairs with a \
         doubly-decidable event; {}/{} regions the sample entered; {}/{} PROVEN DISJOINT \
         pairs with BOTH regions live (the refutable subset)",
        100.0 * c.memb_decidable as f64 / c.memb_total.max(1) as f64,
        c.pairs_with_reach,
        c.pairs_total,
        c.regions_live,
        c.regions_total,
        c.disjoint_live,
        c.proven_disjoint
    );
    let _ = writeln!(s, "engine internal diagnostics: {}", c.internal_diags);
    for d in &c.diag_samples {
        let _ = writeln!(s, "  · {d}");
    }
    s
}

// ---- guarantees the oracle itself rests on -----------------------------------

/// The differential half of the oracle is vacuous without a solver backend, so
/// say so loudly instead of reporting a green structure-only run. Opt out with
/// `SMASH2_ALLOW_NO_SOLVER=1` (matches the golden battery's `assert_ne!`).
#[test]
fn cross_oracle_has_a_solver_backend() {
    if std::env::var_os("SMASH2_ALLOW_NO_SOLVER").is_some() {
        return;
    }
    let a = resolve("region SR\n  select MET.pt > 200\n", "ua.adl").unwrap();
    let b = resolve("region SR\n  select MET.pt < 50\n", "ub.adl").unwrap();
    let mut m = merge_hirs(&[&a, &b]);
    let r = analyze_hir(&mut m, "", ext(), &opts());
    assert_ne!(
        r.solver, "none",
        "the cross-file oracle needs a solver backend (set SMASH2_ALLOW_NO_SOLVER=1 \
         to run the structural invariant only)"
    );
    assert_eq!(
        r.pairwise[0].kind,
        VerdictKind::ProvenDisjoint,
        "sanity: MET>200 vs MET<50 must be proven disjoint across files"
    );
}

/// The repro lines the oracle prints must actually reproduce: an event
/// rebuilt from its parsed form loads again and decides regions identically.
#[test]
fn printed_event_repro_round_trips() {
    let src = "object jets\n  take Jet\n\nobject g\n  take Jet\n  select pt > 30\n\
               \n\nregion SR\n  select size(g) >= 1\n  select MET.pt > 50\n  \
               select pT(jets[0]) > 20\n";
    let h = resolve(src, "u.adl").unwrap();
    let interp = Interp::new(&h, ext());
    for (line, e) in shared_events() {
        let again = parse_event(line, ext())
            .unwrap_or_else(|err| panic!("rebuilt event does not load: {err}\n{line}"));
        assert_eq!(
            interp.eval_region_membership_idx(0, e).ok(),
            interp.eval_region_membership_idx(0, &again).ok(),
            "rebuilt event changed the verdict:\n{line}"
        );
    }
}

/// The generator must keep hitting the axes the merge path is fragile on;
/// a future weight change that starves one of them fails here instead of
/// silently narrowing the net. No solver: pure generation + resolve.
#[test]
fn generation_covers_the_merge_axes() {
    let mut runner = TestRunner::new_with_rng(
        Config::default(),
        TestRng::deterministic_rng(RngAlgorithm::ChaCha),
    );
    let strat = arb_pair();
    let mut census = Census::default();
    let n = 600;
    for _ in 0..n {
        let pair = strat.new_tree(&mut runner).unwrap().current();
        census.cases += 1;
        census_shapes(&pair, &mut census);
        let (sa, sb) = (render_unit(&pair.a), render_unit(&pair.b));
        let a = resolve(&sa, "ua.adl").unwrap_or_else(|e| panic!("{e}"));
        let b = resolve(&sb, "ub.adl").unwrap_or_else(|e| panic!("{e}"));
        let m = merge_hirs(&[&a, &b]);
        check_merge_identity(&m, &mut census).unwrap_or_else(|e| panic!("{e}"));
    }
    println!("{}", render_census(&census));
    let pct = |x: usize| 100.0 * x as f64 / n as f64;
    assert!(
        census.exact > 0
            && census.clean > 0
            && census.lossy > 0
            && census.undeclared > 0
            && census.union_or_slice > 0,
        "a cut family went missing: {census:?}"
    );
    assert!(
        pct(census.same_name_diff_cut) > 25.0,
        "same-name-different-cut objects too rare: {:.1}%",
        pct(census.same_name_diff_cut)
    );
    assert!(
        pct(census.with_unsupported_pred) > 20.0,
        "merged units with an out-of-fragment cut too rare: {:.1}%",
        pct(census.with_unsupported_pred)
    );
    assert!(
        pct(census.lossy_render_collision) > 2.0,
        "render collisions between DISTINCT unsupported cuts too rare ({:.1}%) — this is \
         the case class the fail-closed interner exists for",
        pct(census.lossy_render_collision)
    );
}
