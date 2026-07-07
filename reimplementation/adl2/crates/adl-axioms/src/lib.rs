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
//! asserting them sound for UNSAT-direction proofs. Because that pad is 0,
//! a fact relating a possibly-absent element to a present one can be
//! violated (`0 >= pt(present)` is false), so the back-index ORD families
//! (back-back and front-to-back with `k == 1, i >= 1`) are GUARDED by a
//! `size(C) <= …` disjunct that goes vacuous exactly when the deep/guarded
//! element is missing.

use adl_formula::{DiagTable, Formula, LinAtom, QFormula, Rel};
use adl_sema::{
    AngKind, Collection, CollectionId, CombAxis, CombKind, ElemIndex, ExtDecls, Fragment, HKind,
    HNode, Hir, ParticleRef, Quantity, QuantityArg, QuantityId, QuantityTable, Rat, ScalarSource,
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
    Szslice,
    Szperm,
    CombSize,
    Trig,
    /// Derived cross/intra reconciliation fact: `size(A) <= size(B)` because
    /// A's membership predicate provably implies B's (element-predicate
    /// refinement proven on the subset side). See [`derived_size_le`].
    Xsub,
    /// Derived reconciliation fact: `size(A) = size(B)` (both refinement
    /// directions proven — the two filtered collections select the same
    /// elements of a shared base).
    Xeq,
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
            AxiomId::Szslice => "SZSLICE",
            AxiomId::Szperm => "SZPERM",
            AxiomId::CombSize => "COMBSIZE",
            AxiomId::Trig => "TRIG",
            AxiomId::Xsub => "XSUB",
            AxiomId::Xeq => "XEQ",
        }
    }

    /// All catalog ids, in catalog order.
    pub const ALL: [AxiomId; 16] = [
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
        AxiomId::Szslice,
        AxiomId::Szperm,
        AxiomId::CombSize,
        AxiomId::Trig,
        AxiomId::Xsub,
        AxiomId::Xeq,
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
            statement: "pt(C[i]) >= pt(C[j]) for i < j (front-front, unconditional), same \
                        base/filtered C; back-index families (back-back, and front-to-back with \
                        k == 1, i >= 1) guarded by size(C) so they go vacuous when the deep \
                        element is absent",
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
                        calls named exactly pt/m/mass/e/energy/dr/sqrt (case-insensitive)",
            justification: "true of every physical event because these are magnitudes by \
                            definition: pT, mass and energy of ANY particle combination are \
                            >= 0 (m and E of a summed four-vector by the timelike/lightlike \
                            physical-state condition), dR is a metric distance, and sqrt is \
                            the non-negative real root. The EXACT-NAME rule keeps unrelated \
                            opaque functions (bdt, aplanarity, ...) free, and excludes \
                            eta/phi-of-sum (no sign axiom)",
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
        CatalogEntry {
            id: AxiomId::Szslice,
            statement: "0 <= size(coll[a:b]) <= size(coll); also <= b - a for a concrete \
                        upper bound b >= a",
            justification: "true of every physical event because a half-open contiguous slice \
                            src[a..b] is a sub-list: its length never exceeds the source length, \
                            nor (for a concrete end) the requested window width b - a",
            assumption: "slice = clamped half-open sub-range",
        },
        CatalogEntry {
            id: AxiomId::Szperm,
            statement: "size(sort(C, key, dir)) = size(C)",
            justification: "true of every physical event because a sort is a permutation of the \
                            source list — a bijection preserves cardinality regardless of the \
                            (event-dependent) key. NO per-index ordering fact rides on this; \
                            ORD/IDOM stay off for a non-pT/ascending/union-rooted sort",
            assumption: "sort = permutation",
        },
        CatalogEntry {
            id: AxiomId::CombSize,
            statement: "size(K->axis) = size(K); for a same-source disjoint K over C: \
                        size(C) < 2 => size(K) = 0 and size(K) >= 0; for a cartesian/cross-source \
                        disjoint K over distinct parts: any part empty => size(K) = 0, all parts \
                        non-empty => size(K) >= 1",
            justification: "true of every physical event by tuple combinatorics: a projection \
                            keeps one element per surviving tuple (a bijection onto the tuples); a \
                            same-source pair needs >= 2 distinct source elements to form ANY tuple \
                            (the POSITIVE lower bound size(K) >= 1 is DELIBERATELY OMITTED — \
                            value-distinctness, USER ANSWER 4, lets two value-equal elements form \
                            zero pairs); a cross-source product is non-empty exactly when every \
                            factor is",
            assumption: "comb = tuple enumeration; disjoint distinctness by kinematic value",
        },
        CatalogEntry {
            id: AxiomId::Trig,
            statement: "-1 <= cos(x) <= 1 and -1 <= sin(x) <= 1 for opaque cos/sin calls",
            justification: "true of every physical event because the circular functions are \
                            bounded in [-1, 1] for every real argument, regardless of the (opaque) \
                            argument. NOT applied to tan/asin/... (unbounded / domain-restricted), \
                            and never constant-folded (an irrational cos value is not an exact \
                            rational)",
            assumption: "none",
        },
        CatalogEntry {
            id: AxiomId::Xsub,
            statement: "size(A) <= size(B) when A and B filter the SAME base collection and A's \
                        element predicate provably implies B's (proven on the subset side over a \
                        shared generic element)",
            justification: "true of every physical event because the fact is emitted ONLY when the \
                            solver reports UNSAT for (predA-over AND not predB-under) over one \
                            shared base element: the WEAKEST reading of A's cut already forces the \
                            STRONGEST reading of B's cut, so in ANY event every element A keeps B \
                            keeps too, hence |A| <= |B|. An opaque conjunct in B's predicate is \
                            under-approximated to false (never dropped), so it can only SUPPRESS \
                            the fact, never fabricate it; a residual composite/reduce binder aborts \
                            the pair (fail-closed)",
            assumption: "same base name = same base input (documented cross-file residual)",
        },
        CatalogEntry {
            id: AxiomId::Xeq,
            statement: "size(A) = size(B) when A and B filter the same base and each element \
                        predicate implies the other (both refinement directions proven)",
            justification: "true of every physical event because both directions are the XSUB proof \
                            run each way; each is individually sound (see XSUB), so their \
                            conjunction size(A) <= size(B) <= size(A) holds in every event",
            assumption: "same base name = same base input (documented cross-file residual)",
        },
    ]
}

