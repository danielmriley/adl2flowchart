//! Witness extraction with interpreter re-validation (TESTING.md §3).
//!
//! Every SAT-direction proof is re-validated through the reference
//! interpreter: the solver model is converted to a synthetic event and
//! BOTH regions must accept it. This runs in production: a failed
//! validation downgrades the verdict to POSSIBLY and files an
//! internal-error diagnostic — the verifier can never display a witness
//! the interpreter rejects.
//!
//! When a region's membership depends on an opaque external-function
//! quantity, the interpreter (correctly) has no reference interpretation;
//! the witness then stays a **candidate** (SPEC_ANALYSIS §2 model
//! caveat) and the verdict keeps its printed caveat instead of failing.
//!
//! The event builder is a heuristic realizer (all elements of a base
//! collection are built to pass the full filter chain, sizes and element
//! properties pinned from the model); anything it cannot realize simply
//! fails validation — soundness never depends on the builder.

use adl_interp::Interp;
use adl_sema::{
    Collection, CollectionId, ElemIndex, ExtDecls, Fragment, HKind, HNode, Hir, Quantity,
    QuantityId, ScalarSource,
};
use adl_solver::Model;
use serde_json::{Map, Number, Value};
use std::collections::{BTreeMap, BTreeSet};

/// Largest collection the realizer will materialize.
const MAX_REALIZED: u64 = 64;
/// The same cap as an `f64`, for the engine's model-refinement hints.
pub(crate) const MAX_REALIZED_F: f64 = MAX_REALIZED as f64;

/// Outcome of witness re-validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Validation {
    /// The interpreter accepted the synthetic event in both regions. Carries
    /// the FINAL event JSON (post pT-descending normalization, post
    /// missing-data patching): displayed witness rows must be read back from
    /// this artifact, not from the solver model — the normalization can
    /// permute which element sits at each index after the model assigned
    /// values, so model rows can describe an arrangement the loader rejects.
    Validated(String),
    /// The interpreter cannot evaluate an opaque quantity; the witness
    /// remains a candidate (reason recorded).
    Candidate(String),
    /// Validation failed (interpreter rejected the event, or the event
    /// could not be realized): the verdict must downgrade.
    Rejected(String),
}

pub fn validate_witness(
    hir: &Hir,
    ext: &ExtDecls,
    interp: &Interp<'_>,
    model: &Model,
    mentioned: &BTreeSet<QuantityId>,
    region_a: usize,
    region_b: usize,
) -> Validation {
    let json = match build_event_json(hir, ext, model, mentioned) {
        Ok(j) => j,
        Err(why) => return Validation::Rejected(format!("witness realization failed: {why}")),
    };
    // A region may reference event-level data through statements the
    // encoder dropped as Unknown — those quantities are free (the §2
    // model caveat), so absence in the model is not a failure: default
    // them and re-evaluate. Bounded by the number of distinct missing
    // keys, in practice one or two iterations.
    let mut json = json;
    for _ in 0..8 {
        let event = match adl_interp::parse_event(&json, ext) {
            Ok(e) => e,
            Err(e) => {
                return Validation::Rejected(format!("synthetic event failed the loader: {e}"));
            }
        };
        let mut opaque: Option<String> = None;
        let mut missing: Option<String> = None;
        for idx in [region_a, region_b] {
            // Resolve by INDEX (not name): merged units can share a region
            // name, and a name lookup returns the first match — masking the
            // second region's cuts and fabricating a "validated" overlap.
            let name = hir.symbols.display(hir.regions[idx].name);
            // Non-short-circuiting membership: a decidable failing cut must be
            // seen even when an opaque statement precedes it in source order,
            // otherwise an unsatisfiable region is mistaken for an opaque
            // "candidate" overlap and the pair is falsely PROVEN OVERLAPPING.
            match interp.eval_region_membership_idx(idx, &event) {
                Ok(true) => {}
                Ok(false) => {
                    return Validation::Rejected(format!(
                        "interpreter rejects the witness event in region {name} ({}); \
                         event: {json}",
                        failing_stmts(hir, interp, idx, &event)
                    ));
                }
                Err(e) if e.reason.contains("no reference interpretation") => {
                    opaque = Some(format!(
                        "region {name} depends on an opaque quantity ({})",
                        e.reason
                    ));
                }
                Err(e) => {
                    match patch_missing(&json, &e.reason) {
                        Some(patched) => missing = Some(patched),
                        None => {
                            return Validation::Rejected(format!(
                                "interpreter cannot evaluate region {name} on the witness: {}",
                                e.reason
                            ));
                        }
                    }
                    break;
                }
            }
        }
        if let Some(patched) = missing {
            json = patched;
            continue;
        }
        return match opaque {
            Some(why) => Validation::Candidate(why),
            None => Validation::Validated(json),
        };
    }
    Validation::Rejected("witness event patching did not converge".to_owned())
}

