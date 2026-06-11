//! Error-message quality battery on deliberately broken inputs
//! (PLAN Phase 1 exit criteria; SPEC_LANGUAGE §3.2).

use adl_syntax::ast::{RegionStmt, Section};
use adl_syntax::diag::Severity;
use adl_syntax::{has_errors, parse, render_diagnostics};

fn errors(src: &str) -> Vec<adl_syntax::Diagnostic> {
    parse(src)
        .diags
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

/// 1. `selct` typo: span + label + "did you mean `select`?" help, and the
///    rest of the file still parses.
#[test]
fn selct_typo_gets_did_you_mean() {
    let src = "object jets\n  take Jet\n\nregion SR\n  selct MET > 100\n  select HT > 50\n";
    let r = parse(src);
    let errs: Vec<_> = r
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errs.len(), 1, "{:?}", r.diags);
    let e = errs[0];
    assert!(e.message.contains("`selct`"));
    assert_eq!(e.help.as_deref(), Some("did you mean `select`?"));
    // Recovery: the following select statement still lands in the AST.
    let Section::Region(region) = &r.file.sections[1] else {
        panic!()
    };
    assert!(
        region
            .stmts
            .iter()
            .any(|s| matches!(s, RegionStmt::Cut { .. })),
        "statement after the typo must still parse: {:?}",
        region.stmts
    );
    // Rendered output carries span, line excerpt and help.
    let rendered = render_diagnostics(src, "broken.adl", &r.diags);
    assert!(rendered.contains("broken.adl:5:3"));
    assert!(rendered.contains("selct MET > 100"));
    assert!(rendered.contains("did you mean `select`?"));
}

/// 2. Stray `;`: a lexical error with recovery; everything else parses.
#[test]
fn stray_semicolon_is_reported_and_skipped() {
    let src = "region SR\n  select MET > 100;\n  select HT > 50\n";
    let r = parse(src);
    let errs: Vec<_> = r
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("unexpected character `;`"));
    let Section::Region(region) = &r.file.sections[0] else {
        panic!()
    };
    assert_eq!(region.stmts.len(), 2, "both selects must survive");
}

/// 3. Unterminated string.
#[test]
fn unterminated_string_reported_with_help() {
    let src = "info analysis\n  title \"oops\nregion SR\n  select MET > 100\n";
    let r = parse(src);
    let errs: Vec<_> = r
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("unterminated string"));
    assert!(errs[0].help.as_deref().unwrap_or("").contains("closing"));
    // The region after the broken info block still parses.
    assert!(
        r.file
            .sections
            .iter()
            .any(|s| matches!(s, Section::Region(_)))
    );
}

/// 4. `not not x` must PARSE (divergence 2: it is a syntax error in legacy).
#[test]
fn not_not_x_is_not_an_error() {
    let src = "region SR\n  select not not passed\n";
    assert!(errors(src).is_empty());
}

/// 5. Bad number: scientific notation with a concrete rewrite suggestion.
#[test]
fn bad_number_sci_notation() {
    let src = "region SR\n  select MET > 1e6\n";
    let errs = errors(src);
    assert_eq!(errs.len(), 1);
    assert!(errs[0].message.contains("scientific notation"));
    assert!(errs[0].help.as_deref().unwrap_or("").contains("1000000.0"));
}

/// 6. Mid-file garbage: errors are reported, recovery continues, and the
///    sections before/after the garbage are intact.
#[test]
fn midfile_garbage_recovers() {
    let src = "\
object jets
  take Jet

flooble blah > 5 wibble

region SR
  select size(jets) > 2
";
    let r = parse(src);
    assert!(has_errors(&r.diags));
    let errs: Vec<_> = r
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errs[0].message.contains("`flooble`"),
        "first error names the garbage token: {errs:?}"
    );
    // Both real sections parsed.
    assert!(
        r.file
            .sections
            .iter()
            .any(|s| matches!(s, Section::Object(_)))
    );
    let region = r
        .file
        .sections
        .iter()
        .find_map(|s| match s {
            Section::Region(region) => Some(region),
            _ => None,
        })
        .expect("region after garbage must parse");
    assert_eq!(region.stmts.len(), 1);
}

/// All six broken inputs leave the parser with a nonzero error count and a
/// renderable report (no panics, no empty messages).
#[test]
fn all_broken_inputs_render_cleanly() {
    let cases = [
        "region SR\n  selct MET > 100\n",
        "region SR\n  select MET > 100;\n",
        "info a\n  title \"oops\n",
        "region SR\n  select MET > 1e6\n",
        "object jets\n  take Jet\n@@@???\nregion SR\n  select x > 1\n",
        "region SR\n  select > 5\n",
    ];
    for src in cases {
        let r = parse(src);
        let rendered = render_diagnostics(src, "case.adl", &r.diags);
        assert!(has_errors(&r.diags), "expected errors for {src:?}");
        assert!(!rendered.is_empty());
        for d in &r.diags {
            assert!(!d.message.is_empty());
        }
    }
}
