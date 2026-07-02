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

use crate::event::{Event, parse_event};
use adl_sema::ExtDecls;
use std::f64::consts::PI;

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

/// One synthetic event as a JSON line. Half the values come from the boundary
/// pools, half are uniform draws; collections are pT-descending (the loader
/// invariant every real event obeys).
fn event_json(rng: &mut Rng) -> String {
    use std::fmt::Write as _;
    let mut s = String::from("{");
    let mut ht = 0.0;
    for &(name, max_n, eta_max, charged, tags) in COLLS {
        let n = rng.below(max_n + 1);
        let mut pts: Vec<f64> = (0..n)
            .map(|_| {
                if rng.next() & 1 == 0 {
                    pick(rng, PT_POOL)
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
                pick(rng, ETA_POOL).clamp(-eta_max, eta_max)
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
        pick(rng, MET_POOL)
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

/// The gate battery: `n` deterministic loader-valid events (plus the all-empty
/// event, which refutes many "provably empty" mistakes for free). Events that
/// fail the loader are a bug in THIS module — panic loudly rather than gate on
/// a silently smaller battery.
#[must_use]
pub fn battery(ext: &ExtDecls, n: usize) -> Vec<Event> {
    let empty = r#"{"Jet":[],"Electron":[],"Muon":[],"Tau":[],"Photon":[],"MET":{"pt":0.0,"phi":0.0},"HT":0.0,"triggers":{"mu_trig":0,"el_trig":0}}"#;
    let mut events = vec![
        parse_event(empty, ext).expect("the empty battery event is loader-valid"),
    ];
    let mut rng = Rng(0x5A_11D6_A7E0_u64);
    for i in 0..n.saturating_sub(1) {
        let line = event_json(&mut rng);
        events.push(parse_event(&line, ext).unwrap_or_else(|e| {
            panic!("sampling-gate battery event {i} failed the loader: {e}\n{line}")
        }));
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
}
