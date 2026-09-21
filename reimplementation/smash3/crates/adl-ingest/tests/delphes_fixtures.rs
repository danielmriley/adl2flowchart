//! Fixture-driven tests for the Delphes profile reader
//! (SPEC_EVENT_PIPELINE §1). The committed `.root` fixtures are
//! Delphes-shaped trees written by `fixtures/make_fixtures.py`
//! (provenance in `fixtures/PROVENANCE.md`); the `.expected.jsonl`
//! goldens were verified byte-identical against the independent uproot
//! oracle script at freeze time (and continuously by the env-gated CLI
//! oracle test).

use adl_ingest::{IngestDiag, IngestError, LeafKind, delphes, read_root};
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn golden(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("golden file")
}

#[test]
fn mini_matches_the_frozen_golden_byte_for_byte() {
    let ingested = read_root(&fixture("delphes_mini.root"), &delphes()).expect("ingest");
    assert_eq!(ingested.entries, 13);
    assert_eq!(ingested.lines.len(), 13);
    assert_eq!(ingested.profile_id, "delphes/1");
    assert_eq!(ingested.jsonl(), golden("delphes_mini.expected.jsonl"));
}

#[test]
fn mini_first_event_pins_real_sample_values() {
    // Entry 0 of the T2tt_700_50 sample (SPEC §1.1 probe values): leading
    // jet pT 719.50916 f32, exactly 719.5091552734375 widened to f64.
    let ingested = read_root(&fixture("delphes_mini.root"), &delphes()).expect("ingest");
    let first = &ingested.lines[0];
    assert!(
        first.starts_with(r#"{"Jet":[{"pt":719.5091552734375,"#),
        "{first}"
    );
    assert!(first.contains(r#""btag":1"#), "{first}");
    assert!(
        first.contains(r#""MET":{"pt":653.098876953125,"#),
        "{first}"
    );
    assert!(first.ends_with(r#""weight":1.0}"#), "{first}");
}

#[test]
fn mini_is_byte_deterministic_across_reads() {
    let path = fixture("delphes_mini.root");
    let a = read_root(&path, &delphes()).expect("ingest");
    let b = read_root(&path, &delphes()).expect("ingest");
    assert_eq!(a.jsonl(), b.jsonl());
    assert_eq!(a.diags, b.diags);
}

#[test]
fn mini_diagnostics_name_the_unmappable_content() {
    let ingested = read_root(&fixture("delphes_mini.root"), &delphes()).expect("ingest");
    // LHE multiweights: 13 events × 1 weight, dropped with a count.
    assert!(
        ingested.diags.iter().any(|d| matches!(
            d,
            IngestDiag::LheWeightsDropped { branch, count: 13 } if branch == "Weight.Weight"
        )),
        "{:?}",
        ingested.diags
    );
    // Jet.T is present in the fixture but unmapped: summarized, listed.
    let unmapped = ingested
        .diags
        .iter()
        .find_map(|d| match d {
            IngestDiag::UnmappedLeaves { branch, leaves } if branch == "Jet" => Some(leaves),
            _ => None,
        })
        .expect("Jet unmapped-leaves diagnostic");
    assert_eq!(unmapped, &vec!["Jet.T".to_owned()]);
    // Event.ProcessID sits in a known-drop family: no diagnostic for it.
    assert!(
        !format!("{:?}", ingested.diags).contains("ProcessID"),
        "{:?}",
        ingested.diags
    );
}

#[test]
fn synth_matches_golden_and_diagnoses_tag_bits_met_lhe_and_unknowns() {
    let ingested = read_root(&fixture("delphes_synth.root"), &delphes()).expect("ingest");
    assert_eq!(ingested.jsonl(), golden("delphes_synth.expected.jsonl"));

    // Raw masks 2 and 3 both have non-bit-0 bits set: 2 values diagnosed;
    // emitted flags stay in {0, 1} (mask 2 → 0, mask 3 → 1 — see golden).
    assert!(
        ingested.diags.iter().any(|d| matches!(
            d,
            IngestDiag::TagBitsIgnored { branch, bit: 0, values: 2 } if branch == "Jet.BTag"
        )),
        "{:?}",
        ingested.diags
    );
    // MissingET multiplicities 1/1/2/0: one multi-event, one empty-event.
    assert!(ingested.diags.iter().any(|d| matches!(
        d,
        IngestDiag::MultiElement { branch, events: 1 } if branch == "MissingET"
    )));
    assert!(ingested.diags.iter().any(|d| matches!(
        d,
        IngestDiag::EmptyElement { branch, events: 1 } if branch == "MissingET"
    )));
    // The unknown `Track` family is diagnosed, never silently dropped.
    assert!(ingested.diags.iter().any(|d| matches!(
        d,
        IngestDiag::UnknownBranch { branch, .. } if branch == "Track"
    )));
    // FatJet is mapped by the profile but absent here: verbose-only note.
    let absent = ingested
        .diags
        .iter()
        .find(|d| matches!(d, IngestDiag::AbsentCollection { branch } if branch == "FatJet"))
        .expect("absent-collection note");
    assert!(absent.verbose_only());
}

#[test]
fn synth_tag_domain_is_zero_or_one_in_every_emitted_event() {
    let ingested = read_root(&fixture("delphes_synth.root"), &delphes()).expect("ingest");
    for line in &ingested.lines {
        let v: serde_json::Value = serde_json::from_str(line).expect("valid JSON");
        for obj in v["Jet"].as_array().expect("jets") {
            for tag in ["btag", "tautag"] {
                let t = obj[tag].as_i64().expect("integer tag");
                assert!(t == 0 || t == 1, "{tag}={t} in {line}");
            }
        }
    }
}

#[test]
fn btag_bit_profile_option_selects_the_other_working_point() {
    // [DECIDE-I1]: bit 0 is the default; `btag_bit = 1` is the documented
    // profile option for non-default cards. Raw masks in the synth
    // fixture: [1, 2], [], [3, 0], [0] — under bit 1 the flags become
    // [0, 1], [], [1, 0], [0], and the values with *other* bits set are
    // the two with bit 0 (masks 1 and 3).
    let mut profile = delphes();
    for c in &mut profile.collections {
        for l in &mut c.leaves {
            if l.prop == "btag" {
                l.kind = LeafKind::TagBit(1);
            }
        }
    }
    let ingested = read_root(&fixture("delphes_synth.root"), &profile).expect("ingest");
    let first: serde_json::Value = serde_json::from_str(&ingested.lines[0]).expect("json");
    let flags: Vec<i64> = first["Jet"]
        .as_array()
        .expect("jets")
        .iter()
        .map(|o| o["btag"].as_i64().expect("int"))
        .collect();
    assert_eq!(flags, vec![0, 1]);
    assert!(ingested.diags.iter().any(|d| matches!(
        d,
        IngestDiag::TagBitsIgnored { branch, bit: 1, values: 2 } if branch == "Jet.BTag"
    )));
}

#[test]
fn unordered_collections_are_refused_never_resorted() {
    let err = read_root(&fixture("delphes_badorder.root"), &delphes()).unwrap_err();
    assert_eq!(
        err,
        IngestError::NotPtDescending {
            collection: "Jet".to_owned(),
            entry: 1,
            index: 1,
        }
    );
}

#[test]
fn non_finite_values_are_refused() {
    let err = read_root(&fixture("delphes_nan.root"), &delphes()).unwrap_err();
    assert_eq!(
        err,
        IngestError::NonFinite {
            branch: "Jet.Eta".to_owned(),
            entry: 0,
        }
    );
}

#[test]
fn missing_file_and_missing_tree_are_clean_errors() {
    let err = read_root(&fixture("nope.root"), &delphes()).unwrap_err();
    assert!(matches!(err, IngestError::Open { .. }), "{err}");

    let mut profile = delphes();
    profile.tree = "NotATree".to_owned();
    let err = read_root(&fixture("delphes_mini.root"), &profile).unwrap_err();
    assert!(matches!(err, IngestError::Tree { .. }), "{err}");
}

#[test]
fn generated_script_embeds_the_profile_table() {
    let script = adl_ingest::to_jsonl_py(&delphes());
    for needle in [
        "profile delphes/1",
        r#"("PT", "pt", "f")"#,
        r#"("BTag", "btag", ("tag", 0))"#,
        r#"("Charge", "q", "i")"#,
        r#"("m", "0.000511")"#,
        r#"("m", "0.105658")"#,
        r#"MET = ("MissingET", "MET", "Phi")"#,
        r#"WEIGHT = ("Event", "Weight")"#,
        "def jnum(x):",
        "not pT-descending",
    ] {
        assert!(script.contains(needle), "script missing {needle}");
    }
}
