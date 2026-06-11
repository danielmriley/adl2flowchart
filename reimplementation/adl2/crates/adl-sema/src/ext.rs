//! External standard-library declarations, ingested from the legacy data
//! files (`ext_objs.txt`, `ext_lib.txt`, `property_vars.txt`) plus the
//! base-name spelling map seeded from `object_aliases.txt` semantics
//! (SPEC_ARCHITECTURE §4: the legacy alias table "becomes a small
//! base-name spelling map only").
//!
//! The MET family (`MET`, `MissingET`, `METLV`, `Delphes_MissingET`) is
//! special: it names the per-event missing-momentum vector, and a bare
//! MET-family *value* means its `.pt` magnitude (legacy semantics).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

/// Canonical identity key of the MET base-collection family.
pub const MET_FAMILY_KEY: &str = "met";

/// Names that denote per-event scalars when used as bare values
/// (legacy `ext_objs` entries that are scalar-valued, not collections).
const EVENT_SCALAR_KEYS: &[&str] = &["ht", "st", "fht", "scalarht", "delphes_scalarht"];

/// Tag properties keep their exact surface name as identity (the TAG axiom
/// is exact-name; merging `ctag` into `btag` via the legacy
/// `ctag -> isBTag` mapping would be an unsound over-merge).
const EXACT_NAME_PROPS: &[&str] = &["btag", "ctag", "tautag"];

/// One canonicalized property: identity key + preferred display spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PropEntry {
    canon_key: String,
    display: String,
}

/// Typed declarations loaded from the legacy standard-library files.
#[derive(Debug, Default)]
pub struct ExtDecls {
    /// lc spelling -> canonical *display* base name (e.g. "missinget" -> "MET").
    base_canon: HashMap<String, String>,
    /// lc names of declared external functions (`ext_lib.txt`).
    functions: HashSet<String>,
    /// lc property spelling -> canonical entry (from `property_vars.txt`).
    props: HashMap<String, PropEntry>,
}

impl ExtDecls {
    /// Build from the raw text of the four legacy data files.
    #[must_use]
    pub fn from_sources(ext_objs: &str, ext_lib: &str, property_vars: &str, aliases: &str) -> Self {
        let mut decls = Self::default();
        decls.load_aliases(aliases);
        decls.load_ext_objs(ext_objs);
        decls.load_ext_lib(ext_lib);
        decls.load_property_vars(property_vars);
        decls
    }

    /// The legacy standard library, embedded at compile time from
    /// `legacy_parser/adl/`.
    #[must_use]
    pub fn legacy() -> Self {
        Self::from_sources(
            include_str!("../../../../../legacy_parser/adl/ext_objs.txt"),
            include_str!("../../../../../legacy_parser/adl/ext_lib.txt"),
            include_str!("../../../../../legacy_parser/adl/property_vars.txt"),
            include_str!("../../../../../legacy_parser/adl/object_aliases.txt"),
        )
    }

    /// Load the three data files from `dir` (an `adl/` directory of the
    /// legacy layout). `object_aliases.txt` is optional there; the embedded
    /// copy is used when absent.
    ///
    /// # Errors
    /// Returns the first I/O error encountered reading a required file.
    pub fn load_dir(dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let dir = dir.as_ref();
        let objs = std::fs::read_to_string(dir.join("ext_objs.txt"))?;
        let lib = std::fs::read_to_string(dir.join("ext_lib.txt"))?;
        let props = std::fs::read_to_string(dir.join("property_vars.txt"))?;
        let aliases =
            std::fs::read_to_string(dir.join("object_aliases.txt")).unwrap_or_else(|_| {
                include_str!("../../../../../legacy_parser/adl/object_aliases.txt").to_owned()
            });
        Ok(Self::from_sources(&objs, &lib, &props, &aliases))
    }

