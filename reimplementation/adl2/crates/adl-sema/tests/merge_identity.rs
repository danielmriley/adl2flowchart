//! Identity-invariant battery for the cross-file merge (`merge_hirs`).
//!
//! `merge_hirs` re-interns several resolved units into ONE structural
//! identity space. Every cross-file `PROVEN` verdict rests on a single
//! property: **two quantities unify iff they are structurally identical**.
//! Over-merging is the dangerous direction — it silently turns two physically
//! different values into one solver variable, which is how a false
//! `PROVEN DISJOINT` gets manufactured (`size(o1) >= 3` and `size(o2) <= 1`
//! become a contradiction on one quantity).
//!
//! # Why this file is solver-free — and why that is the point
//!
//! Nothing here calls z3, and every assertion is meaningful with z3 absent
//! from `PATH`. Identity is decided entirely in `adl-sema`, *before* any
//! formula is built: the solver only ever sees the ids the merger handed it,
//! so a solver-level test can never distinguish "these two really are the
//! same quantity" from "the merger collapsed them". Pinning the discipline
//! structurally means these tests keep their teeth in a z3-less environment,
//! run in milliseconds, and fail with a message that names the aliasing pair
//! instead of a flipped verdict three layers downstream.
//!
//! # The invariants (`check_merge_invariants`, applied to every merge here)
//!
//! * **I1** — no `ElemPredId` whose node `has_unsupported()` is referenced by
//!   two or more distinct collections. This is exactly the bug fixed in
//!   `ElemPredInterner`: several `Unsupported` reasons DISCARD the differing
//!   sub-expression, so two physically different cuts render identically;
//!   sharing the id fused their `Filtered` collections and their sizes.
//! * **I1b** (needs the inputs, so it lives in [`merged`]) — the one with
//!   teeth: a collection whose membership depends on a lossy cut is reachable
//!   from the regions of at most ONE source unit. I1 alone cannot catch the
//!   original bug, because over-merging *removes* a collection instead of
//!   making two share a pred — verified by mutating `ElemPredInterner::intern`
//!   and watching I1 stay green while I1b fired on every adversarial case and
//!   on both property sweeps.
//! * **I2** — render/identity agreement for supported predicates: renders are
//!   pairwise distinct among supported preds (so an identical supported cut
//!   from two files is shared *exactly once*), and conversely two distinct
//!   ids may share a render only if at least one of them is unsupported.
//! * **I3** — every `QuantityArg::Opaque` in a merged table carries the
//!   `<unit-ordinal>\u{1}` prefix `Merger::remap_arg` stamps on it, so equal
//!   opaque strings necessarily come from the same source unit.
//! * **I4** — the shared tables stay injective: no two `QuantityId`s are
//!   `Size(c)` for the same `c`, and no `Quantity`/`Collection` value appears
//!   twice.
//! * **I5** (needs the inputs, so it lives in [`merged`]) — merged region
//!   count equals the sum of the inputs' and the merged labels are unique.
//!
//! # Two documented gaps found while writing this (reported, NOT fixed here)
//!
//! * `SortKey::Opaque` is passed through the merge verbatim (unlike
//!   `QuantityArg::Opaque`), and the render it carries embeds source-unit-LOCAL
//!   collection ids. Two units whose *different* sort keys render identically
//!   therefore fuse into one `Sorted` collection — and, contrary to the
//!   comment on `Merger::remap_sort_key`, the fragment gate does **not** block
//!   it for a take-level `take sort(...)`. See
//!   `take_level_opaque_sort_key_collides_across_units_known_gap`.
//! * The `<unit>#n` de-duplication of colliding unit labels can itself be
//!   shadowed by a file literally named `a#2`. See
//!   `unit_label_dedup_is_shadowed_by_a_filename_that_mimics_the_suffix`.

use adl_sema::{
    Collection, CollectionId, ElemPredId, ExtDecls, HKind, HNode, Hir, HirRegionStmt, ParticleRef,
    Quantity, QuantityArg, QuantityId, SortKey, Symbol, analyze_str, merge_hirs, render_node,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

// ---------------------------------------------------------------- harness

fn ext() -> &'static ExtDecls {
    static EXT: OnceLock<ExtDecls> = OnceLock::new();
    EXT.get_or_init(ExtDecls::legacy)
}

fn hir(src: &str, unit: &str) -> Hir {
    let h = analyze_str(src, unit, ext());
    assert!(
        !adl_syntax::diag::has_errors(&h.diags),
        "unit {unit} must resolve without errors (merge_hirs requires it): {:#?}",
        h.diags
    );
    h
}

/// Merge, then assert the full invariant set (I1-I4 structural, plus the two
/// checks that need the inputs: I1b cross-unit isolation and I5 region
/// accounting). Every test in this file goes through here.
fn merged(units: &[&Hir]) -> Hir {
    let m = merge_hirs(units);
    check_merge_invariants(&m);
    // Accounting first: I1b slices the merged regions by the inputs' counts,
    // and a mismatch there should report itself, not surface as a slice panic.
    check_region_accounting(units, &m);
    i1b_lossy_collections_are_never_referenced_by_two_units(units, &m);
    m
}

/// I1-I4: everything checkable from the merged unit alone.
fn check_merge_invariants(m: &Hir) {
    i1_unsupported_preds_are_never_shared(m);
    i2_supported_preds_share_exactly_on_render(m);
    i3_opaque_args_are_unit_namespaced(m);
    i4_shared_tables_stay_injective(m);
}

