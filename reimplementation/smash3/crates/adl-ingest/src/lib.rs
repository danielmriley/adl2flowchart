//! `adl-ingest` — event ingestion via converter profiles
//! (SPEC_EVENT_PIPELINE §1).
//!
//! Experiment specifics live in **profiles** ([`Profile`], a pure data
//! table: branch patterns → canonical keys, tag-derivation rules, weight
//! source); the core event model never sees experiment names. The native
//! reader ([`read_root`], oxyroot pinned `=0.1.25`) turns a Delphes-layout
//! TTree into canonical JSONL event lines; [`to_jsonl_py`] generates the
//! independent uproot oracle script that must reproduce those bytes
//! exactly.
//!
//! Faithfulness rules (ratified): unmappable content is a *diagnostic*
//! ([`IngestDiag`]), never a guess; convention choices are `[DECIDE]`
//! entries surfaced via [`Profile::decides`]; canonical-model invariants
//! (pT-descending order, tag domain) are validated at ingest with hard
//! refusals ([`IngestError`]), never silent fixes; output is
//! byte-deterministic.

pub mod profile;
pub mod reader;
pub mod script;

pub use profile::{
    CollectionSpec, KNOWN_PROFILES, LeafKind, LeafSpec, MetSpec, Profile, ScalarSpec, WeightSpec,
    by_name, delphes,
};
pub use reader::{IngestDiag, IngestError, Ingested, read_root};
pub use script::to_jsonl_py;
