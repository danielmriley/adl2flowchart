//! `adl-axioms` — the audited axiom catalog (SPEC_ANALYSIS §4,
//! SPEC_ARCHITECTURE §6, ADR-008).
//!
//! Axioms are the one place where physics claims enter the math. Every
//! background fact asserted to the solver lives in ONE table
//! ([`catalog`]), each entry carrying its statement, its justification
//! ("true of every physical event because …") and the assumption it
//! rides on; every entry has a test that evaluates the emitted instances
//! on generated physical events (`tests/axioms_hold.rs`).
//!
//! Instances are **emitted over a quantity set**: [`emit_axioms`] takes
//! the quantities mentioned by the formulas under analysis and produces
//! ground [`QFormula`] facts (interning helper quantities — guard sizes,
//! parent properties — as needed, to a fixpoint).
//!
//! Prohibited-by-history (permanent, ADR-008):
//! - "referencing `C[i]` implies `size(C) > i`" — false under guards;
//!   produced a false empty-region proof in the legacy tool. No emitter
//!   may derive size facts from mere mention of an element.
//! - substring tag matching ("`btagDeepB` contains `btag`, so ∈ {0,1}")
//!   — hit continuous discriminants (audit Bug 6). The TAG axiom is
//!   **exact-name only**.
//!
//! Padding semantics: out-of-range element variables are free in the
//! solver's event model (SPEC_ANALYSIS §2). Every axiom here remains
//! satisfiable on every physical event under the canonical pad-with-0
//! extension (missing element properties valued 0), which is what makes
//! asserting them sound for UNSAT-direction proofs.

use adl_formula::{LinAtom, QFormula, Rel};
use adl_sema::{
    AngKind, Collection, CollectionId, ElemIndex, ExtDecls, Fragment, HKind, HNode, Hir,
    ParticleRef, Quantity, QuantityId, QuantityTable, ScalarSource,
};
use std::collections::{BTreeMap, BTreeSet};

/// Crate identity marker used by the bootstrap smoke test.
pub const CRATE_NAME: &str = "adl-axioms";

/// Sound over-approximation of π for range axioms: an axiom bound must
/// be ≥ the true π, and `3.141592653589793 < π`, so we widen by one ulp.
pub const PI_UPPER: f64 = 3.141_592_653_589_794;

/// The axiom identifiers of the bootstrap catalog (SPEC_ANALYSIS §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AxiomId {
    Ord,
    Sz0,
    Sub,
    Uni,
    Nneg,
    Dphi,
    Tag,
    Twin,
    Epred,
    Idom,
}

impl AxiomId {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AxiomId::Ord => "ORD",
            AxiomId::Sz0 => "SZ0",
            AxiomId::Sub => "SUB",
            AxiomId::Uni => "UNI",
            AxiomId::Nneg => "NNEG",
            AxiomId::Dphi => "DPHI",
            AxiomId::Tag => "TAG",
            AxiomId::Twin => "TWIN",
            AxiomId::Epred => "EPRED",
            AxiomId::Idom => "IDOM",
        }
    }

    /// All catalog ids, in catalog order.
    pub const ALL: [AxiomId; 10] = [
        AxiomId::Ord,
        AxiomId::Sz0,
        AxiomId::Sub,
        AxiomId::Uni,
        AxiomId::Nneg,
        AxiomId::Dphi,
        AxiomId::Tag,
        AxiomId::Twin,
        AxiomId::Epred,
        AxiomId::Idom,
    ];
}

impl std::fmt::Display for AxiomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One audited catalog row: statement + justification + assumption tag.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    pub id: AxiomId,
    pub statement: &'static str,
    pub justification: &'static str,
    pub assumption: &'static str,
}

