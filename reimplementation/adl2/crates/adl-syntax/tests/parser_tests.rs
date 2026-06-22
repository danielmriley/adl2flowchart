//! Parser unit tests: every SPEC_LANGUAGE §3.1 divergence plus the grammar
//! constructs the corpus exercises.

use adl_syntax::ast::*;
use adl_syntax::diag::Severity;
use adl_syntax::{dump_ast, parse};

fn parse_ok(src: &str) -> File {
    let r = parse(src);
    let errs: Vec<_> = r
        .diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errs.is_empty(),
        "unexpected errors: {errs:#?}\nsource:\n{src}"
    );
    r.file
}

/// Parse a single `select` condition inside a region and return the Expr.
fn cond(src_cond: &str) -> Expr {
    let src = format!("region R\n  select {src_cond}\n");
    let file = parse_ok(&src);
    let Section::Region(region) = &file.sections[0] else {
        panic!("expected region");
    };
    let RegionStmt::Cut { cond, .. } = &region.stmts[0] else {
        panic!("expected cut");
    };
    cond.clone()
}

fn dump_cond(src_cond: &str) -> String {
    let src = format!("region R\n  select {src_cond}\n");
    let file = parse_ok(&src);
    dump_ast(&src, &file)
}

// ---------- divergence 1: `or` binds looser than `and` ----------

#[test]
fn or_binds_looser_than_and() {
    let e = cond("a and b or c");
    let Expr::Binary {
        op: BinOp::Or, lhs, ..
    } = e
    else {
        panic!("top must be Or, got {e:?}");
    };
    assert!(matches!(*lhs, Expr::Binary { op: BinOp::And, .. }));
}

#[test]
fn double_pipe_and_double_amp_are_or_and() {
    let e = cond("a && b || c");
    let Expr::Binary {
        op: BinOp::Or, lhs, ..
    } = e
    else {
        panic!("top must be Or");
    };
    assert!(matches!(*lhs, Expr::Binary { op: BinOp::And, .. }));
}

// ---------- divergence 2: `not` is properly recursive ----------