/// Patch the synthetic event for a hard "missing event-level data"
/// evaluation error by defaulting the named datum to 0 (a free value —
/// the formulas never constrained it). Returns `None` for any other
/// error.
fn patch_missing(json: &str, reason: &str) -> Option<String> {
    let mut v: Value = serde_json::from_str(json).ok()?;
    let root = v.as_object_mut()?;
    let backtick = |s: &str| -> Option<String> {
        let start = s.find('`')? + 1;
        let end = s[start..].find('`')? + start;
        Some(s[start..end].to_owned())
    };
    if let Some(name) = reason
        .strip_prefix("event has no scalar ")
        .and_then(|_| backtick(reason))
    {
        root.insert(name, num(0.0));
    } else if reason.starts_with("event has no trigger flag ") {
        let name = backtick(reason)?;
        let trig = root
            .entry("triggers")
            .or_insert_with(|| Value::Object(Map::new()));
        trig.as_object_mut()?.insert(name, num(0.0));
    } else if reason == "event has no MET vector" || reason.starts_with("event MET has no ") {
        let component = backtick(reason);
        let met = root
            .entry("MET")
            .or_insert_with(|| Value::Object(Map::new()));
        let met = met.as_object_mut()?;
        met.entry("pt").or_insert_with(|| num(0.0));
        met.entry("phi").or_insert_with(|| num(0.0));
        if let Some(c) = component {
            met.entry(c).or_insert_with(|| num(0.0));
        }
    } else {
        return None;
    }
    Some(v.to_string())
}

/// Which membership statements of `region` fail on `event` (diagnostic
/// detail for the internal bug report a rejected witness files).
fn failing_stmts(
    hir: &Hir,
    interp: &Interp<'_>,
    idx: usize,
    event: &adl_interp::Event,
) -> String {
    use adl_sema::HirRegionStmt;
    let Some(r) = hir.regions.get(idx) else {
        return "region not found".to_owned();
    };
    let mut out = Vec::new();
    for (i, stmt) in r.stmts.iter().enumerate() {
        let verdict = match stmt {
            HirRegionStmt::Select(n) | HirRegionStmt::Trigger(n) => interp.eval_bool(n, event),
            HirRegionStmt::Reject(n) => interp.eval_bool(n, event).map(|v| !v),
            _ => continue,
        };
        match verdict {
            Ok(true) => {}
            Ok(false) => out.push(format!("stmt {i} fails")),
            Err(e) => out.push(format!("stmt {i} errors: {}", e.reason)),
        }
    }
    if out.is_empty() {
        "no single failing statement (inheritance?)".to_owned()
    } else {
        out.join("; ")
    }
}

// ---- model -> synthetic event ------------------------------------------

struct CollPlan {
    /// Realized member collections of this base's family, depth-sorted
    /// (base first), with their target sizes.
    family: Vec<(CollectionId, u64)>,
}

fn depth(hir: &Hir, c: CollectionId) -> u32 {
    match hir.table.collection(c) {
        Collection::Filtered { parent, .. } => depth(hir, *parent) + 1,
        _ => 0,
    }
}

/// Base ancestor of a base/filtered collection.
fn base_of(hir: &Hir, c: CollectionId) -> Option<CollectionId> {
    match hir.table.collection(c) {
        Collection::Base(_) => Some(c),
        Collection::Filtered { parent, .. } => base_of(hir, *parent),
        // A sort/slice shares its source's base (same element set / sub-range).
        Collection::Sorted { source, .. } | Collection::Slice { source, .. } => {
            base_of(hir, *source)
        }
        Collection::Union(_)
        | Collection::Combination { .. }
        | Collection::CombProject { .. } => None,
    }
}

/// All base/filtered collections reachable from `c` (unions expand to
/// their parts). `Err` for combinations.
fn realizable(hir: &Hir, c: CollectionId, out: &mut BTreeSet<CollectionId>) -> Result<(), String> {
    match hir.table.collection(c) {
        Collection::Base(_) | Collection::Filtered { .. } => {
            out.insert(c);
            Ok(())
        }
        Collection::Union(parts) => {
            for &p in parts {
                realizable(hir, p, out)?;
            }
            Ok(())
        }
        // A sort/slice realizes through its source (the witness only needs the
        // element set; the interpreter re-sorts/sub-ranges at validation).
        Collection::Sorted { source, .. } | Collection::Slice { source, .. } => {
            realizable(hir, *source, out)
        }
        // A composite realizes through its binder SOURCES (P3): build those
        // base/filtered collections from the model, then let the interpreter
        // enumerate the tuples and apply the per-tuple cuts during
        // re-validation. The composite itself is never serialized directly.
        // If the candidate/per-tuple cut depends on an opaque mass/pt, the
        // interpreter reports "no reference interpretation" and the witness
        // stays a Candidate — never a false Validated.
        Collection::Combination { parts, .. } => {
            for &p in parts {
                realizable(hir, p, out)?;
            }
            Ok(())
        }
        Collection::CombProject { comb, .. } => realizable(hir, *comb, out),
    }
}

/// Base collections that feed a same-source `disjoint` composite (whose
/// surviving pairs require kinematically-distinct elements, USER ANSWER 4).
/// The realizer gives these per-index distinct `eta` so a disjoint pair can
/// form; every other base keeps the unperturbed `eta = 0` default.
fn disjoint_source_bases(hir: &Hir) -> BTreeSet<CollectionId> {
    use adl_sema::CombKind;
    let mut out = BTreeSet::new();
    for c in hir.table.collections() {
        if let Collection::Combination { parts, kind, .. } = c
            && *kind == CombKind::Disjoint
            && parts.len() >= 2
            && parts.windows(2).all(|w| w[0] == w[1])
            && let Some(b) = base_of(hir, parts[0])
        {
            out.insert(b);
        }
    }
    out
}