/// The normative bootstrap catalog (SPEC_ANALYSIS §4). Adding an entry
/// requires a justification, an assumption tag, and a test in
/// `tests/axioms_hold.rs`.
#[must_use]
pub fn catalog() -> &'static [CatalogEntry] {
    &[
        CatalogEntry {
            id: AxiomId::Ord,
            statement: "pt(C[i]) >= pt(C[j]) for i < j, same base/filtered C",
            justification: "true of every physical event because detector collections are \
                            delivered pT-descending and filtering preserves order",
            assumption: "collections pT-ordered",
        },
        CatalogEntry {
            id: AxiomId::Sz0,
            statement: "size(C) >= 0",
            justification: "true of every physical event because a collection is a finite list",
            assumption: "none",
        },
        CatalogEntry {
            id: AxiomId::Sub,
            statement: "size(F) <= size(P) for single-source filtered F of P",
            justification: "true of every physical event because an object block keeps a subset \
                            of its single take source; NEVER emitted for unions (audit: union \
                            size regression)",
            assumption: "take = filter",
        },
        CatalogEntry {
            id: AxiomId::Uni,
            statement: "size(U) >= size(part) for each part; size(U) <= sum of parts",
            justification: "true of every physical event under both concat and dedup readings \
                            of union",
            assumption: "union = concat/dedup",
        },
        CatalogEntry {
            id: AxiomId::Nneg,
            statement: "pt, m, e, ht-family scalars, MET.pt, dR >= 0; also opaque external \
                        calls named exactly pt/m/mass/e/energy/dr (case-insensitive)",
            justification: "true of every physical event because these are magnitudes by \
                            definition: pT, mass and energy of ANY particle combination are \
                            >= 0 (m and E of a summed four-vector by the timelike/lightlike \
                            physical-state condition), and dR is a metric distance. The \
                            EXACT-NAME rule keeps unrelated opaque functions (bdt, \
                            aplanarity, ...) free",
            assumption: "none",
        },
        CatalogEntry {
            id: AxiomId::Dphi,
            statement: "-pi <= dphi <= pi (bound widened by one ulp for soundness)",
            justification: "true of every physical event because azimuthal differences are \
                            wrapped into one period under either sign convention",
            assumption: "both sign conventions (OPEN-2)",
        },
        CatalogEntry {
            id: AxiomId::Tag,
            statement: "exact-name btag/ctag/tautag element properties and trig(.) are in {0,1}",
            justification: "true of every physical event because tags and trigger flags are \
                            booleans; EXACT-NAME rule keeps continuous discriminants \
                            (btagDeepB, ...) out (audit Bug 6)",
            assumption: "tags boolean; discriminants excluded by exact-name rule",
        },
        CatalogEntry {
            id: AxiomId::Twin,
            statement: "oriented twins: x = y or x = -y for reversed-argument dphi/deta pairs",
            justification: "true of every physical event because reversing the arguments either \
                            preserves or negates the separation, whichever convention holds",
            assumption: "either convention (OPEN-2)",
        },
        CatalogEntry {
            id: AxiomId::Epred,
            statement: "size(F) > i implies predF(F[i]) for filtered F (exactly-encodable \
                        conjuncts of predF)",
            justification: "true of every physical event because every element of a filtered \
                            collection passed the filter; the size guard keeps it vacuous for \
                            absent elements (guarded references do not imply existence)",
            assumption: "take = filter",
        },
        CatalogEntry {
            id: AxiomId::Idom,
            statement: "pt(F[i]) <= pt(P[i]) for filtered F of P",
            justification: "true of every physical event because F[i] equals some P[j] with \
                            j >= i and P is pT-descending; satisfiable for absent elements \
                            under the canonical pad-with-0 extension",
            assumption: "ORD + SUB",
        },
    ]
}

/// Catalog row for `id`.
#[must_use]
pub fn catalog_entry(id: AxiomId) -> &'static CatalogEntry {
    catalog()
        .iter()
        .find(|e| e.id == id)
        .expect("every AxiomId has a catalog row")
}

/// One emitted ground fact.
#[derive(Debug, Clone, PartialEq)]
pub struct AxiomInstance {
    pub id: AxiomId,
    pub formula: QFormula,
    /// Human description in source notation ("jets[0].pt >= jets[1].pt").
    pub description: String,
}

/// The emitted axiom set for one analysis quantity set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AxiomSet {
    pub instances: Vec<AxiomInstance>,
}

impl AxiomSet {
    /// Every quantity referenced by the emitted instances.
    #[must_use]
    pub fn quantities(&self) -> BTreeSet<QuantityId> {
        let mut out = BTreeSet::new();
        for inst in &self.instances {
            collect_quantities(&inst.formula, &mut out);
        }
        out
    }