/// I1. An element predicate that contains an `Unsupported` node has a lossy
/// render (the reason string throws the differing sub-expression away), so it
/// must never be the shared identity of more than one collection.
fn i1_unsupported_preds_are_never_shared(m: &Hir) {
    let mut users: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (i, c) in m.table.collections().iter().enumerate() {
        match c {
            Collection::Filtered { pred, .. } => users.entry(pred.0).or_default().push(i),
            Collection::Combination { cuts, .. } => {
                for p in cuts {
                    let slot = users.entry(p.0).or_default();
                    if !slot.contains(&i) {
                        slot.push(i);
                    }
                }
            }
            _ => {}
        }
    }
    for (&pid, colls) in &users {
        let pred = &m.elem_preds[pid as usize];
        if !pred.node.has_unsupported() {
            continue;
        }
        assert_eq!(
            colls.len(),
            1,
            "I1 violated: unsupported pred P{pid} ({:?}) is the identity of {} collections {:?} \
             — a lossy render became shared identity (false-PROVEN factory)",
            pred.render,
            colls.len(),
            colls
                .iter()
                .map(|&i| format!("C{i}={:?}", m.table.collections()[i]))
                .collect::<Vec<_>>()
        );
    }
}

/// I1b, the invariant with teeth. I1 alone cannot see the bug it names:
/// over-merging two units' lossy cuts *removes* a collection rather than
/// making two collections share a pred, so the merged unit still satisfies I1
/// (verified by mutating `ElemPredInterner::intern` — I1 stayed green).
///
/// The observable consequence of the fail-closed rule is stronger and is what
/// this checks: **a collection whose membership depends on a lossy (`Unsupported`)
/// cut must be reachable from the regions of at most ONE source unit.** Each
/// unit's remap mints its own fresh pred and therefore its own collection, so
/// two units can never legitimately land on the same one. When they do, they
/// are sharing a solver variable (`size`, `[i].prop`) whose two meanings the
/// render threw away — the exact false-`PROVEN` mechanism.
///
/// Unit attribution comes from the input region counts (exact index ranges),
/// not from the `<unit>::` label prefix, so it stays correct even when two
/// files' labels collide.
fn i1b_lossy_collections_are_never_referenced_by_two_units(units: &[&Hir], m: &Hir) {
    let mut lossy: HashSet<CollectionId> = HashSet::new();
    for (i, c) in m.table.collections().iter().enumerate() {
        let preds: Vec<ElemPredId> = match c {
            Collection::Filtered { pred, .. } => vec![*pred],
            Collection::Combination { cuts, .. } => cuts.clone(),
            _ => continue,
        };
        if preds
            .iter()
            .any(|p| m.elem_preds[p.0 as usize].node.has_unsupported())
        {
            lossy.insert(CollectionId(u32::try_from(i).unwrap()));
        }
    }
    if lossy.is_empty() {
        return;
    }

    let mut owner: HashMap<CollectionId, usize> = HashMap::new();
    let mut base = 0usize;
    for (uidx, u) in units.iter().enumerate() {
        let mut refs: HashSet<CollectionId> = HashSet::new();
        for r in &m.regions[base..base + u.regions.len()] {
            for s in &r.stmts {
                match s {
                    HirRegionStmt::Select(n)
                    | HirRegionStmt::Reject(n)
                    | HirRegionStmt::Trigger(n)
                    | HirRegionStmt::Bin { var: n, .. }
                    | HirRegionStmt::BinCond { cond: n, .. } => node_colls(m, n, &mut refs),
                    _ => {}
                }
            }
        }
        expand_colls(m, &mut refs);
        for c in refs.intersection(&lossy) {
            if let Some(&prev) = owner.get(c)
                && prev != uidx
            {
                panic!(
                    "I1b violated: {c} ({:?}) depends on a lossy cut yet is referenced by \
                     regions of BOTH unit {prev} ({}) and unit {uidx} ({}) — two files now \
                     share one solver variable for two different cuts",
                    m.table.collection(*c),
                    units[prev].unit,
                    units[uidx].unit
                );
            }
            owner.insert(*c, uidx);
        }
        base += u.regions.len();
    }
}

/// Collections mentioned directly by a resolved statement tree.
fn node_colls(m: &Hir, n: &HNode, out: &mut HashSet<CollectionId>) {
    match &n.kind {
        HKind::Quantity(q) => quant_colls(m, *q, out, &mut HashSet::new()),
        HKind::Reduce { coll, .. }
        | HKind::CollProp { coll, .. }
        | HKind::CollValue(coll) => {
            out.insert(*coll);
        }
        HKind::Particle(p) => particle_colls(p, out),
        _ => {}
    }
    for c in n.children() {
        node_colls(m, c, out);
    }
}

fn quant_colls(
    m: &Hir,
    q: QuantityId,
    out: &mut HashSet<CollectionId>,
    seen: &mut HashSet<QuantityId>,
) {
    if !seen.insert(q) {
        return;
    }
    match m.table.quantity(q) {
        Quantity::EventScalar(_) => {}
        Quantity::Size(c) => {
            out.insert(*c);
        }
        Quantity::ElemProp { coll, .. } => {
            out.insert(*coll);
        }
        Quantity::AngularSep { a, b, .. } => {
            particle_colls(a, out);
            particle_colls(b, out);
        }
        Quantity::ExternalFn { args, .. } => {
            for a in args {
                match a {
                    QuantityArg::Quantity(q2) => quant_colls(m, *q2, out, seen),
                    QuantityArg::Particle(p) => particle_colls(p, out),
                    QuantityArg::Collection(c) | QuantityArg::CollProp { coll: c, .. } => {
                        out.insert(*c);
                    }
                    QuantityArg::Num(_) | QuantityArg::Opaque(_) => {}
                }
            }
        }
    }
}

