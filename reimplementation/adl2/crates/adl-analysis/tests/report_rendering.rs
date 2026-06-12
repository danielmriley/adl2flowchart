//! Snapshot tests for the default human rendering (`Report::human_default`):
//! findings first, region table, verdict matrix, grouped pairwise verdicts.
//!
//! These run with the real solver (like the golden battery — the suite
//! requires a backend) so the snapshots pin the physics-facing layout on
//! real analyses: CMS-SUS-16-032 (empty regions + matrix + groups + bin
//! findings), CMS-SUS-16-033 (13 regions, overlap/subset groups), and a
//! tiny two-region file (no matrix, singleton group). The solver label is
//! normalized so the snapshot is backend-independent; verdicts are not.

use adl_analysis::{AnalysisOptions, SolverChoice, analyze_source};
use adl_sema::ExtDecls;
use std::path::PathBuf;
use std::time::Duration;

fn render(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../..")
        .join(rel);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let ext = ExtDecls::legacy();
    let opts = AnalysisOptions {
        solver: SolverChoice::Auto,
        timeout: Duration::from_secs(20),
        ..AnalysisOptions::default()
    };
    let r = analyze_source(&src, &name, &ext, &opts)
        .unwrap_or_else(|e| panic!("{name} must resolve cleanly:\n{e}"));
    assert_ne!(
        r.solver, "none",
        "rendering snapshots need a solver backend"
    );
    let plain = r.human_default(false);
    assert_eq!(
        plain,
        r.human_default(false),
        "{name}: plain rendering must be deterministic"
    );
    assert!(
        !plain.contains('\u{1b}'),
        "{name}: color=false must emit no ANSI escapes"
    );
    // Backend-independent snapshot body: normalize the solver label only.
    plain.replace(&format!("(solver: {})", r.solver), "(solver: <backend>)")
}

#[test]
fn default_rendering_cms_sus_16_032() {
    insta::assert_snapshot!(
        "default_cms_sus_16_032",
        render("examples/Examples/CMS-SUS-16-032.adl")
    );
}

#[test]
fn default_rendering_cms_sus_16_033() {
    insta::assert_snapshot!(
        "default_cms_sus_16_033",
        render("examples/CMS/CMS-SUS-16-033_Delphes.adl")
    );
}

#[test]
fn default_rendering_tiny_reject_or_band() {
    insta::assert_snapshot!(
        "default_reject_or_band",
        render("legacy_parser/tests/golden/reject_or_band.adl")
    );
}