    /// Distinct axiom ids used, in catalog order.
    #[must_use]
    pub fn ids_used(&self) -> Vec<AxiomId> {
        AxiomId::ALL
            .into_iter()
            .filter(|id| self.instances.iter().any(|i| i.id == *id))
            .collect()
    }
}

/// Collect every quantity mentioned in `f` into `out`.
pub fn collect_quantities(f: &QFormula, out: &mut BTreeSet<QuantityId>) {
    match f {
        QFormula::True | QFormula::False => {}
        QFormula::Atom(a) => out.extend(a.terms().iter().map(|&(_, q)| q)),
        QFormula::And(v) | QFormula::Or(v) => {
            for p in v {
                collect_quantities(p, out);
            }
        }
    }
}

/// Oriented twin pairs (same oriented kind, reversed arguments) inside
/// `qs`. Pairs whose combined quantities contain such a twin cap the
/// SAT direction at POSSIBLY until OPEN-2 is resolved (SPEC_ANALYSIS §4).
#[must_use]
pub fn twin_pairs(
    table: &QuantityTable,
    qs: &BTreeSet<QuantityId>,
) -> Vec<(QuantityId, QuantityId)> {
    let mut out = Vec::new();
    let list: Vec<QuantityId> = qs.iter().copied().collect();
    for (n, &q1) in list.iter().enumerate() {
        let Quantity::AngularSep {
            kind: k1,
            a: a1,
            b: b1,
            oriented: true,
        } = table.quantity(q1)
        else {
            continue;
        };
        for &q2 in &list[n + 1..] {
            if let Quantity::AngularSep {
                kind: k2,
                a: a2,
                b: b2,
                oriented: true,
            } = table.quantity(q2)
                && k1 == k2
                && a1 == b2
                && b1 == a2
                && a1 != b1
            {
                out.push((q1, q2));
            }
        }
    }
    out
}

// ---- human labels (source notation) -------------------------------------

/// Human label for a collection: first bound name, else base name.
#[must_use]
pub fn collection_label(hir: &Hir, c: CollectionId) -> String {
    if let Some(names) = hir.coll_names.get(c.0 as usize)
        && let Some(sym) = names.first()
    {
        return hir.symbols.display(*sym).to_owned();
    }
    match hir.table.collection(c) {
        Collection::Base(sym) => hir.symbols.display(*sym).to_owned(),
        _ => format!("{c}"),
    }
}

fn particle_label(hir: &Hir, p: &ParticleRef) -> String {
    match p {
        ParticleRef::Elem { coll, index } => format!("{}[{index}]", collection_label(hir, *coll)),
        ParticleRef::Whole(coll) => format!("{}[*]", collection_label(hir, *coll)),
        ParticleRef::Met => "MET".to_owned(),
        ParticleRef::Binder { coll, name } => {
            format!(
                "{}@{}",
                collection_label(hir, *coll),
                hir.symbols.display(*name)
            )
        }
    }
}

/// Human label for a quantity, in source notation.
#[must_use]
pub fn quantity_label(hir: &Hir, q: QuantityId) -> String {
    match hir.table.quantity(q) {
        Quantity::EventScalar(src) => match src {
            ScalarSource::MetProp(p) => format!("MET.{}", hir.table.prop_display(*p)),
            ScalarSource::EventVar(s) => hir.symbols.display(*s).to_owned(),
            ScalarSource::Trigger(s) => format!("trig({})", hir.symbols.display(*s)),
        },
        Quantity::Size(c) => format!("size({})", collection_label(hir, *c)),
        Quantity::ElemProp { coll, index, prop } => format!(
            "{}[{index}].{}",
            collection_label(hir, *coll),
            hir.table.prop_display(*prop)
        ),
        Quantity::AngularSep { kind, a, b, .. } => format!(
            "{}({}, {})",
            kind.as_str(),
            particle_label(hir, a),
            particle_label(hir, b)
        ),
        Quantity::ExternalFn { name, .. } => {
            format!("{}(...)", hir.symbols.display(*name))
        }
    }
}

// ---- emission ------------------------------------------------------------