fn particle_colls(p: &ParticleRef, out: &mut HashSet<CollectionId>) {
    match p {
        ParticleRef::Elem { coll, .. }
        | ParticleRef::Whole(coll)
        | ParticleRef::Binder { coll, .. } => {
            out.insert(*coll);
        }
        ParticleRef::Sum(parts) => {
            for q in parts {
                particle_colls(q, out);
            }
        }
        ParticleRef::Met
        | ParticleRef::ThisElem
        | ParticleRef::ReduceElem => {}
    }
}

/// Close a reference set over the collection DAG (a region that mentions
/// `sort(o)` also depends on `o`).
fn expand_colls(m: &Hir, out: &mut HashSet<CollectionId>) {
    let mut stack: Vec<CollectionId> = out.iter().copied().collect();
    while let Some(c) = stack.pop() {
        let mut next: Vec<CollectionId> = Vec::new();
        match m.table.collection(c) {
            Collection::Base(_) => {}
            Collection::Filtered { parent, .. } => next.push(*parent),
            Collection::Union(parts) => next.extend(parts.iter().copied()),
            Collection::Sorted { source, .. } | Collection::Slice { source, .. } => {
                next.push(*source);
            }
            Collection::Combination { parts, members, .. } => {
                next.extend(parts.iter().copied());
                next.extend(members.iter().map(|b| b.source));
            }
            Collection::CombProject { comb, .. } => next.push(*comb),
        }
        for n in next {
            if out.insert(n) {
                stack.push(n);
            }
        }
    }
}

/// I2. Both directions of the render-as-identity rule for SUPPORTED preds:
///
/// * forward — two distinct ids never share a render unless at least one is
///   unsupported (the only sanctioned source of render collisions);
/// * converse — among supported preds the renders are pairwise distinct, i.e.
///   an identical supported cut arriving from two files is interned once, not
///   duplicated. (Duplication would not be *unsound*, but it would silently
///   destroy the cross-file sharing the whole merge exists to provide.)
fn i2_supported_preds_share_exactly_on_render(m: &Hir) {
    let n = m.elem_preds.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let (a, b) = (&m.elem_preds[i], &m.elem_preds[j]);
            if a.render != b.render {
                continue;
            }
            assert!(
                a.node.has_unsupported() || b.node.has_unsupported(),
                "I2 violated: distinct preds P{i}/P{j} share render {:?} but both are \
                 fully supported — supported preds must be interned on the render key",
                a.render
            );
        }
    }
    let mut by_render: HashMap<&str, usize> = HashMap::new();
    for (i, p) in m.elem_preds.iter().enumerate() {
        if p.node.has_unsupported() {
            continue;
        }
        if let Some(prev) = by_render.insert(p.render.as_str(), i) {
            panic!(
                "I2 violated: supported preds P{prev} and P{i} both render {:?} — \
                 an identical cut from two units failed to unify",
                p.render
            );
        }
    }
}

/// I3. `Merger::remap_arg` namespaces every opaque external argument by the
/// source unit's ordinal (`"{ord}\u{1}{render}"`) because the render embeds
/// unit-LOCAL collection ids. Assert the prefix discipline directly: with it,
/// two equal opaque strings provably come from the same unit.
fn i3_opaque_args_are_unit_namespaced(m: &Hir) {
    for (qi, q) in m.table.quantities().iter().enumerate() {
        let Quantity::ExternalFn { args, .. } = q else {
            continue;
        };
        for (ai, a) in args.iter().enumerate() {
            let QuantityArg::Opaque(s) = a else {
                continue;
            };
            let Some((ord, rest)) = s.split_once('\u{1}') else {
                panic!(
                    "I3 violated: Q{qi} arg {ai} is Opaque({s:?}) with no unit-ordinal \
                     prefix — two units' local ids can now collide"
                );
            };
            assert!(
                !ord.is_empty() && ord.bytes().all(|b| b.is_ascii_digit()),
                "I3 violated: Q{qi} arg {ai} has a non-ordinal namespace {ord:?} in {s:?}"
            );
            assert!(
                !rest.contains('\u{1}'),
                "I3 violated: Q{qi} arg {ai} was namespaced twice: {s:?}"
            );
        }
    }
}

/// I4. The shared tables are interners, so they must stay injective after the
/// remap. `Size(c)` gets its own named check because it is the quantity a
/// collection-identity slip turns into a false verdict.
fn i4_shared_tables_stay_injective(m: &Hir) {
    let mut size_of: HashMap<CollectionId, QuantityId> = HashMap::new();
    for (i, q) in m.table.quantities().iter().enumerate() {
        if let Quantity::Size(c) = q {
            let id = QuantityId(u32::try_from(i).unwrap());
            if let Some(prev) = size_of.insert(*c, id) {
                panic!("I4 violated: {prev} and {id} are both Size({c})");
            }
        }
    }
    let mut seen_q: HashMap<&Quantity, usize> = HashMap::new();
    for (i, q) in m.table.quantities().iter().enumerate() {
        if let Some(prev) = seen_q.insert(q, i) {
            panic!("I4 violated: Q{prev} and Q{i} are the same quantity value {q:?}");
        }
    }
    let mut seen_c: HashMap<&Collection, usize> = HashMap::new();
    for (i, c) in m.table.collections().iter().enumerate() {
        if let Some(prev) = seen_c.insert(c, i) {
            panic!("I4 violated: C{prev} and C{i} are the same collection value {c:?}");
        }
    }
}

