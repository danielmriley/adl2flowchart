//! In-verify adversarial refutation search (trustworthy-verify M1).
//!
//! After an UNSAT-side PROVEN (disjoint / empty / subset), search a small
//! deterministic battery of cut-anchored + flat-spot probe events through
//! the reference interpreter. A hit demotes the verdict — the same fail-
//! closed contract as the sampling gate, but targeting the f64 half-ulp /
//! mul / ratio families that fixed-pool sampling historically missed
//! (CE-11/C4, CE-12/C5, CE-14).
//!
//! Independent of `adl-difftest`: analysis must not depend on the oracle
//! crate. Probe builders reuse [`adl_interp::sample`] boundary JSON helpers.

use adl_interp::sample::{
    absence_family, clamp_magnitude, ht_boundary_json, met_boundary_json, obj_boundary_json,
    MAX_CUT_CONSTANTS,
};
use adl_interp::{parse_event, Event, Interp};
use adl_sema::ExtDecls;

/// Hard cap on probe events evaluated per UNSAT-side claim.
///
/// Kept modest on purpose: every probe runs full region membership (which can
/// include expensive kinematics), and a corpus file with hundreds of PROVEN
/// pairs pays it per pair. The cap is only sound to keep small because
/// [`probe_scalars`] hands values back in **priority order** — the cut
/// constants themselves and their ulp neighbours first, derived flat-spot
/// families after — so truncating drops the speculative probes, never the
/// anchors. (Returning them in numeric order, as an earlier version did, made
/// the budget cover only the smallest values: with cuts at 30 and 100 the
/// derived `k − a` family filled the budget before 100 was ever probed.)
pub const MAX_REFUTE_PROBES: usize = 64;

/// Absolute-value cap for probe scalars (matches the sampling gate).
const MAX_INJECT_ABS: f64 = 1.0e6;

/// Ulps walked off each cut constant (high priority).
const ULP_WALK_CUT: u32 = 4;
/// Ulps walked off derived flat-spot families (add/mul/ratio).
const ULP_WALK_DERIVED: u32 = 1;
/// Max cut constants participating in O(n²) add/mul/ratio families.
const MAX_DERIVED_CUTS: usize = 8;

fn injectible(c: f64) -> bool {
    c.is_finite() && c.abs() <= MAX_INJECT_ABS
}

fn push_with_ulps(out: &mut Vec<f64>, v: f64, walk: u32) {
    if !injectible(v) {
        return;
    }
    out.push(v);
    let mut u = v;
    let mut d = v;
    for _ in 0..walk {
        u = u.next_up();
        d = d.next_down();
        if injectible(u) {
            out.push(u);
        }
        if injectible(d) {
            out.push(d);
        }
    }
}

/// Clamp every value into the loader's magnitude domain and drop repeats,
/// **keeping first-seen order** so the priority in [`probe_scalars`] survives
/// the [`MAX_REFUTE_PROBES`] truncation.
fn clamp_and_dedup(values: Vec<f64>) -> Vec<f64> {
    let mut seen: Vec<u64> = Vec::with_capacity(values.len());
    let mut out = Vec::with_capacity(values.len());
    for v in values {
        let v = clamp_magnitude(v);
        let bits = v.to_bits();
        if !seen.contains(&bits) {
            seen.push(bits);
            out.push(v);
        }
    }
    out
}