/// Emit the axiom instances relevant to `quantities` (to a fixpoint:
/// helper quantities interned by one round — guard sizes, parent
/// properties — get their own SZ0/NNEG/ORD/... facts in the next).
#[must_use]
pub fn emit_axioms(hir: &mut Hir, ext: &ExtDecls, quantities: &BTreeSet<QuantityId>) -> AxiomSet {
    let mut qs: BTreeSet<QuantityId> = quantities.clone();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut instances: Vec<AxiomInstance> = Vec::new();
    // Chains are finite, so this terminates; the cap is a safety net.
    for _ in 0..32 {
        let snapshot: Vec<QuantityId> = qs.iter().copied().collect();
        let round = emit_round(hir, ext, &snapshot);
        let mut grew = false;
        for inst in round {
            let key = format!("{}|{:?}", inst.id, inst.formula);
            if seen.insert(key) {
                let mut mentioned = BTreeSet::new();
                collect_quantities(&inst.formula, &mut mentioned);
                for q in mentioned {
                    grew |= qs.insert(q);
                }
                instances.push(inst);
            }
        }
        if !grew {
            break;
        }
    }
    instances.sort_by(|a, b| (a.id, &a.description).cmp(&(b.id, &b.description)));
    AxiomSet { instances }
}

struct Emit<'h> {
    hir: &'h mut Hir,
    ext: &'h ExtDecls,
    pt_key: String,
    nneg_prop_keys: Vec<String>,
    tag_keys: [&'static str; 3],
    out: Vec<AxiomInstance>,
}

fn emit_round(hir: &mut Hir, ext: &ExtDecls, qs: &[QuantityId]) -> Vec<AxiomInstance> {
    let pt_key = ext.prop_canon("pt").0;
    let nneg_prop_keys = vec![
        ext.prop_canon("pt").0,
        ext.prop_canon("m").0,
        ext.prop_canon("e").0,
    ];
    let mut em = Emit {
        hir,
        ext,
        pt_key,
        nneg_prop_keys,
        tag_keys: ["btag", "ctag", "tautag"],
        out: Vec::new(),
    };
    em.ord(qs);
    em.sz0(qs);
    em.sub(qs);
    em.uni(qs);
    em.nneg(qs);
    em.dphi(qs);
    em.tag(qs);
    em.twin(qs);
    em.epred(qs);
    em.idom(qs);
    em.out
}

