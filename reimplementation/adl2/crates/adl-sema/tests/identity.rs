//! Identity unit battery (PLAN Phase-2 exit criteria).
//!
//! Pure rename ≡ source; filtered ≢ parent; `jets[0].x` ≢ `jets[1].x`;
//! `dphi(a,b)` ≢ `dphi(b,a)` but `dR(a,b)` ≡ `dR(b,a)`; define resolves
//! to its body; definition cycles are errors.

use adl_sema::{AngKind, Collection, ExtDecls, HKind, HirRegionStmt, Quantity, analyze_str};
use adl_syntax::diag::Severity;

fn analyze(src: &str) -> adl_sema::Hir {
    analyze_str(src, "test.adl", &ExtDecls::legacy())
}

fn select_nodes(hir: &adl_sema::Hir, region: &str) -> Vec<adl_sema::HNode> {
    hir.region(region)
        .expect("region exists")
        .stmts
        .iter()
        .filter_map(|s| match s {
            HirRegionStmt::Select(n) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn pure_rename_is_same_collection_id_transitively() {
    let hir = analyze(
        "object eles\n  take Ele\n\
         object electrons2\n  take eles\n\
         object electrons3\n  take electrons2\n\
         object MHT\n  take MissingET\n\
         object MET2\n  take MHT\n",
    );
    // eles is itself a pure rename of the ELECTRON base.
    let eles = hir.collection_of("eles").unwrap();
    let e2 = hir.collection_of("electrons2").unwrap();
    let e3 = hir.collection_of("electrons3").unwrap();
    assert_eq!(eles, e2);
    assert_eq!(e2, e3);
    assert!(matches!(hir.table.collection(eles), Collection::Base(_)));

    // MET-family spelling map: MissingET == MET base; renames chain through.
    let mht = hir.collection_of("MHT").unwrap();
    let met2 = hir.collection_of("MET2").unwrap();
    assert_eq!(mht, met2);
    let Collection::Base(sym) = hir.table.collection(mht) else {
        panic!("MET family must resolve to a base collection");
    };
    assert_eq!(hir.symbols.key(*sym), "met");

    // All rename objects report the alias fact.
    for name in ["electrons2", "electrons3", "MET2"] {
        let sym = hir.symbols.lookup(name).unwrap();
        let obj = hir.objects.iter().find(|o| o.name == sym).unwrap();
        assert!(obj.pure_alias_of.is_some(), "{name} should be a pure alias");
    }
}

#[test]
fn filtered_collection_is_distinct_from_parent() {
    let hir = analyze(
        "object jets\n  take Jet\n  select pT > 30\n\
         object alljets\n  take Jet\n",
    );
    let jets = hir.collection_of("jets").unwrap();
    let alljets = hir.collection_of("alljets").unwrap();
    assert_ne!(jets, alljets, "filtered must never unify with its parent");
    let Collection::Filtered { parent, .. } = hir.table.collection(jets) else {
        panic!("jets must be Filtered");
    };
    assert_eq!(*parent, alljets);
}

#[test]
fn indexed_element_properties_do_not_alias() {
    let hir = analyze(
        "object jets\n  take Jet\n\
         region SR\n  select jets[0].pT > 100\n  select jets[1].pT > 50\n",
    );
    let elems: Vec<&Quantity> = hir
        .table
        .quantities()
        .iter()
        .filter(|q| matches!(q, Quantity::ElemProp { .. }))
        .collect();
    assert_eq!(elems.len(), 2, "jets[0].pt and jets[1].pt are distinct");
    // Same collection and property, different index.
    let (
        Quantity::ElemProp {
            coll: c0,
            index: i0,
            prop: p0,
        },
        Quantity::ElemProp {
            coll: c1,
            index: i1,
            prop: p1,
        },
    ) = (elems[0], elems[1])
    else {
        unreachable!()
    };
    assert_eq!(c0, c1);
    assert_eq!(p0, p1);
    assert_ne!(i0, i1);
}

#[test]
fn property_spellings_unify_but_case_preserved_elsewhere() {
    let hir = analyze(
        "object jets\n  take Jet\n\
         region SR\n  select jets[0].pT > 100\n  select pt(jets[0]) > 100\n  select {jets[0]}Pt > 100\n",
    );
    let elems: Vec<&Quantity> = hir
        .table
        .quantities()
        .iter()
        .filter(|q| matches!(q, Quantity::ElemProp { .. }))
        .collect();
    assert_eq!(
        elems.len(),
        1,
        "pT/pt/Pt of the same element are ONE quantity"
    );
}

#[test]
fn oriented_angular_pairs_stay_distinct_unoriented_merge() {
    let hir = analyze(
        "object eles\n  take Ele\n\
         object muons\n  take Muo\n\
         region SR\n\
           select dPhi(eles[0], muons[0]) > 1\n\
           select dPhi(muons[0], eles[0]) > 1\n\
           select dR(eles[0], muons[0]) > 0.4\n\
           select dR(muons[0], eles[0]) > 0.4\n",
    );
    let dphis: Vec<&Quantity> = hir
        .table
        .quantities()
        .iter()
        .filter(|q| {
            matches!(
                q,
                Quantity::AngularSep {
                    kind: AngKind::DPhi,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(dphis.len(), 2, "dPhi is oriented: argument order matters");
    for q in &dphis {
        let Quantity::AngularSep { oriented, .. } = q else {
            unreachable!()
        };
        assert!(*oriented);
    }

    let drs: Vec<&Quantity> = hir
        .table
        .quantities()
        .iter()
        .filter(|q| {
            matches!(
                q,
                Quantity::AngularSep {
                    kind: AngKind::DR,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(
        drs.len(),
        1,
        "dR is unoriented: both orders are ONE quantity"
    );
    let Quantity::AngularSep { oriented, .. } = drs[0] else {
        unreachable!()
    };
    assert!(!*oriented);
}

#[test]
fn define_reference_resolves_to_body_hir() {
    let hir = analyze(
        "define halfmet = MET / 2\n\
         region SR\n  select halfmet < 10\n",
    );
    let def = hir.define("halfmet").unwrap();
    assert_eq!(def.kind, adl_sema::DefineKind::Numeric);
    let selects = select_nodes(&hir, "SR");
    let HKind::Cmp { lhs, .. } = &selects[0].kind else {
        panic!("expected comparison");
    };
    assert_eq!(
        lhs.as_ref(),
        &def.body,
        "the reference site inlines the define's body HIR"
    );
}

#[test]
fn boolean_define_resolves_to_predicate() {
    let hir = analyze(
        "define lowmet = MET < 100\n\
         region SR\n  select lowmet\n",
    );
    let def = hir.define("lowmet").unwrap();
    assert_eq!(def.kind, adl_sema::DefineKind::Boolean);
    let selects = select_nodes(&hir, "SR");
    assert_eq!(&selects[0], &def.body);
}

#[test]
fn define_cycle_is_an_error() {
    let hir = analyze(
        "define a = b + 1\n\
         define b = a + 1\n\
         region SR\n  select a > 0\n",
    );
    assert!(
        hir.diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("cycle")),
        "definition cycle must be a resolution error: {:?}",
        hir.diags
    );
}

#[test]
fn object_take_cycle_is_an_error() {
    let hir = analyze("object a\n  take b\nobject b\n  take a\n");
    assert!(
        hir.diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("cycle")),
        "object take cycle must be a resolution error: {:?}",
        hir.diags
    );
}

#[test]
fn bare_met_family_value_means_pt_magnitude() {
    let hir = analyze(
        "object MET\n  take MissingET\n\
         region SR\n  select MET > 250\n  select MET.pT > 250\n",
    );
    let selects = select_nodes(&hir, "SR");
    let lhs_kind = |n: &adl_sema::HNode| -> HKind {
        let HKind::Cmp { lhs, .. } = &n.kind else {
            panic!("expected comparison")
        };
        lhs.kind.clone()
    };
    assert_eq!(
        lhs_kind(&selects[0]),
        lhs_kind(&selects[1]),
        "bare MET and MET.pt are the same quantity"
    );
}

#[test]
fn size_aliases_are_one_quantity() {
    let hir = analyze(
        "object jets\n  take Jet\n\
         region SR\n  select Size(jets) > 2\n  select size(jets) > 2\n  select count(jets) > 2\n  select jets.size > 2\n",
    );
    let sizes: Vec<&Quantity> = hir
        .table
        .quantities()
        .iter()
        .filter(|q| matches!(q, Quantity::Size(_)))
        .collect();
    assert_eq!(sizes.len(), 1, "Size/size/count/.size are ONE quantity");
}

#[test]
fn union_order_is_part_of_identity() {
    let hir = analyze(
        "object eles\n  take Ele\n\
         object muons\n  take Muo\n\
         object lep1\n  take union(eles, muons)\n\
         object lep2\n  take union(muons, eles)\n\
         object lep3\n  take eles\n  take muons\n",
    );
    let l1 = hir.collection_of("lep1").unwrap();
    let l2 = hir.collection_of("lep2").unwrap();
    let l3 = hir.collection_of("lep3").unwrap();
    assert_ne!(l1, l2, "union order affects element indexing");
    assert_eq!(l1, l3, "multi-take is the same union by construction");
}

#[test]
fn negative_index_is_diagnosed_unsupported() {
    let hir = analyze(
        "object jets\n  take Jet\n\
         region SR\n  select jets[-1].pT > 30\n",
    );
    let selects = select_nodes(&hir, "SR");
    assert!(
        selects[0].has_unsupported(),
        "[-n] must surface as Unsupported (OPEN-3)"
    );
}

#[test]
fn unknown_function_is_interned_but_unsupported() {
    let hir = analyze("object muons\n  take Muon\n  select D0 < 2\n  select D0(Muon) < 2\n");
    let jets = hir.collection_of("muons").unwrap();
    let Collection::Filtered { pred, .. } = hir.table.collection(jets) else {
        panic!("muons must be Filtered");
    };
    let pred = hir.elem_pred(*pred);
    assert!(
        pred.node.has_unsupported(),
        "unknown property/function is out of fragment"
    );
}

#[test]
fn region_used_as_predicate_inlines_prior_region() {
    let hir = analyze(
        "region presel\n  select MET > 100\n\
         region SR1\n  select presel\n  select MET > 200\n\
         region SR2\n  presel\n  select MET > 300\n",
    );
    let s1 = select_nodes(&hir, "SR1");
    assert!(matches!(s1[0].kind, HKind::RegionPred(0)));
    let sr2 = hir.region("SR2").unwrap();
    assert!(matches!(
        sr2.stmts[0],
        HirRegionStmt::Inherit { region: 0, .. }
    ));
}

// ---- P2 sort -> alias (soundness-critical) -----------------------------

#[test]
fn descending_pt_sort_of_pt_ordered_source_aliases_to_source() {
    // A stable descending-pT sort of a pT-descending source (base or
    // filtered) is the identity permutation: it canonicalizes to the SOURCE
    // collection id, so it inherits ORD/IDOM/EPRED and cross-region identity.
    let hir = analyze(
        "object jets\n  take Jet\n  select pT > 30\n\
         object sjets\n  take sort(jets, pt(jets), descend)\n",
    );
    let jets = hir.collection_of("jets").unwrap();
    let sjets = hir.collection_of("sjets").unwrap();
    assert_eq!(sjets, jets, "descending-pt sort of an ordered source is an alias");
    // No opaque Sorted collection was interned for this shape.
    assert!(
        !hir.table
            .collections()
            .iter()
            .any(|c| matches!(c, Collection::Sorted { .. })),
        "the aliased sort must not leave a distinct Sorted collection"
    );
}

#[test]
fn ascending_sort_does_not_alias_and_stays_opaque_sorted() {
    let hir = analyze(
        "object jets\n  take Jet\n\
         object sjets\n  take sort(jets, pt(jets), ascend)\n",
    );
    let jets = hir.collection_of("jets").unwrap();
    let sjets = hir.collection_of("sjets").unwrap();
    assert_ne!(sjets, jets, "ascending sort is NOT the identity permutation");
    assert!(
        matches!(hir.table.collection(sjets), Collection::Sorted { .. }),
        "ascending sort stays an opaque Sorted"
    );
    // pt_ordered must be false for it (no ORD/IDOM index facts).
    let pt = ExtDecls::legacy().prop_canon("pt").0;
    assert!(!hir.table.pt_ordered(sjets, &pt));
}

#[test]
fn sort_over_union_does_not_alias() {
    // The source is a union (not pT-descending), so even a descending-pT sort
    // must NOT alias — the gravest false-PROVEN trap.
    let hir = analyze(
        "object eles\n  take Ele\n\
         object muons\n  take Muo\n\
         object leptons\n  take union(eles, muons)\n\
         object sleptons\n  take sort(leptons, pt(leptons), descend)\n",
    );
    let leptons = hir.collection_of("leptons").unwrap();
    let sleptons = hir.collection_of("sleptons").unwrap();
    assert_ne!(sleptons, leptons, "sort over a union must not alias");
    assert!(matches!(
        hir.table.collection(sleptons),
        Collection::Sorted { .. }
    ));
    let pt = ExtDecls::legacy().prop_canon("pt").0;
    assert!(!hir.table.pt_ordered(sleptons, &pt));
}

#[test]
fn sort_by_non_pt_key_does_not_alias() {
    let hir = analyze(
        "object jets\n  take Jet\n\
         object sjets\n  take sort(jets, eta(jets), descend)\n",
    );
    let jets = hir.collection_of("jets").unwrap();
    let sjets = hir.collection_of("sjets").unwrap();
    assert_ne!(sjets, jets, "a non-pt sort key must not alias");
    assert!(matches!(
        hir.table.collection(sjets),
        Collection::Sorted { .. }
    ));
}