#[test]
fn not_not_x_parses() {
    let e = cond("not not x");
    let Expr::Unary {
        op: UnaryOp::Not,
        expr,
        ..
    } = e
    else {
        panic!("outer not");
    };
    assert!(matches!(
        *expr,
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
}

#[test]
fn not_parenthesized_or_parses() {
    let e = cond("not (a or b)");
    let Expr::Unary {
        op: UnaryOp::Not,
        expr,
        ..
    } = e
    else {
        panic!("not");
    };
    assert!(matches!(*expr, Expr::Binary { op: BinOp::Or, .. }));
}

#[test]
fn define_not_parses() {
    let file = parse_ok("define x = not y\n");
    let Section::Define(d) = &file.sections[0] else {
        panic!("define");
    };
    assert!(matches!(
        d.body,
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
}

#[test]
fn bang_is_not() {
    assert!(matches!(
        cond("!x"),
        Expr::Unary {
            op: UnaryOp::Not,
            ..
        }
    ));
}

// ---------- divergence 3: dotted access is grammar, not a token ----------

#[test]
fn dotted_access_on_index() {
    // `jets[0].pt` was impossible with lexed dotted tokens.
    let e = cond("jets[0].pt > 30");
    let Expr::Cmp { lhs, .. } = e else {
        panic!("cmp")
    };
    let Expr::Dot { base, field, .. } = *lhs else {
        panic!("dot, got {lhs:?}");
    };
    assert_eq!(field.name, "pt");
    assert!(matches!(*base, Expr::Index { .. }));
}

#[test]
fn dotted_chain() {
    let e = cond("a.b.c > 1");
    let Expr::Cmp { lhs, .. } = e else {
        panic!("cmp")
    };
    let Expr::Dot { base, field, .. } = *lhs else {
        panic!("dot")
    };
    assert_eq!(field.name, "c");
    assert!(matches!(*base, Expr::Dot { .. }));
}

// ---------- divergence 4: unsigned literals + unary minus everywhere ----------

#[test]
fn unary_minus_in_expression() {
    let e = cond("x == -1");
    let Expr::Cmp { rhs, .. } = e else {
        panic!("cmp")
    };
    assert!(matches!(
        *rhs,
        Expr::Unary {
            op: UnaryOp::Neg,
            ..
        }
    ));
}

#[test]
fn negative_band_bounds() {
    let e = cond("eta [] -2.4 2.4");
    let Expr::Band { lo, hi, .. } = e else {
        panic!("band")
    };
    assert!(lo.neg);
    assert_eq!(lo.canon(), "-2.4");
    assert_eq!(hi.canon(), "2.4");
}

#[test]
fn negative_table_cells() {
    let src = "table t\n  tabletype efficiency\n  nvars 1\n  errors false\n  0.5 -5.5 0.0\n";
    let file = parse_ok(src);
    let Section::Table(t) = &file.sections[0] else {
        panic!("table")
    };
    let vals: Vec<String> = t.values.iter().map(NumLit::canon).collect();
    assert_eq!(vals, vec!["0.5", "-5.5", "0.0"]);
}

#[test]
fn five_minus_three_is_subtraction() {
    let e = cond("x > 5-3");
    let Expr::Cmp { rhs, .. } = e else {
        panic!("cmp")
    };
    assert!(matches!(*rhs, Expr::Binary { op: BinOp::Sub, .. }));
}

// ---------- divergence 5: bin boundaries are reals ----------

#[test]
fn real_bin_edges_preserved() {
    let src = "region R\n  select x > 1\n  bin MET 250.5 300.7 500.0\n";
    let file = parse_ok(src);
    let Section::Region(region) = &file.sections[0] else {
        panic!()
    };
    let RegionStmt::Bin {
        body: BinBody::Boundaries { edges, .. },
        ..
    } = &region.stmts[1]
    else {
        panic!("boundary bin, got {:?}", region.stmts[1]);
    };
    let es: Vec<String> = edges.iter().map(NumLit::canon).collect();
    assert_eq!(es, vec!["250.5", "300.7", "500.0"]);
    assert!(edges.iter().all(|e| e.is_real));
}

#[test]
fn negative_bin_edges() {
    let src = "region R\n  bin eta -2.4 0.0 2.4\n";
    let file = parse_ok(src);
    let Section::Region(region) = &file.sections[0] else {
        panic!()
    };
    let RegionStmt::Bin {
        body: BinBody::Boundaries { edges, .. },
        ..
    } = &region.stmts[0]
    else {
        panic!("boundary bin");
    };
    assert_eq!(edges[0].canon(), "-2.4");
}

// ---------- divergence 6: multi-arg union ----------

#[test]
fn union_with_three_members() {
    let file = parse_ok("object leptons\n  take union(eles, muons, taus)\n");
    let Section::Object(obj) = &file.sections[0] else {
        panic!()
    };
    let ObjectStmt::Take {
        source: TakeSource::Union { members, .. },
        ..
    } = &obj.stmts[0]
    else {
        panic!("union take");
    };
    assert_eq!(members.len(), 3);
}

// ---------- divergence 7: particle-list arguments ----------

#[test]
fn particle_list_argument() {
    let e = cond("pT(jets[0] jets[1]) > 100");
    let Expr::Cmp { lhs, .. } = e else {
        panic!("cmp")
    };
    let Expr::Call { args, .. } = *lhs else {
        panic!("call")
    };
    let Arg::Expr(arg) = &args[0] else {
        panic!("expr arg")
    };
    let Expr::ParticleList { items, .. } = arg.as_ref() else {
        panic!("particle list, got {arg:?}");
    };
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| matches!(i, Expr::Index { .. })));
}

#[test]
fn comb_with_negative_index_particle_list() {
    let file = parse_ok("object OS\n  take COMB(leptons[-1] leptons[-2])\n");
    let Section::Object(obj) = &file.sections[0] else {
        panic!()
    };
    let ObjectStmt::Take {
        source: TakeSource::Call { name, args },
        ..
    } = &obj.stmts[0]
    else {
        panic!("call take");
    };
    assert_eq!(name.name, "COMB");
    let Arg::Expr(arg) = &args[0] else { panic!() };
    assert!(matches!(arg.as_ref(), Expr::ParticleList { .. }));
}

#[test]
fn define_body_particle_list() {
    let file = parse_ok("define Zreco : leptons[-1] leptons[-2]\n");
    let Section::Define(d) = &file.sections[0] else {
        panic!()
    };
    assert!(matches!(d.body, Expr::ParticleList { .. }));
}

// ---------- underscore indexing (`goodJets_1`, live in ex04/ex10/SUS-16-033) ----------

#[test]
fn underscore_indexing_on_good_jets() {
    let e = cond("pT(goodJets_1) > 100");
    let Expr::Cmp { lhs, .. } = e else {
        panic!("cmp")
    };
    let Expr::Call { args, .. } = *lhs else {
        panic!("call")
    };
    let Arg::Expr(arg) = &args[0] else { panic!() };
    let Expr::UnderscoreIndex { base, index, .. } = arg.as_ref() else {
        panic!("underscore index, got {arg:?}");
    };
    let Expr::Ident(id) = base.as_ref() else {
        panic!()
    };
    assert_eq!(id.name, "goodJets");
    assert_eq!(index.value, 1);
    assert!(!index.neg);
}

#[test]
fn trailing_underscore_means_all_elements() {
    let e = cond("{ JET_ }Pt > 20");
    let Expr::Cmp { lhs, .. } = e else {
        panic!("cmp")
    };
    let Expr::Braced { args, prop, .. } = *lhs else {
        panic!("braced")
    };
    assert_eq!(prop.name, "Pt");
    let Arg::Expr(arg) = &args[0] else { panic!() };
    assert!(matches!(arg.as_ref(), Expr::UnderscoreAll { .. }));
}

// ---------- ternary ----------

#[test]
fn ternary_with_and_without_else() {
    let e = cond("g > 1 ? a > 2 : b > 3");
    assert!(matches!(e, Expr::Ternary { els: Some(_), .. }));
    let e = cond("n(jets) > 2 ? dphi(jets[2], MET) > 0.3");
    assert!(matches!(e, Expr::Ternary { els: None, .. }));
}

#[test]
fn nested_ternary_associates_like_legacy_corpus() {
    // `Dab < 0 ? Dac < 0 ? A > 0.8 : B > 0.8 : C > 0.8`
    let e = cond("Dab < 0 ? Dac < 0 ? A > 0.8 : B > 0.8 : C > 0.8");
    let Expr::Ternary { then, els, .. } = e else {
        panic!("ternary")
    };
    assert!(matches!(*then, Expr::Ternary { els: Some(_), .. }));
    assert!(els.is_some());
}

#[test]
fn ternary_with_or_guard() {
    let e = cond("abs(id) == 11 or abs(id) == 13 ? pT > 5 : pT > 10");
    let Expr::Ternary { guard, .. } = e else {
        panic!("ternary")
    };
    assert!(matches!(*guard, Expr::Binary { op: BinOp::Or, .. }));
}

// ---------- bands ----------

#[test]
fn excluded_band() {
    let e = cond("x ][ 0.5 2.0");
    assert!(matches!(
        e,
        Expr::Band {
            kind: BandKind::Out,
            ..
        }
    ));
}

// ---------- slices and indices ----------

#[test]
fn slice_forms() {
    assert!(matches!(
        cond("min(dR(jets[0:2], MET)) > 0.3"),
        Expr::Cmp { .. }
    ));
    let d = dump_cond("min(dR(jets[0:2], MET)) > 0.3");
    assert!(d.contains("Slice 0:2"), "{d}");
    let d = dump_cond("f(jets[:2]) > 1");
    assert!(d.contains("Slice :2"), "{d}");
    let d = dump_cond("f(jets[3:]) > 1");
    assert!(d.contains("Slice 3:"), "{d}");
}

#[test]
fn negative_index_parses_with_warning() {
    let src = "define b = goodjet[-2]\n";
    let r = parse(src);
    assert!(!adl_syntax::has_errors(&r.diags));
    assert!(
        r.diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("OPEN-3"))
    );
}

// ---------- abs bars, braced property, calls ----------

#[test]
fn abs_bars() {
    let e = cond("|eta| < 2.4");
    let Expr::Cmp { lhs, .. } = e else { panic!() };
    assert!(matches!(*lhs, Expr::Abs { .. }));
}

#[test]
fn braced_multi_arg_property() {
    let e = cond("{ JET_ , ELE_ }dR >= 0.2");
    let Expr::Cmp { lhs, .. } = e else { panic!() };
    let Expr::Braced { args, prop, .. } = *lhs else {
        panic!()
    };
    assert_eq!(args.len(), 2);
    assert_eq!(prop.name, "dR");
}

#[test]
fn call_with_spaces_before_paren() {
    let e = cond("Size ( tightphotons ) >= 1");
    let Expr::Cmp { lhs, .. } = e else { panic!() };
    assert!(matches!(*lhs, Expr::Call { .. }));
}

// ---------- `~=` ----------

#[test]
fn tilde_eq_parses_with_one_warning_per_file() {
    let src = "region R\n  select a ~= 0\n  select b ~= 1\n";
    let r = parse(src);
    assert!(!adl_syntax::has_errors(&r.diags));
    let warns = r
        .diags
        .iter()
        .filter(|d| d.message.contains("OPEN-4"))
        .count();
    assert_eq!(warns, 1);
}

// ---------- path tokens ----------

#[test]
fn path_token_in_arg_position() {
    let src = "define b = BDT(TMVA_BDT.weights-2016-x.xml, dxyVtx)\n";
    let r = parse(src);
    assert!(!adl_syntax::has_errors(&r.diags), "{:?}", r.diags);
    let Section::Define(d) = &r.file.sections[0] else {
        panic!()
    };
    let Expr::Call { args, .. } = &d.body else {
        panic!()
    };
    let Arg::Path(p) = &args[0] else {
        panic!("path arg, got {:?}", args[0]);
    };
    assert_eq!(p.value, "TMVA_BDT.weights-2016-x.xml");
    // Deprecation warning suggests quoting.
    assert!(
        r.diags
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("deprecated"))
    );
}

