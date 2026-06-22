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
    /// `float[]` (or `double[]`) widened to f64.
    F32,
    /// Any signed/unsigned integer width (`int8`..`uint32`) — charges,
    /// jet IDs, iso categories: emitted as a JSON integer, per-width
    /// decoded to i64.
    I32,
    /// `bool[]` (lepton quality flags like `tightId`): emitted as the
    /// JSON integer 0 or 1.
    Bool,
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

/// How a collection's per-event element count branch is named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterStyle {
    /// `<branch>_size` (Delphes).
    SizeSuffix,
    /// `n<branch>` (NanoAOD).
    NPrefix,
}

/// Branch-naming conventions of an input format, so the reader and the
/// `to_jsonl.py` oracle stay one data table across Delphes (dotted leaves,
/// `_size` counters, one-element MET/scalar branches) and NanoAOD
/// (underscored leaves, `n<branch>` counters, flat per-event scalars).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Naming {
    /// Separator between a collection branch and its leaf (`"."` | `"_"`).
    pub leaf_sep: &'static str,
    /// How element-count branches are named.
    pub counter: CounterStyle,
    /// MET / scalars / weight are flat one-value-per-event scalar branches
    /// (NanoAOD) rather than one-element collection branches read via a
    /// counter (Delphes).
    pub flat_event_vars: bool,
}

/// A converter profile: the complete branch → canonical-model data table.
#[derive(Debug, Clone)]
pub struct Profile {
    /// Branch-naming conventions (Delphes vs NanoAOD).
    pub naming: Naming,
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

    /// Full branch name of a leaf, per the profile's naming convention
    /// (`Jet.PT` for Delphes, `Jet_pt` for NanoAOD). An empty `leaf` names
    /// a bare top-level branch (`genWeight`).
    #[must_use]
    pub fn leaf_branch(&self, branch: &str, leaf: &str) -> String {
        if leaf.is_empty() {
            branch.to_owned()
        } else {
            format!("{branch}{}{leaf}", self.naming.leaf_sep)
        }
    }

    /// Element-count branch name for a collection (`Jet_size` | `nJet`).
    #[must_use]
    pub fn counter_branch(&self, branch: &str) -> String {
        match self.naming.counter {
            CounterStyle::SizeSuffix => format!("{branch}_size"),
            CounterStyle::NPrefix => format!("n{branch}"),
        }
    }

    /// Whether a leaf-branch name is an element-count branch under this
    /// naming convention; returns the collection prefix it counts if so.
    #[must_use]
    pub fn counter_prefix<'a>(&self, name: &'a str) -> Option<&'a str> {
        match self.naming.counter {
            CounterStyle::SizeSuffix => name.strip_suffix("_size"),
            CounterStyle::NPrefix => name
                .strip_prefix('n')
                .filter(|rest| rest.chars().next().is_some_and(char::is_uppercase)),
        }
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
                self.leaf_branch(&w.branch, &w.leaf),
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
        naming: Naming {
            leaf_sep: ".",
            counter: CounterStyle::SizeSuffix,
            flat_event_vars: false,
        },
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

