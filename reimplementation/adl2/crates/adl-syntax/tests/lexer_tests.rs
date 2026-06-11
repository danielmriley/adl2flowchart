//! Unit tests for every lexical rule in SPEC_LANGUAGE §2.

use adl_syntax::diag::Severity;
use adl_syntax::lex;
use adl_syntax::token::{Kw, TokKind};

/// Significant token kinds (newlines and EOF stripped).
fn kinds(src: &str) -> Vec<TokKind> {
    lex(src)
        .tokens
        .into_iter()
        .map(|t| t.kind)
        .filter(|k| !matches!(k, TokKind::Newline | TokKind::Eof))
        .collect()
}

fn errors(src: &str) -> Vec<String> {
    lex(src)
        .diags
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message)
        .collect()
}

fn ident(name: &str) -> TokKind {
    TokKind::Ident(name.to_string())
}

// ---------- identifiers and the underscore rule ----------

#[test]
fn simple_identifier() {
    assert_eq!(kinds("goodJets"), vec![ident("goodJets")]);
}

#[test]
fn underscore_joins_letter_segments() {
    // `_` followed by a letter stays inside the identifier.
    assert_eq!(kinds("HLT_iso_mu"), vec![ident("HLT_iso_mu")]);
    assert_eq!(kinds("m_T2"), vec![ident("m_T2")]);
    assert_eq!(kinds("Delphes_Photon"), vec![ident("Delphes_Photon")]);
}

#[test]
fn underscore_before_digit_splits_identifier() {
    // `goodJets_1` lexes as `goodJets` `_` `1` (underscore-indexing operator).
    assert_eq!(
        kinds("goodJets_1"),
        vec![ident("goodJets"), TokKind::Underscore, TokKind::Int(1)]
    );
}

#[test]
fn underscore_split_emits_note() {
    let out = lex("goodJets_1");
    assert!(
        out.diags
            .iter()
            .any(|d| d.severity == Severity::Note && d.message.contains("underscore-indexing")),
        "expected a split note, got {:?}",
        out.diags
    );
}

#[test]
fn trailing_underscore_is_separate_token() {
    // `JET_` (legacy per-element loop notation): ident then `_`.
    assert_eq!(
        kinds("JET_ }"),
        vec![ident("JET"), TokKind::Underscore, TokKind::RBrace]
    );
}

#[test]
fn digits_allowed_inside_segment() {
    assert_eq!(kinds("jets4DTiso"), vec![ident("jets4DTiso")]);
    assert_eq!(kinds("sep21v1"), vec![ident("sep21v1")]);
}

// ---------- keywords ----------

#[test]
fn keywords_are_case_insensitive() {
    assert_eq!(kinds("select"), vec![TokKind::Kw(Kw::Select)]);
    assert_eq!(kinds("SELECT"), vec![TokKind::Kw(Kw::Select)]);
    assert_eq!(kinds("Union"), vec![TokKind::Kw(Kw::Union)]);
    assert_eq!(kinds("OR"), vec![TokKind::Kw(Kw::Or)]);
    assert_eq!(kinds("histoList"), vec![TokKind::Kw(Kw::HistoList)]);
    assert_eq!(kinds("HISTOLIST"), vec![TokKind::Kw(Kw::HistoList)]);
    assert_eq!(kinds("ALL"), vec![TokKind::Kw(Kw::All)]);
}

#[test]
fn keyword_prefix_is_not_keyword() {
    // Max-munch: the whole word is checked, not a prefix.
    assert_eq!(kinds("selection"), vec![ident("selection")]);
    assert_eq!(kinds("binning"), vec![ident("binning")]);
}

// ---------- numbers ----------

#[test]
fn unsigned_int_and_real() {
    assert_eq!(kinds("42"), vec![TokKind::Int(42)]);
    assert_eq!(kinds("13.5"), vec![TokKind::Real(13.5)]);
}

#[test]
fn no_signed_literal_lexing() {
    // Divergence 4: `-5` is unary minus + literal; `5-3` is sub, not `5` `-3`.
    assert_eq!(kinds("-5"), vec![TokKind::Minus, TokKind::Int(5)]);
    assert_eq!(
        kinds("5-3"),
        vec![TokKind::Int(5), TokKind::Minus, TokKind::Int(3)]
    );
}

#[test]
fn real_requires_digits_both_sides() {
    // `5.` leaves the dot as a token (dotted access is grammar-level).
    assert_eq!(kinds("5 ."), vec![TokKind::Int(5), TokKind::Dot]);
    // `MET.phi` is ident dot ident, not a number or single token.
    assert_eq!(
        kinds("MET.phi"),
        vec![ident("MET"), TokKind::Dot, ident("phi")]
    );
}