#[test]
fn dotted_access_not_mistaken_for_path() {
    // `MET.phi` has no `-`/`/`: stays grammar-level dotted access.
    let e = cond("cos(MET.phi - jets[0].phi) > 0");
    let Expr::Cmp { lhs, .. } = e else { panic!() };
    let Expr::Call { args, .. } = *lhs else {
        panic!()
    };
    let Arg::Expr(arg) = &args[0] else { panic!() };
    assert!(matches!(arg.as_ref(), Expr::Binary { op: BinOp::Sub, .. }));
}

// ---------- blocks ----------

#[test]
fn object_block_take_variants() {
    let src = "object a take Jet\nobject b : Ele\nobject c\n  using Muo\n";
    let file = parse_ok(src);
    let kws: Vec<String> = file
        .sections
        .iter()
        .map(|s| {
            let Section::Object(o) = s else { panic!() };
            let ObjectStmt::Take { keyword, .. } = &o.stmts[0] else {
                panic!()
            };
            keyword.clone()
        })
        .collect();
    assert_eq!(kws, vec!["take", ":", "using"]);
}

#[test]
fn take_binders_and_alias() {
    let src = "object cleanjets\n  take jets j\n  reject dR(j, electrons) < 0.2\n\
               composite OS\n  take leptons l1, l2\n  select l1.pdgID + l2.pdgID == 0\n\
               object OSd : COMB(a[-1] a[-2]) alias adilepton\n";
    let file = parse_ok(src);
    let Section::Object(o1) = &file.sections[0] else {
        panic!()
    };
    let ObjectStmt::Take { binders, .. } = &o1.stmts[0] else {
        panic!()
    };
    assert_eq!(binders.len(), 1);
    assert!(matches!(o1.stmts[1], ObjectStmt::Reject { .. }));
    let Section::Object(o2) = &file.sections[1] else {
        panic!()
    };
    assert_eq!(o2.keyword, ObjectKw::Composite);
    let ObjectStmt::Take { binders, .. } = &o2.stmts[0] else {
        panic!()
    };
    assert_eq!(
        binders.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(),
        vec!["l1", "l2"]
    );
    let Section::Object(o3) = &file.sections[2] else {
        panic!()
    };
    let ObjectStmt::Take { alias, .. } = &o3.stmts[0] else {
        panic!()
    };
    assert_eq!(alias.as_ref().unwrap().name, "adilepton");
}