/// The CMS NanoAOD profile (`Events` tree, `n<Coll>` counters, underscored
/// leaves, flat per-event MET/weight scalars). Maps the standard physics
/// objects; b-tagging is the continuous `btagDeepB` discriminant (a float
/// `btag` property, cut on directly in ADL — NanoAOD has no working-point
/// bit), masses come from each collection's own `mass` leaf, and the weight
/// is `genWeight`. Triggers (`HLT_*`) and gen/LHE branches are not mapped in
/// v1 (surfaced as unmapped/unknown diagnostics, never errors).
#[must_use]
pub fn nanoaod() -> Profile {
    fn kin() -> Vec<LeafSpec> {
        vec![
            f32_leaf("pt", "pt"),
            f32_leaf("eta", "eta"),
            f32_leaf("phi", "phi"),
            f32_leaf("mass", "m"),
        ]
    }
    let charge = || LeafSpec {
        leaf: "charge".to_owned(),
        prop: "q".to_owned(),
        kind: LeafKind::I32,
    };
    let i32_leaf = |leaf: &str, prop: &str| LeafSpec {
        leaf: leaf.to_owned(),
        prop: prop.to_owned(),
        kind: LeafKind::I32,
    };
    let bool_leaf = |leaf: &str, prop: &str| LeafSpec {
        leaf: leaf.to_owned(),
        prop: prop.to_owned(),
        kind: LeafKind::Bool,
    };
    // b-tag discriminants are CONTINUOUS floats and must be emitted under
    // their own NanoAOD/ADL spellings — NOT `btag` (a {0,1} tag bit under
    // the TAG axiom). `btagDeepB` (DeepCSV), `btagDeepFlavB` (DeepJet, the
    // official Run-2 recommendation), `btagCSVV2` all resolve to continuous
    // ADL identities, matching real cuts (`select btagDeepFlavB > 0.3`).
    // FatJet carries only `btagCSVV2` on this layout, so the extra DeepJet
    // discriminant is mapped on Jet only (a mapped-but-absent leaf is a hard
    // error, so we do not map branches a collection does not carry).
    let col = |branch: &str, leaves: Vec<LeafSpec>| CollectionSpec {
        branch: branch.to_owned(),
        key: branch.to_owned(),
        leaves,
        constants: vec![],
    };
    let mut jet = kin();
    jet.push(f32_leaf("btagDeepB", "btagDeepB"));
    jet.push(f32_leaf("btagDeepFlavB", "btagDeepFlavB"));
    jet.push(f32_leaf("btagCSVV2", "btagCSVV2"));
    jet.push(i32_leaf("jetId", "jetId"));
    jet.push(i32_leaf("puId", "puId"));
    let mut fatjet = kin();
    fatjet.push(f32_leaf("btagDeepB", "btagDeepB"));
    fatjet.push(f32_leaf("btagCSVV2", "btagCSVV2"));
    let lepton = || {
        let mut v = kin();
        v.push(charge());
        v
    };
    // Muon quality IDs (`bool[]`) and the iso-working-point category
    // (`pfIsoId`, `uint8[]`): real preselections cut `select tightId == 1`
    // and `select pfIsoId >= 4`.
    let muon = || {
        let mut v = lepton();
        v.push(bool_leaf("tightId", "tightId"));
        v.push(bool_leaf("looseId", "looseId"));
        v.push(i32_leaf("pfIsoId", "pfIsoId"));
        v
    };

    Profile {
        naming: Naming {
            leaf_sep: "_",
            counter: CounterStyle::NPrefix,
            flat_event_vars: true,
        },
        name: "nanoaod".to_owned(),
        version: 1,
        tree: "Events".to_owned(),
        collections: vec![
            col("Jet", jet),
            col("FatJet", fatjet),
            col("Electron", lepton()),
            col("Muon", muon()),
            col("Tau", lepton()),
            col("Photon", kin()),
        ],
        met: Some(MetSpec {
            branch: "MET".to_owned(),
            pt_leaf: "pt".to_owned(),
            phi_leaf: "phi".to_owned(),
            known_dropped_leaves: vec![],
        }),
        scalars: vec![],
        weight: Some(WeightSpec {
            branch: "genWeight".to_owned(),
            leaf: String::new(),
        }),
        lhe_weights: None,
        known_drop_branches: vec![],
    }
}

/// Look up a profile by CLI name. `None` for unknown names; the caller
/// reports the supported list.
#[must_use]
pub fn by_name(name: &str) -> Option<Profile> {
    match name.to_ascii_lowercase().as_str() {
        "delphes" | "delphes/1" => Some(delphes()),
        "nanoaod" | "nanoaod/1" => Some(nanoaod()),
        _ => None,
    }
}

