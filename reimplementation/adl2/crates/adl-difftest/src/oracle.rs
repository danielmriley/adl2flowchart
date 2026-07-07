//! Sampling oracle (TESTING.md §2): run a generated case through both
//! the verifier ([`adl_analysis::analyze_source`]) and the reference
//! interpreter over a shared deterministic event sample, then check the
//! soundness contract:
//!
//! - PROVEN DISJOINT ⇒ no sampled event passes both regions;
//! - PROVEN OVERLAPPING ⇒ the engine's witness event was accepted by the
//!   interpreter in both regions (`witness_validated == Some(true)`; the
//!   vocabulary contains no opaque quantities, so a candidate-only
//!   witness is also a failure);
//! - PROVEN SUBSET ⇒ no sampled counterexample;
//! - REGION EMPTY ⇒ no sampled member.
//!
//! The event sample = boundary grid + seeded random vocabulary events +
//! toy-generator events (with `btag` injected for electrons and forced
//! 0-element collection variants). All samples stay inside the axiom
//! catalog's physical-event class: collections pT-descending, tags and
//! triggers ∈ {0, 1}, `pt`/`m`/`HT`/`MET` ≥ 0, `dPhi` computed wrapped.

use crate::{SplitMix64, toy_jsonl};
use adl_analysis::{AnalysisOptions, EmptyStatus, Report, VerdictKind, analyze_source};
use adl_interp::{Event, Interp, parse_event};
use adl_sema::{ExtDecls, analyze_str};
use serde_json::{Map, Value};
use std::fmt::Write as _;

/// One analyzed + interpreted case.
pub struct CaseRun {
    pub report: Report,
    /// Per sampled event: (passes RA, passes RB) — by region *name*.
    pub passes: Vec<(bool, bool)>,
}

/// Run one rendered case end to end.
///
/// # Errors
/// Frontend errors and interpreter evaluation errors are generator bugs
/// (the vocabulary must stay inside the checked fragment): both come
/// back as `Err` so the property test fails loudly.
pub fn run_case(
    src: &str,
    ext: &ExtDecls,
    events: &[Event],
    opts: &AnalysisOptions,
) -> Result<CaseRun, String> {
    let hir = analyze_str(src, "generated.adl", ext);
    if adl_syntax::diag::has_errors(&hir.diags) {
        return Err(format!(
            "generated source failed the frontend:\n{}",
            adl_syntax::diag::render(src, "generated.adl", &hir.diags)
        ));
    }
    let interp = Interp::new(&hir, ext);
    let mut passes = Vec::with_capacity(events.len());
    for (i, e) in events.iter().enumerate() {
        let a = interp
            .eval_region_by_name("RA", e)
            .map_err(|err| format!("event {i}: interpreter error in RA: {}", err.reason))?;
        let b = interp
            .eval_region_by_name("RB", e)
            .map_err(|err| format!("event {i}: interpreter error in RB: {}", err.reason))?;
        passes.push((a, b));
    }
    let report = analyze_source(src, "generated.adl", ext, opts)
        .map_err(|e| format!("analysis frontend error:\n{e}"))?;
    Ok(CaseRun { report, passes })
}

/// Index of a region's membership column by report name (`RA` → 0).
fn col(name: &str) -> Result<usize, String> {
    match name {
        "RA" => Ok(0),
        "RB" => Ok(1),
        other => Err(format!("unexpected region name `{other}` in report")),
    }
}

fn pass_of(p: &(bool, bool), idx: usize) -> bool {
    if idx == 0 { p.0 } else { p.1 }
}