impl Emit<'_> {
    fn push(&mut self, id: AxiomId, formula: QFormula, description: String) {
        self.out.push(AxiomInstance {
            id,
            formula,
            description,
        });
    }

    fn atom(terms: &[(f64, QuantityId)], rel: Rel, k: f64) -> QFormula {
        QFormula::Atom(
            LinAtom::new(terms.iter().copied(), rel, k)
                .expect("axiom constants are small finite literals"),
        )
    }

    fn label(&self, q: QuantityId) -> String {
        quantity_label(self.hir, q)
    }

    /// Is `c` a base or (transitively single-source) filtered collection?
    /// ORD/IDOM ride on pT ordering, which unions/combinations do not keep
    /// (a union can interleave).
    fn pt_ordered(&self, c: CollectionId) -> bool {
        matches!(
            self.hir.table.collection(c),
            Collection::Base(_) | Collection::Filtered { .. }
        )
    }

    fn elem_pt_quantities(
        &self,
        qs: &[QuantityId],
    ) -> BTreeMap<CollectionId, Vec<(u32, QuantityId)>> {
        let mut by_coll: BTreeMap<CollectionId, Vec<(u32, QuantityId)>> = BTreeMap::new();
        for &q in qs {
            if let Quantity::ElemProp {
                coll,
                index: ElemIndex::FromFront(i),
                prop,
            } = self.hir.table.quantity(q)
                && self.hir.table.prop_key(*prop) == self.pt_key
                && self.pt_ordered(*coll)
            {
                by_coll.entry(*coll).or_default().push((*i, q));
            }
        }
        for v in by_coll.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        by_coll
    }

    // ORD: pt(C[i]) >= pt(C[j]) for i < j (mentioned indices, same C).
    fn ord(&mut self, qs: &[QuantityId]) {
        for (_, idx) in self.elem_pt_quantities(qs) {
            for a in 0..idx.len() {
                for b in a + 1..idx.len() {
                    let (i, qi) = idx[a];
                    let (j, qj) = idx[b];
                    if i < j {
                        let f = Self::atom(&[(1.0, qi), (-1.0, qj)], Rel::Ge, 0.0);
                        let d = format!("{} >= {}", self.label(qi), self.label(qj));
                        self.push(AxiomId::Ord, f, d);
                    }
                }
            }
        }
    }

    // SZ0: size(C) >= 0.
    fn sz0(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            if matches!(self.hir.table.quantity(q), Quantity::Size(_)) {
                let f = Self::atom(&[(1.0, q)], Rel::Ge, 0.0);
                let d = format!("{} >= 0", self.label(q));
                self.push(AxiomId::Sz0, f, d);
            }
        }
    }

    // SUB: size(F) <= size(P), single-source filtered only (NEVER unions).
    fn sub(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            let Quantity::Size(c) = self.hir.table.quantity(q) else {
                continue;
            };
            let Collection::Filtered { parent, .. } = self.hir.table.collection(*c) else {
                continue;
            };
            let parent = *parent;
            let qp = self.hir.table.intern_quantity(Quantity::Size(parent));
            let f = Self::atom(&[(1.0, q), (-1.0, qp)], Rel::Le, 0.0);
            let d = format!("{} <= {}", self.label(q), self.label(qp));
            self.push(AxiomId::Sub, f, d);
        }
    }

    // UNI: size(U) >= each part, size(U) <= sum of parts.
    fn uni(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            let Quantity::Size(c) = self.hir.table.quantity(q) else {
                continue;
            };
            let Collection::Union(parts) = self.hir.table.collection(*c) else {
                continue;
            };
            let parts = parts.clone();
            let part_sizes: Vec<QuantityId> = parts
                .iter()
                .map(|&p| self.hir.table.intern_quantity(Quantity::Size(p)))
                .collect();
            for &qp in &part_sizes {
                let f = Self::atom(&[(1.0, q), (-1.0, qp)], Rel::Ge, 0.0);
                let d = format!("{} >= {}", self.label(q), self.label(qp));
                self.push(AxiomId::Uni, f, d);
            }
            let mut terms = vec![(1.0, q)];
            terms.extend(part_sizes.iter().map(|&qp| (-1.0, qp)));
            let f = Self::atom(&terms, Rel::Le, 0.0);
            let d = format!(
                "{} <= {}",
                self.label(q),
                part_sizes
                    .iter()
                    .map(|&qp| self.label(qp))
                    .collect::<Vec<_>>()
                    .join(" + ")
            );
            self.push(AxiomId::Uni, f, d);
        }
    }

    // NNEG: pt/m/e element props, ht-family scalars, MET.pt, dR >= 0;
    // opaque external calls named exactly pt/m/mass/e/energy/dr
    // (case-insensitive symbol key) are magnitudes of SOME particle
    // combination, hence >= 0 regardless of the (opaque) arguments.
    fn nneg(&mut self, qs: &[QuantityId]) {
        const NNEG_EXTFN_KEYS: [&str; 6] = ["pt", "m", "mass", "e", "energy", "dr"];
        for &q in qs {
            let nonneg = match self.hir.table.quantity(q) {
                Quantity::ElemProp { prop, .. } => self
                    .nneg_prop_keys
                    .iter()
                    .any(|k| k == self.hir.table.prop_key(*prop)),
                Quantity::EventScalar(ScalarSource::MetProp(p)) => {
                    self.hir.table.prop_key(*p) == self.ext.prop_canon("pt").0
                }
                Quantity::EventScalar(ScalarSource::EventVar(s)) => {
                    self.ext.is_event_scalar(self.hir.symbols.key(*s))
                }
                Quantity::AngularSep {
                    kind: AngKind::DR, ..
                } => true,
                Quantity::ExternalFn { name, .. } => {
                    NNEG_EXTFN_KEYS.contains(&self.hir.symbols.key(*name))
                }
                _ => false,
            };
            if nonneg {
                let f = Self::atom(&[(1.0, q)], Rel::Ge, 0.0);
                let d = format!("{} >= 0", self.label(q));
                self.push(AxiomId::Nneg, f, d);
            }
        }
    }

    // DPHI: -pi <= dphi <= pi (widened bound; convention-neutral).
    fn dphi(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            if matches!(
                self.hir.table.quantity(q),
                Quantity::AngularSep {
                    kind: AngKind::DPhi,
                    ..
                }
            ) {
                let f = QFormula::And(vec![
                    Self::atom(&[(1.0, q)], Rel::Le, PI_UPPER),
                    Self::atom(&[(1.0, q)], Rel::Ge, -PI_UPPER),
                ]);
                let d = format!("-pi <= {} <= pi", self.label(q));
                self.push(AxiomId::Dphi, f, d);
            }
        }
    }

    // TAG: exact-name btag/ctag/tautag and trig(.) in {0,1}.
    fn tag(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            let is_tag = match self.hir.table.quantity(q) {
                Quantity::ElemProp { prop, .. } => {
                    self.tag_keys.contains(&self.hir.table.prop_key(*prop))
                }
                Quantity::EventScalar(ScalarSource::Trigger(_)) => true,
                _ => false,
            };
            if is_tag {
                let f = QFormula::Or(vec![
                    Self::atom(&[(1.0, q)], Rel::Eq, 0.0),
                    Self::atom(&[(1.0, q)], Rel::Eq, 1.0),
                ]);
                let d = format!("{} in {{0, 1}}", self.label(q));
                self.push(AxiomId::Tag, f, d);
            }
        }
    }

    // TWIN: x = y or x = -y for oriented reversed-argument pairs.
    fn twin(&mut self, qs: &[QuantityId]) {
        let set: BTreeSet<QuantityId> = qs.iter().copied().collect();
        for (q1, q2) in twin_pairs(&self.hir.table, &set) {
            let f = QFormula::Or(vec![
                Self::atom(&[(1.0, q1), (-1.0, q2)], Rel::Eq, 0.0),
                Self::atom(&[(1.0, q1), (1.0, q2)], Rel::Eq, 0.0),
            ]);
            let d = format!("{} = +/- {}", self.label(q1), self.label(q2));
            self.push(AxiomId::Twin, f, d);
        }
    }

    // EPRED: size(F) > i  =>  predF(F[i])   (exactly-encodable part).
    fn epred(&mut self, qs: &[QuantityId]) {
        let mut targets: BTreeSet<(CollectionId, u32)> = BTreeSet::new();
        for &q in qs {
            if let Quantity::ElemProp {
                coll,
                index: ElemIndex::FromFront(i),
                ..
            } = self.hir.table.quantity(q)
                && matches!(
                    self.hir.table.collection(*coll),
                    Collection::Filtered { .. }
                )
            {
                targets.insert((*coll, *i));
            }
        }
        for (coll, i) in targets {
            let pred_id = match self.hir.table.collection(coll) {
                Collection::Filtered { pred, .. } => pred.0,
                _ => continue,
            };
            let pred_node = self.hir.elem_preds[pred_id as usize].node.clone();
            let Some(pred_f) = encode_elem_pred(&mut self.hir.table, &pred_node, coll, i) else {
                continue;
            };
            let size_q = self.hir.table.intern_quantity(Quantity::Size(coll));
            #[allow(clippy::cast_lossless)]
            let guard = Self::atom(&[(1.0, size_q)], Rel::Le, i as f64);
            let f = QFormula::Or(vec![guard, pred_f]);
            let d = format!(
                "size({}) > {i} => filter predicate holds for {}[{i}]",
                collection_label(self.hir, coll),
                collection_label(self.hir, coll),
            );
            self.push(AxiomId::Epred, f, d);
        }
    }

    // IDOM: pt(F[i]) <= pt(P[i]) for filtered F of P.
    fn idom(&mut self, qs: &[QuantityId]) {
        let by_coll = self.elem_pt_quantities(qs);
        for (coll, idx) in by_coll {
            let Collection::Filtered { parent, .. } = self.hir.table.collection(coll) else {
                continue;
            };
            let parent = *parent;
            if !self.pt_ordered(parent) {
                continue;
            }
            let pt_prop = self
                .hir
                .table
                .intern_prop(&self.pt_key, &self.ext.prop_canon("pt").1);
            for (i, qf) in idx {
                let qp = self.hir.table.intern_quantity(Quantity::ElemProp {
                    coll: parent,
                    index: ElemIndex::FromFront(i),
                    prop: pt_prop,
                });
                let f = Self::atom(&[(1.0, qf), (-1.0, qp)], Rel::Le, 0.0);
                let d = format!("{} <= {}", self.label(qf), self.label(qp));
                self.push(AxiomId::Idom, f, d);
            }
        }
    }
}