/// The profile names [`by_name`] accepts (for error messages).
pub const KNOWN_PROFILES: &[&str] = &["delphes", "nanoaod"];

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
        assert!(by_name("NanoAOD").is_some());
        assert!(by_name("nanoaod/1").is_some());
        assert!(by_name("cms").is_none());
        for name in KNOWN_PROFILES {
            assert!(by_name(name).is_some(), "{name}");
        }
    }

    #[test]
    fn naming_helpers_follow_the_convention() {
        let d = delphes();
        assert_eq!(d.leaf_branch("Jet", "PT"), "Jet.PT");
        assert_eq!(d.counter_branch("Jet"), "Jet_size");
        assert_eq!(d.counter_prefix("Jet_size"), Some("Jet"));
        assert_eq!(d.counter_prefix("Jet.PT"), None);

        let n = nanoaod();
        assert_eq!(n.leaf_branch("Jet", "pt"), "Jet_pt");
        assert_eq!(n.leaf_branch("genWeight", ""), "genWeight"); // empty leaf → bare branch
        assert_eq!(n.counter_branch("Jet"), "nJet");
        assert_eq!(n.counter_prefix("nJet"), Some("Jet"));
        assert_eq!(n.counter_prefix("nElectron"), Some("Electron"));
        assert_eq!(n.counter_prefix("genWeight"), None); // 'n' then lowercase, not a counter
        assert_eq!(n.counter_prefix("Jet_pt"), None);
    }

    #[test]
    fn nanoaod_profile_shape() {
        let p = nanoaod();
        assert_eq!(p.id(), "nanoaod/1");
        assert_eq!(p.tree, "Events");
        assert_eq!(p.naming.leaf_sep, "_");
        assert_eq!(p.naming.counter, CounterStyle::NPrefix);
        assert!(p.naming.flat_event_vars, "MET/weight are flat scalars");

        let keys: Vec<&str> = p.collections.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["Jet", "FatJet", "Electron", "Muon", "Tau", "Photon"]);

        // Jet maps the kinematics + the float b-tag discriminant under its
        // own ADL spelling `btagDeepB` (a continuous property, NOT the {0,1}
        // `btag` tag bit).
        let jet = p.collections.iter().find(|c| c.key == "Jet").unwrap();
        let props: Vec<&str> = jet.leaves.iter().map(|l| l.prop.as_str()).collect();
        assert_eq!(
            props,
            [
                "pt", "eta", "phi", "m", "btagDeepB", "btagDeepFlavB", "btagCSVV2", "jetId", "puId"
            ]
        );
        let btag = jet.leaves.iter().find(|l| l.prop == "btagDeepB").unwrap();
        assert_eq!(btag.leaf, "btagDeepB");
        assert_eq!(btag.kind, LeafKind::F32);
        // The DeepJet discriminant the audit added (official Run-2 b-tag).
        let deepflav = jet.leaves.iter().find(|l| l.prop == "btagDeepFlavB").unwrap();
        assert_eq!((deepflav.leaf.as_str(), deepflav.kind), ("btagDeepFlavB", LeafKind::F32));
        // Jet-quality IDs are plain integer leaves.
        let jet_id = jet.leaves.iter().find(|l| l.prop == "jetId").unwrap();
        assert_eq!((jet_id.leaf.as_str(), jet_id.kind), ("jetId", LeafKind::I32));

        // FatJet carries only btagCSVV2 (no DeepJet branch on this layout).
        let fatjet = p.collections.iter().find(|c| c.key == "FatJet").unwrap();
        let fprops: Vec<&str> = fatjet.leaves.iter().map(|l| l.prop.as_str()).collect();
        assert_eq!(fprops, ["pt", "eta", "phi", "m", "btagDeepB", "btagCSVV2"]);

        // Muons carry an integer charge plus bool quality IDs and an iso id.
        let muon = p.collections.iter().find(|c| c.key == "Muon").unwrap();
        let q = muon.leaves.iter().find(|l| l.prop == "q").unwrap();
        assert_eq!((q.leaf.as_str(), q.kind), ("charge", LeafKind::I32));
        let tight = muon.leaves.iter().find(|l| l.prop == "tightId").unwrap();
        assert_eq!((tight.leaf.as_str(), tight.kind), ("tightId", LeafKind::Bool));
        let iso = muon.leaves.iter().find(|l| l.prop == "pfIsoId").unwrap();
        assert_eq!((iso.leaf.as_str(), iso.kind), ("pfIsoId", LeafKind::I32));

        // MET is flat MET_pt/MET_phi; weight is the flat genWeight scalar.
        let met = p.met.as_ref().unwrap();
        assert_eq!(p.leaf_branch(&met.branch, &met.pt_leaf), "MET_pt");
        let w = p.weight.as_ref().unwrap();
        assert_eq!(p.leaf_branch(&w.branch, &w.leaf), "genWeight");

        // decides() renders the flat weight branch with no trailing separator.
        let d = p.decides();
        assert!(d.iter().any(|(k, v)| k == "weight_branch" && v == "genWeight"), "{d:?}");
    }
}