#[test]
fn scientific_notation_is_an_error_with_help() {
    let out = lex("1e6");
    let err = out
        .diags
        .iter()
        .find(|d| d.severity == Severity::Error)
        .expect("expected a lexical error for 1e6");
    assert!(err.message.contains("scientific notation"));
    assert!(err.help.as_deref().unwrap_or("").contains("1000000.0"));
}

#[test]
fn raw_number_text_is_preserved() {
    let toks = lex("7000.0").tokens;
    assert_eq!(toks[0].text, "7000.0");
}

// ---------- strings ----------

#[test]
fn string_literal() {
    assert_eq!(
        kinds("\"jet1 pT (GeV)\""),
        vec![TokKind::Str("jet1 pT (GeV)".to_string())]
    );
}

#[test]
fn unterminated_string_is_an_error() {
    let errs = errors("title \"oops\nselect x > 1");
    assert!(errs.iter().any(|m| m.contains("unterminated string")));
    // Lexing continues after the error (multi-error reporting).
    let toks = kinds("title \"oops\nselect x > 1");
    assert!(toks.contains(&TokKind::Kw(Kw::Select)));
}

// ---------- comments / whitespace / newlines ----------

#[test]
fn comments_run_to_end_of_line() {
    assert_eq!(
        kinds("take Jet # take and loop over all jets\nselect"),
        vec![TokKind::Kw(Kw::Take), ident("Jet"), TokKind::Kw(Kw::Select)]
    );
}

#[test]
fn newline_tokens_are_emitted() {
    let toks = lex("a\nb").tokens;
    let kinds: Vec<&TokKind> = toks.iter().map(|t| &t.kind).collect();
    assert!(matches!(kinds[1], TokKind::Newline));
}

#[test]
fn non_ascii_in_comments_is_fine() {
    assert!(errors("# at s√= 13 TeV\nselect x > 1").is_empty());
}

// ---------- operators ----------

#[test]
fn comparison_operators() {
    assert_eq!(
        kinds("> < >= <= == != ~="),
        vec![
            TokKind::Gt,
            TokKind::Lt,
            TokKind::Ge,
            TokKind::Le,
            TokKind::EqEq,
            TokKind::Ne,
            TokKind::TildeEq,
        ]
    );
}

#[test]
fn band_operators_require_adjacency() {
    assert_eq!(kinds("[]"), vec![TokKind::BandIn]);
    assert_eq!(kinds("]["), vec![TokKind::BandOut]);
    // Separated brackets stay separate (indexing).
    assert_eq!(
        kinds("[0]"),
        vec![TokKind::LBracket, TokKind::Int(0), TokKind::RBracket]
    );
}

#[test]
fn plus_minus_token() {
    assert_eq!(kinds("+-"), vec![TokKind::PlusMinus]);
    assert_eq!(kinds("+ -"), vec![TokKind::Plus, TokKind::Minus]);
}

#[test]
fn logical_operator_tokens() {
    assert_eq!(kinds("&&"), vec![TokKind::AmpAmp]);
    assert_eq!(kinds("||"), vec![TokKind::PipePipe]);
    assert_eq!(kinds("|x|"), vec![TokKind::Pipe, ident("x"), TokKind::Pipe]);
    assert_eq!(kinds("!"), vec![TokKind::Bang]);
}

#[test]
fn arithmetic_and_structure_tokens() {
    assert_eq!(
        kinds("+ - * / ^ = ? : ( ) { } , . _"),
        vec![
            TokKind::Plus,
            TokKind::Minus,
            TokKind::Star,
            TokKind::Slash,
            TokKind::Caret,
            TokKind::Assign,
            TokKind::Question,
            TokKind::Colon,
            TokKind::LParen,
            TokKind::RParen,
            TokKind::LBrace,
            TokKind::RBrace,
            TokKind::Comma,
            TokKind::Dot,
            TokKind::Underscore,
        ]
    );
}

// ---------- error recovery ----------

#[test]
fn unknown_character_is_skipped_with_error() {
    let out = lex("select ; x > 1");
    assert!(
        out.diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("unexpected character"))
    );
    // The `;` is skipped; surrounding tokens survive.
    let ks = kinds("select ; x > 1");
    assert_eq!(
        ks,
        vec![
            TokKind::Kw(Kw::Select),
            ident("x"),
            TokKind::Gt,
            TokKind::Int(1)
        ]
    );
}

#[test]
fn multiple_lexical_errors_in_one_run() {
    let errs = errors("a ; b @ c");
    assert_eq!(errs.len(), 2);
}

#[test]
fn spans_carry_line_and_column() {
    let toks = lex("a\n  b").tokens;
    let b = toks
        .iter()
        .find(|t| matches!(&t.kind, TokKind::Ident(n) if n == "b"))
        .unwrap();
    assert_eq!(b.line, 2);
    assert_eq!(b.span.start, 4); // "a\n  " = 4 bytes
}