// ---- exact element-predicate encoding (for EPRED) ------------------------

/// Encode the element predicate `node` for element `coll[index]`,
/// **exactly or not at all** — except at the top-level conjunction, where
/// keeping only the encodable conjuncts is a sound weakening (every
/// conjunct is separately implied by the predicate).
///
/// This is deliberately conservative: anything outside plain linear
/// comparisons/bands/boolean structure over the implicit element's
/// properties (and event quantities) is dropped.
fn encode_elem_pred(
    table: &mut QuantityTable,
    node: &HNode,
    coll: CollectionId,
    index: u32,
) -> Option<QFormula> {
    if let HKind::And(parts) = &node.kind
        && node.tag.is_in_fragment()
    {
        let kept: Vec<QFormula> = parts
            .iter()
            .filter_map(|p| encode_pred_exact(table, p, coll, index))
            .collect();
        if kept.is_empty() {
            return None;
        }
        return Some(QFormula::And(kept));
    }
    encode_pred_exact(table, node, coll, index)
}

fn encode_pred_exact(
    table: &mut QuantityTable,
    node: &HNode,
    coll: CollectionId,
    index: u32,
) -> Option<QFormula> {
    if matches!(node.tag, Fragment::Unsupported(_)) {
        return None;
    }
    match &node.kind {
        HKind::Bool(b) => Some(if *b { QFormula::True } else { QFormula::False }),
        HKind::And(v) => {
            let parts: Option<Vec<QFormula>> = v
                .iter()
                .map(|p| encode_pred_exact(table, p, coll, index))
                .collect();
            Some(QFormula::And(parts?))
        }
        HKind::Or(v) => {
            let parts: Option<Vec<QFormula>> = v
                .iter()
                .map(|p| encode_pred_exact(table, p, coll, index))
                .collect();
            Some(QFormula::Or(parts?))
        }
        HKind::Not(inner) => Some(encode_pred_exact(table, inner, coll, index)?.not()),
        HKind::Cmp { op, lhs, rhs } => {
            let l = lin_pred(table, lhs, coll, index)?;
            let r = lin_pred(table, rhs, coll, index)?;
            let diff = l.sub(&r)?;
            let rel = match op {
                adl_syntax::ast::CmpOp::Gt => Rel::Gt,
                adl_syntax::ast::CmpOp::Lt => Rel::Lt,
                adl_syntax::ast::CmpOp::Ge => Rel::Ge,
                adl_syntax::ast::CmpOp::Le => Rel::Le,
                adl_syntax::ast::CmpOp::Eq => Rel::Eq,
                adl_syntax::ast::CmpOp::Ne | adl_syntax::ast::CmpOp::ApproxEq => Rel::Ne,
            };
            lin_atom(diff.terms, rel, -diff.k)
        }
        HKind::Band { kind, expr, lo, hi } => {
            let e = lin_pred(table, expr, coll, index)?;
            let lo: f64 = lo.parse().ok().filter(|v: &f64| v.is_finite())?;
            let hi: f64 = hi.parse().ok().filter(|v: &f64| v.is_finite())?;
            let (lo_rel, hi_rel, combine_and) = match kind {
                adl_syntax::ast::BandKind::In => (Rel::Ge, Rel::Le, true),
                adl_syntax::ast::BandKind::Out => (Rel::Le, Rel::Ge, false),
            };
            let lo_b = lin_atom(e.terms.clone(), lo_rel, lo - e.k)?;
            let hi_b = lin_atom(e.terms, hi_rel, hi - e.k)?;
            Some(if combine_and {
                QFormula::And(vec![lo_b, hi_b])
            } else {
                QFormula::Or(vec![lo_b, hi_b])
            })
        }
        _ => None,
    }
}