#[test]
fn multiple_takes_in_one_object() {
    let file = parse_ok("object leptons\n  take electrons\n  take muons\n");
    let Section::Object(o) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(o.stmts.len(), 2);
}

#[test]
fn region_statements_roundup() {
    let src = "\
region R
  select ALL
  reject MET < 50
  trigger hltsingleele
  weight xsec 0.688016
  weight trigger 0.65
  histo h , \"t\", 20, 0, 20, size(jets)
  print size(jets), MET
  save out csv size(jets), MET
  counts results 997 +- 32 , 933
  sort pt(jet) ascend
  baseline
  type control
";
    let file = parse_ok(src);
    let Section::Region(r) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(r.stmts.len(), 12);
    assert!(matches!(r.stmts[0], RegionStmt::Cut { .. }));
    assert!(matches!(r.stmts[1], RegionStmt::Reject { .. }));
    assert!(matches!(r.stmts[2], RegionStmt::Trigger { .. }));
    assert!(matches!(r.stmts[3], RegionStmt::Weight { .. }));
    let RegionStmt::Weight { name, .. } = &r.stmts[4] else {
        panic!()
    };
    assert_eq!(name.name, "trigger");
    assert!(matches!(r.stmts[5], RegionStmt::Histo { .. }));
    assert!(matches!(r.stmts[6], RegionStmt::Print { .. }));
    assert!(matches!(r.stmts[7], RegionStmt::Save { .. }));
    let RegionStmt::Counts { items, .. } = &r.stmts[8] else {
        panic!()
    };
    assert_eq!(items.join(" "), "997 +- 32 , 933");
    let RegionStmt::Sort { raw, .. } = &r.stmts[9] else {
        panic!()
    };
    assert_eq!(raw, "pt(jet) ascend");
    assert!(matches!(&r.stmts[10], RegionStmt::RegionRef(id) if id.name == "baseline"));
    let RegionStmt::TypeTag { value, .. } = &r.stmts[11] else {
        panic!()
    };
    assert_eq!(value.name, "control");
}