/// I5. Regions are relabelled `<unit>::<region>` and rebased, never dropped or
/// fused: the count is the sum of the inputs' and the labels are unique
/// (uniqueness is what keeps the cross-file/intra-file pair classification and
/// the report rows distinguishable).
fn check_region_accounting(units: &[&Hir], m: &Hir) {
    let want: usize = units.iter().map(|u| u.regions.len()).sum();
    assert_eq!(m.regions.len(), want, "I5: merged region count");
    assert_eq!(
        m.region_name_order.len(),
        want,
        "I5: region_name_order must stay index-aligned with regions"
    );
    let mut seen: HashSet<Symbol> = HashSet::new();
    for (i, r) in m.regions.iter().enumerate() {
        let label = m.symbols.display(r.name);
        assert!(
            label.contains("::"),
            "I5: merged region {i} is not unit-qualified: {label:?}"
        );
        assert_eq!(r.name, m.region_name_order[i], "I5: name/order mismatch");
        assert!(
            seen.insert(r.name),
            "I5 violated: two merged regions share the label {label:?} — cross/intra \
             pair classification and the report rows become ambiguous"
        );
    }
}

// ------------------------------------------------------------- accessors

fn filtered_preds(m: &Hir) -> Vec<ElemPredId> {
    m.table
        .collections()
        .iter()
        .filter_map(|c| match c {
            Collection::Filtered { pred, .. } => Some(*pred),
            _ => None,
        })
        .collect()
}

fn n_filtered(m: &Hir) -> usize {
    filtered_preds(m).len()
}

fn n_size_quantities(m: &Hir) -> usize {
    m.table
        .quantities()
        .iter()
        .filter(|q| matches!(q, Quantity::Size(_)))
        .count()
}

/// One object block with `cut` over `Jet`, plus a region that measures it.
/// The object and region names are deliberately IDENTICAL across units: names
/// are labels, never identity, so a name match must not create sharing and a
/// name mismatch must not destroy it.
fn cut_unit(cut: &str, region_cmp: &str) -> String {
    format!("object o\n  take Jet\n  {cut}\nregion R\n  select size(o) {region_cmp}\n")
}

// -------------------------------------------------- adversarial: cut renders

/// Two units, the SAME unresolved identifier in the cut.
///
/// What the tool does: `passIso` resolves to an `Unsupported` node carrying
/// the reason "unresolved identifier passIso", so the cut `has_unsupported()`
/// and the interner mints a FRESH id per unit — the two `Filtered` collections
/// stay distinct even though their renders are byte-identical.
///
/// Whether that is "right" is arguable: the same unknown name applied to the
/// same base plausibly denotes the same cut, and refusing to share costs a
/// cross-file `PROVEN`. But the two directions are not symmetric. Sharing a
/// lossy render is unrecoverable — it fuses `size(o)` into one variable and
/// can fabricate a proof. Refusing to share only weakens a verdict to
/// `POSSIBLY`. The tool takes the fail-closed side, and this test pins it so
/// nobody "optimizes" the sharing back in without re-deriving the render's
/// injectivity first.
#[test]
fn same_unresolved_identifier_in_both_units_is_fail_closed() {
    let a = hir(&cut_unit("select passIso > 0.5", ">= 3"), "a");
    let b = hir(&cut_unit("select passIso > 0.5", "<= 1"), "b");
    assert_eq!(
        a.elem_preds[0].render, b.elem_preds[0].render,
        "test premise: the two cuts must render identically"
    );
    assert!(a.elem_preds[0].node.has_unsupported());

    let m = merged(&[&a, &b]);
    assert_eq!(n_filtered(&m), 2, "fail-closed: no sharing on a lossy render");
    let preds = filtered_preds(&m);
    assert_ne!(preds[0], preds[1]);
    assert_eq!(n_size_quantities(&m), 2, "and the two sizes stay separate");
}

/// Two units, DIFFERENT unresolved identifiers. The renders differ here, so
/// even a render-keyed interner would keep them apart — this is the control
/// that proves the previous test is not passing for a trivial reason.
#[test]
fn different_unresolved_identifiers_never_share() {
    let a = hir(&cut_unit("select passIso > 0.5", ">= 3"), "a");
    let b = hir(&cut_unit("select mediumId > 0.5", "<= 1"), "b");
    assert_ne!(a.elem_preds[0].render, b.elem_preds[0].render);

    let m = merged(&[&a, &b]);
    assert_eq!(n_filtered(&m), 2);
    assert_ne!(filtered_preds(&m)[0], filtered_preds(&m)[1]);
}

/// The original bug, re-asserted through the generic invariant checker rather
/// than a hand-rolled count (`merge.rs::merge_never_unifies_unsupported_cuts`
/// pins the count; this pins the *invariant*).
///
/// The `sum` reason keeps only the reducer kind and a plural-reference COUNT,
/// so `sum(pt(Muon) + pt(Electron))` and `sum(eta(Photon) * eta(Tau))` render
/// identically while denoting completely different cuts.
#[test]
fn reducer_reason_collision_stays_distinct() {
    let a = hir(&cut_unit("select sum(pt(Muon) + pt(Electron)) > 5", ">= 3"), "a");
    let b = hir(&cut_unit("select sum(eta(Photon) * eta(Tau)) > 5", "<= 1"), "b");
    assert_eq!(
        a.elem_preds[0].render, b.elem_preds[0].render,
        "test premise: the reducer reason must discard the differing body"
    );

    let m = merged(&[&a, &b]);
    assert_eq!(n_filtered(&m), 2);
    assert_eq!(n_size_quantities(&m), 2);
}

