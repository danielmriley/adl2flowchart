//! HIR snapshots for the legacy golden suite, quantity-table dump
//! snapshots for the two PLAN-named real analyses, and a corpus-wide
//! smoke + determinism check.

use adl_sema::{ExtDecls, analyze_str, hir_dump, object_table, quantity_table_dump};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    // crates/adl-sema -> crates -> adl2 -> reimplementation -> repo root
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(4)
        .expect("repo root")
        .to_path_buf()
}

fn analyze_file(path: &Path) -> adl_sema::Hir {
    let src =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let unit = path
        .file_name()
        .expect("file name")
        .to_string_lossy()
        .into_owned();
    analyze_str(&src, &unit, &ExtDecls::legacy())
}

#[test]
fn hir_snapshots_for_golden_suite() {
    let dir = repo_root().join("legacy_parser/tests/golden");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "adl"))
        .collect();
    paths.sort();
    assert!(
        !paths.is_empty(),
        "no golden files found in {}",
        dir.display()
    );

    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("snapshots/golden");
    settings.set_prepend_module_to_snapshot(false);
    let _guard = settings.bind_to_scope();
    for path in paths {
        let stem = path
            .file_stem()
            .expect("file stem")
            .to_string_lossy()
            .into_owned();
        let hir = analyze_file(&path);
        insta::assert_snapshot!(format!("hir__{stem}"), hir_dump(&hir));
    }
}

#[test]
fn quantity_table_snapshot_cms_sus_16_032() {
    let path = repo_root().join("examples/Examples/CMS-SUS-16-032.adl");
    let hir = analyze_file(&path);
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!("qtable__CMS-SUS-16-032", quantity_table_dump(&hir));
}

#[test]
fn quantity_table_snapshot_cms_sus_16_033_delphes() {
    let path = repo_root().join("examples/CMS/CMS-SUS-16-033_Delphes.adl");
    let hir = analyze_file(&path);
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!("qtable__CMS-SUS-16-033_Delphes", quantity_table_dump(&hir));
}

/// Object-attribute summary (plain path) for a filtered-collection-heavy
/// analysis: jets/bjets/cjets base chains, the leptons union, and the
/// fragment status of each filtered collection.
#[test]
fn object_table_snapshot_cms_sus_16_032() {
    let path = repo_root().join("examples/Examples/CMS-SUS-16-032.adl");
    let hir = analyze_file(&path);
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!("objects__CMS-SUS-16-032", object_table(&hir, false));
}

/// Object-attribute summary for a union-heavy analysis (lepton/DT unions,
/// deep derivation chains, pure renames collapsed with `=`).
#[test]
fn object_table_snapshot_cms_sus_21_006() {
    let path = repo_root().join("examples/cl_examples/CMS-SUS-21-006.adl");
    let hir = analyze_file(&path);
    let mut settings = insta::Settings::clone_current();
    settings.set_prepend_module_to_snapshot(false);
    let _guard = settings.bind_to_scope();
    insta::assert_snapshot!("objects__CMS-SUS-21-006", object_table(&hir, false));
}

/// The object table is deterministic and ANSI-free on the plain path.
#[test]
fn object_table_is_deterministic_and_plain() {
    let path = repo_root().join("examples/Examples/CMS-SUS-16-032.adl");
    let a = object_table(&analyze_file(&path), false);
    let b = object_table(&analyze_file(&path), false);
    assert_eq!(a, b, "object table must be deterministic");
    assert!(!a.contains('\u{1b}'), "plain path must be ANSI-free");
    let colored = object_table(&analyze_file(&path), true);
    assert!(colored.contains('\u{1b}'), "color path must emit ANSI");
}

/// Every corpus file resolves without panicking, with no error-severity
/// diagnostics (warnings are honest coverage notes), and resolution is
/// deterministic (two runs produce byte-identical dumps).
#[test]
fn corpus_smoke_and_determinism() {
    let dir = repo_root().join("examples");
    let mut paths = Vec::new();
    collect_adl(&dir, &mut paths);
    paths.sort();
    assert_eq!(paths.len(), 68, "expected the 68-file corpus");
    for path in paths {
        let a = analyze_file(&path);
        let errors: Vec<_> = a
            .diags
            .iter()
            .filter(|d| d.severity == adl_syntax::diag::Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "{} produced error diagnostics: {errors:?}",
            path.display()
        );
        let b = analyze_file(&path);
        assert_eq!(
            hir_dump(&a),
            hir_dump(&b),
            "non-deterministic HIR dump for {}",
            path.display()
        );
        assert_eq!(
            quantity_table_dump(&a),
            quantity_table_dump(&b),
            "non-deterministic quantity table for {}",
            path.display()
        );
        let ta = object_table(&a, false);
        assert_eq!(
            ta,
            object_table(&b, false),
            "non-deterministic object table for {}",
            path.display()
        );
        assert!(
            !ta.contains('\u{1b}'),
            "plain object table must be ANSI-free for {}",
            path.display()
        );
    }
}

fn collect_adl(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
    {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_adl(&path, out);
        } else if path.extension().is_some_and(|x| x == "adl") {
            out.push(path);
        }
    }
}