    fn load_aliases(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let mut toks = line.split_whitespace();
            let Some(canon) = toks.next() else { continue };
            self.base_canon
                .insert(canon.to_ascii_lowercase(), canon.to_owned());
            for alias in toks {
                self.base_canon
                    .insert(alias.to_ascii_lowercase(), canon.to_owned());
            }
        }
    }

    fn load_ext_objs(&mut self, text: &str) {
        for line in text.lines() {
            let name = line.split('#').next().unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            // Names already covered by the spelling map keep their family
            // canon; everything else canonicalizes to itself.
            self.base_canon
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| name.to_owned());
        }
    }

    fn load_ext_lib(&mut self, text: &str) {
        for line in text.lines() {
            let name = line.split('#').next().unwrap_or("").trim();
            if name.is_empty() {
                continue;
            }
            self.functions.insert(name.to_ascii_lowercase());
        }
    }

    fn load_property_vars(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((name, internal)) = line.split_once("->") else {
                continue;
            };
            let name = name.trim();
            let internal = internal.trim();
            if name.is_empty() {
                continue;
            }
            let lc = name.to_ascii_lowercase();
            let canon_key = if EXACT_NAME_PROPS.contains(&lc.as_str())
                || internal.is_empty()
                || internal.eq_ignore_ascii_case("blank")
            {
                lc.clone()
            } else {
                internal.to_ascii_lowercase()
            };
            // First-wins for both the spelling and the group display name,
            // so the surface name that introduces a group labels it
            // (e.g. group "mof" displays as "m", covering "mass" too).
            let display = self
                .props
                .values()
                .find(|p| p.canon_key == canon_key)
                .map_or_else(|| name.to_owned(), |p| p.display.clone());
            self.props
                .entry(lc)
                .or_insert(PropEntry { canon_key, display });
        }
    }

    /// Canonical display name of a base collection, if `name` is declared.
    #[must_use]
    pub fn base_collection(&self, name: &str) -> Option<&str> {
        self.base_canon
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }

    /// Is `name` (any spelling) a member of the MET family?
    #[must_use]
    pub fn is_met_family(&self, name: &str) -> bool {
        self.base_collection(name)
            .is_some_and(|c| c.eq_ignore_ascii_case(MET_FAMILY_KEY))
    }

    /// Is `name` a per-event scalar when used as a bare value?
    #[must_use]
    pub fn is_event_scalar(&self, name: &str) -> bool {
        EVENT_SCALAR_KEYS.contains(&name.to_ascii_lowercase().as_str())
    }

    /// Is `name` a declared external function (`ext_lib.txt`)?
    #[must_use]
    pub fn is_function(&self, name: &str) -> bool {
        self.functions.contains(&name.to_ascii_lowercase())
    }

    /// Canonicalize a property spelling: returns `(identity_key, display)`.
    /// Unknown properties canonicalize to their own lowercase name.
    #[must_use]
    pub fn prop_canon(&self, name: &str) -> (String, String) {
        let lc = name.to_ascii_lowercase();
        match self.props.get(&lc) {
            Some(entry) => (entry.canon_key.clone(), entry.display.clone()),
            None => (lc, name.to_owned()),
        }
    }

    /// Is `name` a known property spelling (`property_vars.txt`)?
    #[must_use]
    pub fn is_property(&self, name: &str) -> bool {
        self.props.contains_key(&name.to_ascii_lowercase())
    }

    /// Deterministic description of the loaded tables (debug aid).
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = String::new();
        let mut bases: Vec<_> = self.base_canon.iter().collect();
        bases.sort();
        let _ = writeln!(out, "bases: {}", bases.len());
        let _ = writeln!(out, "functions: {}", self.functions.len());
        let _ = writeln!(out, "properties: {}", self.props.len());
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn met_family_unifies_per_alias_map() {
        let ext = ExtDecls::legacy();
        for spelling in ["MET", "MissingET", "METLV", "Delphes_MissingET", "metlv"] {
            assert_eq!(ext.base_collection(spelling), Some("MET"), "{spelling}");
            assert!(ext.is_met_family(spelling), "{spelling}");
        }
        assert!(!ext.is_met_family("Jet"));
        assert_eq!(ext.base_collection("AK4jet"), Some("JET"));
        assert_eq!(ext.base_collection("Ele"), Some("ELECTRON"));
    }

    #[test]
    fn properties_canonicalize_but_tags_stay_exact_name() {
        let ext = ExtDecls::legacy();
        let (pt1, _) = ext.prop_canon("pT");
        let (pt2, _) = ext.prop_canon("pt");
        let (pt3, _) = ext.prop_canon("Pt");
        assert_eq!(pt1, pt2);
        assert_eq!(pt2, pt3);
        let (m, _) = ext.prop_canon("m");
        let (mass, _) = ext.prop_canon("mass");
        assert_eq!(m, mass);
        // ctag -> isBTag in the legacy file must NOT merge ctag with btag.
        let (btag, _) = ext.prop_canon("btag");
        let (ctag, _) = ext.prop_canon("ctag");
        assert_ne!(btag, ctag);
    }

    #[test]
    fn functions_are_case_insensitive() {
        let ext = ExtDecls::legacy();
        assert!(ext.is_function("dR"));
        assert!(ext.is_function("dr"));
        assert!(ext.is_function("SQRT"));
        assert!(!ext.is_function("D0"));
    }
}