/// Adversarial probe scalars from unit cut constants + known flat-spot
/// families. Deterministic, in **priority order** (see
/// [`MAX_REFUTE_PROBES`]), clamped into the loader's domain and deduped.
///
/// Families, most valuable first:
/// - cut ± ulps — the boundary the region actually names
/// - **add** (`q+c ⋈ k`): `k − c` and ulps (covers C4 half-ulp: `1−0.5` →
///   `0.5000000000000001`)
/// - **mul** (`q*c ⋈ k`): `k / fl(c)` and ulps (covers C5)
/// - **ratio** (`L/d ⋈ c`): `fl(c)·fl(d)` ± ulps (covers CE-14)
#[must_use]
pub fn probe_scalars(cut_consts: &[f64]) -> Vec<f64> {
    let cuts: Vec<f64> = cut_consts
        .iter()
        .copied()
        .filter(|&c| injectible(c))
        .take(MAX_CUT_CONSTANTS)
        .collect();
    let derived: Vec<f64> = cuts.iter().copied().take(MAX_DERIVED_CUTS).collect();
    let mut out = Vec::new();
    for &c in &cuts {
        push_with_ulps(&mut out, c, ULP_WALK_CUT);
    }
    // Add: q ≈ k − a (f64), the half-ulp flat-spot family for q+a ⋈ k.
    for &a in &derived {
        for &k in &derived {
            push_with_ulps(&mut out, k - a, ULP_WALK_DERIVED);
        }
    }
    // Mul: q ≈ k / fl(c).
    for &c in &derived {
        if c == 0.0 {
            continue;
        }
        for &k in &derived {
            push_with_ulps(&mut out, k / c, ULP_WALK_DERIVED);
        }
    }
    // Ratio clearing: fl(c)·fl(d) and ulps (next_up of cleared bound).
    for &c in &derived {
        for &d in &derived {
            push_with_ulps(&mut out, c * d, ULP_WALK_DERIVED);
        }
    }
    clamp_and_dedup(out)
}

fn push_event(ext: &ExtDecls, events: &mut Vec<Event>, line: String) {
    if events.len() >= MAX_REFUTE_PROBES {
        return;
    }
    let e = parse_event(&line, ext).unwrap_or_else(|err| {
        panic!("refute-gate probe failed the loader: {err}\n{line}")
    });
    events.push(e);
}

/// Loader-valid probe events for the given cut constants.
///
/// Deterministic, budget-aware order: for every scalar emit MET then Jet
/// first (covers scalar cuts and object-pT ratio cuts), then a second pass
/// for HT / Electron / Muon. The cut-anchored part is capped at
/// [`MAX_REFUTE_PROBES`]; the fixed **absence family**
/// ([`adl_interp::sample::absence_family`]) is appended OUTSIDE that budget
/// so an f64-heavy unit can never crowd out the absent-property probes —
/// they are anchors of a different axis, not speculative neighbours.
#[must_use]
pub fn probe_events(ext: &ExtDecls, cut_consts: &[f64]) -> Vec<Event> {
    let scalars = probe_scalars(cut_consts);
    let mut events = Vec::new();
    // Pass 1 — high-value channels for every scalar.
    for &v in &scalars {
        if events.len() >= MAX_REFUTE_PROBES {
            break;
        }
        push_event(ext, &mut events, met_boundary_json(v));
        push_event(ext, &mut events, obj_boundary_json("Jet", v));
    }
    // Pass 2 — remaining channels.
    for &v in &scalars {
        if events.len() >= MAX_REFUTE_PROBES {
            break;
        }
        push_event(ext, &mut events, ht_boundary_json(v));
        push_event(ext, &mut events, obj_boundary_json("Electron", v));
        push_event(ext, &mut events, obj_boundary_json("Muon", v));
    }
    // Pass 3 — the absence axis (SPEC_PRESENCE_MODEL §9 step 1).
    for line in absence_family() {
        let e = parse_event(&line, ext)
            .unwrap_or_else(|err| panic!("refute-gate absence probe failed the loader: {err}\n{line}"));
        events.push(e);
    }
    events
}

fn memb(interp: &Interp<'_>, idx: usize, e: &Event) -> Option<bool> {
    interp.eval_region_membership_idx(idx, e).ok()
}

/// DISJOINT refutation: an event the interpreter accepts in both regions.
#[must_use]
pub fn search_shared_membership(
    interp: &Interp<'_>,
    ia: usize,
    ib: usize,
    probes: &[Event],
) -> Option<Event> {
    for e in probes {
        if memb(interp, ia, e) == Some(true) && memb(interp, ib, e) == Some(true) {
            return Some(e.clone());
        }
    }
    None
}

/// EMPTY refutation: an event that is a member of the region.
#[must_use]
pub fn search_membership(
    interp: &Interp<'_>,
    idx: usize,
    probes: &[Event],
) -> Option<Event> {
    for e in probes {
        if memb(interp, idx, e) == Some(true) {
            return Some(e.clone());
        }
    }
    None
}

