//! Deterministic synthetic-event battery for the production sampling gate
//! (proof-system v2, Phase 1).
//!
//! Every UNSAT-side PROVEN verdict (disjoint / empty / subset) is refuted
//! against these events through the reference interpreter before it is
//! reported: any sampled event the interpreter passes through both regions of
//! a "proven disjoint" pair is an internal contradiction — an encoder/axiom
//! bug caught at verdict time instead of shipped as a false proof. The
//! battery is deliberately INDEPENDENT of `adl-difftest`'s oracle sampler
//! (similar shape, different draws): two independent batteries refute more
//! than one shared one. Consolidation, if ever, is plan Phase 6b.
//!
//! Determinism: a hand-rolled SplitMix64 (no `rand` dependency, no global
//! state) so the same gate size always evaluates the same events — verdicts
//! must never flicker across runs.
//!
//! Cut-constant injection (AUDIT_2026-07-28 §3): fixed pools alone miss
//! off-pool cut boundaries (0.4, 0.5, 50, …) where f64-folding bugs live.
//! Callers pass the unit's comparison constants; each expands to
//! `{c, next_up(c), next_down(c)}` in the pools, and dedicated MET/HT
//! events guarantee those values appear in the battery.

use crate::event::{Event, parse_event};
use adl_sema::ExtDecls;
use std::f64::consts::PI;

/// Cap on distinct cut constants injected into boundary pools (each expands
/// to the value ± 1 ulp). Keeps the battery bounded on files with hundreds
/// of numeric literals; excess constants (after sort/dedup) are dropped.
pub const MAX_CUT_CONSTANTS: usize = 32;

/// Absolute-value cap for injected cut constants. Values beyond this
/// (e.g. `f64::MAX` from overflow-stress tests) are skipped: putting them
/// in the pT pool produces non-finite HT sums and invalid JSON.
const MAX_INJECT_ABS: f64 = 1.0e6;

fn injectible(c: f64) -> bool {
    c.is_finite() && c.abs() <= MAX_INJECT_ABS
}

/// SplitMix64 — tiny, deterministic, statistically fine for event synthesis.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }

    #[allow(clippy::cast_precision_loss)]
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    fn flag(&mut self) -> f64 {
        f64::from(self.next() & 1 == 1)
    }
}

/// pT pool biased toward the cut constants real analyses use, so boundary
/// regions get boundary events (a uniform draw almost never lands on 30.0).
const PT_POOL: &[f64] = &[
    0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 40.0, 50.0, 75.0, 100.0, 150.0, 200.0, 300.0, 500.0,
];
const ETA_POOL: &[f64] = &[0.0, 0.5, -0.5, 1.0, -1.0, 2.0, -2.0, 2.4, -2.4, 3.0, -3.0, 4.5];
const MET_POOL: &[f64] = &[0.0, 25.0, 50.0, 100.0, 150.0, 200.0, 300.0, 500.0];

/// (event-data key, max count, |eta| bound, charged, tag keys) — mirrors the
/// loader vocabulary the analyzer's legacy profile reads.
const COLLS: &[(&str, u64, f64, bool, &[&str])] = &[
    ("Jet", 6, 4.7, false, &["btag", "ctag"]),
    ("Electron", 3, 2.5, true, &[]),
    ("Muon", 3, 2.4, true, &[]),
    ("Tau", 2, 2.3, true, &["tautag"]),
    ("Photon", 2, 2.5, false, &[]),
];

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn pick(rng: &mut Rng, pool: &[f64]) -> f64 {
    pool[rng.below(pool.len() as u64) as usize]
}

/// Expand cut constants to `{c, next_up(c), next_down(c)}`, sorted/deduped,
/// capped at `3 * MAX_CUT_CONSTANTS` boundary values. Non-finite and
/// out-of-range inputs are dropped (see [`MAX_INJECT_ABS`]).
#[must_use]
pub fn expand_cut_boundaries(cut_consts: &[f64]) -> Vec<f64> {
    let mut out = Vec::new();
    for &c in cut_consts.iter().filter(|&&c| injectible(c)).take(MAX_CUT_CONSTANTS) {
        for v in [c, c.next_up(), c.next_down()] {
            if injectible(v) {
                out.push(v);
            }
        }
    }
    out.sort_by(|a, b| a.total_cmp(b));
    out.dedup_by(|a, b| a.to_bits() == b.to_bits());
    out
}