/// Check the encoder-vs-interpreter soundness contract on one run.
///
/// # Errors
/// Returns a description of the violated property — every `Err` is a
/// REAL engine bug by the soundness contract.
pub fn check_sound(run: &CaseRun) -> Result<(), String> {
    let pair = run
        .report
        .pairwise
        .first()
        .ok_or("report has no pairwise entry")?;
    let ia = col(&pair.a)?;
    let ib = col(&pair.b)?;

    match pair.kind {
        VerdictKind::ProvenDisjoint => {
            for (i, p) in run.passes.iter().enumerate() {
                if pass_of(p, ia) && pass_of(p, ib) {
                    return Err(format!(
                        "PROVEN DISJOINT, but sampled event {i} passes BOTH regions \
                         (reason: {})",
                        pair.reason
                    ));
                }
            }
        }
        VerdictKind::CandidateDisjoint => {
            // Not a claim — a solver-UNSAT the certifier could not verify.
            // Consistency: the tier only exists when certification RAN and
            // failed; a certified=true pair must never carry it.
            if pair.certified != Some(false) {
                return Err(format!(
                    "CANDIDATE DISJOINT but certified = {:?} (the tier means \
                     certification ran and could not verify; reason: {})",
                    pair.certified, pair.reason
                ));
            }
        }
        VerdictKind::ProvenOverlapping => {
            // The witness must have been re-validated through the
            // interpreter; the vocabulary has no opaque quantities, so
            // `Some(false)` (candidate-only) is a failure too.
            if pair.witness_validated != Some(true) {
                return Err(format!(
                    "PROVEN OVERLAPPING but witness_validated = {:?} (witness must pass \
                     both regions via the interpreter; reason: {})",
                    pair.witness_validated, pair.reason
                ));
            }
        }
        VerdictKind::CandidateOverlapping => {
            // Not a proof — a joint model that rests on an opaque quantity the
            // interpreter cannot decide (so witness_validated is Some(false)).
            // It makes no PROVEN claim, so there is nothing to refute. The
            // generator vocabulary is opaque-free, so this should be rare; if
            // it ever appears with witness_validated == Some(true) the
            // labelling is wrong (a validated overlap must be ProvenOverlapping).
            if pair.witness_validated == Some(true) {
                return Err(format!(
                    "CANDIDATE OVERLAPPING but witness_validated = Some(true) — a \
                     validated overlap must be labelled PROVEN OVERLAPPING (reason: {})",
                    pair.reason
                ));
            }
        }
        VerdictKind::PossiblyOverlapping | VerdictKind::Unknown => {}
    }

    if pair.subset_a_in_b {
        for (i, p) in run.passes.iter().enumerate() {
            if pass_of(p, ia) && !pass_of(p, ib) {
                return Err(format!(
                    "PROVEN SUBSET {} ⊆ {}, but sampled event {i} is a counterexample",
                    pair.a, pair.b
                ));
            }
        }
    }
    if pair.subset_b_in_a {
        for (i, p) in run.passes.iter().enumerate() {
            if pass_of(p, ib) && !pass_of(p, ia) {
                return Err(format!(
                    "PROVEN SUBSET {} ⊆ {}, but sampled event {i} is a counterexample",
                    pair.b, pair.a
                ));
            }
        }
    }

    for r in &run.report.regions {
        if r.empty == EmptyStatus::Proven {
            let idx = col(&r.name)?;
            for (i, p) in run.passes.iter().enumerate() {
                if pass_of(p, idx) {
                    return Err(format!(
                        "REGION {} PROVEN EMPTY, but sampled event {i} is a member",
                        r.name
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Order-normalized verdict summary for the metamorphic battery
/// (witness values / reasons / cores are explicitly NOT compared —
/// verdicts must be invariant, explanations may differ in wording).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub kind: VerdictKind,
    pub ra_in_rb: bool,
    pub rb_in_ra: bool,
    pub empty_ra: EmptyStatus,
    pub empty_rb: EmptyStatus,
}

impl Summary {
    /// Metamorphic consistency between two renderings of the same case.
    /// Every soundness-bearing fact must match exactly — DISJOINT, EMPTY,
    /// and the subset flags are all UNSAT-derived and deterministic. Two
    /// tolerated differences, both "proof strength", never truth:
    ///
    /// - PROVEN vs CANDIDATE vs POSSIBLY OVERLAPPING: whether an overlap's
    ///   witness is *realized* is a property of the heuristic event builder
    ///   (witness.rs — "soundness never depends on the builder"), so it can
    ///   legitimately differ with the solver's model / region order.
    /// - PROVEN vs CANDIDATE DISJOINT: the UNSAT itself is deterministic,
    ///   but the *certificate* search runs on the solver's minimized core,
    ///   and core CHOICE is not invariant under statement inlining — a
    ///   paste-variant core of two small facts can certify while the
    ///   inherit-variant core (one monolithic region-reference conjunction)
    ///   exceeds the case-split budget. CANDIDATE is the honest downgrade.
    ///
    /// The strict interpreter-membership check in the battery remains the
    /// real net.
    #[must_use]
    pub fn consistent(&self, other: &Summary) -> bool {
        let overlapping = |k: VerdictKind| {
            matches!(
                k,
                VerdictKind::ProvenOverlapping
                    | VerdictKind::CandidateOverlapping
                    | VerdictKind::PossiblyOverlapping
            )
        };
        let disjoint = |k: VerdictKind| {
            matches!(
                k,
                VerdictKind::ProvenDisjoint | VerdictKind::CandidateDisjoint
            )
        };
        let kind_ok = self.kind == other.kind
            || (overlapping(self.kind) && overlapping(other.kind))
            || (disjoint(self.kind) && disjoint(other.kind));
        kind_ok
            && self.ra_in_rb == other.ra_in_rb
            && self.rb_in_ra == other.rb_in_ra
            && self.empty_ra == other.empty_ra
            && self.empty_rb == other.empty_rb
    }
}

/// Extract the normalized summary (regions keyed by name, pair oriented
/// RA→RB regardless of declaration order).
///
/// # Errors
/// Returns a description when the report shape is unexpected.
pub fn summary(report: &Report) -> Result<Summary, String> {
    let pair = report
        .pairwise
        .first()
        .ok_or("report has no pairwise entry")?;
    let (ra_in_rb, rb_in_ra) = if pair.a == "RA" {
        (pair.subset_a_in_b, pair.subset_b_in_a)
    } else {
        (pair.subset_b_in_a, pair.subset_a_in_b)
    };
    let empty_of = |name: &str| -> Result<EmptyStatus, String> {
        report
            .regions
            .iter()
            .find(|r| r.name == name)
            .map(|r| r.empty.clone())
            .ok_or_else(|| format!("region {name} missing from report"))
    };
    Ok(Summary {
        kind: pair.kind,
        ra_in_rb,
        rb_in_ra,
        empty_ra: empty_of("RA")?,
        empty_rb: empty_of("RB")?,
    })
}

// ---- deterministic event sample ---------------------------------------------

const ETA_CYCLE: &[f64] = &[-2.0, -1.0, -0.5, 0.0, 1.0, 2.0];
const PHI_CYCLE: &[f64] = &[-3.0, -1.5, 0.0, 1.5, 3.0];
const HT_CYCLE: &[f64] = &[0.0, 100.0, 400.0];

fn obj(pt: f64, eta: f64, phi: f64, btag: f64) -> Value {
    let mut o = Map::new();
    o.insert("pt".into(), pt.into());
    o.insert("eta".into(), eta.into());
    o.insert("phi".into(), phi.into());
    o.insert("m".into(), 1.0.into());
    o.insert("btag".into(), btag.into());
    Value::Object(o)
}

fn event_json(jets: &[Value], eles: &[Value], met: f64, met_phi: f64, ht: f64) -> String {
    let mut root = Map::new();
    root.insert("Jet".into(), Value::Array(jets.to_vec()));
    root.insert("Electron".into(), Value::Array(eles.to_vec()));
    let mut m = Map::new();
    m.insert("pt".into(), met.into());
    m.insert("phi".into(), met_phi.into());
    root.insert("MET".into(), Value::Object(m));
    root.insert("HT".into(), ht.into());
    Value::Object(root).to_string()
}

/// Boundary grid: every pool constant appears exactly as an event value,
/// collection sizes run 0..=3, equal-pT ties included.
fn grid_jsonl() -> Vec<String> {
    const JET_CFG: &[&[f64]] = &[
        &[],
        &[100.0],
        &[400.0, 25.0],
        &[200.0, 100.0, 50.0],
        &[25.0, 25.0],
        &[0.0],
    ];
    const ELE_CFG: &[&[f64]] = &[&[], &[50.0], &[100.0, 0.0]];
    const METS: &[f64] = &[0.0, 50.0, 200.0, 500.0];
    let mut out = Vec::new();
    let mut k = 0usize;
    for jets in JET_CFG {
        for eles in ELE_CFG {
            for &met in METS {
                let jet_objs: Vec<Value> = jets
                    .iter()
                    .enumerate()
                    .map(|(i, &pt)| {
                        obj(
                            pt,
                            ETA_CYCLE[(k + i) % ETA_CYCLE.len()],
                            PHI_CYCLE[(k + i) % PHI_CYCLE.len()],
                            f64::from(u8::try_from((k + i) % 2).unwrap_or(0)),
                        )
                    })
                    .collect();
                let ele_objs: Vec<Value> = eles
                    .iter()
                    .enumerate()
                    .map(|(i, &pt)| {
                        obj(
                            pt,
                            ETA_CYCLE[(k + 2 * i + 3) % ETA_CYCLE.len()],
                            PHI_CYCLE[(k + i + 2) % PHI_CYCLE.len()],
                            f64::from(u8::try_from((k + i + 1) % 2).unwrap_or(0)),
                        )
                    })
                    .collect();
                out.push(event_json(
                    &jet_objs,
                    &ele_objs,
                    met,
                    PHI_CYCLE[k % PHI_CYCLE.len()],
                    HT_CYCLE[k % HT_CYCLE.len()],
                ));
                k += 1;
            }
        }
    }
    out
}

/// Seeded random events with values clustered around the constant pools
/// (jitter keeps strict/non-strict boundaries distinguishable).
fn random_jsonl(seed: u64, n: usize) -> Vec<String> {
    const JITTER: &[f64] = &[0.0, 0.5, -0.5, 7.0, -7.0];
    let mut rng = SplitMix64::new(seed);
    let mut out = Vec::new();
    for _ in 0..n {
        let coll = |max_n: usize, rng: &mut SplitMix64| -> Vec<Value> {
            let n = rng.below(max_n + 1);
            let mut pts: Vec<f64> = (0..n)
                .map(|_| {
                    let base = crate::casegen::PT_POOL[rng.below(crate::casegen::PT_POOL.len())];
                    (base + JITTER[rng.below(JITTER.len())]).max(0.0)
                })
                .collect();
            pts.sort_by(|a, b| b.total_cmp(a));
            pts.into_iter()
                .map(|pt| {
                    let eta = crate::casegen::ETA_POOL[rng.below(crate::casegen::ETA_POOL.len())]
                        + 0.1 * JITTER[rng.below(JITTER.len())];
                    let phi = rng.in_range(-std::f64::consts::PI, std::f64::consts::PI);
                    obj(pt, eta, phi, f64::from(rng.flag()))
                })
                .collect()
        };
        let jets = coll(3, &mut rng);
        let eles = coll(3, &mut rng);
        let met = (crate::casegen::PT_POOL[rng.below(crate::casegen::PT_POOL.len())]
            + JITTER[rng.below(JITTER.len())])
        .max(0.0);
        let ht = (crate::casegen::PT_POOL[rng.below(crate::casegen::PT_POOL.len())]
            + JITTER[rng.below(JITTER.len())])
        .max(0.0);
        out.push(event_json(
            &jets,
            &eles,
            met,
            rng.in_range(-std::f64::consts::PI, std::f64::consts::PI),
            ht,
        ));
    }
    out
}

/// Toy-generator events adapted to the vocabulary: electrons get an
/// injected `btag` flag (the vocabulary promises `pt`/`eta`/`btag` on
/// both collections), and the first events also yield variants with
/// forced 0-element collections.
fn toy_derived_jsonl(seed: u64, n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut flag = SplitMix64::new(seed ^ 0xBADC_0FFE);
    for (i, line) in toy_jsonl(seed, n).lines().enumerate() {
        let mut v: Value = serde_json::from_str(line).expect("toy generator emits valid JSON");
        if let Some(Value::Array(eles)) = v.get_mut("Electron") {
            for e in eles {
                if let Value::Object(o) = e {
                    o.insert("btag".into(), f64::from(flag.flag()).into());
                }
            }
        }
        out.push(v.to_string());
        if i < 8 {
            let mut nj = v.clone();
            nj["Jet"] = Value::Array(Vec::new());
            out.push(nj.to_string());
        } else if i < 16 {
            let mut ne = v.clone();
            ne["Electron"] = Value::Array(Vec::new());
            out.push(ne.to_string());
        }
    }
    out
}

/// The shared deterministic event sample (grid + seeded random + toy).
///
/// # Panics
/// Panics if a generated record fails the interpreter's loader — that
/// would be a sampler bug.
#[must_use]
pub fn sample_events(ext: &ExtDecls) -> Vec<Event> {
    let mut lines = grid_jsonl();
    lines.extend(random_jsonl(0xD1FF_7E57, 64));
    lines.extend(toy_derived_jsonl(7, 24));
    lines
        .iter()
        .map(|l| {
            parse_event(l, ext).unwrap_or_else(|e| panic!("sampler emitted a bad event: {e}\n{l}"))
        })
        .collect()
}

/// Render a failure with its full ADL source attached (the message a
/// human needs to reproduce/minimize).
#[must_use]
pub fn with_source(err: &str, src: &str) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{err}");
    let _ = writeln!(s, "--- generated ADL source ---");
    s.push_str(src);
    s
}