/// An opaque external over the implicit element, IDENTICAL name and args in
/// both units: `bdt(this) > 0.5`.
///
/// What the tool does: `this` is element-context-tainted, so the call never
/// interns an `ExternalFn` identity at all — it becomes
/// `Unsupported("call `bdt` over an element-context argument")`. The renders
/// collide, the interner mints fresh ids, and the collections stay distinct.
/// Same fail-closed posture as the unresolved-identifier case, reached by a
/// different route (`Resolver::context_tainted`, soundness review S2).
#[test]
fn identical_opaque_element_external_cut_is_fail_closed() {
    let a = hir(&cut_unit("select bdt(this) > 0.5", ">= 3"), "a");
    let b = hir(&cut_unit("select bdt(this) > 0.5", "<= 1"), "b");
    assert_eq!(a.elem_preds[0].render, b.elem_preds[0].render);
    assert!(a.elem_preds[0].node.has_unsupported());
    assert!(
        !a.table
            .quantities()
            .iter()
            .any(|q| matches!(q, Quantity::ExternalFn { .. })),
        "test premise: the tainted call must not intern an ExternalFn identity"
    );

    let m = merged(&[&a, &b]);
    assert_eq!(n_filtered(&m), 2);
    assert_eq!(n_size_quantities(&m), 2);
}

/// Case folding. Symbol interning is case-insensitive, so `PassIso` and
/// `passiso` are the same *name* — but the unsupported REASON string embeds
/// the source spelling, so the two renders differ. Either way the preds are
/// unsupported and stay unshared; the point of the test is that neither the
/// case-insensitive symbol path nor the case-sensitive render path can leak
/// one unit's identity into the other.
#[test]
fn case_folded_unresolved_identifiers_stay_distinct() {
    let a = hir(&cut_unit("select PassIso > 0.5", ">= 3"), "a");
    let b = hir(&cut_unit("select passiso > 0.5", "<= 1"), "b");

    let m = merged(&[&a, &b]);
    assert_eq!(n_filtered(&m), 2);
    assert_ne!(filtered_preds(&m)[0], filtered_preds(&m)[1]);
}

/// Case folding on a SUPPORTED cut, where sharing is the correct answer:
/// `select pT > 30` and `select pt > 30` are the same cut and must unify.
#[test]
fn case_folded_supported_cuts_do_unify() {
    let a = hir(&cut_unit("select pT > 30", ">= 3"), "a");
    let b = hir(&cut_unit("select pt > 30", "<= 1"), "b");

    let m = merged(&[&a, &b]);
    assert_eq!(
        n_filtered(&m),
        1,
        "property spelling is case-insensitive; the cut is the same"
    );
    assert_eq!(n_size_quantities(&m), 1);
}

/// Three units, the same colliding trio. Two-unit merges can hide an ordering
/// bug in the memo tables (`Memo` is per-unit, the interner is global); a
/// three-way merge exercises the third unit against an already-populated
/// shared space.
#[test]
fn three_unit_merge_of_the_colliding_trio() {
    let a = hir(&cut_unit("select sum(pt(Muon) + pt(Electron)) > 5", ">= 3"), "a");
    let b = hir(&cut_unit("select sum(eta(Photon) * eta(Tau)) > 5", "<= 1"), "b");
    let c = hir(&cut_unit("select sum(phi(Muon) - phi(Tau)) > 5", "== 2"), "c");
    for h in [&a, &b, &c] {
        assert_eq!(h.elem_preds[0].render, a.elem_preds[0].render);
    }

    let m = merged(&[&a, &b, &c]);
    assert_eq!(n_filtered(&m), 3, "three lossy cuts, three collections");
    assert_eq!(n_size_quantities(&m), 3);
    let preds = filtered_preds(&m);
    assert_eq!(
        preds.iter().collect::<HashSet<_>>().len(),
        3,
        "all three pred ids must be distinct"
    );
}

/// The fail-closed rule must not cost legitimate sharing, three ways.
#[test]
fn three_unit_merge_of_identical_supported_cuts_unifies_once() {
    let a = hir(&cut_unit("select pt > 30", ">= 3"), "a");
    let b = hir(&cut_unit("select pt > 30", "<= 1"), "b");
    let c = hir(&cut_unit("select pt > 30", "== 2"), "c");

    let m = merged(&[&a, &b, &c]);
    assert_eq!(n_filtered(&m), 1, "one cut, one collection, three files");
    assert_eq!(n_size_quantities(&m), 1, "and ONE shared size quantity");
    assert_eq!(m.regions.len(), 3);
}

fn base_names(m: &Hir) -> Vec<&str> {
    m.table
        .collections()
        .iter()
        .filter_map(|c| match c {
            Collection::Base(s) => Some(m.symbols.display(*s)),
            _ => None,
        })
        .collect()
}

/// A block whose *input* could not be resolved at all (an unsupported take
/// call) falls back to `unresolved_base`, which mints a UNIT-UNIQUE symbol
/// `<unit>::<name>#unresolved`. Two files' byte-identical broken blocks
/// therefore cannot alias each other, nor a same-spelled detector base.
#[test]
fn unresolvable_take_input_mints_a_unit_unique_base() {
    let src = "object o\n  take fromRoot(Jet)\n  select pt > 30\n\
               region R\n  select size(o) >= 1\n";
    let a = hir(src, "a");
    let b = hir(src, "b");
    let m = merged(&[&a, &b]);
    let bases = base_names(&m);
    assert!(
        bases.contains(&"a::o#unresolved") && bases.contains(&"b::o#unresolved"),
        "each unit keeps its own private base: {bases:?}"
    );
    assert_eq!(n_filtered(&m), 2);
    assert_eq!(n_size_quantities(&m), 2);
}

/// The contrasting case, documented because the asymmetry is easy to misread:
/// an unknown *collection name* is NOT unit-scoped. `take NoSuchThing` mints a
/// private base named after the identifier ("treated as a private base
/// collection"), so two files using the same unknown name DO unify on it —
/// the same name-based judgement the tool already makes for declared bases
/// (`Jet` is `Jet` in every file). Only a block whose input is structurally
/// unresolvable gets the unit-unique treatment (previous test).
#[test]
fn unknown_collection_names_unify_across_units_by_name() {
    let mk = |cmp: &str| {
        format!("object o\n  take NoSuchThing\n  select pt > 30\nregion R\n  select size(o) {cmp}\n")
    };
    let a = hir(&mk(">= 3"), "a");
    let b = hir(&mk("<= 1"), "b");
    let m = merged(&[&a, &b]);
    assert_eq!(base_names(&m), vec!["nosuchthing"]);
    assert_eq!(n_filtered(&m), 1, "same name + same cut = one collection");
    assert_eq!(n_size_quantities(&m), 1);
}

