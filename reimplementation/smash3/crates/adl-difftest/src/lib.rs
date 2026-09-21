//! `adl-difftest` — generators, sampling oracle, CutLang/legacy harnesses
//! (SPEC_ARCHITECTURE §1; TESTING.md §1–2).
//!
//! Phase 3 contributes the **deterministic seeded toy-event generator**:
//! physically plausible JSONL event records for driving the reference
//! interpreter (and, later, the encoder-vs-interpreter property tests).
//!
//! Guarantees (locked by this crate's tests):
//! - same seed ⇒ byte-identical JSONL (determinism);
//! - every collection is pT-descending with `pt ≥ 0` (PHASE0: events
//!   arrive ordered, the interpreter never re-sorts);
//! - `|eta|` bounded per collection, `phi ∈ [−π, π)`, `m ≥ 0`;
//! - tags (`btag`/`ctag`/`tautag`) and trigger flags ∈ {0, 1};
//! - `HT` = Σ jet pT ≥ 0, `MET.pt ≥ 0`.

pub mod casegen;
pub mod oracle;

use adl_interp::{Event, EventError, read_jsonl};
use adl_sema::ExtDecls;
use serde_json::{Map, Value};

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-difftest";

/// SplitMix64: tiny, deterministic, seedable PRNG (public-domain
/// algorithm; no external dependency, identical on every platform).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)` with 53 bits of precision.
    pub fn next_f64(&mut self) -> f64 {
        #[allow(clippy::cast_precision_loss)] // 53-bit mantissa by construction
        let v = (self.next_u64() >> 11) as f64;
        v / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// Uniform in `0..n` (`0` when `n == 0`).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            usize::try_from(self.next_u64() % n as u64).unwrap_or(0)
        }
    }

    /// A fair 0/1 flag.
    pub fn flag(&mut self) -> u8 {
        (self.next_u64() & 1) as u8
    }
}

/// Per-collection generation spec: physical ranges per SPEC_LANGUAGE §4.1.
struct CollSpec {
    name: &'static str,
    max_n: usize,
    eta_max: f64,
    charged: bool,
    tags: &'static [&'static str],
}

const COLLS: &[CollSpec] = &[
    CollSpec {
        name: "Jet",
        max_n: 6,
        eta_max: 4.7,
        charged: false,
        tags: &["btag", "ctag"],
    },
    CollSpec {
        name: "Electron",
        max_n: 3,
        eta_max: 2.5,
        charged: true,
        tags: &[],
    },
    CollSpec {
        name: "Muon",
        max_n: 3,
        eta_max: 2.4,
        charged: true,
        tags: &[],
    },
    CollSpec {
        name: "Tau",
        max_n: 2,
        eta_max: 2.3,
        charged: true,
        tags: &["tautag"],
    },
    CollSpec {
        name: "Photon",
        max_n: 2,
        eta_max: 2.5,
        charged: false,
        tags: &[],
    },
];

/// Trigger flags emitted on every toy event.
pub const TRIGGER_NAMES: &[&str] = &["mu_trig", "el_trig"];

/// The largest `|eta|` the generator can emit (loosest collection bound).
pub const ETA_BOUND: f64 = 4.7;

const PT_MAX: f64 = 500.0;
const MET_MAX: f64 = 600.0;
const MASS_MAX: f64 = 120.0;

/// Round to 3 decimals so the JSONL is compact and deterministic.
fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// Generate one toy event as a JSON value (deterministic given `rng`
/// state). serde_json's default map is ordered, so serialization is
/// byte-stable.
pub fn toy_event(rng: &mut SplitMix64) -> Value {
    use std::f64::consts::PI;
    let mut root = Map::new();
    let mut ht = 0.0;
    for spec in COLLS {
        let n = rng.below(spec.max_n + 1);
        // pt ≥ 0, sorted descending: events arrive pT-ordered (PHASE0).
        let mut pts: Vec<f64> = (0..n).map(|_| round3(rng.in_range(0.0, PT_MAX))).collect();
        pts.sort_by(|a, b| b.total_cmp(a));
        if spec.name == "Jet" {
            ht = pts.iter().sum::<f64>();
        }
        let objs: Vec<Value> = pts
            .into_iter()
            .map(|pt| {
                let mut o = Map::new();
                o.insert("pt".into(), pt.into());
                o.insert(
                    "eta".into(),
                    round3(rng.in_range(-spec.eta_max, spec.eta_max)).into(),
                );
                o.insert("phi".into(), round3(rng.in_range(-PI, PI)).into());
                o.insert("m".into(), round3(rng.in_range(0.0, MASS_MAX)).into());
                if spec.charged {
                    let q = if rng.flag() == 1 { 1.0 } else { -1.0 };
                    o.insert("charge".into(), q.into());
                }
                for tag in spec.tags {
                    o.insert((*tag).into(), f64::from(rng.flag()).into());
                }
                Value::Object(o)
            })
            .collect();
        root.insert(spec.name.into(), Value::Array(objs));
    }

    let mut met = Map::new();
    met.insert("pt".into(), round3(rng.in_range(0.0, MET_MAX)).into());
    met.insert("phi".into(), round3(rng.in_range(-PI, PI)).into());
    root.insert("MET".into(), Value::Object(met));
    root.insert("HT".into(), round3(ht).into());

    let mut triggers = Map::new();
    for name in TRIGGER_NAMES {
        triggers.insert((*name).into(), f64::from(rng.flag()).into());
    }
    root.insert("triggers".into(), Value::Object(triggers));

    Value::Object(root)
}

/// Generate `n_events` toy events as a JSONL document (one per line).
/// Deterministic: the same `seed` yields byte-identical output.
#[must_use]
pub fn toy_jsonl(seed: u64, n_events: usize) -> String {
    let mut rng = SplitMix64::new(seed);
    let mut out = String::new();
    for _ in 0..n_events {
        out.push_str(&toy_event(&mut rng).to_string());
        out.push('\n');
    }
    out
}

/// Generate toy events and round-trip them through the interpreter's
/// JSONL loader (which validates pT ordering and trigger flags).
///
/// # Errors
/// Returns an [`EventError`] if a generated record fails validation —
/// which would be a generator bug; the tests lock this.
pub fn toy_events(seed: u64, n_events: usize, ext: &ExtDecls) -> Result<Vec<Event>, EventError> {
    read_jsonl(&toy_jsonl(seed, n_events), ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-difftest");
    }

    #[test]
    fn splitmix_is_deterministic() {
        let mut a = SplitMix64::new(7);
        let mut b = SplitMix64::new(7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        let mut c = SplitMix64::new(8);
        assert_ne!(a.next_u64(), c.next_u64());
    }

    #[test]
    fn next_f64_is_in_unit_interval() {
        let mut rng = SplitMix64::new(1);
        for _ in 0..1000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v));
        }
    }
}