fn build_event_json(
    hir: &Hir,
    ext: &ExtDecls,
    model: &Model,
    mentioned: &BTreeSet<QuantityId>,
) -> Result<String, String> {
    let met_key = hir.symbols.lookup(adl_sema::ext::MET_FAMILY_KEY);
    let is_met_base = |c: CollectionId| -> bool {
        matches!(hir.table.collection(c), Collection::Base(s) if Some(*s) == met_key)
    };

    // -- which collections matter, and how big -----------------------------
    let mut needed: BTreeSet<CollectionId> = BTreeSet::new();
    let mut sizes: BTreeMap<CollectionId, u64> = BTreeMap::new();
    let mut elem_pins: BTreeMap<(CollectionId, u32), Vec<(String, f64)>> = BTreeMap::new();

    // Pass 1: explicit size pins. The encoder's element-existence guards
    // put `size(C)` atoms in every formula that needs an element, so a
    // model size value is authoritative — including size = 0 (a region
    // can be *in* by virtue of a missing element making a rejected
    // comparison false).
    let mut size_pinned: BTreeSet<CollectionId> = BTreeSet::new();
    for (q, v) in model.iter() {
        if let Quantity::Size(c) = hir.table.quantity(q) {
            if is_met_base(*c) {
                continue;
            }
            realizable(hir, *c, &mut needed)?;
            let n = v.round().max(0.0);
            if n > MAX_REALIZED as f64 {
                return Err(format!("collection size {n} exceeds the realizer cap"));
            }
            let e = sizes.entry(*c).or_insert(0);
            *e = (*e).max(n as u64);
            size_pinned.insert(*c);
        }
    }

    // Pass 2: element mentions. A mentioned element only *bumps* the
    // size when the model carries no explicit size for its collection
    // (e.g. a backend that returns partial models).
    for (q, v) in model.iter() {
        match hir.table.quantity(q) {
            Quantity::ElemProp { coll, index, prop } => {
                let ElemIndex::FromFront(i) = index else {
                    continue;
                };
                if is_met_base(*coll) {
                    continue;
                }
                realizable(hir, *coll, &mut needed)?;
                if mentioned.contains(&q) && !size_pinned.contains(coll) {
                    let e = sizes.entry(*coll).or_insert(0);
                    *e = (*e).max(u64::from(*i) + 1);
                }
                elem_pins
                    .entry((*coll, *i))
                    .or_default()
                    .push((hir.table.prop_key(*prop).to_owned(), v));
            }
            Quantity::AngularSep { a, b, .. } => {
                for p in [a, b] {
                    if let adl_sema::ParticleRef::Elem { coll, index } = p
                        && !is_met_base(*coll)
                    {
                        realizable(hir, *coll, &mut needed)?;
                        if mentioned.contains(&q)
                            && !size_pinned.contains(coll)
                            && let ElemIndex::FromFront(i) = index
                        {
                            let e = sizes.entry(*coll).or_insert(0);
                            *e = (*e).max(u64::from(*i) + 1);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Unions derive from their parts; only base/filtered get realized.
    // Group families per base and propagate sizes up to the base.
    let mut families: BTreeMap<CollectionId, Vec<CollectionId>> = BTreeMap::new();
    for &c in &needed {
        let Some(b) = base_of(hir, c) else { continue };
        if is_met_base(b) {
            continue;
        }
        families.entry(b).or_default().push(c);
    }
    let mut plans: BTreeMap<CollectionId, CollPlan> = BTreeMap::new();
    for (b, mut members) in families {
        members.sort_by_key(|&c| (depth(hir, c), c));
        members.dedup();
        // All-pass realization: every base element passes every filter,
        // so every family member shares the base count (the maximum any
        // member needs). Smaller pinned sizes are honored only through
        // validation — a mismatch downgrades the verdict, never lies.
        let n_base = members
            .iter()
            .map(|c| sizes.get(c).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        let family = members.into_iter().map(|c| (c, n_base)).collect();
        plans.insert(b, CollPlan { family });
    }

    // -- build objects (phase 1: pins, repair, pT fill) ----------------------
    let pt_key = ext.prop_canon("pt").0;
    let mut built: BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>> = BTreeMap::new();

    for (base, plan) in &plans {
        let n = plan.family.first().map_or(0, |&(_, n)| n);
        let mut objs: Vec<BTreeMap<String, f64>> = vec![BTreeMap::new(); n as usize];
        let mut pinned: Vec<BTreeSet<String>> = vec![BTreeSet::new(); n as usize];

        // Pin properties from the model, shallow-to-deep so the deepest
        // (most-constrained, formula-visible) value wins.
        for &(c, n_c) in &plan.family {
            for j in 0..n_c {
                let Ok(idx32) = u32::try_from(j) else {
                    continue;
                };
                if let Some(pins) = elem_pins.get(&(c, idx32)) {
                    for (key, v) in pins {
                        objs[j as usize].insert(key.clone(), *v);
                        pinned[j as usize].insert(key.clone());
                    }
                }
            }
        }

        // Repair pass: make every element satisfy every filter predicate
        // along the family chain (free properties only).
        for &(c, n_c) in &plan.family {
            let Collection::Filtered { pred, .. } = hir.table.collection(c) else {
                continue;
            };
            let pred_node = &hir.elem_preds[pred.0 as usize].node;
            for j in 0..n_c as usize {
                if eval_pred(pred_node, &objs[j], model, hir) != Some(true) {
                    repair(pred_node, &mut objs[j], &pinned[j], hir);
                }
            }
        }

        // pT fill + monotonicity: unset pT takes the previous element's
        // value (keeps the collection pT-descending for the loader).
        // Always runs: synthetic objects must carry the standard
        // property set (SPEC_LANGUAGE §4.1) — a region can reference a
        // property through a statement whose atoms folded away (e.g.
        // `pT(j[0]) − pT(j[0]) < 25`), and a missing property would
        // soft-fail a comparison the formula proved trivially true.
        {
            let mut last: Option<f64> = None;
            let first_set = objs.iter().find_map(|o| o.get(&pt_key).copied());
            for o in &mut objs {
                match o.get(&pt_key) {
                    Some(&v) => last = Some(v),
                    None => {
                        let v = last.or(first_set).unwrap_or(50.0);
                        o.insert(pt_key.clone(), v);
                        last = Some(v);
                    }
                }
            }
        }
        built.insert(*base, objs);
    }

    // -- MET / scalars / triggers -------------------------------------------
    let mut met = Map::new();
    let mut scalars: Vec<(String, f64)> = Vec::new();
    let mut triggers = Map::new();
    for (q, v) in model.iter() {
        if let Quantity::EventScalar(src) = hir.table.quantity(q) {
            match src {
                ScalarSource::MetProp(p) => {
                    met.insert(hir.table.prop_key(*p).to_owned(), num(v));
                }
                ScalarSource::EventVar(s) => {
                    scalars.push((hir.symbols.key(*s).to_owned(), v));
                }
                ScalarSource::Trigger(s) => {
                    let flag = if v >= 0.5 { 1.0 } else { 0.0 };
                    triggers.insert(hir.symbols.key(*s).to_owned(), num(flag));
                }
            }
        }
    }

    // -- phase 2: realize angular model values into phi/eta ------------------
    realize_angulars(hir, ext, model, mentioned, &mut built, &mut met);

    // -- phase 2.5: default the remaining standard properties ----------------
    // (free values; the formulas never constrained them — SPEC §4.1 says
    // objects carry them, and the interpreter soft-fails their absence.)
    //
    // For base collections that feed a same-source `disjoint` composite, the
    // `eta` default is made per-index DISTINCT (`0.1 * index`) so the source
    // elements are kinematically distinct (USER ANSWER 4): with all-equal
    // eta/phi/m, the value-distinctness drop nulls every pair and
    // `size(K) >= 1` can never validate, downgrading a sound overlap to
    // POSSIBLY. The perturbation is SCOPED to disjoint-source bases so it
    // does not churn unrelated witnesses; a free per-index eta only ever
    // fills an UNSET value (a pinned/angular-realized eta wins), so it never
    // makes a real member event rejected — validation remains the safety net.
    let disjoint_source_bases = disjoint_source_bases(hir);
    {
        let eta_key = ext.prop_canon("eta").0;
        let phi_key = ext.prop_canon("phi").0;
        let m_key = ext.prop_canon("m").0;
        let const_defaults: [(&str, f64); 6] = [
            (&eta_key, 0.0),
            (&phi_key, 0.0),
            (&m_key, 0.0),
            ("btag", 0.0),
            ("ctag", 0.0),
            ("tautag", 0.0),
        ];
        for (base, objs) in built.iter_mut() {
            let distinct_eta = disjoint_source_bases.contains(base);
            for (i, o) in objs.iter_mut().enumerate() {
                if distinct_eta {
                    #[allow(clippy::cast_precision_loss)]
                    o.entry(eta_key.clone()).or_insert(0.1 * i as f64);
                }
                for (k, v) in const_defaults {
                    o.entry(k.to_owned()).or_insert(v);
                }
            }
        }
    }

    // -- phase 2.75: normalize each base to pT-descending --------------------
    // The loader refuses any non-pT-descending collection (it never re-sorts),
    // and a solver model can hand back an ascending/equal arrangement (padded
    // out-of-range elements; ORD does not pin every slot). A STABLE sort by pt
    // descending repairs exactly those otherwise-rejected events; a build that
    // is already descending is unchanged (stable no-op), so no currently-valid
    // witness is perturbed and validation stays the safety net.
    {
        let pt_key = ext.prop_canon("pt").0;
        for objs in built.values_mut() {
            objs.sort_by(|a, b| {
                let pa = a.get(&pt_key).copied().unwrap_or(f64::NEG_INFINITY);
                let pb = b.get(&pt_key).copied().unwrap_or(f64::NEG_INFINITY);
                pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
            });
        }
    }

    // -- phase 3: serialize ---------------------------------------------------
    let mut root = Map::new();
    for (base, objs) in built {
        let display = match hir.table.collection(base) {
            Collection::Base(s) => hir.symbols.display(*s).to_owned(),
            _ => continue,
        };
        let arr: Vec<Value> = objs
            .into_iter()
            .map(|o| {
                let mut m = Map::new();
                for (k, v) in o {
                    m.insert(k, num(v));
                }
                Value::Object(m)
            })
            .collect();
        root.insert(display, Value::Array(arr));
    }
    if !met.is_empty() {
        root.insert("MET".to_owned(), Value::Object(met));
    }
    for (k, v) in scalars {
        root.entry(k).or_insert_with(|| num(v));
    }
    if !triggers.is_empty() {
        root.insert("triggers".to_owned(), Value::Object(triggers));
    }

    Ok(Value::Object(root).to_string())
}

fn num(v: f64) -> Value {
    Number::from_f64(if v.is_finite() { v } else { 0.0 }).map_or(Value::Null, Value::Number)
}

/// Anchor of an angular separation in the built event.
enum Loc {
    Met,
    Obj(CollectionId, usize),
}

/// Best-effort realization of angular separations: set free `phi`/`eta`
/// components of the anchors so the interpreter reproduces the model's
/// `dPhi`/`dEta` values (`dX(a, b) = x_a − x_b`, `dPhi` wrapped).
/// Doubly-pinned or conflicting anchors are left alone — validation
/// remains the safety net (a mismatch downgrades the verdict, never
/// lies).
fn realize_angulars(
    hir: &Hir,
    ext: &ExtDecls,
    model: &Model,
    mentioned: &BTreeSet<QuantityId>,
    built: &mut BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>>,
    met: &mut Map<String, Value>,
) {
    use adl_sema::{AngKind, ParticleRef};
    let phi_key = ext.prop_canon("phi").0;
    let eta_key = ext.prop_canon("eta").0;

    let loc_of = |p: &ParticleRef| -> Option<Loc> {
        match p {
            ParticleRef::Met => Some(Loc::Met),
            ParticleRef::Elem {
                coll,
                index: ElemIndex::FromFront(i),
            } => base_of(hir, *coll).map(|b| Loc::Obj(b, *i as usize)),
            _ => None,
        }
    };

    for (q, v) in model.iter() {
        if !mentioned.contains(&q) {
            continue;
        }
        let Quantity::AngularSep { kind, a, b, .. } = hir.table.quantity(q) else {
            continue;
        };
        let (Some(la), Some(lb)) = (loc_of(a), loc_of(b)) else {
            continue;
        };
        if *kind == AngKind::DR {
            // dR = hypot(dEta, wrap(dPhi)); realize the target v>=0 by forcing
            // dPhi = 0 (equal phi) and |dEta| = v, since hypot(v, 0) = v exactly.
            // Only fills UNSET components and only for object anchors (MET has no
            // eta); anything already pinned is left to validation.
            realize_dr(v, &la, &lb, &eta_key, &phi_key, built);
            continue;
        }
        let key = match kind {
            AngKind::DPhi => &phi_key,
            AngKind::DEta => &eta_key,
            AngKind::DR => unreachable!("dR handled above"),
        };
        if *kind == AngKind::DEta && (matches!(la, Loc::Met) || matches!(lb, Loc::Met)) {
            continue; // MET has no pseudorapidity
        }
        // Current values; a missing *element* makes the constraint moot
        // (the existence-guarded formula branch was not taken).
        let read = |loc: &Loc,
                    built: &BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>>,
                    met: &Map<String, Value>|
         -> Option<Option<f64>> {
            match loc {
                Loc::Met => Some(met.get(key).and_then(Value::as_f64)),
                Loc::Obj(base, i) => built.get(base)?.get(*i).map(|o| o.get(key).copied()),
            }
        };
        let (Some(cur_a), Some(cur_b)) = (read(&la, built, met), read(&lb, built, met)) else {
            continue;
        };
        let write = |loc: &Loc,
                     value: f64,
                     built: &mut BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>>,
                     met: &mut Map<String, Value>| {
            match loc {
                Loc::Met => {
                    met.insert(key.clone(), num(value));
                }
                Loc::Obj(base, i) => {
                    if let Some(objs) = built.get_mut(base)
                        && let Some(o) = objs.get_mut(*i)
                    {
                        o.insert(key.clone(), value);
                    }
                }
            }
        };
        // Fix-point correction: choose the free component so that the
        // INTERPRETER's computed separation reproduces the model value
        // bit-exactly (f64 `wrap`/subtraction round-trips are not exact;
        // z3 favors boundary values where one ulp flips an atom).
        let realized = |x: f64, other: f64, flip: bool| -> f64 {
            let d = if flip { other - x } else { x - other };
            match kind {
                AngKind::DPhi => adl_interp::wrap_dphi(d),
                _ => d,
            }
        };
        let correct = |mut x: f64, other: f64, flip: bool| -> f64 {
            for _ in 0..4 {
                let got = realized(x, other, flip);
                if got == v || !(v - got).is_finite() {
                    break;
                }
                x += if flip { got - v } else { v - got };
            }
            x
        };
        match (cur_a, cur_b) {
            (Some(_), Some(_)) => {} // both pinned: validation decides
            (None, Some(vb)) => write(&la, correct(v + vb, vb, false), built, met),
            (Some(va), None) => write(&lb, correct(va - v, va, true), built, met),
            (None, None) => {
                write(&lb, 0.0, built, met);
                write(&la, correct(v, 0.0, false), built, met);
            }
        }
    }
}

/// Choose `x` so the interpreter's `|x - other|` reproduces `v` BIT-EXACTLY
/// where an f64 grid point exists (review F13): `fl(other + v) - other` is
/// inexact for ~a quarter of typical (eta, dR) pairs, and a one-ulp miss
/// rejects a boundary-forced witness (equality atoms, touching bands). Runs
/// a short fix-point on the `other + v` side, then — the `+`-side x-grid ulp
/// can be coarser than the dEta-target's — the `other - v` side, keeping
/// whichever round-trips exactly (else the best `+`-side effort; validation
/// decides).
fn eta_for_exact_gap(other: f64, v: f64) -> f64 {
    let fixpoint = |start: f64| -> f64 {
        let mut x = start;
        for _ in 0..4 {
            let got = (x - other).abs();
            if got == v || !(v - got).is_finite() {
                break;
            }
            x = if x >= other { x + (v - got) } else { x - (v - got) };
        }
        x
    };
    let plus = fixpoint(other + v);
    if (plus - other).abs() == v {
        return plus;
    }
    let minus = fixpoint(other - v);
    if (minus - other).abs() == v {
        return minus;
    }
    plus
}

/// Realize a target `dR = v` (`v >= 0`) between two object anchors by filling
/// their free `eta`/`phi` so the interpreter's `hypot(dEta, wrap(dPhi))`
/// reproduces `v`. Preferred plane: `dPhi = 0` (equal `phi`) and `|dEta| = v`
/// (`hypot(v, 0) = v` and `wrap(0) = 0` are exact). When BOTH etas are
/// already pinned (e.g. a dEta cut realized first), the remainder goes
/// through the phi plane instead: `wrap(dPhi) = sqrt(v² − dEta²)` (review
/// F14 — the old code returned, so every dEta-pinned dR witness rejected and
/// exhausted its retries). Only OBJECT anchors carry a pseudorapidity (MET
/// has none), and only UNSET components are written — a value already pinned
/// is left alone and validation remains the safety net (a bad guess only
/// downgrades to POSSIBLY, never lies).
fn realize_dr(
    v: f64,
    la: &Loc,
    lb: &Loc,
    eta_key: &str,
    phi_key: &str,
    built: &mut BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>>,
) {
    if !v.is_finite() || v < 0.0 {
        return;
    }
    let (Loc::Obj(base_a, ia), Loc::Obj(base_b, ib)) = (la, lb) else {
        return; // MET (or an unlocatable anchor) has no eta — validation decides
    };
    // `Some(None)` = the element exists but the component is unset (free);
    // `None` = the element itself is absent (existence guard not taken).
    let get = |base: &CollectionId, i: usize, key: &str| -> Option<Option<f64>> {
        built.get(base)?.get(i).map(|o| o.get(key).copied())
    };
    let (Some(eta_a), Some(eta_b)) = (get(base_a, *ia, eta_key), get(base_b, *ib, eta_key)) else {
        return;
    };
    let (Some(phi_a), Some(phi_b)) = (get(base_a, *ia, phi_key), get(base_b, *ib, phi_key)) else {
        return;
    };
    let mut set = |base: &CollectionId, i: usize, key: &str, val: f64| {
        if let Some(objs) = built.get_mut(base)
            && let Some(o) = objs.get_mut(i)
        {
            o.insert(key.to_owned(), val);
        }
    };

    // Both etas pinned: realize the remainder through the phi plane.
    if let (Some(ea), Some(eb)) = (eta_a, eta_b) {
        let de = ea - eb;
        let rem2 = v.mul_add(v, -(de * de));
        if rem2 < 0.0 {
            return; // dR < |dEta|: unrealizable in this plane — validation decides
        }
        let dphi = rem2.sqrt();
        if !dphi.is_finite() || dphi > std::f64::consts::PI {
            return; // wrap(·) cannot produce it
        }
        let (free_base, free, pinned, pinned_is_a) = match (phi_a, phi_b) {
            (Some(_), Some(_)) => return, // both pinned — validation decides
            (Some(pa), None) => (base_b, *ib, pa, true),
            (None, Some(pb)) => (base_a, *ia, pb, false),
            (None, None) => {
                set(base_b, *ib, phi_key, 0.0);
                (base_a, *ia, 0.0, false)
            }
        };
        // Short correction toward the interpreter's hypot on the REALIZED
        // phi difference (both the sqrt and the `fl(pinned ± dphi)`
        // round-trip are inexact; the interior ε-margin absorbs any
        // residual, and validation still decides).
        let mut val = if pinned_is_a { pinned - dphi } else { pinned + dphi };
        for _ in 0..4 {
            let d = if pinned_is_a { pinned - val } else { val - pinned };
            let got = de.hypot(adl_interp::wrap_dphi(d));
            if got == v || !(v - got).is_finite() || !(0.0..=std::f64::consts::PI).contains(&d) {
                break;
            }
            val = if pinned_is_a { val - (v - got) } else { val + (v - got) };
        }
        set(free_base, free, phi_key, val);
        return;
    }

    // dPhi = 0 plane: pick a shared phi. Two already-pinned, differing phi
    // cannot be made equal without clobbering — leave to validation.
    let phi_target = match (phi_a, phi_b) {
        (Some(pa), Some(pb)) if pa != pb => return,
        (Some(pa), _) => pa,
        (_, Some(pb)) => pb,
        (None, None) => 0.0,
    };
    // Set |dEta| = v by writing whichever eta is free (fix-point corrected —
    // review F13: `fl(pinned ± v)` alone misses the target by an ulp on
    // ~a quarter of non-dyadic boundaries).
    match (eta_a, eta_b) {
        (Some(_), Some(_)) => unreachable!("handled by the phi-plane branch"),
        (None, Some(eb)) => set(base_a, *ia, eta_key, eta_for_exact_gap(eb, v)),
        (Some(ea), None) => set(base_b, *ib, eta_key, eta_for_exact_gap(ea, v)),
        (None, None) => {
            set(base_b, *ib, eta_key, 0.0);
            set(base_a, *ia, eta_key, v);
        }
    }
    if phi_a.is_none() {
        set(base_a, *ia, phi_key, phi_target);
    }
    if phi_b.is_none() {
        set(base_b, *ib, phi_key, phi_target);
    }
}

// ---- tiny element-predicate evaluator / repairer -------------------------
//
// Same conservative fragment as the EPRED encoder: linear comparisons,
// bands and boolean structure over the implicit element's properties
// (plus model-valued event quantities). `None` = cannot tell.

fn eval_pred(node: &HNode, obj: &BTreeMap<String, f64>, model: &Model, hir: &Hir) -> Option<bool> {
    if matches!(node.tag, Fragment::Unsupported(_)) {
        return None;
    }
    match &node.kind {
        HKind::Bool(b) => Some(*b),
        HKind::And(v) => {
            let mut all = true;
            for p in v {
                match eval_pred(p, obj, model, hir) {
                    Some(false) => return Some(false),
                    Some(true) => {}
                    None => all = false,
                }
            }
            if all { Some(true) } else { None }
        }
        HKind::Or(v) => {
            let mut any_unknown = false;
            for p in v {
                match eval_pred(p, obj, model, hir) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => any_unknown = true,
                }
            }
            if any_unknown { None } else { Some(false) }
        }
        HKind::Not(inner) => eval_pred(inner, obj, model, hir).map(|b| !b),
        HKind::Cmp { op, lhs, rhs } => {
            let l = eval_num(lhs, obj, model, hir)?;
            let r = eval_num(rhs, obj, model, hir)?;
            Some(match op {
                adl_syntax::ast::CmpOp::Gt => l > r,
                adl_syntax::ast::CmpOp::Lt => l < r,
                adl_syntax::ast::CmpOp::Ge => l >= r,
                adl_syntax::ast::CmpOp::Le => l <= r,
                adl_syntax::ast::CmpOp::Eq => l == r,
                adl_syntax::ast::CmpOp::Ne | adl_syntax::ast::CmpOp::ApproxEq => l != r,
            })
        }
        HKind::Band { kind, expr, lo, hi } => {
            let v = eval_num(expr, obj, model, hir)?;
            let lo: f64 = lo.parse().ok()?;
            let hi: f64 = hi.parse().ok()?;
            Some(match kind {
                adl_syntax::ast::BandKind::In => lo <= v && v <= hi,
                adl_syntax::ast::BandKind::Out => v <= lo || v >= hi,
            })
        }
        _ => None,
    }
}

fn eval_num(node: &HNode, obj: &BTreeMap<String, f64>, model: &Model, hir: &Hir) -> Option<f64> {
    if matches!(node.tag, Fragment::Unsupported(_)) {
        return None;
    }
    match &node.kind {
        HKind::Num(s) => s.parse().ok(),
        HKind::ElemSelfProp(p) => obj.get(hir.table.prop_key(*p)).copied(),
        HKind::Quantity(q) => model.get(*q),
        HKind::Neg(a) => Some(-eval_num(a, obj, model, hir)?),
        HKind::Abs(a) => Some(eval_num(a, obj, model, hir)?.abs()),
        HKind::Binary { op, lhs, rhs } => {
            let l = eval_num(lhs, obj, model, hir)?;
            let r = eval_num(rhs, obj, model, hir)?;
            let v = match op {
                adl_sema::ArithOp::Add => l + r,
                adl_sema::ArithOp::Sub => l - r,
                adl_sema::ArithOp::Mul => l * r,
                adl_sema::ArithOp::Div => l / r,
                adl_sema::ArithOp::Pow => l.powf(r),
            };
            v.is_finite().then_some(v)
        }
        _ => None,
    }
}

/// Best-effort repair: set free (unpinned) properties so simple
/// per-property comparisons on the predicate's And-spine hold.
fn repair(node: &HNode, obj: &mut BTreeMap<String, f64>, pinned: &BTreeSet<String>, hir: &Hir) {
    match &node.kind {
        HKind::And(v) => {
            for p in v {
                repair(p, obj, pinned, hir);
            }
        }
        HKind::Cmp { op, lhs, rhs } => {
            // prop ⋈ const or const ⋈ prop.
            let (prop, k, op) = match (&lhs.kind, &rhs.kind) {
                (HKind::ElemSelfProp(p), HKind::Num(n)) => {
                    let Ok(k) = n.parse::<f64>() else { return };
                    (*p, k, *op)
                }
                (HKind::Num(n), HKind::ElemSelfProp(p)) => {
                    let Ok(k) = n.parse::<f64>() else { return };
                    (*p, k, flip(*op))
                }
                _ => return,
            };
            repair_prop(obj, pinned, hir.table.prop_key(prop), op, k);
        }
        HKind::Band { kind, expr, lo, hi } => {
            if let HKind::ElemSelfProp(p) = &expr.kind
                && *kind == adl_syntax::ast::BandKind::In
                && let (Ok(lo), Ok(hi)) = (lo.parse::<f64>(), hi.parse::<f64>())
            {
                let key = hir.table.prop_key(*p);
                if !pinned.contains(key) {
                    obj.insert(key.to_owned(), f64::midpoint(lo, hi));
                }
            }
        }
        _ => {}
    }
}

fn flip(op: adl_syntax::ast::CmpOp) -> adl_syntax::ast::CmpOp {
    use adl_syntax::ast::CmpOp::{ApproxEq, Eq, Ge, Gt, Le, Lt, Ne};
    match op {
        Gt => Lt,
        Lt => Gt,
        Ge => Le,
        Le => Ge,
        Eq => Eq,
        Ne => Ne,
        ApproxEq => ApproxEq,
    }
}

fn repair_prop(
    obj: &mut BTreeMap<String, f64>,
    pinned: &BTreeSet<String>,
    key: &str,
    op: adl_syntax::ast::CmpOp,
    k: f64,
) {
    if pinned.contains(key) {
        return;
    }
    let current = obj.get(key).copied();
    let satisfied = current.is_some_and(|v| match op {
        adl_syntax::ast::CmpOp::Gt => v > k,
        adl_syntax::ast::CmpOp::Lt => v < k,
        adl_syntax::ast::CmpOp::Ge => v >= k,
        adl_syntax::ast::CmpOp::Le => v <= k,
        adl_syntax::ast::CmpOp::Eq => v == k,
        adl_syntax::ast::CmpOp::Ne | adl_syntax::ast::CmpOp::ApproxEq => v != k,
    });
    if satisfied {
        return;
    }
    let v = match op {
        adl_syntax::ast::CmpOp::Gt => k + 1.0,
        adl_syntax::ast::CmpOp::Ge | adl_syntax::ast::CmpOp::Eq => k,
        adl_syntax::ast::CmpOp::Lt => k - 1.0,
        adl_syntax::ast::CmpOp::Le => k,
        adl_syntax::ast::CmpOp::Ne | adl_syntax::ast::CmpOp::ApproxEq => k + 1.0,
    };
    obj.insert(key.to_owned(), v);
}

#[cfg(test)]
mod realize_dr_tests {
    //! Deterministic branch-matrix coverage for `realize_dr` (review F9 —
    //! previously only the (None, None) happy path was pinned, and only
    //! when z3's model happened to take that shape).

    use super::{Loc, realize_dr};
    use adl_sema::CollectionId;
    use std::collections::BTreeMap;

    const ETA: &str = "etaof";
    const PHI: &str = "phiof";
    const BASE: CollectionId = CollectionId(0);

    fn built_with(
        elems: &[&[(&str, f64)]],
    ) -> BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>> {
        let objs = elems
            .iter()
            .map(|props| {
                props
                    .iter()
                    .map(|&(k, v)| (k.to_owned(), v))
                    .collect::<BTreeMap<_, _>>()
            })
            .collect();
        BTreeMap::from([(BASE, objs)])
    }

    fn interp_dr(
        built: &BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>>,
        a: usize,
        b: usize,
    ) -> f64 {
        let o = &built[&BASE];
        let de = o[a][ETA] - o[b][ETA];
        let dp = adl_interp::wrap_dphi(o[a][PHI] - o[b][PHI]);
        de.hypot(dp)
    }

    fn realize(
        v: f64,
        built: &mut BTreeMap<CollectionId, Vec<BTreeMap<String, f64>>>,
    ) {
        realize_dr(v, &Loc::Obj(BASE, 0), &Loc::Obj(BASE, 1), ETA, PHI, built);
    }

    #[test]
    fn both_free_is_exact() {
        let mut b = built_with(&[&[], &[]]);
        realize(1.5, &mut b);
        assert_eq!(interp_dr(&b, 0, 1), 1.5);
    }

    #[test]
    fn pinned_eta_round_trips_exactly_or_within_one_ulp() {
        // Review F13: `fl(pinned ± v)` alone missed the model value on ~a
        // quarter of non-dyadic boundary pairs, rejecting boundary-forced
        // witnesses. The fix-point (trying both gap directions) must land
        // the interpreter's dR EXACTLY where an f64 grid point exists — the
        // documented repro (eta = 0.3, touching dR = 0.4 bands) — and within
        // one ulp everywhere (some pairs, e.g. (2.4, 0.4), have NO exact
        // grid point in either direction: the difference grid of both
        // binades skips v; the ε-interior margin absorbs the residual).
        for (eb, v, exact) in [
            (0.3, 0.4, true),
            (0.1, 0.2, true),
            (2.4, 0.4, false),
            (-1.7, 0.9, false),
            (0.7, 0.05, false),
        ] {
            for pinned_a in [false, true] {
                let mut b = if pinned_a {
                    built_with(&[&[(ETA, eb)], &[]])
                } else {
                    built_with(&[&[], &[(ETA, eb)]])
                };
                realize(v, &mut b);
                let got = interp_dr(&b, 0, 1);
                if exact {
                    assert_eq!(got, v, "eb={eb} v={v} pinned_a={pinned_a}");
                } else {
                    // One step of the DIFFERENCE grid: the x values live in
                    // the |eb| ± v binade, so achievable |x − eb| step by
                    // that binade's ulp, not v's.
                    let grid = (eb.abs() + v) * f64::EPSILON;
                    assert!(
                        (got - v).abs() <= grid,
                        "eb={eb} v={v} pinned_a={pinned_a}: got {got} (off by {}, grid {grid})",
                        (got - v).abs()
                    );
                }
            }
        }
    }

    #[test]
    fn both_etas_pinned_realizes_through_phi() {
        // Review F14: a prior dEta realization pins both etas; the remainder
        // must go through the phi plane instead of returning (which rejected
        // every such witness and spammed INTERNAL diagnostics). The interior
        // ε-margin is ~1e-6, so a 1e-9 residual is more than validated.
        let mut b = built_with(&[&[(ETA, 0.8)], &[(ETA, 0.0)]]);
        realize(1.7, &mut b);
        assert!(
            (interp_dr(&b, 0, 1) - 1.7).abs() < 1e-9,
            "got {}",
            interp_dr(&b, 0, 1)
        );
        // One phi already pinned: the free side absorbs the correction.
        let mut b = built_with(&[&[(ETA, 0.5), (PHI, 1.0)], &[(ETA, 0.0)]]);
        realize(1.3, &mut b);
        assert!(
            (interp_dr(&b, 0, 1) - 1.3).abs() < 1e-9,
            "got {}",
            interp_dr(&b, 0, 1)
        );
    }

    #[test]
    fn unrealizable_shapes_write_nothing() {
        // dR below the pinned |dEta|: no phi assignment can shrink it.
        let mut b = built_with(&[&[(ETA, 2.0)], &[(ETA, 0.0)]]);
        let before = b.clone();
        realize(1.0, &mut b);
        assert_eq!(b, before, "dR < |dEta| must not fabricate a fill");
        // Differing pinned phis on the dPhi=0 route: no clobber.
        let mut b = built_with(&[&[(PHI, 0.0)], &[(ETA, 0.0), (PHI, 2.0)]]);
        let before = b.clone();
        realize(0.4, &mut b);
        assert_eq!(b, before, "pinned differing phis must not be clobbered");
        // Negative / non-finite targets: no write.
        for bad in [-0.1, f64::NAN, f64::INFINITY] {
            let mut b = built_with(&[&[], &[]]);
            let before = b.clone();
            realize(bad, &mut b);
            assert_eq!(b, before, "target {bad} must not realize");
        }
    }
}
