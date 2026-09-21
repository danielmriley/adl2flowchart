//! AST snapshot tests (insta): canonical dumps + diagnostics for a
//! representative corpus slice and for every legacy golden file
//! (TESTING.md layer 2).

use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../..")
}

/// Snapshot value: canonical AST dump plus rendered diagnostics, separated
/// by a marker so grammar and diagnostic drift both show up in review.
fn snapshot_for(rel_path: &str) -> String {
    let path = repo_root().join(rel_path);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    let result = adl_syntax::parse(&src);
    let dump = adl_syntax::dump_ast(&src, &result.file);
    let diags = adl_syntax::render_diagnostics(&src, rel_path, &result.diags);
    format!("{dump}---- diagnostics ----\n{diags}")
}

macro_rules! corpus_snapshot {
    ($test_name:ident, $rel:expr) => {
        #[test]
        fn $test_name() {
            insta::assert_snapshot!(stringify!($test_name), snapshot_for($rel));
        }
    };
}

// ---------- 12 representative corpus files ----------

corpus_snapshot!(
    corpus_ex00_helloworld,
    "examples/tutorials/ex00_helloworld.adl"
);
corpus_snapshot!(
    corpus_ex02_histograms,
    "examples/tutorials/ex02_histograms.adl"
);
corpus_snapshot!(corpus_ex04_syntaxes, "examples/tutorials/ex04_syntaxes.adl");
corpus_snapshot!(corpus_ex06_bins, "examples/tutorials/ex06_bins.adl");
corpus_snapshot!(
    corpus_ex07_chi2optimize,
    "examples/tutorials/ex07_chi2optimize.adl"
);
corpus_snapshot!(
    corpus_ex10_tableweight,
    "examples/tutorials/ex10_tableweight.adl"
);
corpus_snapshot!(corpus_ex12_counts, "examples/tutorials/ex12_counts.adl");
corpus_snapshot!(
    corpus_atlas_susy_jetmet,
    "examples/Examples/ATLAS-SUSYJetMET-1605-03814.adl"
);
corpus_snapshot!(
    corpus_atlas_exot_1704,
    "examples/Examples/ATLAS-EXOT-1704-0384.adl"
);
corpus_snapshot!(
    corpus_cms_sus_16_041,
    "examples/CMS/CMS-SUS-16-041_Delphes.adl"
);
corpus_snapshot!(
    corpus_cms_sus_21_002,
    "examples/cl_examples/CMS-SUS-21-002.adl"
);
corpus_snapshot!(corpus_small_cms, "examples/small_samples/cms.adl");

// ---------- every legacy golden file ----------

corpus_snapshot!(
    golden_angular_order,
    "legacy_parser/tests/golden/angular_order.adl"
);
corpus_snapshot!(
    golden_bad_syntax,
    "legacy_parser/tests/golden/bad_syntax.adl"
);
corpus_snapshot!(
    golden_bins_partition,
    "legacy_parser/tests/golden/bins_partition.adl"
);
corpus_snapshot!(
    golden_btag_discriminant,
    "legacy_parser/tests/golden/btag_discriminant.adl"
);
corpus_snapshot!(
    golden_btag_threshold,
    "legacy_parser/tests/golden/btag_threshold.adl"
);
corpus_snapshot!(
    golden_collection_quant,
    "legacy_parser/tests/golden/collection_quant.adl"
);
corpus_snapshot!(
    golden_define_arith,
    "legacy_parser/tests/golden/define_arith.adl"
);
corpus_snapshot!(
    golden_define_under_or,
    "legacy_parser/tests/golden/define_under_or.adl"
);
corpus_snapshot!(
    golden_disjoint_jet_index,
    "legacy_parser/tests/golden/disjoint_jet_index.adl"
);
corpus_snapshot!(
    golden_disjoint_pt,
    "legacy_parser/tests/golden/disjoint_pt.adl"
);
corpus_snapshot!(
    golden_independent_jet_index,
    "legacy_parser/tests/golden/independent_jet_index.adl"
);
corpus_snapshot!(
    golden_inf_constant,
    "legacy_parser/tests/golden/inf_constant.adl"
);
corpus_snapshot!(
    golden_ite_conditional_dphi,
    "legacy_parser/tests/golden/ite_conditional_dphi.adl"
);
corpus_snapshot!(golden_not_tag, "legacy_parser/tests/golden/not_tag.adl");
corpus_snapshot!(golden_or_met, "legacy_parser/tests/golden/or_met.adl");
corpus_snapshot!(
    golden_or_unencodable_branch,
    "legacy_parser/tests/golden/or_unencodable_branch.adl"
);
corpus_snapshot!(
    golden_overlap_met,
    "legacy_parser/tests/golden/overlap_met.adl"
);
corpus_snapshot!(
    golden_quant_empty_forall,
    "legacy_parser/tests/golden/quant_empty_forall.adl"
);
corpus_snapshot!(golden_ratio_met, "legacy_parser/tests/golden/ratio_met.adl");
corpus_snapshot!(
    golden_reject_and_band,
    "legacy_parser/tests/golden/reject_and_band.adl"
);
corpus_snapshot!(
    golden_reject_or_band,
    "legacy_parser/tests/golden/reject_or_band.adl"
);
corpus_snapshot!(
    golden_size_bjets,
    "legacy_parser/tests/golden/size_bjets.adl"
);
corpus_snapshot!(golden_tag_index, "legacy_parser/tests/golden/tag_index.adl");
corpus_snapshot!(
    golden_union_size,
    "legacy_parser/tests/golden/union_size.adl"
);
corpus_snapshot!(
    golden_vacuous_dphi,
    "legacy_parser/tests/golden/vacuous_dphi.adl"
);