struct PredLin {
    terms: BTreeMap<QuantityId, f64>,
    k: f64,
}

impl PredLin {
    fn constant(k: f64) -> Self {
        Self {
            terms: BTreeMap::new(),
            k,
        }
    }

    fn finite(&self) -> bool {
        self.k.is_finite() && self.terms.values().all(|c| c.is_finite())
    }

    fn sub(mut self, other: &Self) -> Option<Self> {
        self.k -= other.k;
        for (&q, &c) in &other.terms {
            *self.terms.entry(q).or_insert(0.0) -= c;
        }
        self.finite().then_some(self)
    }

    fn scale(mut self, c: f64) -> Option<Self> {
        self.k *= c;
        for v in self.terms.values_mut() {
            *v *= c;
        }
        self.finite().then_some(self)
    }
}

fn lin_atom(terms: BTreeMap<QuantityId, f64>, rel: Rel, k: f64) -> Option<QFormula> {
    if terms.is_empty() {
        if !k.is_finite() {
            return None;
        }
        return Some(if rel.eval(0.0, k) {
            QFormula::True
        } else {
            QFormula::False
        });
    }
    LinAtom::new(terms.into_iter().map(|(q, c)| (c, q)), rel, k)
        .ok()
        .map(QFormula::Atom)
}