// ------------------------------------------------- adversarial: opaque args

/// An opaque STRING argument, identical in both units: `bdt(jets, "loose")`.
///
/// What the tool does: `remap_arg` stamps the source unit's ordinal on every
/// `QuantityArg::Opaque`, so the two calls do NOT unify even though the
/// collection argument does. That is deliberate over-conservatism — the same
/// namespacing is what stops an opaque render carrying unit-LOCAL collection
/// ids from aliasing (see the next test) — and it costs a cross-file
/// `PROVEN` on genuinely shared external calls. Documented, not a bug.
#[test]
fn identical_opaque_string_args_are_namespaced_per_unit() {
    let src = "object jets\n  take Jet\n  select pt > 30\n\
               region R\n  select bdt(jets, \"loose\") > 0.5\n";
    let a = hir(src, "a");
    let b = hir(src, "b");

    let m = merged(&[&a, &b]);
    let opaques: Vec<&String> = m
        .table
        .quantities()
        .iter()
        .filter_map(|q| match q {
            Quantity::ExternalFn { args, .. } => args.iter().find_map(|a| match a {
                QuantityArg::Opaque(s) => Some(s),
                _ => None,
            }),
            _ => None,
        })
        .collect();
    assert_eq!(opaques.len(), 2, "one per unit: {opaques:?}");
    assert_eq!(opaques[0], "0\u{1}\"loose\"");
    assert_eq!(opaques[1], "1\u{1}\"loose\"");
    // The collection argument still unified — only the opaque half is split.
    assert_eq!(n_filtered(&m), 1);
}

/// The reason the namespacing exists: an opaque render embeds unit-LOCAL
/// collection ids (`C2#jets`). Both units bind a DIFFERENT `jets` at local id
/// `C2`, so the pre-merge renders are byte-identical while denoting different
/// collections. Without the ordinal prefix the two `ExternalFn` quantities
/// would intern to one shared id — a fabricated cross-file `PROVEN`.
#[test]
fn colliding_local_id_renders_in_opaque_args_are_kept_apart() {
    let mk = |cut: &str| {
        format!(
            "object pad\n  take Jet\n  select pt > 30\n\
             object jets\n  take Jet\n  {cut}\n\
             region R\n  select bdt(jets.pt + 1) > 0.5\n"
        )
    };
    let a = hir(&mk("select pt > 50"), "a");
    let b = hir(&mk("select pt > 90"), "b");
    let raw = |h: &Hir| -> String {
        h.table
            .quantities()
            .iter()
            .find_map(|q| match q {
                Quantity::ExternalFn { args, .. } => args.iter().find_map(|a| match a {
                    QuantityArg::Opaque(s) => Some(s.clone()),
                    _ => None,
                }),
                _ => None,
            })
            .expect("an opaque arg")
    };
    assert_eq!(
        raw(&a),
        raw(&b),
        "test premise: two DIFFERENT collections must render the same local id"
    );

    let m = merged(&[&a, &b]);
    let ext_fns = m
        .table
        .quantities()
        .iter()
        .filter(|q| matches!(q, Quantity::ExternalFn { .. }))
        .count();
    assert_eq!(
        ext_fns, 2,
        "colliding local-id renders must not intern to one quantity"
    );
}

// ------------------------------------------------- adversarial: sort keys

/// The documented half of the `SortKey::Opaque` argument, and it holds: a
/// REGION-level `sort` whose shape the recognizer does not cover never
/// produces a `SortKey` at all. It fails closed to the taint cascade, and
/// every following element-indexed statement is tagged `Unsupported`, so no
/// quantity — and no proof — can rest on the re-binding.
#[test]
fn region_level_opaque_sort_taints_following_indexed_statements() {
    let mk = |key: &str| {
        format!(
            "object jets\n  take Jet\n  select pt > 30\n\
             region R\n  sort {key} descend\n  select jets[0].pt > 100\n"
        )
    };
    let a = hir(&mk("bdt(jets)"), "a");
    let b = hir(&mk("mva(jets)"), "b");

    let m = merged(&[&a, &b]);
    assert!(
        !m.table
            .collections()
            .iter()
            .any(|c| matches!(c, Collection::Sorted { .. })),
        "an unrecognized region-level sort must not intern a Sorted collection"
    );
    let mut selects = 0;
    let mut sort_markers = 0;
    for r in &m.regions {
        for s in &r.stmts {
            match s {
                HirRegionStmt::Select(n) => {
                    selects += 1;
                    assert!(
                        n.has_unsupported(),
                        "indexed access after an opaque sort must leave the fragment: {:?}",
                        render_node(&m, n)
                    );
                }
                HirRegionStmt::NonMembership { kind: "sort", tag, .. } => {
                    sort_markers += 1;
                    assert!(!tag.is_in_fragment(), "the sort marker itself is dropped");
                }
                _ => {}
            }
        }
    }
    assert_eq!((selects, sort_markers), (2, 2));
}