#[test]
fn region_take_is_inheritance_synonym_for_a_bare_ref() {
    // Canonical ADL inherits a region's cuts with `take <region>`; smash2's
    // native form is a bare region name. Both must parse to RegionRef.
    let src = "region base\n  select x > 1\nregion sr\n  take base\n  select y > 2\n";
    let file = parse_ok(src);
    let Section::Region(r) = &file.sections[1] else {
        panic!()
    };
    assert!(matches!(&r.stmts[0], RegionStmt::RegionRef(id) if id.name == "base"));
}

#[test]
fn bin_with_label_and_boolean_body() {
    let src = "region R\n  bin \"35\" m(j[0]) [] 65 105 and MET > 300\n";
    let file = parse_ok(src);
    let Section::Region(r) = &file.sections[0] else {
        panic!()
    };
    let RegionStmt::Bin { label, body, .. } = &r.stmts[0] else {
        panic!()
    };
    assert_eq!(label.as_ref().unwrap().value, "35");
    assert!(matches!(body, BinBody::Cond(_)));
}

#[test]
fn histo_variable_bin_numlist() {
    let src = "region R\n  histo h, \"t\", 0.0 10.0 20.0 50.0, MET\n";
    let file = parse_ok(src);
    let Section::Region(r) = &file.sections[0] else {
        panic!()
    };
    let RegionStmt::Histo { args, .. } = &r.stmts[0] else {
        panic!()
    };
    let HistoArg::NumList(edges) = &args[0] else {
        panic!("numlist, got {:?}", args[0]);
    };
    assert_eq!(edges.len(), 4);
    assert!(matches!(args[1], HistoArg::Expr(_)));
}

#[test]
fn info_table_countsformat_blocks() {
    let src = "\
info analysis
  title \"a title\"
  sqrtS 13.0
  experiment CMS
countsformat results
  process estbg, \"Total estimated BG\", stat, syst
  process obsdata, \"Observed data\"
table eleTrigger1
  tabletype efficiency
  nvars 1
  errors false
  0.3 0.0 10.0
  0.5 10.0 20.0
";
    let file = parse_ok(src);
    let Section::Info(info) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(info.lines.len(), 3);
    let Section::CountsFormat(cf) = &file.sections[1] else {
        panic!()
    };
    assert_eq!(cf.processes.len(), 2);
    assert_eq!(cf.processes[0].columns.len(), 2);
    let Section::Table(t) = &file.sections[2] else {
        panic!()
    };
    assert_eq!(t.nvars, 1);
    assert!(!t.errors);
    assert_eq!(t.values.len(), 6);
}

#[test]
fn algo_and_histolist_are_region_blocks() {
    let src = "algo presel\ncmd ALL\nhistoList hl\n  histo h, \"t\", 10, 0, 1, MET\n";
    let file = parse_ok(src);
    let Section::Region(a) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(a.keyword, RegionKw::Algo);
    let Section::Region(h) = &file.sections[1] else {
        panic!()
    };
    assert_eq!(h.keyword, RegionKw::HistoList);
}

