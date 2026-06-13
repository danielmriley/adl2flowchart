//! The SPEC_EVENT_PIPELINE §6 provenance object: one canonical JSON
//! object, identical bytes across every output of a run (`histos.json`,
//! `cutflow.json`, `out.root` TNamed, the `--json` report, and the
//! ingest sibling file). It records what was run — tool version, the
//! exact ADL bytes hashed, the input identity (basename + sha256 + event
//! count + profile), an optional generator seed, and the per-run
//! `[DECIDE]` choices.
//!
//! Built from plain inputs ([`Provenance::adl_sha256`] /
//! [`Provenance::input_sha256`] hash bytes once at the call site), then
//! rendered with the same ordered [`JsonWriter`] the canonical JSONs use,
//! so the embedded object is byte-deterministic and free of
//! wall-clock timestamps (determinism by construction, §6).

use crate::json::JsonWriter;
use crate::sha256::sha256_hex;

/// Identity of the input events: basename, content hash, event count, and
/// the converter profile id (`"delphes/1"`). Absent for a bare JSONL run
/// with no profile — provenance still records the file and hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputIdentity {
    pub file: String,
    pub sha256: String,
    pub events: u64,
    /// `name/version` of the ingest profile, when one was used.
    pub profile: Option<String>,
}

/// The provenance object, ready to render (`to_json`) or embed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// `"smash2 <semver>[+<git>]"` — the running tool's identity.
    pub tool: String,
    /// ADL source basename.
    pub adl_file: String,
    /// sha256 of the exact ADL bytes parsed.
    pub adl_sha256: String,
    /// Input event identity, when a file was read.
    pub input: Option<InputIdentity>,
    /// Generator seed — present only for synthesized events.
    pub seed: Option<u64>,
    /// Per-run `[DECIDE]` choices `(key, value)` in deterministic order
    /// (from the active ingest profile; empty for a bare JSONL run).
    pub decides: Vec<(String, String)>,
}

impl Provenance {
    /// sha256 hex of the exact ADL source bytes parsed (§6: "the exact
    /// bytes parsed").
    #[must_use]
    pub fn adl_sha256(src: &[u8]) -> String {
        sha256_hex(src)
    }

    /// sha256 hex of the input event bytes (§6 input identity;
    /// [DECIDE-P1] always full-hash).
    #[must_use]
    pub fn input_sha256(bytes: &[u8]) -> String {
        sha256_hex(bytes)
    }

    /// The canonical JSON object string (no trailing newline, compact or
    /// pretty). The same writer discipline as the surrounding documents,
    /// so embedding it preserves their byte-determinism.
    #[must_use]
    pub fn to_json(&self, pretty: bool) -> String {
        let mut w = JsonWriter::new(pretty);
        self.write(&mut w);
        w.finish_no_newline()
    }

    /// Render the object into an existing writer under whatever key the
    /// caller just opened (used to embed it inside `histos.json` /
    /// `cutflow.json`). Field order is fixed.
    pub(crate) fn write(&self, w: &mut JsonWriter) {
        w.open('{');
        w.key("tool");
        w.str_val(&self.tool);
        w.key("adl");
        w.open('{');
        w.key("file");
        w.str_val(&self.adl_file);
        w.key("sha256");
        w.str_val(&self.adl_sha256);
        w.close('}');
        if let Some(input) = &self.input {
            w.key("input");
            w.open('{');
            w.key("file");
            w.str_val(&input.file);
            w.key("sha256");
            w.str_val(&input.sha256);
            w.key("events");
            w.raw(&input.events.to_string());
            if let Some(profile) = &input.profile {
                w.key("profile");
                w.str_val(profile);
            }
            w.close('}');
        }
        if let Some(seed) = self.seed {
            w.key("seed");
            w.raw(&seed.to_string());
        }
        if !self.decides.is_empty() {
            w.key("decides");
            w.open('{');
            for (k, v) in &self.decides {
                w.key(k);
                w.str_val(v);
            }
            w.close('}');
        }
        w.close('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Provenance {
        Provenance {
            tool: "smash2 0.1.0".to_owned(),
            adl_file: "ex02_histograms.adl".to_owned(),
            adl_sha256: Provenance::adl_sha256(b"region SR\n  select MET > 200\n"),
            input: Some(InputIdentity {
                file: "T2tt_700_50.root".to_owned(),
                sha256: Provenance::input_sha256(b"events"),
                events: 20000,
                profile: Some("delphes/1".to_owned()),
            }),
            seed: None,
            decides: vec![
                ("btag_bit".to_owned(), "0".to_owned()),
                ("lepton_mass".to_owned(), "pdg".to_owned()),
            ],
        }
    }

    #[test]
    fn canonical_compact_form_is_stable_and_parseable() {
        let p = sample();
        let json = p.to_json(false);
        // Fixed field order, no trailing newline, valid JSON.
        assert!(json.starts_with("{\"tool\":\"smash2 0.1.0\",\"adl\":{\"file\":"));
        assert!(json.contains("\"input\":{\"file\":\"T2tt_700_50.root\",\"sha256\":"));
        assert!(json.contains("\"events\":20000,\"profile\":\"delphes/1\""));
        assert!(json.contains("\"decides\":{\"btag_bit\":\"0\",\"lepton_mass\":\"pdg\"}"));
        assert!(!json.ends_with('\n'));
        let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert_eq!(v["adl"]["sha256"].as_str().unwrap().len(), 64);
        // Determinism.
        assert_eq!(json, p.to_json(false));
    }

    #[test]
    fn optional_fields_drop_cleanly() {
        let p = Provenance {
            tool: "smash2 0.1.0".to_owned(),
            adl_file: "f.adl".to_owned(),
            adl_sha256: Provenance::adl_sha256(b"x"),
            input: None,
            seed: Some(42),
            decides: Vec::new(),
        };
        let json = p.to_json(false);
        assert!(!json.contains("input"));
        assert!(!json.contains("decides"));
        assert!(json.contains("\"seed\":42"));
        serde_json::from_str::<serde_json::Value>(&json).expect("valid JSON");
    }
}