/// KNOWN GAP — reported, deliberately NOT fixed here.
///
/// `Merger::remap_key` passes `SortKey::Opaque` through verbatim (unlike
/// `QuantityArg::Opaque`, which it namespaces by unit ordinal). The comment
/// added in commit 2589bcc argues this is safe because "an opaque sort shape
/// drops its region from the lowered fragment". That is true for a
/// REGION-level sort (previous test) but NOT for a take-level
/// `take sort(coll, key)`: the key is opaque, the `Sorted` collection is
/// interned, `size`/`elem` quantities over it stay in-fragment, and the key's
/// render embeds unit-LOCAL collection ids.
///
/// Here both units define `sj = sort(jets, dR(jets, refs[0]))` over the SAME
/// `jets`, but with a different `refs` (pt>50 vs pt>90). `refs` lands at local
/// id `C2` in both files, so both keys render
/// `"C1#jets,dR(C2#refs[0], C1#jets[*])"` — byte-identical, physically
/// different. The merge fuses the two `Sorted` collections, and `sj[0].pt`
/// becomes ONE quantity across the two files even though it denotes a
/// different jet in each (they are different orderings of the same set).
///
/// FIXED 2026-07-25: `Merger::remap_key` now namespaces the opaque key by
/// unit ordinal exactly as `remap_arg` namespaces opaque args, so the two
/// units' sorted views never unify. This test pins the fix; the verdict-level
/// pin lives in adl-analysis/tests/merge_identity_sortkey.rs.
#[test]
fn take_level_opaque_sort_keys_never_unify_across_units() {
    let mk = |refcut: &str| {
        format!(
            "object jets\n  take Jet\n  select pt > 30\n\
             object refs\n  take Jet\n  {refcut}\n\
             object sj\n  take sort(jets, dR(jets, refs[0]))\n\
             region R\n  select sj[0].pt > 100\n"
        )
    };
    let a = hir(&mk("select pt > 50"), "a");
    let b = hir(&mk("select pt > 90"), "b");
    let key = |h: &Hir| -> String {
        h.table
            .collections()
            .iter()
            .find_map(|c| match c {
                Collection::Sorted { key: SortKey::Opaque(s), .. } => Some(s.clone()),
                _ => None,
            })
            .expect("an opaque sort key")
    };
    assert_eq!(
        key(&a),
        key(&b),
        "test premise: the two physically different keys must render alike"
    );

    let m = merged(&[&a, &b]);
    // The structural invariants I1-I5 all still hold: this gap lives in a
    // pass-through, not in the predicate interner.
    let sorted = m
        .table
        .collections()
        .iter()
        .filter(|c| matches!(c, Collection::Sorted { .. }))
        .count();
    assert_eq!(
        sorted, 2,
        "two units' opaque sort keys must never fuse (unit-namespaced keys)"
    );
    let elem_props = m
        .table
        .quantities()
        .iter()
        .filter(|q| matches!(q, Quantity::ElemProp { .. }))
        .count();
    // With one, `sj[0].pt` would be a single solver variable shared by two
    // files whose sort orders differ — enough for a false PROVEN DISJOINT
    // (pinned at verdict level in adl-analysis/tests/merge_identity_sortkey.rs).
    assert_eq!(elem_props, 2, "sj[0].pt must stay per-unit");
}

// --------------------------------------------- adversarial: region labels

/// The ordinary collision: two files with the SAME basename. Labels are
/// disambiguated with `#n`, so I5 (unique labels) holds.
#[test]
fn duplicate_unit_names_are_disambiguated() {
    let a = hir("region R\n  select MET.pt > 10\n", "sr.adl");
    let b = hir("region R\n  select MET.pt > 20\n", "SR.adl");
    let m = merged(&[&a, &b]);
    let labels: Vec<&str> = m
        .region_name_order
        .iter()
        .map(|&s| m.symbols.display(s))
        .collect();
    assert_eq!(labels, vec!["sr.adl::R", "SR.adl#2::R"]);
}

/// FIXED 2026-07-25: the `#n` disambiguation is minted in the same namespace
/// it protects, so a file literally named `a#2` used to collide with the
/// label generated for the second file named `a` (two merged regions with
/// one label — inverting cross/intra classification). The merger now bumps
/// `n` until the label is genuinely fresh.
#[test]
fn unit_label_dedup_survives_a_filename_that_mimics_the_suffix() {
    let a = hir("region R\n  select MET.pt > 10\n", "a");
    let b = hir("region R\n  select MET.pt > 20\n", "a#2");
    let c = hir("region R\n  select MET.pt > 30\n", "a");
    let m = merge_hirs(&[&a, &b, &c]);
    check_merge_invariants(&m);

    let labels: Vec<&str> = m
        .region_name_order
        .iter()
        .map(|&s| m.symbols.display(s))
        .collect();
    assert_eq!(labels, vec!["a::R", "a#2::R", "a#3::R"]);
    assert_eq!(
        labels.iter().collect::<HashSet<_>>().len(),
        3,
        "three regions, three distinct labels"
    );
}

/// A single-unit merge is the identity on structure (and must still satisfy
/// every invariant — including the opaque-arg namespacing, which applies even
/// when there is only one ordinal to stamp).
#[test]
fn merge_of_one_unit_preserves_structure_and_invariants() {
    let a = hir(
        "object jets\n  take Jet\n  select pt > 30\n\
         region R\n  select size(jets) >= 1\n  select bdt(jets, \"loose\") > 0.5\n\
         region S\n  select MET.pt < 5\n",
        "solo",
    );
    let m = merged(&[&a]);
    assert_eq!(m.regions.len(), 2);
    assert_eq!(n_filtered(&m), 1);
}

// -------------------------------------------------------- property sweep

