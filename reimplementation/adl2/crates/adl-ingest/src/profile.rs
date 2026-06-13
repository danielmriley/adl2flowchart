//! Converter profiles (SPEC_EVENT_PIPELINE §1): a **pure data table**
//! mapping experiment branch names to the canonical event model. The core
//! `Event`/`Interp` never see experiment names; everything
//! experiment-specific lives here.
//!
//! Convention-dependent choices are explicit `[DECIDE]` entries in the
//! spec; the constructed profile records the ratified-or-recommended
//! default for each, and [`Profile::decides`] surfaces the per-run values
//! (for `--verbose` now, provenance in §6 later).

use std::fmt::Write as _;

/// How a leaf's raw values become a canonical property value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafKind {
    /// `float[]` widened f32 → f64.
    F32,
    /// `int32_t[]` (charges, ...): emitted as a JSON integer.
    I32,
    /// `uint32_t[]` working-point bitmask → flag ∈ {0, 1} from the given
    /// bit ([DECIDE-I1]); set higher/other bits are *diagnosed*, never
    /// silently folded in.
    TagBit(u32),
}

/// One mapped leaf of a collection branch: `<branch>.<leaf>` → `prop`.
#[derive(Debug, Clone)]
pub struct LeafSpec {
    /// Leaf name after the dot (`"PT"`).
    pub leaf: String,
    /// Canonical property key emitted in JSONL (`"pt"`).
    pub prop: String,
    pub kind: LeafKind,
}

/// One mapped object collection (`Jet`, `Electron`, ...).
#[derive(Debug, Clone)]
pub struct CollectionSpec {
    /// Branch name prefix in the tree (`"Jet"` → leaves `Jet.PT`, ... and
    /// counter `Jet_size`).
    pub branch: String,
    /// Emission key in JSONL; its lowercase form is the canonical
    /// base-collection name (`"FatJet"` → `fatjet`, [DECIDE-I3]).
    pub key: String,
    /// Mapped leaves, in canonical emission order.
    pub leaves: Vec<LeafSpec>,
    /// Constant properties appended to every object (`("m", 0.000511)`
    /// for electrons — PDG masses, [DECIDE-I2]). Profile-side, never
    /// core-model-side.
    pub constants: Vec<(String, f64)>,
}

/// The per-event missing-momentum vector (one-element collection branch).
#[derive(Debug, Clone)]
pub struct MetSpec {
    /// Branch prefix (`"MissingET"`).
    pub branch: String,
    /// Leaf giving the magnitude (`"MET"` → `MET.pt`).
    pub pt_leaf: String,
    /// Leaf giving the azimuth (`"Phi"` → `MET.phi`).
    pub phi_leaf: String,
    /// Leaves of this branch the profile drops *by design* (`"Eta"`: a
    /// transverse vector has no η).
    pub known_dropped_leaves: Vec<String>,
}

/// A per-event scalar read from a one-element collection branch.
#[derive(Debug, Clone)]
pub struct ScalarSpec {
    /// Branch prefix (`"ScalarHT"`).
    pub branch: String,
    /// Leaf name (`"HT"`).
    pub leaf: String,
    /// Emission key in JSONL (`"HT"`).
    pub key: String,
}

/// The event-weight source ([DECIDE-I4]).
#[derive(Debug, Clone)]
pub struct WeightSpec {
    /// Branch prefix (`"Event"`).
    pub branch: String,
    /// Leaf name (`"Weight"`).
    pub leaf: String,
}

/// A converter profile: the complete branch → canonical-model data table.
#[derive(Debug, Clone)]
pub struct Profile {
    /// Profile name (`"delphes"`).
    pub name: String,
    /// Profile version; `name/version` identifies the mapping in
    /// provenance (`"delphes/1"`).
    pub version: u32,
    /// Name of the TTree holding the events (`"Delphes"`).
    pub tree: String,
    pub collections: Vec<CollectionSpec>,
    pub met: Option<MetSpec>,
    pub scalars: Vec<ScalarSpec>,
    pub weight: Option<WeightSpec>,
    /// `(branch, leaf)` of the LHE multiweight vector — dropped in v1
    /// with a once-per-file diagnostic, per the spec table.
    pub lhe_weights: Option<(String, String)>,
    /// Branch prefixes whose leaves are dropped *by design* (gen-level
    /// content, bookkeeping). Listed under `--verbose`, silent otherwise.
    pub known_drop_branches: Vec<String>,
}

impl Profile {
    /// `name/version` identity string used in diagnostics and provenance.
    #[must_use]
    pub fn id(&self) -> String {
        format!("{}/{}", self.name, self.version)
    }

    /// The per-run `[DECIDE]` choices baked into this table, derived from
    /// the table itself (never a second copy): `(key, value)` pairs in
    /// deterministic order. Surfaced by `--verbose` and, later, §6
    /// provenance.
    #[must_use]
    pub fn decides(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        // Tag bits, per tag property, if any collection maps one.
        for tag in ["btag", "tautag"] {
            if let Some(bit) = self
                .collections
                .iter()
                .flat_map(|c| &c.leaves)
                .find_map(|l| match l.kind {
                    LeafKind::TagBit(b) if l.prop == tag => Some(b),
                    _ => None,
                })
            {
                out.push((format!("{tag}_bit"), bit.to_string()));
            }
        }
        // Lepton (constant) masses.
        let mut masses = String::new();
        for c in &self.collections {
            for (prop, v) in &c.constants {
                if prop == "m" {
                    if !masses.is_empty() {
                        masses.push_str(", ");
                    }
                    let _ = write!(masses, "{} {v}", c.key);
                }
            }
        }
        if !masses.is_empty() {
            out.push(("lepton_mass".to_owned(), format!("pdg ({masses})")));
        }
        // FatJet canonical name, when the profile maps a fat-jet branch.
        if let Some(c) = self.collections.iter().find(|c| c.branch == "FatJet") {
            out.push(("fatjet_name".to_owned(), c.key.to_ascii_lowercase()));
        }
        if let Some(w) = &self.weight {
            out.push((
                "weight_branch".to_owned(),
                format!("{}.{}", w.branch, w.leaf),
            ));
        }
        out
    }
}