#[test]
fn select_with_region_name_is_plain_ident_cut() {
    let e = cond("noncompressed");
    assert!(matches!(e, Expr::Ident(id) if id.name == "noncompressed"));
}

// ---------- determinism of the canonical dump ----------

#[test]
fn dump_is_deterministic() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../../examples/tutorials/ex04_syntaxes.adl"
    ))
    .expect("corpus file");
    let a = {
        let r = parse(&src);
        dump_ast(&src, &r.file)
    };
    let b = {
        let r = parse(&src);
        dump_ast(&src, &r.file)
    };
    assert_eq!(a, b);
}

#[test]
fn info_freeform_value_and_composite_candidate() {
    // Regression for the two NPS-corpus parser gaps:
    //  1. info-line values are free-form metadata (URLs, arithmetic,
    //     punctuation) consumed raw to end of line — never tokenized/rejected.
    //  2. `candidate <name> = <expr>` is the NPS-dialect synonym for the
    //     composite derived object `object <name> = <expr>`.
    let src = "info analysis\n\
               \x20 title Search for SUSY in diphoton + MET\n\
               \x20 arXiv XXXX.XXXXX\n\
               \x20 hepdata https://www.hepdata.net/record/xxxx\n\
               composite dilepton\n\
               \x20 take disjoint(leptons l1, leptons l2)\n\
               \x20 candidate ll = l1 + l2\n\
               \x20 select mass(ll) > 20\n";
    let file = parse_ok(src);

    let Section::Info(info) = &file.sections[0] else {
        panic!("expected info block");
    };
    assert_eq!(info.lines.len(), 3);
    assert_eq!(info.lines[0].key.name, "title");
    assert_eq!(info.lines[0].value, "Search for SUSY in diphoton + MET");
    assert_eq!(info.lines[1].value, "XXXX.XXXXX");
    assert_eq!(info.lines[2].key.name, "hepdata");
    assert_eq!(info.lines[2].value, "https://www.hepdata.net/record/xxxx");

    let Section::Object(comp) = &file.sections[1] else {
        panic!("expected composite block");
    };
    assert_eq!(comp.keyword, ObjectKw::Composite);
    let derived = comp
        .stmts
        .iter()
        .find_map(|s| match s {
            ObjectStmt::Derived { keyword, name, .. } => Some((keyword.clone(), name.name.clone())),
            _ => None,
        })
        .expect("composite block must contain a derived candidate");
    assert_eq!(derived, ("candidate".to_string(), "ll".to_string()));
}

#[test]
fn composite_object_derived_synonym_and_take_colon_not_captured() {
    // Canonical spelling `object Z = l1 + l2` inside composite is a derived
    // candidate; but `object foo : take(...)` (`:` separator) must still
    // terminate the composite and start a new top-level object block.
    let src = "composite Zc\n\
               \x20 take disjoint(leptons l1, leptons l2)\n\
               \x20 object Z = l1 + l2\n\
               object downstream\n\
               \x20 take Zc\n";
    let file = parse_ok(src);
    let Section::Object(comp) = &file.sections[0] else {
        panic!("expected composite");
    };
    assert!(comp.stmts.iter().any(|s| matches!(
        s,
        ObjectStmt::Derived { keyword, .. } if keyword == "object"
    )));
    // The following `object downstream` is its own section, not swallowed.
    assert_eq!(file.sections.len(), 2);
    let Section::Object(downstream) = &file.sections[1] else {
        panic!("expected downstream object block");
    };
    assert_eq!(downstream.name.name, "downstream");
}

// ---------- `->` member access (NPS dialect: `dijet->j1[0]`) ----------

#[test]
fn member_access_simple() {
    let e = cond("size(dijet->jj) == 1");
    // size(...) is a Call; its argument is a Member.
    let Expr::Cmp { lhs, .. } = &e else {
        panic!("expected cmp, got {e:?}");
    };
    let Expr::Call { args, name, .. } = lhs.as_ref() else {
        panic!("expected call");
    };
    assert_eq!(name.name, "size");
    let Arg::Expr(arg) = &args[0] else { panic!() };
    let Expr::Member { field, .. } = arg.as_ref() else {
        panic!("expected member, got {arg:?}");
    };
    assert_eq!(field.name, "jj");
}