fn lin_pred(
    table: &mut QuantityTable,
    node: &HNode,
    coll: CollectionId,
    index: u32,
) -> Option<PredLin> {
    if matches!(node.tag, Fragment::Unsupported(_)) {
        return None;
    }
    match &node.kind {
        HKind::Num(s) => {
            let v: f64 = s.parse().ok()?;
            v.is_finite().then(|| PredLin::constant(v))
        }
        HKind::ElemSelfProp(prop) => {
            let q = table.intern_quantity(Quantity::ElemProp {
                coll,
                index: ElemIndex::FromFront(index),
                prop: *prop,
            });
            Some(PredLin {
                terms: BTreeMap::from([(q, 1.0)]),
                k: 0.0,
            })
        }
        HKind::Quantity(q) => Some(PredLin {
            terms: BTreeMap::from([(*q, 1.0)]),
            k: 0.0,
        }),
        HKind::Neg(a) => lin_pred(table, a, coll, index)?.scale(-1.0),
        HKind::Binary { op, lhs, rhs } => {
            let l = lin_pred(table, lhs, coll, index)?;
            let r = lin_pred(table, rhs, coll, index)?;
            match op {
                adl_sema::ArithOp::Add => {
                    let mut out = l;
                    out.k += r.k;
                    for (q, c) in r.terms {
                        *out.terms.entry(q).or_insert(0.0) += c;
                    }
                    out.finite().then_some(out)
                }
                adl_sema::ArithOp::Sub => l.sub(&r),
                adl_sema::ArithOp::Mul => {
                    if l.terms.is_empty() {
                        r.scale(l.k)
                    } else if r.terms.is_empty() {
                        l.scale(r.k)
                    } else {
                        None
                    }
                }
                adl_sema::ArithOp::Div => {
                    if r.terms.is_empty() && r.k != 0.0 {
                        l.scale(1.0 / r.k)
                    } else {
                        None
                    }
                }
                adl_sema::ArithOp::Pow => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_is_wired() {
        assert_eq!(CRATE_NAME, "adl-axioms");
    }

    #[test]
    fn catalog_is_complete_and_audited() {
        let cat = catalog();
        assert_eq!(cat.len(), AxiomId::ALL.len());
        for id in AxiomId::ALL {
            let e = catalog_entry(id);
            assert!(!e.statement.is_empty());
            assert!(
                e.justification.contains("true of every physical event"),
                "{id}: justification must argue physical truth"
            );
            assert!(!e.assumption.is_empty());
        }
    }

    #[test]
    fn pi_upper_strictly_covers_f64_pi() {
        const { assert!(PI_UPPER > std::f64::consts::PI) }
    }
}
