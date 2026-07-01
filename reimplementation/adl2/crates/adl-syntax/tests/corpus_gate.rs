//! Corpus gate (PLAN Phase 1 exit criteria): every ADL file in `examples/`
//! parses with zero errors. `scripts/corpus_gate.sh` runs the same check via
//! the `parse_adl` example binary.

use adl_syntax::diag::Severity;
use std::path::PathBuf;

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../examples")
}

fn collect_adl_files(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect_adl_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "adl") {
            out.push(path);
        }
    }
}

#[test]
fn all_corpus_files_parse_with_zero_errors() {
    let root = corpus_root();
    assert!(root.is_dir(), "corpus not found at {}", root.display());
    let mut files = Vec::new();
    collect_adl_files(&root, &mut files);
    files.sort();
    assert_eq!(
        files.len(),
        133,
        "expected the 133-file corpus (68 base + 57 golden + 8 golden-cross), got {}",
        files.len()
    );

    let mut failures = Vec::new();
    for path in &files {
        let src = std::fs::read_to_string(path).expect("read corpus file");
        let result = adl_syntax::parse(&src);
        let errors: Vec<_> = result
            .diags
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            let name = path
                .strip_prefix(&root)
                .unwrap_or(path)
                .display()
                .to_string();
            failures.push(format!(
                "{name}: {} error(s); first: {}",
                errors.len(),
                errors[0].message
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "corpus files failed to parse:\n{}",
        failures.join("\n")
    );
}