/// A deterministic grammar of cut/region shapes, mixing:
/// exact cuts with varying constants, unsupported reducer shapes whose reason
/// strings collide, unresolved identifiers (same, different, case-folded),
/// element-context opaque calls, and opaque external args. Every pair (and a
/// sampled set of triples) is merged and checked.
const TEMPLATES: &[&str] = &[
    // -- exact, supported cuts: identical ones MUST unify, different ones MUST NOT
    "object o\n  take Jet\n  select pt > 30\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select pt > 20\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select pt > 30.0\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select abs(eta) < 2.4\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select pt > 30\n  select abs(eta) < 2.4\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  reject pt < 20\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select pt [] 30 50\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Ele\n  select pt > 30\nregion R\n  select size(o) >= 1\n",
    // -- unsupported reducer shapes whose REASON strings collide
    "object o\n  take Jet\n  select sum(pt(Muon) + pt(Electron)) > 5\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select sum(eta(Photon) * eta(Tau)) > 5\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select sum(phi(Muon) - phi(Tau)) > 5\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select any(dR(this, Muon) > 0.4)\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select all(dR(this, Electron) > 0.4)\nregion R\n  select size(o) >= 1\n",
    // -- unresolved identifiers: same, different, case-folded
    "object o\n  take Jet\n  select passIso > 0.5\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select mediumId > 0.5\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select PASSISO > 0.5\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select ptcone30 / 10 < 0.1\nregion R\n  select size(o) >= 1\n",
    // -- element-context opaque calls (fail closed at resolve time)
    "object o\n  take Jet\n  select bdt(this) > 0.5\nregion R\n  select size(o) >= 1\n",
    "object o\n  take Jet\n  select mva(this) > 0.5\nregion R\n  select size(o) >= 1\n",
    // -- opaque external args at region level (the namespaced path)
    "object o\n  take Jet\n  select pt > 30\nregion R\n  select bdt(o, \"loose\") > 0.5\n",
    "object o\n  take Jet\n  select pt > 40\nregion R\n  select bdt(o, \"loose\") > 0.5\n",
    "object o\n  take Jet\n  select pt > 30\nregion R\n  select bdt(o.pt + 1) > 0.5\n",
    // -- unresolvable base (unit-unique private symbol)
    "object o\n  take NoSuchThing\n  select pt > 30\nregion R\n  select size(o) >= 1\n",
    // -- take-level sorts (one aliasing pt-descending, one opaque key)
    "object o\n  take sort(Jet, pt(Jet), descend)\nregion R\n  select size(o) >= 1\n",
    "object j\n  take Jet\n  select pt > 30\nobject o\n  take sort(j, eta(j), ascend)\nregion R\n  select size(o) >= 1\n",
];

#[test]
fn property_sweep_all_unit_pairs() {
    let units: Vec<Hir> = TEMPLATES
        .iter()
        .enumerate()
        .map(|(i, s)| hir(s, &format!("u{i}")))
        .collect();
    // Re-resolve under a second unit label so a pair (i, i) is two genuinely
    // separate units, not the same Hir twice.
    let alts: Vec<Hir> = TEMPLATES
        .iter()
        .enumerate()
        .map(|(i, s)| hir(s, &format!("v{i}")))
        .collect();

    let mut checked = 0;
    for a in &units {
        for b in &alts {
            merged(&[a, b]);
            checked += 1;
        }
    }
    assert_eq!(checked, TEMPLATES.len() * TEMPLATES.len());
}

/// Deterministic sample of three-unit merges: the third unit lands in an
/// already-populated shared space, which is where a per-unit memo table that
/// leaked across units would show up.
#[test]
fn property_sweep_sampled_unit_triples() {
    let n = TEMPLATES.len();
    let labels = ["x", "y", "z"];
    let mk = |t: usize, slot: usize| hir(TEMPLATES[t], &format!("{}{t}", labels[slot]));

    // xorshift64*, seeded — deterministic across runs and platforms.
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut checked = 0;
    for _ in 0..250 {
        let i = (next() as usize) % n;
        let j = (next() as usize) % n;
        let k = (next() as usize) % n;
        let (a, b, c) = (mk(i, 0), mk(j, 1), mk(k, 2));
        merged(&[&a, &b, &c]);
        checked += 1;
    }
    assert_eq!(checked, 250);
}

/// The sweep would pass vacuously if the templates stopped producing the
/// shapes it is meant to stress. Pin the corpus's own coverage.
#[test]
fn property_sweep_corpus_actually_covers_the_hard_shapes() {
    let units: Vec<Hir> = TEMPLATES
        .iter()
        .enumerate()
        .map(|(i, s)| hir(s, &format!("u{i}")))
        .collect();

    let unsupported_preds: usize = units
        .iter()
        .map(|h| {
            h.elem_preds
                .iter()
                .filter(|p| p.node.has_unsupported())
                .count()
        })
        .sum();
    assert!(
        unsupported_preds >= 8,
        "corpus must exercise lossy renders: {unsupported_preds}"
    );

    let mut renders: HashMap<&str, usize> = HashMap::new();
    for h in &units {
        for p in &h.elem_preds {
            if p.node.has_unsupported() {
                *renders.entry(p.render.as_str()).or_default() += 1;
            }
        }
    }
    assert!(
        renders.values().any(|&n| n > 1),
        "corpus must contain at least one COLLIDING unsupported render: {renders:?}"
    );

    let opaque_args = units
        .iter()
        .filter(|h| {
            h.table.quantities().iter().any(|q| match q {
                Quantity::ExternalFn { args, .. } => {
                    args.iter().any(|a| matches!(a, QuantityArg::Opaque(_)))
                }
                _ => false,
            })
        })
        .count();
    assert!(opaque_args >= 2, "corpus must exercise opaque args");

    let sorted = units
        .iter()
        .filter(|h| {
            h.table
                .collections()
                .iter()
                .any(|c| matches!(c, Collection::Sorted { .. }))
        })
        .count();
    assert!(sorted >= 1, "corpus must exercise a Sorted collection");
}