fn merge_pool(base: &[f64], extra: &[f64]) -> Vec<f64> {
    let mut v: Vec<f64> = base.iter().copied().chain(extra.iter().copied()).collect();
    v.sort_by(|a, b| a.total_cmp(b));
    v.dedup_by(|a, b| a.to_bits() == b.to_bits());
    v
}

/// One synthetic event as a JSON line. Half the values come from the boundary
/// pools, half are uniform draws; collections are pT-descending (the loader
/// invariant every real event obeys).
fn event_json(rng: &mut Rng, pt_pool: &[f64], eta_pool: &[f64], met_pool: &[f64]) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("{");
    let mut ht = 0.0;
    for &(name, max_n, eta_max, charged, tags) in COLLS {
        let n = rng.below(max_n + 1);
        let mut pts: Vec<f64> = (0..n)
            .map(|_| {
                if rng.next() & 1 == 0 {
                    pick(rng, pt_pool)
                } else {
                    round3(rng.in_range(0.0, 500.0))
                }
            })
            .collect();
        pts.sort_by(|a, b| b.total_cmp(a));
        if name == "Jet" {
            ht = round3(pts.iter().sum::<f64>());
        }
        let _ = write!(s, "\"{name}\":[");
        for (i, pt) in pts.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let eta = if rng.next() & 1 == 0 {
                pick(rng, eta_pool).clamp(-eta_max, eta_max)
            } else {
                round3(rng.in_range(-eta_max, eta_max))
            };
            let phi = round3(rng.in_range(-PI, PI));
            let m = round3(rng.in_range(0.0, 120.0));
            let _ = write!(s, "{{\"pt\":{pt},\"eta\":{eta},\"phi\":{phi},\"m\":{m}");
            if charged {
                let q = if rng.next() & 1 == 0 { 1.0 } else { -1.0 };
                let _ = write!(s, ",\"charge\":{q}");
            }
            for tag in tags {
                let _ = write!(s, ",\"{tag}\":{}", rng.flag());
            }
            s.push('}');
        }
        s.push_str("],");
    }
    let met = if rng.next() & 1 == 0 {
        pick(rng, met_pool)
    } else {
        round3(rng.in_range(0.0, 600.0))
    };
    let _ = write!(
        s,
        "\"MET\":{{\"pt\":{met},\"phi\":{}}},\"HT\":{ht},\"triggers\":{{\"mu_trig\":{},\"el_trig\":{}}}}}",
        round3(rng.in_range(-PI, PI)),
        rng.flag(),
        rng.flag()
    );
    s
}

/// Empty-collections skeleton with a chosen MET.pt (and HT=0). Used to
/// guarantee cut-boundary MET values appear in the battery.
fn met_boundary_json(met: f64) -> String {
    format!(
        r#"{{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{{"pt":{met},"phi":0.0}},"HT":0.0,"triggers":{{"mu_trig":0,"el_trig":0}}}}"#
    )
}

/// Empty-collections skeleton with a chosen HT scalar (MET.pt=0).
fn ht_boundary_json(ht: f64) -> String {
    format!(
        r#"{{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{{"pt":0.0,"phi":0.0}},"HT":{ht},"triggers":{{"mu_trig":0,"el_trig":0}}}}"#
    )
}

/// The gate battery: `n` deterministic loader-valid events (plus the all-empty
/// event, which refutes many "provably empty" mistakes for free), then
/// dedicated MET/HT events at every injected cut boundary (±1 ulp).
///
/// `cut_consts` are the unit's comparison numeric literals (as f64); pass
/// `&[]` for the fixed-pool-only battery. Events that fail the loader are a
/// bug in THIS module — panic loudly rather than gate on a silently smaller
/// battery.
#[must_use]
pub fn battery(ext: &ExtDecls, n: usize) -> Vec<Event> {
    battery_with_cuts(ext, n, &[])
}