fn f32_leaf(leaf: &str, prop: &str) -> LeafSpec {
    LeafSpec {
        leaf: leaf.to_owned(),
        prop: prop.to_owned(),
        kind: LeafKind::F32,
    }
}

fn jet_like_leaves(btag_bit: u32, tautag_bit: u32) -> Vec<LeafSpec> {
    vec![
        f32_leaf("PT", "pt"),
        f32_leaf("Eta", "eta"),
        f32_leaf("Phi", "phi"),
        f32_leaf("Mass", "m"),
        LeafSpec {
            leaf: "BTag".to_owned(),
            prop: "btag".to_owned(),
            kind: LeafKind::TagBit(btag_bit),
        },
        LeafSpec {
            leaf: "TauTag".to_owned(),
            prop: "tautag".to_owned(),
            kind: LeafKind::TagBit(tautag_bit),
        },
    ]
}

fn lepton_leaves() -> Vec<LeafSpec> {
    vec![
        f32_leaf("PT", "pt"),
        f32_leaf("Eta", "eta"),
        f32_leaf("Phi", "phi"),
        LeafSpec {
            leaf: "Charge".to_owned(),
            prop: "q".to_owned(),
            kind: LeafKind::I32,
        },
    ]
}

/// The Delphes 3.4.x profile (SPEC_EVENT_PIPELINE §1.2), with the spec's
/// recommended defaults for every `[DECIDE]` entry:
///
/// - **[DECIDE-I1]** `BTag`/`TauTag` use bit 0 (the card's default working
///   point); other set bits are diagnosed, not folded in.
/// - **[DECIDE-I2]** lepton masses are the PDG constants (e 0.000511 GeV,
///   μ 0.105658 GeV), as profile constants.
/// - **[DECIDE-I3]** the fat-jet collection is canonical `fatjet`
///   (spelling aliases handled by the base-collection spelling map).
/// - **[DECIDE-I4]** the event weight is `Event.Weight`.
#[must_use]
pub fn delphes() -> Profile {
    let col = |branch: &str, leaves: Vec<LeafSpec>, constants: Vec<(String, f64)>| CollectionSpec {
        branch: branch.to_owned(),
        key: branch.to_owned(),
        leaves,
        constants,
    };
    Profile {
        name: "delphes".to_owned(),
        version: 1,
        tree: "Delphes".to_owned(),
        collections: vec![
            col("Jet", jet_like_leaves(0, 0), vec![]),
            col("FatJet", jet_like_leaves(0, 0), vec![]),
            col(
                "Electron",
                lepton_leaves(),
                vec![("m".to_owned(), 0.000511)],
            ),
            col("Muon", lepton_leaves(), vec![("m".to_owned(), 0.105658)]),
            col(
                "Photon",
                vec![
                    f32_leaf("PT", "pt"),
                    f32_leaf("Eta", "eta"),
                    f32_leaf("Phi", "phi"),
                    f32_leaf("E", "e"),
                ],
                vec![],
            ),
        ],
        met: Some(MetSpec {
            branch: "MissingET".to_owned(),
            pt_leaf: "MET".to_owned(),
            phi_leaf: "Phi".to_owned(),
            known_dropped_leaves: vec!["Eta".to_owned()],
        }),
        scalars: vec![ScalarSpec {
            branch: "ScalarHT".to_owned(),
            leaf: "HT".to_owned(),
            key: "HT".to_owned(),
        }],
        weight: Some(WeightSpec {
            branch: "Event".to_owned(),
            leaf: "Weight".to_owned(),
        }),
        lhe_weights: Some(("Weight".to_owned(), "Weight".to_owned())),
        known_drop_branches: vec![
            "Event".to_owned(),
            "Weight".to_owned(),
            "GenJet".to_owned(),
            "GenMissingET".to_owned(),
        ],
    }
}

/// Look up a profile by CLI name. `None` for unknown names; the caller
/// reports the supported list.
#[must_use]
pub fn by_name(name: &str) -> Option<Profile> {
    match name.to_ascii_lowercase().as_str() {
        "delphes" | "delphes/1" => Some(delphes()),
        _ => None,
    }
}

/// The profile names [`by_name`] accepts (for error messages).
pub const KNOWN_PROFILES: &[&str] = &["delphes"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delphes_decides_surface_the_spec_defaults() {
        let p = delphes();
        let d = p.decides();
        let get = |k: &str| {
            d.iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.as_str())
                .unwrap_or_else(|| panic!("missing decide {k}"))
        };
        assert_eq!(get("btag_bit"), "0");
        assert_eq!(get("tautag_bit"), "0");
        assert_eq!(get("lepton_mass"), "pdg (Electron 0.000511, Muon 0.105658)");
        assert_eq!(get("fatjet_name"), "fatjet");
        assert_eq!(get("weight_branch"), "Event.Weight");
        assert_eq!(p.id(), "delphes/1");
    }

    #[test]
    fn by_name_is_case_insensitive_and_total_over_known() {
        assert!(by_name("Delphes").is_some());
        assert!(by_name("delphes/1").is_some());
        assert!(by_name("nanoaod").is_none());
        for name in KNOWN_PROFILES {
            assert!(by_name(name).is_some(), "{name}");
        }
    }
}