/// The canonical encoding of a derived size-refinement fact `size(sub) <=
/// size(sup)`, used by the reconciliation engine. Kept here (beside the [`Sub`]
/// axiom it mirrors — `Emit::sub`) so the SUB and XSUB size encodings can never
/// drift apart. `sub` and `sup` MUST be DISTINCT [`Quantity::Size`] ids.
///
/// [`Sub`]: AxiomId::Sub
#[must_use]
pub fn derived_size_le(sub: QuantityId, sup: QuantityId) -> QFormula {
    let one = Rat::from_decimal_f64(1.0).expect("finite");
    let neg_one = Rat::from_decimal_f64(-1.0).expect("finite");
    let zero = Rat::from_decimal_f64(0.0).expect("finite");
    QFormula::Atom(LinAtom::new([(one, sub), (neg_one, sup)], Rel::Le, zero))
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
        ParticleRef::ThisElem => "this".to_owned(),
        ParticleRef::ReduceElem => "elem".to_owned(),
        ParticleRef::Sum(parts) => {
            let parts: Vec<String> = parts.iter().map(|p| particle_label(hir, p)).collect();
            format!("({})", parts.join(" + "))
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
    em.trig(qs);
    em.dphi(qs);
    em.tag(qs);
    em.twin(qs);
    em.epred(qs);
    em.idom(qs);
    em.szslice(qs);
    em.szperm(qs);
    em.comb_size(qs);
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
        let r = |v: f64| Rat::from_decimal_f64(v).expect("axiom constants are finite literals");
        QFormula::Atom(LinAtom::new(
            terms.iter().map(|&(c, q)| (r(c), q)),
            rel,
            r(k),
        ))
    }

    /// The definedness chokepoint for ELEMENT facts (proof-system v2
    /// Phase 2 — partiality by construction): the fact is asserted only
    /// where every element-dependent term exists, `Or(⋁ some element
    /// absent, fact)`, with the existence floors read from the table's
    /// single source ([`QuantityTable::existence_floor`] — the same floors
    /// the formula encoder's leaf guards use, so the two sides can never
    /// disagree about definedness). Stating an element fact any other way
    /// re-opens the extension-conflict false-PROVEN class (review S3: two
    /// axiom families demanding incompatible values for an ABSENT element's
    /// free variable render the base frame unsatisfiable on real events).
    /// Returns the guarded formula and the `size(C) > n ∧ … ⇒ ` description
    /// prefix (empty when no term is element-dependent).
    fn guarded(&mut self, terms: &[(f64, QuantityId)], rel: Rel, k: f64) -> (QFormula, String) {
        let mut floors: BTreeMap<CollectionId, u32> = BTreeMap::new();
        for &(_, q) in terms {
            self.hir.table.existence_floor(q, &mut floors);
        }
        let fact = Self::atom(terms, rel, k);
        if floors.is_empty() {
            return (fact, String::new());
        }
        let mut parts: Vec<QFormula> = Vec::new();
        let mut prefix = String::new();
        for (coll, floor) in floors {
            let sq = self.hir.table.intern_quantity(Quantity::Size(coll));
            parts.push(Self::atom(&[(1.0, sq)], Rel::Le, f64::from(floor)));
            prefix.push_str(&format!(
                "size({}) > {floor} ∧ ",
                collection_label(self.hir, coll)
            ));
        }
        // The trailing "∧ " becomes the implication arrow.
        prefix.truncate(prefix.len() - "∧ ".len());
        prefix.push_str("⇒ ");
        parts.push(fact);
        (QFormula::Or(parts), prefix)
    }

    fn label(&self, q: QuantityId) -> String {
        quantity_label(self.hir, q)
    }

    /// Is `c` pT-descending? ORD/IDOM/EPRED ride on pT ordering. Delegates to
    /// the single shared walk on [`QuantityTable::pt_ordered`] (plan §risk 5 —
    /// no second copy may diverge): true for a base, a filtered chain rooted at
    /// a base (filtering preserves source order), a slice of a pT-descending
    /// source (a contiguous sub-sequence stays sorted), and a descending-pT
    /// sort of a pT-descending source (the identity permutation — but that
    /// exact shape is aliased to the source at resolve time, so it never
    /// reaches here as a distinct `Sorted`). A union/combination/projection,
    /// a non-pT or ascending sort, is `false` (the sound posture).
    fn pt_ordered(&self, c: CollectionId) -> bool {
        self.hir.table.pt_ordered(c, &self.pt_key)
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

    /// Back-indexed pt quantities (`pt(C[-k])`), grouped per collection and
    /// sorted by depth `k`. Same guards as the front collector.
    fn elem_pt_back_quantities(
        &self,
        qs: &[QuantityId],
    ) -> BTreeMap<CollectionId, Vec<(u32, QuantityId)>> {
        let mut by_coll: BTreeMap<CollectionId, Vec<(u32, QuantityId)>> = BTreeMap::new();
        for &q in qs {
            if let Quantity::ElemProp {
                coll,
                index: ElemIndex::FromBack(k),
                prop,
            } = self.hir.table.quantity(q)
                && self.hir.table.prop_key(*prop) == self.pt_key
                && self.pt_ordered(*coll)
            {
                by_coll.entry(*coll).or_default().push((*k, q));
            }
        }
        for v in by_coll.values_mut() {
            v.sort_unstable();
            v.dedup();
        }
        by_coll
    }

    // ORD: pt(C[i]) >= pt(C[j]) for i < j (mentioned indices, same C).
    // ALL three families are emitted through the `guarded` definedness
    // chokepoint (v2 Phase 2): each fact is asserted only where both
    // elements exist. Region atoms carry their own existence guards from
    // the SAME floors, so any pair a region can actually distinguish keeps
    // its ORD fact live — the guards cost no legitimate proof, and an
    // extension conflict on absent elements (review S3) is unrepresentable.
    fn ord(&mut self, qs: &[QuantityId]) {
        for (_, idx) in self.elem_pt_quantities(qs) {
            for a in 0..idx.len() {
                for b in a + 1..idx.len() {
                    let (i, qi) = idx[a];
                    let (j, qj) = idx[b];
                    if i < j {
                        let (f, g) = self.guarded(&[(1.0, qi), (-1.0, qj)], Rel::Ge, 0.0);
                        let d = format!("{g}{} >= {}", self.label(qi), self.label(qj));
                        self.push(AxiomId::Ord, f, d);
                    }
                }
            }
        }
        // Back-index ORD: a back element closer to the front has the higher
        // pt under pT-descending, i.e. `pt(C[-k2]) >= pt(C[-k1])` for k1 < k2
        // ([-1] is the last/lowest).
        for (_, idx) in self.elem_pt_back_quantities(qs) {
            for a in 0..idx.len() {
                for b in a + 1..idx.len() {
                    let (k1, q1) = idx[a];
                    let (k2, q2) = idx[b];
                    if k1 < k2 {
                        let (f, g) = self.guarded(&[(1.0, q2), (-1.0, q1)], Rel::Ge, 0.0);
                        let d = format!("{g}{} >= {}", self.label(q2), self.label(q1));
                        self.push(AxiomId::Ord, f, d);
                    }
                }
            }
        }
        // Front-to-back ORD: relate a front-indexed element to a back-indexed
        // one in the ONLY two cases where their relative order is fixed for
        // every collection size — `pt(C[i]) >= pt(C[-k])` holds iff `i == 0`
        // OR `k == 1`. When both exist the front sits at position `i`, the
        // back at `size-k`, and in these cases `i <= size-k` (equality when
        // they alias at `size = i+k`), so the front pt dominates. `i >= 1 &&
        // k >= 2` can straddle (at `size = max(i+1, k)` the front element
        // drops below the back one), so those stay omitted.
        let front = self.elem_pt_quantities(qs);
        let back = self.elem_pt_back_quantities(qs);
        for (coll, fidx) in &front {
            let Some(bidx) = back.get(coll) else { continue };
            let _ = coll;
            for &(i, qi) in fidx {
                for &(k, qk) in bidx {
                    if i == 0 || k == 1 {
                        let (f, g) = self.guarded(&[(1.0, qi), (-1.0, qk)], Rel::Ge, 0.0);
                        let d = format!("{g}{} >= {}", self.label(qi), self.label(qk));
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
    // Built via `derived_size_le` — the ONE encoding of a size-refinement
    // fact, shared with the engine's XSUB/XEQ emission and with the
    // formula-equality dedup that keeps XSUB from re-asserting a
    // SUB-covered pair.
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
            let f = derived_size_le(q, qp);
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
    // opaque external calls named exactly pt/m/mass/e/energy/dr/sqrt
    // (case-insensitive symbol key) are magnitudes of SOME particle
    // combination, hence >= 0 regardless of the (opaque) arguments.
    // `sqrt` is the non-negative real root by definition; the mass/pt/e of
    // a 4-vector sum (`mass(l1+l2)`) interns as the exact-name `mass`/`pt`/`e`
    // getter, so it inherits NNEG via the existing exact-name rule. `eta`/`phi`
    // of a sum are deliberately ABSENT (eta unbounded, phi convention-
    // dependent — a sign axiom there is the false-PROVEN trap, plan §risk 4).
    fn nneg(&mut self, qs: &[QuantityId]) {
        const NNEG_EXTFN_KEYS: [&str; 7] = ["pt", "m", "mass", "e", "energy", "dr", "sqrt"];
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

    // TRIG: -1 <= cos(x) <= 1 and -1 <= sin(x) <= 1 for opaque cos/sin calls
    // (P3). Universally true of any real argument; sound regardless of the
    // (opaque) argument. Only the bounded circular functions — tan/asin/etc.
    // are deliberately excluded (unbounded / domain-restricted).
    fn trig(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            let Quantity::ExternalFn { name, .. } = self.hir.table.quantity(q) else {
                continue;
            };
            let key = self.hir.symbols.key(*name);
            if key == "cos" || key == "sin" {
                let f = QFormula::And(vec![
                    Self::atom(&[(1.0, q)], Rel::Le, 1.0),
                    Self::atom(&[(1.0, q)], Rel::Ge, -1.0),
                ]);
                let d = format!("-1 <= {} <= 1", self.label(q));
                self.push(AxiomId::Trig, f, d);
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
                // Through the definedness chokepoint (v2 Phase 2): IDOM is
                // the axiom the unguarded F2B-ORD conflicted with (review
                // S3) — both sides of that class now state facts only where
                // the elements exist.
                let (f, g) = self.guarded(&[(1.0, qf), (-1.0, qp)], Rel::Le, 0.0);
                let d = format!("{g}{} <= {}", self.label(qf), self.label(qp));
                self.push(AxiomId::Idom, f, d);
            }
        }
    }

    // SZSLICE: 0 <= size(src[a:b]) <= size(src); also <= b - a for a
    // concrete upper bound b >= a. The `<= size(src) - a` clamped bound is
    // an ITE (deferred); these two are the unconditional linear subset.
    fn szslice(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            let Quantity::Size(c) = self.hir.table.quantity(q) else {
                continue;
            };
            let Collection::Slice { source, start, end } = *self.hir.table.collection(*c) else {
                continue;
            };
            // size(slice) <= size(src).
            let qsrc = self.hir.table.intern_quantity(Quantity::Size(source));
            let f = Self::atom(&[(1.0, q), (-1.0, qsrc)], Rel::Le, 0.0);
            let d = format!("{} <= {}", self.label(q), self.label(qsrc));
            self.push(AxiomId::Szslice, f, d);
            // size(slice) <= b - a, only when the window width is non-negative
            // (a concrete end >= start; an inverted window clamps to empty, so
            // `size <= negative` would be false — never emitted).
            if let Some(end) = end
                && end >= start
            {
                let width = f64::from(end - start);
                let f = Self::atom(&[(1.0, q)], Rel::Le, width);
                let d = format!("{} <= {}", self.label(q), end - start);
                self.push(AxiomId::Szslice, f, d);
            }
        }
    }

    // SZPERM: size(sort(C, key, dir)) = size(C). A permutation is a bijection,
    // so it preserves cardinality for ANY key/direction. (The descending-pT
    // sort of a pT-descending source is already aliased to the source at
    // resolve time, so it never reaches here as a distinct `Sorted`; this
    // covers the opaque non-pT / ascending / union-rooted sorts.)
    fn szperm(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            let Quantity::Size(c) = self.hir.table.quantity(q) else {
                continue;
            };
            let Collection::Sorted { source, .. } = *self.hir.table.collection(*c) else {
                continue;
            };
            let qsrc = self.hir.table.intern_quantity(Quantity::Size(source));
            let f = Self::atom(&[(1.0, q), (-1.0, qsrc)], Rel::Eq, 0.0);
            let d = format!("{} = {}", self.label(q), self.label(qsrc));
            self.push(AxiomId::Szperm, f, d);
        }
    }

    // COMBSIZE: tuple-combinatorics size facts (plan §risk 2, USER ANSWER 4).
    //
    // (a) COMB-MEMBER-SIZE: size(K->axis) = size(K). A projection keeps exactly
    //     one element per surviving tuple (a bijection onto the tuples), so the
    //     count is identical for any axis (member or candidate). Sound even when
    //     the combination has per-tuple cuts (the projection is over the SAME
    //     post-cut tuple set).
    //
    // (b) On the combination size itself, the SOUND existence facts:
    //     - size(K) >= 0 always;
    //     - any source part empty  => size(K) = 0   (no tuples to form at all,
    //       independent of cuts);
    //     - same-source disjoint over C with size(C) < 2 => size(K) = 0
    //       (a strictly-increasing pair needs two source positions);
    //     - all parts non-empty => size(K) >= 1 ONLY for a cuts-free
    //       cartesian / cross-source-disjoint combination. The same-source
    //       disjoint POSITIVE lower bound is DELIBERATELY OMITTED (USER ANSWER
    //       4: two kinematically value-equal elements form zero pairs), and the
    //       lower bound is suppressed whenever the combination carries cuts (a
    //       per-tuple filter can drop every tuple).
    fn comb_size(&mut self, qs: &[QuantityId]) {
        for &q in qs {
            let Quantity::Size(c) = *self.hir.table.quantity(q) else {
                continue;
            };
            match self.hir.table.collection(c).clone() {
                // (a) projection size = combination size.
                Collection::CombProject { comb, axis } => {
                    let qcomb = self.hir.table.intern_quantity(Quantity::Size(comb));
                    let f = Self::atom(&[(1.0, q), (-1.0, qcomb)], Rel::Eq, 0.0);
                    let axis_label = match axis {
                        CombAxis::Member(s) | CombAxis::Candidate(s) => {
                            self.hir.symbols.display(s).to_owned()
                        }
                    };
                    let d = format!("{} = {} (->{axis_label})", self.label(q), self.label(qcomb));
                    self.push(AxiomId::CombSize, f, d);
                }
                Collection::Combination {
                    parts,
                    kind,
                    cuts,
                    candidate,
                    ..
                } => {
                    // size(K) >= 0.
                    let f = Self::atom(&[(1.0, q)], Rel::Ge, 0.0);
                    let d = format!("{} >= 0", self.label(q));
                    self.push(AxiomId::CombSize, f, d);

                    let part_sizes: Vec<QuantityId> = parts
                        .iter()
                        .map(|&p| self.hir.table.intern_quantity(Quantity::Size(p)))
                        .collect();
                    // Same-source disjoint over >= 2 binders: a strictly-
                    // increasing index tuple needs >= 2 distinct source
                    // positions, so a source with < 2 elements forms no tuple.
                    // (A single-binder "disjoint" is degenerate — one tuple per
                    // element — so the < 2 zero fact would be unsound; it falls
                    // to the `else` per-factor-empty handling instead.)
                    let same_source = parts.len() >= 2 && parts.windows(2).all(|w| w[0] == w[1]);

                    if kind == CombKind::Disjoint && same_source {
                        // size(C) < 2  =>  size(K) = 0, i.e. size(C) >= 2 OR
                        // size(K) = 0. (size(C) is an integer; < 2 ⇔ <= 1.)
                        let qc = part_sizes[0];
                        let guard = Self::atom(&[(1.0, qc)], Rel::Ge, 2.0);
                        let empty = Self::atom(&[(1.0, q)], Rel::Eq, 0.0);
                        let f = QFormula::Or(vec![guard, empty]);
                        let d = format!("{} < 2 => {} = 0", self.label(qc), self.label(q));
                        self.push(AxiomId::CombSize, f, d);
                    } else {
                        // Cross-source / cartesian: any factor empty => size = 0.
                        // size(part_i) = 0  =>  size(K) = 0, encoded as
                        // size(part_i) >= 1  OR  size(K) = 0 for each factor.
                        for &qp in &part_sizes {
                            let guard = Self::atom(&[(1.0, qp)], Rel::Ge, 1.0);
                            let empty = Self::atom(&[(1.0, q)], Rel::Eq, 0.0);
                            let f = QFormula::Or(vec![guard, empty]);
                            let d = format!("{} = 0 => {} = 0", self.label(qp), self.label(q));
                            self.push(AxiomId::CombSize, f, d);
                        }
                        // all-parts-nonempty => size(K) >= 1, ONLY for a bare
                        // CARTESIAN (kind == Cartesian) with no per-tuple cuts (a
                        // cut can drop every tuple), no candidate (a candidate
                        // whose 4-vector is a soft non-value — a missing
                        // mass/property — drops its tuple), and at least one part.
                        // A cartesian product applies NO value-distinctness drop,
                        // so non-empty factors guarantee >= 1 surviving tuple. A
                        // cross-source DISJOINT is EXCLUDED: USER ANSWER 4's
                        // value-distinctness can drop the sole pair (two
                        // kinematically value-equal elements across the sources
                        // form 0 pairs), so its lower bound is unsound.
                        if kind == CombKind::Cartesian
                            && cuts.is_empty()
                            && candidate.is_none()
                            && !part_sizes.is_empty()
                        {
                            // ¬(∀i size(part_i) >= 1)  ∨  size(K) >= 1, i.e.
                            // ⋁ᵢ size(part_i) <= 0  ∨  size(K) >= 1.
                            let mut alts: Vec<QFormula> = part_sizes
                                .iter()
                                .map(|&qp| Self::atom(&[(1.0, qp)], Rel::Le, 0.0))
                                .collect();
                            alts.push(Self::atom(&[(1.0, q)], Rel::Ge, 1.0));
                            let parts_lbl = part_sizes
                                .iter()
                                .map(|&qp| self.label(qp))
                                .collect::<Vec<_>>()
                                .join(", ");
                            let d = format!("all of [{parts_lbl}] >= 1 => {} >= 1", self.label(q));
                            self.push(AxiomId::CombSize, QFormula::Or(alts), d);
                        }
                    }
                }
                _ => {}
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

/// Reserved element index used ONLY by cross-collection reconciliation: it
/// grounds every element-self property of a filter predicate onto ONE generic
/// element `base[GENERIC_INDEX]`, so `pt`/`eta`/`btag` intern to the SAME
/// [`QuantityId`] across two different predicates over the same base.
/// Unreachable from source by construction: every resolver/encoder path that
/// derives a front index clamps to [`adl_sema::MAX_SOURCE_ELEM_INDEX`]
/// (strictly below), so no real element access, EPRED, or OPEN1 expansion can
/// intern this id — the generic element provably carries no per-element
/// axioms in the subset frames.
pub const GENERIC_INDEX: u32 = u32::MAX;
const _: () = assert!(
    GENERIC_INDEX > adl_sema::MAX_SOURCE_ELEM_INDEX,
    "the generic-element sentinel must sit above every source-reachable index"
);

/// Encode a single-element filter predicate onto the generic element
/// `base[index]` (pass `index = GENERIC_INDEX`) as an EXACT three-valued
/// [`Formula`]. Opaque / non-linear conjuncts become [`Formula::Unknown`]
/// (`.under()` → false, `.over()` → true) — NEVER dropped, NEVER defaulted to
/// true — so the FULL superset predicate is honoured on the `.under()` side.
/// Returns `None` (fail-closed: the whole reconciliation pair is NO-RELATION)
/// only when the predicate references a residual composite binder / reduce
/// element, which would make grounding onto one generic element unsound.
///
/// Unlike [`encode_elem_pred`] (which drops un-encodable top-level conjuncts —
/// sound for EPRED's over-weakening, UNSOUND for a superset), this recurses
/// `And`/`Or`/`Not` at the [`Formula`] level so only opaque *leaves* become
/// `Unknown`.
#[must_use]
pub fn encode_elem_pred_generic(
    table: &mut QuantityTable,
    node: &HNode,
    base: CollectionId,
    index: u32,
    diags: &mut DiagTable,
) -> Option<Formula> {
    if references_binder_or_reduce(table, node) || references_concrete_peer(table, node) {
        return None;
    }
    Some(encode_pred_formula(table, node, base, index, diags))
}

/// True if `node` references a CONCRETE peer element of the base — an element
/// at a fixed index (`Jet[1]`), an angular separation between such, or a
/// collection size. The generic-element grounding is sound ONLY for the
/// self-element (which enters as [`HKind::ElemSelfProp`], grounded to the
/// generic index) plus constants and opaque leaves. A concrete peer instead
/// keeps its SHARED analysis quantity id, and the base-frame ORD/IDOM/TWIN/
/// TAG/EPRED and size axioms constraining that id hold only under a `size > k`
/// guard the subset frame never replays; leaking one in fabricates a
/// refinement (e.g. `select pt > pt(Jet[1])` + ORD ⇒ a false `size <=` fact).
/// Reconciliation must fail closed. Opaque externals are NOT peers: they
/// intern to a free quantity (no guarded axiom), so they stay live as Unknown.
fn references_concrete_peer(table: &QuantityTable, node: &HNode) -> bool {
    if let HKind::Quantity(q) = &node.kind {
        match table.quantity(*q) {
            Quantity::ElemProp { .. } | Quantity::AngularSep { .. } | Quantity::Size(_) => {
                return true;
            }
            _ => {}
        }
    }
    node.children()
        .iter()
        .any(|c| references_concrete_peer(table, c))
}

fn encode_pred_formula(
    table: &mut QuantityTable,
    node: &HNode,
    base: CollectionId,
    index: u32,
    diags: &mut DiagTable,
) -> Formula {
    if matches!(node.tag, Fragment::Unsupported(_)) {
        return Formula::Unknown(diags.push(node.span, "opaque element-predicate conjunct"));
    }
    match &node.kind {
        HKind::Bool(b) => {
            if *b {
                Formula::True
            } else {
                Formula::False
            }
        }
        HKind::And(v) => Formula::And(
            v.iter()
                .map(|p| encode_pred_formula(table, p, base, index, diags))
                .collect(),
        ),
        HKind::Or(v) => Formula::Or(
            v.iter()
                .map(|p| encode_pred_formula(table, p, base, index, diags))
                .collect(),
        ),
        HKind::Not(a) => encode_pred_formula(table, a, base, index, diags).not(),
        // Bottom out at the leaves so one opaque conjunct becomes `Unknown`
        // without collapsing the whole predicate (encode_pred_exact returns
        // None if ANY nested leaf is opaque).
        HKind::Cmp { .. } | HKind::Band { .. } => match encode_pred_exact(table, node, base, index)
        {
            Some(qf) => lift_qformula(qf),
            None => Formula::Unknown(diags.push(
                node.span,
                "non-linear or opaque comparison in element predicate",
            )),
        },
        _ => Formula::Unknown(diags.push(node.span, "unsupported element-predicate shape")),
    }
}

/// Lift an [`QFormula`] (Unknown/Dual-free by type) into a [`Formula`]. Total
/// and exact.
fn lift_qformula(q: QFormula) -> Formula {
    match q {
        QFormula::True => Formula::True,
        QFormula::False => Formula::False,
        QFormula::Atom(a) => Formula::Atom(a),
        QFormula::And(v) => Formula::And(v.into_iter().map(lift_qformula).collect()),
        QFormula::Or(v) => Formula::Or(v.into_iter().map(lift_qformula).collect()),
    }
}

/// True if any quantity in `node` references a composite binder / reduce /
/// this-element particle: such a predicate does not ground onto one generic
/// element, so reconciliation must fail closed (NO-RELATION).
fn references_binder_or_reduce(table: &QuantityTable, node: &HNode) -> bool {
    fn p_bad(p: &ParticleRef) -> bool {
        match p {
            ParticleRef::Binder { .. } | ParticleRef::ReduceElem | ParticleRef::ThisElem => true,
            ParticleRef::Sum(parts) => parts.iter().any(p_bad),
            _ => false,
        }
    }
    if let HKind::Quantity(q) = &node.kind {
        match table.quantity(*q) {
            Quantity::AngularSep { a, b, .. } => {
                if p_bad(a) || p_bad(b) {
                    return true;
                }
            }
            Quantity::ExternalFn { args, .. } => {
                if args
                    .iter()
                    .any(|a| matches!(a, QuantityArg::Particle(p) if p_bad(p)))
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    node.children()
        .iter()
        .any(|c| references_binder_or_reduce(table, c))
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
            let rel = match op {
                adl_syntax::ast::CmpOp::Gt => Rel::Gt,
                adl_syntax::ast::CmpOp::Lt => Rel::Lt,
                adl_syntax::ast::CmpOp::Ge => Rel::Ge,
                adl_syntax::ast::CmpOp::Le => Rel::Le,
                adl_syntax::ast::CmpOp::Eq => Rel::Eq,
                adl_syntax::ast::CmpOp::Ne | adl_syntax::ast::CmpOp::ApproxEq => Rel::Ne,
            };
            // A `var / const` ratio side cannot be linearized by `lin_pred`
            // (folding the f64 reciprocal asserts a too-strong predicate).
            // Clear the constant denominator EXACTLY at the comparison level,
            // exactly as the main encoder's `ratio()` does.
            if let Some(a) = clear_ratio(table, lhs, rhs, rel, coll, index) {
                return Some(a);
            }
            if let Some(a) = clear_ratio(table, rhs, lhs, rel.flipped(), coll, index) {
                return Some(a);
            }
            // `|E| ⋈ c` — the exact two-sided expansion the main encoder's
            // `abs_cmp` uses, mirrored at the element-predicate level so the
            // corpus-universal `abs(eta) < 2.4` object cut encodes exactly
            // for EPRED and reconciliation instead of failing opaque (the
            // dominant reason cross-analysis reconciliation stayed sparse).
            if let HKind::Abs(inner) = &lhs.kind
                && let Some(r) = lin_pred(table, rhs, coll, index)
                && r.terms.is_empty()
            {
                return abs_pred(table, inner, rel, &r.k, coll, index);
            }
            if let HKind::Abs(inner) = &rhs.kind
                && let Some(l) = lin_pred(table, lhs, coll, index)
                && l.terms.is_empty()
            {
                return abs_pred(table, inner, rel.flipped(), &l.k, coll, index);
            }
            let l = lin_pred(table, lhs, coll, index)?;
            let r = lin_pred(table, rhs, coll, index)?;
            let diff = l.sub(&r);
            let k = -&diff.k;
            Some(lin_atom(diff.terms, rel, k))
        }
        HKind::Band { kind, expr, lo, hi } => {
            let e = lin_pred(table, expr, coll, index)?;
            let lo = lo.parse::<f64>().ok().and_then(Rat::from_decimal_f64)?;
            let hi = hi.parse::<f64>().ok().and_then(Rat::from_decimal_f64)?;
            let (lo_rel, hi_rel, combine_and) = match kind {
                adl_syntax::ast::BandKind::In => (Rel::Ge, Rel::Le, true),
                adl_syntax::ast::BandKind::Out => (Rel::Le, Rel::Ge, false),
            };
            let lo_k = &lo - &e.k;
            let hi_k = &hi - &e.k;
            let lo_b = lin_atom(e.terms.clone(), lo_rel, lo_k);
            let hi_b = lin_atom(e.terms, hi_rel, hi_k);
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
    terms: BTreeMap<QuantityId, Rat>,
    k: Rat,
}

impl PredLin {
    fn constant(k: Rat) -> Self {
        Self {
            terms: BTreeMap::new(),
            k,
        }
    }

    fn sub(mut self, other: &Self) -> Self {
        self.k = &self.k - &other.k;
        for (q, c) in &other.terms {
            let entry = self.terms.entry(*q).or_insert_with(Rat::zero);
            *entry = &*entry - c;
        }
        self
    }

    fn scale(mut self, c: &Rat) -> Self {
        self.k = &self.k * c;
        for v in self.terms.values_mut() {
            *v = &*v * c;
        }
        self
    }
}

/// `|E| ⋈ c` over the implicit element, expanded exactly as the main
/// encoder's `abs_cmp` (adl-formula) — the two implementations MUST agree,
/// pinned by `abs_expansion_agrees_with_ground_truth` (the same 252-cell
/// truth table both are correct against). A comparison against a
/// negative constant is itself constant (`|E| >= 0`): without that fold,
/// `|E| == c` (c<0) would encode SAT and `|E| != c` (c<0) as a two-point
/// exclusion — both false-PROVEN factories. For `c >= 0` the expansion is
/// the exact two-sided form with E's own constant `k` folded into the
/// bounds.
fn abs_pred(
    table: &mut QuantityTable,
    inner: &HNode,
    rel: Rel,
    c: &Rat,
    coll: CollectionId,
    index: u32,
) -> Option<QFormula> {
    if c.is_negative() {
        return Some(match rel {
            Rel::Lt | Rel::Le | Rel::Eq => QFormula::False,
            Rel::Gt | Rel::Ge | Rel::Ne => QFormula::True,
        });
    }
    let e = lin_pred(table, inner, coll, index)?;
    let hi = c - &e.k;
    let lo = &(-c) - &e.k;
    let bound = |r: Rel, k: &Rat| lin_atom(e.terms.clone(), r, k.clone());
    Some(match rel {
        Rel::Lt => QFormula::And(vec![bound(Rel::Lt, &hi), bound(Rel::Gt, &lo)]),
        Rel::Le => QFormula::And(vec![bound(Rel::Le, &hi), bound(Rel::Ge, &lo)]),
        Rel::Gt => QFormula::Or(vec![bound(Rel::Gt, &hi), bound(Rel::Lt, &lo)]),
        Rel::Ge => QFormula::Or(vec![bound(Rel::Ge, &hi), bound(Rel::Le, &lo)]),
        Rel::Eq => QFormula::Or(vec![bound(Rel::Eq, &hi), bound(Rel::Eq, &lo)]),
        Rel::Ne => QFormula::And(vec![bound(Rel::Ne, &hi), bound(Rel::Ne, &lo)]),
    })
}

fn lin_atom(terms: BTreeMap<QuantityId, Rat>, rel: Rel, k: Rat) -> QFormula {
    if terms.is_empty() {
        return if rel.eval(&Rat::zero(), &k) {
            QFormula::True
        } else {
            QFormula::False
        };
    }
    QFormula::Atom(LinAtom::new(terms.into_iter().map(|(q, c)| (c, q)), rel, k))
}

/// Encode `ratio_side ⋈ other_side` when `ratio_side` is `num / den` with a
/// constant denominator, by clearing the denominator with EXACT coefficients
/// — `num ⋈ other·den` (`den > 0`) or `num ⋈̄ other·den` (`den < 0`) — instead
/// of folding the inexact f64 reciprocal `1/den` (which would assert an EPRED
/// predicate stronger than the truth). Mirrors `adl-formula`'s `ratio()`.
/// Returns `None` when the shape does not match (a non-`Div` side, a
/// non-constant denominator, or a non-linear operand), leaving the caller's
/// generic path to drop the conjunct (a sound EPRED weakening).
fn clear_ratio(
    table: &mut QuantityTable,
    ratio_side: &HNode,
    other_side: &HNode,
    rel: Rel,
    coll: CollectionId,
    index: u32,
) -> Option<QFormula> {
    let HKind::Binary {
        op: adl_sema::ArithOp::Div,
        lhs: num,
        rhs: den,
    } = &ratio_side.kind
    else {
        return None;
    };
    let d = lin_pred(table, den, coll, index)?;
    if !d.terms.is_empty() {
        return None; // non-constant denominator: out of the linear fragment
    }
    if d.k.is_zero() {
        // `x / 0` is never a member (the interpreter's comparison is false).
        return Some(QFormula::False);
    }
    let l = lin_pred(table, num, coll, index)?;
    let r = lin_pred(table, other_side, coll, index)?;
    // L/d ⋈ R  ⇔  L ⋈ R·d  (relation flips when d < 0).
    let rd = r.scale(&d.k);
    let e = l.sub(&rd);
    let rel = if d.k.is_negative() {
        rel.flipped()
    } else {
        rel
    };
    let k = -&e.k;
    Some(lin_atom(e.terms, rel, k))
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
        HKind::Num(s) => s
            .parse::<f64>()
            .ok()
            .and_then(Rat::from_decimal_f64)
            .map(PredLin::constant),
        HKind::ElemSelfProp(prop) => {
            let q = table.intern_quantity(Quantity::ElemProp {
                coll,
                index: ElemIndex::FromFront(index),
                prop: *prop,
            });
            Some(PredLin {
                terms: BTreeMap::from([(q, Rat::one())]),
                k: Rat::zero(),
            })
        }
        HKind::Quantity(q) => {
            // Stopgap net until structural element keys (plan Phase 3): an
            // opaque external whose args carry element-context text — an
            // unsupported render, a `this.`/`@elem.` self-property, or any
            // `@`-scoped leaf — interns to a quantity that silently aliases
            // physically distinct per-element values across frames it does not
            // belong to (soundness review S2). Reject those so they fall to the
            // caller's opaque handling (EPRED drops the conjunct, reconciliation
            // lifts it to Unknown). Externals over RESOLVED args (aplanarity
            // over a Whole-collection Particle) carry no such string and still
            // encode. The primary fix lands in adl-sema.
            if let Quantity::ExternalFn { args, .. } = table.quantity(*q)
                && args.iter().any(|a| {
                    matches!(a, QuantityArg::Opaque(s)
                        if s.contains("<unsupported:")
                            || s.starts_with("this.")
                            || s.starts_with("@elem.")
                            || s.contains('@'))
                })
            {
                return None;
            }
            Some(PredLin {
                terms: BTreeMap::from([(*q, Rat::one())]),
                k: Rat::zero(),
            })
        }
        HKind::Neg(a) => Some(lin_pred(table, a, coll, index)?.scale(&Rat::from_i64(-1))),
        HKind::Binary { op, lhs, rhs } => {
            let l = lin_pred(table, lhs, coll, index)?;
            let r = lin_pred(table, rhs, coll, index)?;
            match op {
                adl_sema::ArithOp::Add => {
                    let mut out = l;
                    out.k = &out.k + &r.k;
                    for (q, c) in r.terms {
                        let entry = out.terms.entry(q).or_insert_with(Rat::zero);
                        *entry = &*entry + &c;
                    }
                    Some(out)
                }
                adl_sema::ArithOp::Sub => Some(l.sub(&r)),
                adl_sema::ArithOp::Mul => {
                    if l.terms.is_empty() {
                        Some(r.scale(&l.k))
                    } else if r.terms.is_empty() {
                        Some(l.scale(&r.k))
                    } else {
                        None
                    }
                }
                adl_sema::ArithOp::Div => {
                    if !r.terms.is_empty() || r.k.is_zero() {
                        // Non-constant or zero denominator: not linear here.
                        return None;
                    }
                    if l.terms.is_empty() {
                        // Constant / constant: exact rational division.
                        return l.k.checked_div(&r.k).map(PredLin::constant);
                    }
                    // var / const: deferred to the comparison level
                    // (`clear_ratio`), where the denominator is cleared with
                    // EXACT coefficients (folding `scale(1/d)` would assert an
                    // EPRED predicate stronger than the truth — false-PROVEN).
                    None
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
    use adl_sema::analyze_str;

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

    fn emit_ord(src: &str) -> (Hir, AxiomSet) {
        let ext = ExtDecls::legacy();
        let mut hir = analyze_str(src, "ord_guard.adl", &ext);
        assert!(
            !adl_syntax::diag::has_errors(&hir.diags),
            "source must resolve cleanly: {:#?}",
            hir.diags
        );
        let n = hir.table.quantities().len();
        let qs: BTreeSet<QuantityId> = (0..n)
            .map(|i| QuantityId(u32::try_from(i).unwrap()))
            .collect();
        let axioms = emit_axioms(&mut hir, &ext, &qs);
        (hir, axioms)
    }

    fn is_size_guard(hir: &Hir, f: &QFormula) -> bool {
        matches!(f, QFormula::Atom(a)
            if a.rel() == Rel::Le
                && a.terms().len() == 1
                && matches!(hir.table.quantity(a.terms()[0].1), Quantity::Size(_)))
    }

    // Front indices 0,1 and back indices 1,2 over a pT-ordered filtered
    // collection: exercises all three ORD families in one emission.
    const ORD_SRC: &str = "\
object jets
  take Jet
  select pT > 30

region SR
  select jets[0].pT > 0
  select jets[1].pT > 0
  select jets[-1].pT > 0
  select jets[-2].pT > 0
";

    #[test]
    fn front_to_back_i1_k1_carries_size_guard() {
        let (hir, axioms) = emit_ord(ORD_SRC);
        let inst = axioms
            .instances
            .iter()
            .find(|i| {
                i.id == AxiomId::Ord
                    && i.description.contains("jets[1]")
                    && i.description.contains("jets[-1]")
            })
            .expect("guarded front-to-back jets[1] >= jets[-1] must be emitted");
        let QFormula::Or(arms) = &inst.formula else {
            panic!(
                "F2B ORD (k==1, i>=1) must be guarded (Or): {:?}",
                inst.formula
            );
        };
        assert_eq!(arms.len(), 2, "{:?}", inst.formula);
        assert!(
            is_size_guard(&hir, &arms[0]),
            "first arm must be the `size(jets) <= i` guard: {:?}",
            arms[0]
        );
    }

    #[test]
    fn back_back_ord_carries_size_guard() {
        let (hir, axioms) = emit_ord(ORD_SRC);
        let inst = axioms
            .instances
            .iter()
            .find(|i| {
                i.id == AxiomId::Ord
                    && i.description.contains("jets[-2]")
                    && i.description.contains("jets[-1]")
            })
            .expect("back-back jets[-2] >= jets[-1] must be emitted");
        let QFormula::Or(arms) = &inst.formula else {
            panic!("back-back ORD must be guarded (Or): {:?}", inst.formula);
        };
        assert_eq!(arms.len(), 2, "{:?}", inst.formula);
        assert!(
            is_size_guard(&hir, &arms[0]),
            "first arm must be the `size(jets) <= k2-1` guard: {:?}",
            arms[0]
        );
    }

    #[test]
    fn front_to_back_i0_is_guarded_like_every_element_fact() {
        // v2 Phase 2: EVERY element fact goes through the definedness
        // chokepoint — including i==0, which was previously left bare on the
        // pad-0-consistency argument. Uniform guarding costs no legitimate
        // proof (region atoms carry the same existence floors, so any pair a
        // region can distinguish keeps the fact live) and makes unguarded
        // element facts unrepresentable.
        let (hir, axioms) = emit_ord(ORD_SRC);
        let inst = axioms
            .instances
            .iter()
            .find(|i| {
                i.id == AxiomId::Ord
                    && i.description.contains("jets[0]")
                    && i.description.contains("jets[-1]")
            })
            .expect("front-to-back i==0 jets[0] >= jets[-1] must be emitted");
        let QFormula::Or(parts) = &inst.formula else {
            panic!("i==0 front-to-back must be guarded: {:?}", inst.formula);
        };
        assert!(
            parts.iter().any(|p| is_size_guard(&hir, p)),
            "one branch must be the existence guard: {:?}",
            inst.formula
        );
        assert!(
            inst.description.contains('⇒'),
            "description states the guard: {}",
            inst.description
        );
    }

    #[test]
    fn object_cut_over_collapsed_external_yields_no_epred_conjunct() {
        // `sqrt(this.pT)` is an opaque external over the element's OWN pt; it
        // carries per-element context that cannot become a shared solver
        // variable, so it must never enter an EPRED conjunct. The linear
        // `pT > 30` sibling still encodes — only the collapsed external dies.
        let src = "\
object jets
  take Jet
  select pT > 30
  select sqrt(this.pT) > 5

region SR
  select jets[0].pT > 0
";
        let (hir, axioms) = emit_ord(src);
        let epreds: Vec<_> = axioms
            .instances
            .iter()
            .filter(|i| i.id == AxiomId::Epred)
            .collect();
        assert!(!epreds.is_empty(), "jets is filtered and referenced at [0]");
        for inst in epreds {
            let mut qset = BTreeSet::new();
            collect_quantities(&inst.formula, &mut qset);
            for q in qset {
                assert!(
                    !matches!(hir.table.quantity(q), Quantity::ExternalFn { .. }),
                    "sqrt(this.pT) must not leak into an EPRED conjunct: {:?}",
                    inst.formula
                );
            }
        }
    }
}

#[cfg(test)]
mod recon_encoder_tests {
    use super::*;
    use adl_sema::Symbol;
    use adl_syntax::ast::CmpOp;
    use adl_syntax::span::Span;

    #[test]
    fn bool_leaves_encode_exactly() {
        let mut t = QuantityTable::default();
        let mut d = DiagTable::default();
        let base = t.intern_collection(Collection::Base(Symbol(0)));
        let yes = HNode::new(HKind::Bool(true), Span::default());
        let no = HNode::new(HKind::Bool(false), Span::default());
        assert_eq!(
            encode_elem_pred_generic(&mut t, &yes, base, GENERIC_INDEX, &mut d),
            Some(Formula::True)
        );
        assert_eq!(
            encode_elem_pred_generic(&mut t, &no, base, GENERIC_INDEX, &mut d),
            Some(Formula::False)
        );
    }

    #[test]
    fn opaque_conjunct_becomes_unknown_not_dropped() {
        let mut t = QuantityTable::default();
        let mut d = DiagTable::default();
        let base = t.intern_collection(Collection::Base(Symbol(0)));
        let node = HNode::new(
            HKind::And(vec![
                HNode::new(HKind::Bool(true), Span::default()),
                HNode::unsupported(Span::default(), "opaque cut"),
            ]),
            Span::default(),
        );
        let f = encode_elem_pred_generic(&mut t, &node, base, GENERIC_INDEX, &mut d).unwrap();
        // The opaque conjunct survives as Unknown (never dropped, never true),
        // so `.under()` keeps a superset predicate honest.
        assert!(
            !f.is_exact(),
            "opaque conjunct must survive as Unknown: {f:?}"
        );
        match f {
            Formula::And(v) => {
                assert_eq!(v.len(), 2);
                assert!(matches!(v[1], Formula::Unknown(_)));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn residual_binder_fails_closed_to_none() {
        let mut t = QuantityTable::default();
        let mut d = DiagTable::default();
        let base = t.intern_collection(Collection::Base(Symbol(0)));
        // dR(binder, MET): a composite-binder particle cannot ground onto one
        // generic element, so the whole pair must be NO-RELATION (None).
        let q = t.intern_quantity(Quantity::AngularSep {
            kind: AngKind::DR,
            a: ParticleRef::Binder {
                coll: base,
                name: Symbol(1),
            },
            b: ParticleRef::Met,
            oriented: false,
        });
        let node = HNode::new(
            HKind::Cmp {
                op: CmpOp::Gt,
                lhs: Box::new(HNode::new(HKind::Quantity(q), Span::default())),
                rhs: Box::new(HNode::new(HKind::Num("0".to_owned()), Span::default())),
            },
            Span::default(),
        );
        assert_eq!(
            encode_elem_pred_generic(&mut t, &node, base, GENERIC_INDEX, &mut d),
            None
        );
    }

    #[test]
    fn plain_node_is_not_flagged_as_binder() {
        let t = QuantityTable::default();
        let node = HNode::new(HKind::Bool(true), Span::default());
        assert!(!references_binder_or_reduce(&t, &node));
    }

    // Stopgap identity net (review S2): an external whose opaque arg carries
    // element-context text may not enter a fact as a shared linear term.
    #[test]
    fn lin_pred_rejects_element_context_leaked_external() {
        let mut t = QuantityTable::default();
        let base = t.intern_collection(Collection::Base(Symbol(0)));
        let leaf = |t: &mut QuantityTable, name: u32, arg: QuantityArg| {
            let q = t.intern_quantity(Quantity::ExternalFn {
                name: Symbol(name),
                args: vec![arg],
            });
            HNode::new(HKind::Quantity(q), Span::default())
        };

        // `@elem.`, `this.`, an `@`-scoped leak, and an `<unsupported:` render
        // all die.
        for (name, s) in [
            (1u32, "@elem.pt"),
            (2, "this.pt"),
            (3, "phi@2"),
            (4, "<unsupported: unresolved `x`>"),
        ] {
            let node = leaf(&mut t, name, QuantityArg::Opaque(s.to_owned()));
            assert!(
                lin_pred(&mut t, &node, base, 0).is_none(),
                "leaked arg {s:?} must be rejected"
            );
        }

        // A RESOLVED-arg external (opaque scalar over a Whole collection) and a
        // benign opaque path string still encode — only leaked context dies.
        let resolved = leaf(&mut t, 5, QuantityArg::Collection(base));
        assert!(lin_pred(&mut t, &resolved, base, 0).is_some());
        let benign = leaf(&mut t, 6, QuantityArg::Opaque("weights.xml".to_owned()));
        assert!(lin_pred(&mut t, &benign, base, 0).is_some());
    }
}

#[cfg(test)]
mod abs_pred_tests {
    //! The `|E| ⋈ c` element-level expansion (the abs unlock): exactness,
    //! agreement with f64 ground truth on a boundary grid, the negative-c
    //! constant folds (each a false-PROVEN factory if omitted), and the
    //! flipped `c ⋈ |E|` orientation.

    use super::*;
    use adl_sema::Symbol;
    use adl_syntax::ast::CmpOp;
    use adl_syntax::span::Span;

    fn abs_cmp_node(t: &mut QuantityTable, op: CmpOp, c: &str, flipped: bool) -> HNode {
        let p = t.intern_prop("etaof", "eta");
        let abs = HNode::new(
            HKind::Abs(Box::new(HNode::new(
                HKind::ElemSelfProp(p),
                Span::default(),
            ))),
            Span::default(),
        );
        let num = HNode::new(HKind::Num(c.to_owned()), Span::default());
        let (lhs, rhs) = if flipped { (num, abs) } else { (abs, num) };
        HNode::new(
            HKind::Cmp {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            Span::default(),
        )
    }

    fn eval_q(f: &QFormula, q: QuantityId, v: f64) -> bool {
        match f {
            QFormula::True => true,
            QFormula::False => false,
            QFormula::And(parts) => parts.iter().all(|p| eval_q(p, q, v)),
            QFormula::Or(parts) => parts.iter().any(|p| eval_q(p, q, v)),
            QFormula::Atom(a) => {
                assert_eq!(a.terms().len(), 1, "single-quantity atoms only");
                assert_eq!(a.terms()[0].1, q, "one shared eta quantity expected");
                let lhs = a.terms()[0].0.to_f64() * v;
                let k = a.constant().to_f64();
                match a.rel() {
                    Rel::Lt => lhs < k,
                    Rel::Le => lhs <= k,
                    Rel::Gt => lhs > k,
                    Rel::Ge => lhs >= k,
                    Rel::Eq => lhs == k,
                    Rel::Ne => lhs != k,
                }
            }
        }
    }

    /// Every relation, positive/zero/negative constants, over a boundary
    /// value grid: the encoded formula must agree with f64 `abs` ground
    /// truth pointwise (the values are dyadic-exact or shared literals, so
    /// f64 comparison is exact here).
    #[test]
    fn abs_expansion_agrees_with_ground_truth() {
        let ops = [
            CmpOp::Lt,
            CmpOp::Le,
            CmpOp::Gt,
            CmpOp::Ge,
            CmpOp::Eq,
            CmpOp::Ne,
        ];
        let rels = [Rel::Lt, Rel::Le, Rel::Gt, Rel::Ge, Rel::Eq, Rel::Ne];
        let values: [f64; 7] = [-3.0, -2.4, -1.0, 0.0, 1.0, 2.4, 3.0];
        for (op, rel) in ops.iter().zip(rels) {
            for (c_txt, c_val) in [("2.4", 2.4), ("0", 0.0), ("-1", -1.0)] {
                for flipped in [false, true] {
                    let mut t = QuantityTable::default();
                    let base = t.intern_collection(Collection::Base(Symbol(0)));
                    let node = abs_cmp_node(&mut t, *op, c_txt, flipped);
                    let f = encode_pred_exact(&mut t, &node, base, 0)
                        .unwrap_or_else(|| panic!("abs({op:?}, {c_txt}) must encode"));
                    // The eta ElemProp is whatever lin_pred interned; read it
                    // off the formula itself (constant folds carry no atom).
                    fn first_q(f: &QFormula) -> Option<QuantityId> {
                        match f {
                            QFormula::Atom(a) => Some(a.terms()[0].1),
                            QFormula::And(v) | QFormula::Or(v) => v.iter().find_map(first_q),
                            _ => None,
                        }
                    }
                    let q = first_q(&f).unwrap_or(QuantityId(0));
                    for v in values {
                        // `2.4` as a literal and as a bound share the exact
                        // rational, so f64 equality is faithful on this grid.
                        // The node is `|E| rel c` — or `c rel |E|` when
                        // flipped, whose truth swaps the operand order.
                        let (a, b) = if flipped {
                            (c_val, v.abs())
                        } else {
                            (v.abs(), c_val)
                        };
                        let truth = match rel {
                            Rel::Lt => a < b,
                            Rel::Le => a <= b,
                            Rel::Gt => a > b,
                            Rel::Ge => a >= b,
                            Rel::Eq => a == b,
                            Rel::Ne => a != b,
                        };
                        assert_eq!(
                            eval_q(&f, q, v),
                            truth,
                            "|{v}| {rel:?} {c_val} (flipped={flipped}): {f:?}"
                        );
                    }
                }
            }
        }
    }

    /// The reconciliation path (`encode_elem_pred_generic`) now encodes the
    /// corpus-universal `abs(eta) < 2.4` EXACTLY — no Unknown hedge, so a
    /// same-base refinement over abs-cut collections can finally prove.
    #[test]
    fn generic_encoder_handles_abs_exactly() {
        let mut t = QuantityTable::default();
        let mut d = DiagTable::default();
        let base = t.intern_collection(Collection::Base(Symbol(0)));
        let node = abs_cmp_node(&mut t, CmpOp::Lt, "2.4", false);
        let f = encode_elem_pred_generic(&mut t, &node, base, GENERIC_INDEX, &mut d)
            .expect("abs cut must ground onto the generic element");
        assert!(
            f.is_exact(),
            "abs(eta) < 2.4 must encode with no Unknown: {f:?}"
        );
    }
}