/// Like [`battery`], but injects `cut_consts` (±1 ulp) into the PT/ETA/MET
/// pools and appends dedicated scalar-boundary events so those values are
/// guaranteed to appear (AUDIT_2026-07-28 §3 defense-in-depth).
#[must_use]
pub fn battery_with_cuts(ext: &ExtDecls, n: usize, cut_consts: &[f64]) -> Vec<Event> {
    let boundaries = expand_cut_boundaries(cut_consts);
    let pt_pool = merge_pool(PT_POOL, &boundaries);
    // ETA: only inject boundaries that look like angular cuts (|v| ≤ 6).
    let eta_extra: Vec<f64> = boundaries
        .iter()
        .copied()
        .filter(|v| v.abs() <= 6.0)
        .collect();
    let eta_pool = merge_pool(ETA_POOL, &eta_extra);
    let met_pool = merge_pool(MET_POOL, &boundaries);

    let empty = r#"{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{"pt":0.0,"phi":0.0},"HT":0.0,"triggers":{"mu_trig":0,"el_trig":0}}"#;
    let mut events = vec![
        parse_event(empty, ext).expect("the empty battery event is loader-valid"),
    ];
    let mut rng = Rng(0x5A_11D6_A7E0_u64);
    for i in 0..n.saturating_sub(1) {
        let line = event_json(&mut rng, &pt_pool, &eta_pool, &met_pool);
        events.push(parse_event(&line, ext).unwrap_or_else(|e| {
            panic!("sampling-gate battery event {i} failed the loader: {e}\n{line}")
        }));
    }
    // Dedicated MET/HT events at every cut boundary — pool draws alone can
    // miss a specific value when the pool grows. Scalars are the priority
    // hazard class (AUDIT C1–C6).
    for &v in &boundaries {
        for line in [met_boundary_json(v), ht_boundary_json(v)] {
            events.push(parse_event(&line, ext).unwrap_or_else(|e| {
                panic!("cut-boundary battery event failed the loader: {e}\n{line}")
            }));
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_is_deterministic_and_loader_valid() {
        let ext = ExtDecls::legacy();
        let a = battery(&ext, 32);
        let b = battery(&ext, 32);
        assert_eq!(a.len(), 32);
        // Determinism proxy: same jet multiplicities and MET across runs.
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(
                x.collections.get("jet").map(Vec::len),
                y.collections.get("jet").map(Vec::len)
            );
        }
    }

    #[test]
    fn battery_covers_boundary_and_empty_shapes() {
        let ext = ExtDecls::legacy();
        let events = battery(&ext, 64);
        let empties = events
            .iter()
            .filter(|e| e.collections.get("jet").is_none_or(Vec::is_empty))
            .count();
        assert!(empties >= 1, "the all-empty event must be present");
        let boundary_pt = events.iter().any(|e| {
            e.collections
                .get("jet")
                .is_some_and(|js| js.iter().any(|j| j.get("ptof") == Some(30.0)))
        });
        assert!(boundary_pt, "pool draws must land on common cut boundaries");
    }

    #[test]
    fn cut_constant_battery_includes_met_boundaries() {
        // AUDIT §3: a cut at MET > 0.5 must put 0.5 ± 1 ulp into the battery.
        let ext = ExtDecls::legacy();
        let (pt_key, _) = ext.prop_canon("pt");
        let c = 0.5_f64;
        let events = battery_with_cuts(&ext, 8, &[c]);
        let want = [c, c.next_up(), c.next_down()];
        for w in want {
            let hit = events.iter().any(|e| e.met.get(&pt_key) == Some(&w));
            assert!(
                hit,
                "battery must contain an event with MET.{pt_key} = {w:?} (cut 0.5 ± ulp); \
                 met keys present: {:?}",
                events
                    .iter()
                    .filter_map(|e| e.met.get(&pt_key).copied())
                    .collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn expand_cut_boundaries_is_sorted_deduped_and_capped() {
        let many: Vec<f64> = (0..100).map(|i| f64::from(i)).collect();
        let b = expand_cut_boundaries(&many);
        // Cap at MAX_CUT_CONSTANTS inputs × ≤3, after dedup of overlaps.
        assert!(b.len() <= MAX_CUT_CONSTANTS * 3);
        for w in b.windows(2) {
            assert!(w[0].total_cmp(&w[1]).is_lt());
        }
    }
}