#[test]
fn member_access_then_index() {
    // `dijet->j1[0]` is Member then Index in the same postfix loop.
    let e = cond("eta(dijet->j1[0]) > 2.0");
    let Expr::Cmp { lhs, .. } = &e else { panic!() };
    let Expr::Call { args, .. } = lhs.as_ref() else {
        panic!()
    };
    let Arg::Expr(arg) = &args[0] else { panic!() };
    let Expr::Index { base, .. } = arg.as_ref() else {
        panic!("expected index, got {arg:?}");
    };
    let Expr::Member { field, .. } = base.as_ref() else {
        panic!("expected member base");
    };
    assert_eq!(field.name, "j1");
}

#[test]
fn arrow_does_not_break_subtraction() {
    // The greedy `->` lex must not steal `-` from `a - b` or negatives.
    let e = cond("eta(j1) - eta(j2) > -1.5");
    assert!(matches!(e, Expr::Cmp { .. }));
}

// ---------- `sort(...)` take source (NPS: `take sort(jets, pt(jets), descend)`) ----------

#[test]
fn sort_take_source_is_call() {
    let file = parse_ok("object sortedJets\n  take sort(jets, pt(jets), descend)\n");
    let Section::Object(obj) = &file.sections[0] else {
        panic!()
    };
    let ObjectStmt::Take {
        source: TakeSource::Call { name, args },
        ..
    } = &obj.stmts[0]
    else {
        panic!("expected call take source");
    };
    assert_eq!(name.name, "sort");
    assert_eq!(args.len(), 3);
}

// ---------- `all(...)` as a call (correctness hardening) ----------

#[test]
fn all_with_args_is_call() {
    let e = cond("all(jets, pt > 30)");
    let Expr::Call { name, .. } = &e else {
        panic!("expected call, got {e:?}");
    };
    assert_eq!(name.name, "all");
}

#[test]
fn bare_all_still_keyword() {
    let e = cond("pt(jets) > all");
    let Expr::Cmp { rhs, .. } = &e else { panic!() };
    assert!(matches!(rhs.as_ref(), Expr::All(_)));
}

// ---------- underscore-in-section-name (NPS25011 `SR3L_1`, NPS25009 `SR_3b3j`) ----------

#[test]
fn region_name_with_underscore_digit() {
    let file = parse_ok("region SR3L_1\n  select pt(jets) > 30\n");
    let Section::Region(r) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(r.name.name, "SR3L_1");
    assert_eq!(r.stmts.len(), 1);
}

#[test]
fn region_name_with_underscore_digit_then_ident() {
    let file = parse_ok("region SR_3b3j\n  select pt(jets) > 30\n");
    let Section::Region(r) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(r.name.name, "SR_3b3j");
}

#[test]
fn object_name_with_underscore_digit() {
    let file = parse_ok("object jets_2x\n  take Jet\n");
    let Section::Object(o) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(o.name.name, "jets_2x");
}

#[test]
fn spaced_underscore_index_not_absorbed_into_name() {
    // `region R` followed by a select using `goodJets_1` indexing must NOT
    // absorb the indexing into the region name.
    let file = parse_ok("region R\n  select pT(goodJets_1) > 30\n");
    let Section::Region(r) = &file.sections[0] else {
        panic!()
    };
    assert_eq!(r.name.name, "R");
}

// ---------- recover_block_stmt backstop (unrecognized stmt -> warning, not error) ----------

#[test]
fn unrecognized_region_stmt_is_warning_not_error() {
    // A statement starting with a token the dispatcher does not recognize as a
    // keyword or a bare region reference (here a leading `*`) yields `None`
    // from `parse_region_stmt`, triggering the warning-level recovery backstop.
    let r = parse("region R\n  * garbage here\n  select pt(jets) > 30\n");
    let has_error = r.diags.iter().any(|d| d.severity == Severity::Error);
    assert!(!has_error, "should not be an error: {:#?}", r.diags);
    let has_warn = r
        .diags
        .iter()
        .any(|d| d.severity == Severity::Warning && d.message.contains("unrecognized"));
    assert!(has_warn, "expected a recovery warning: {:#?}", r.diags);
    // The well-formed `select` after the bad line still parses.
    let Section::Region(region) = &r.file.sections[0] else {
        panic!()
    };
    assert!(
        region
            .stmts
            .iter()
            .any(|s| matches!(s, RegionStmt::Cut { .. }))
    );
}
