//! DOT renders for five corpus files, committed as insta snapshots
//! (SPEC_ARCHITECTURE §1/§9 — PLAN Phase 6 exit criterion "DOT renders
//! for the corpus"). These five exercise the structural features the
//! flowchart/AST emitters care about:
//!
//! - `disjoint_jet_index` — base→filtered `take`, indexed element cuts;
//! - `collection_quant`   — pure-rename object, unindexed collection cut;
//! - `bins_partition`     — boundary-list bins on a pure-rename MET object;
//! - `ex01_selection`     — union object, region inheritance, filtered-of-
//!   filtered lineage, `ALL`;
//! - `ex06_bins`          — union, defines, boolean bins, boundary bins.
//!
//! Each snapshot also doubles as a determinism check: a snapshot diff on a
//! rerun would mean nondeterministic output.

use adl_sema::{ExtDecls, Hir, analyze_str};
use adl_viz::{ast_dot, flowchart_dot};
use std::path::PathBuf;

fn corpus_root() -> PathBuf {
    // crate dir = .../adl2/crates/adl-viz
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .join("../../../../examples")
        .canonicalize()
        .expect("examples dir resolves")
}

fn golden_root() -> PathBuf {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .join("../../../../legacy_parser/tests/golden")
        .canonicalize()
        .expect("golden dir resolves")
}

fn hir_of(path: &PathBuf) -> Hir {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let hir = analyze_str(&src, &name, &ExtDecls::legacy());
    assert!(
        !adl_syntax::diag::has_errors(&hir.diags),
        "{name} resolved with errors"
    );
    hir
}

macro_rules! dot_snapshots {
    ($name:ident, $root:ident, $file:literal) => {
        #[test]
        fn $name() {
            let path = $root().join($file);
            let hir = hir_of(&path);
            let fc = flowchart_dot(&hir);
            let ast = ast_dot(&hir);
            // Determinism: a second render must be byte-identical.
            assert_eq!(fc, flowchart_dot(&hir_of(&path)));
            assert_eq!(ast, ast_dot(&hir_of(&path)));
            insta::assert_snapshot!(concat!(stringify!($name), "_flowchart"), fc);
            insta::assert_snapshot!(concat!(stringify!($name), "_ast"), ast);
        }
    };
}

dot_snapshots!(disjoint_jet_index, golden_root, "disjoint_jet_index.adl");
dot_snapshots!(collection_quant, golden_root, "collection_quant.adl");
dot_snapshots!(bins_partition, golden_root, "bins_partition.adl");
dot_snapshots!(ex01_selection, corpus_root, "tutorials/ex01_selection.adl");
dot_snapshots!(ex06_bins, corpus_root, "tutorials/ex06_bins.adl");