/// SUBSET refutation: an event in `sub` but not in `sup`.
#[must_use]
pub fn search_subset_counterexample(
    interp: &Interp<'_>,
    sub: usize,
    sup: usize,
    probes: &[Event],
) -> Option<Event> {
    for e in probes {
        if memb(interp, sub, e) == Some(true) && memb(interp, sup, e) == Some(false) {
            return Some(e.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_scalars_cover_c4_c5_ce14_families() {
        // C4: MET+0.5 <= 1 vs MET > 0.5 → half-ulp witness.
        let c4 = probe_scalars(&[0.5, 1.0]);
        assert!(
            c4.contains(&0.5000000000000001),
            "C4 half-ulp missing from {c4:?}"
        );
        // C5: MET*0.3 <= 1 → k/fl(c).
        let c5 = probe_scalars(&[0.3, 1.0, 3.3333333333333335]);
        assert!(
            c5.contains(&3.3333333333333335),
            "C5 mul quotient missing from {c5:?}"
        );
        // CE-14: pT/0.3 <= 0.1 → next_up(0.1*0.3).
        let ce14 = probe_scalars(&[0.3, 0.1, 0.03]);
        let cleared = 0.1_f64 * 0.3_f64;
        assert!(
            ce14.iter().any(|&v| v == cleared.next_up()),
            "CE-14 next_up(cleared) missing; cleared={cleared:?} probes={ce14:?}"
        );
    }

    #[test]
    fn probe_events_include_c4_witness_met() {
        let ext = ExtDecls::legacy();
        let (pt_key, _) = ext.prop_canon("pt");
        let w = 0.5000000000000001_f64;
        let events = probe_events(&ext, &[0.5, 1.0]);
        assert!(
            events
                .iter()
                .any(|e| e.met.get(&pt_key).is_some_and(|r| r.to_f64() == w)),
            "C4 MET witness must survive the probe budget"
        );
    }

    #[test]
    fn probe_events_are_capped_and_loader_valid() {
        let ext = ExtDecls::legacy();
        let many: Vec<f64> = (0..40).map(f64::from).collect();
        let events = probe_events(&ext, &many);
        assert!(events.len() <= MAX_REFUTE_PROBES + absence_family().len());
        assert!(!events.is_empty());
    }

    /// The absence axis must survive a unit with a hundred cut constants —
    /// it is an anchor family, not a speculative neighbour.
    #[test]
    fn absence_probes_survive_a_saturated_cut_budget() {
        let ext = ExtDecls::legacy();
        let many: Vec<f64> = (0..40).map(f64::from).collect();
        let events = probe_events(&ext, &many);
        assert!(
            events.iter().any(|e| e.met.is_empty()),
            "MET-less probe crowded out by cut anchors"
        );
        assert!(
            events.iter().any(|e| e
                .collections
                .get("jet")
                .is_some_and(|js| !js.is_empty() && js.iter().all(|j| j.get("btag").is_none()))),
            "btag-less jet probe crowded out by cut anchors"
        );
    }

    /// The budget must be spent on the cuts the region actually names, not on
    /// whichever derived value happens to be numerically smallest.
    #[test]
    fn every_cut_anchor_survives_the_probe_budget() {
        let ext = ExtDecls::legacy();
        let (pt_key, _) = ext.prop_canon("pt");
        let cuts = [30.0, 100.0, 250.0, 800.0];
        let events = probe_events(&ext, &cuts);
        for c in cuts {
            let want = adl_sema::Rat::from_decimal_f64(c).unwrap();
            assert!(
                events
                    .iter()
                    .any(|e| e.met.get(&pt_key) == Some(&want)),
                "cut anchor {c} never reached the probe battery"
            );
        }
    }

    /// A `< 0` anchor must not become a negative-pT event: that event is
    /// outside the axioms' domain and could "refute" a true claim.
    #[test]
    fn negative_anchors_are_clamped_into_the_domain() {
        let ext = ExtDecls::legacy();
        let (pt_key, _) = ext.prop_canon("pt");
        assert!(probe_scalars(&[-5.0, 0.0]).iter().all(|v| *v >= 0.0));
        for e in probe_events(&ext, &[-5.0, -1.0, 0.0]) {
            if let Some(met) = e.met.get(&pt_key) {
                assert!(!met.is_negative(), "negative MET probe: {}", met.to_f64());
            }
            for objs in e.collections.values() {
                for o in objs {
                    if let Some(pt) = o.get(&pt_key) {
                        assert!(!pt.is_negative(), "negative pT probe: {}", pt.to_f64());
                    }
                }
            }
        }
    }
}
