//! The typed Quantity/Collection identity model (SPEC_ARCHITECTURE §4).
//!
//! Event quantities are typed, interned values whose identity is
//! structural — never a string key. Two quantities unify only by
//! construction (same definition); relations between *different*
//! quantities are facts proven downstream (axioms/solver), never merges.

use crate::intern::Symbol;
use std::collections::HashMap;

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident, $prefix:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

id_type!(
    /// Interned collection identity.
    CollectionId,
    "C"
);
id_type!(
    /// Interned event-quantity identity.
    QuantityId,
    "Q"
);
id_type!(
    /// Interned element-predicate identity (the cut set of a filtered
    /// collection, as a predicate over the implicit element).
    ElemPredId,
    "P"
);
id_type!(
    /// Interned property identity (canonicalized via `property_vars.txt`).
    PropId,
    "prop"
);

/// Element index within an ordered collection. 0-based. `FromFront(i)` is
/// `coll[i]`; `FromBack(k)` is `coll[-k]` (`[-1]` = last), resolved as an
/// in-fragment element leaf guarded by `size >= k` (OPEN-3, resolved).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ElemIndex {
    FromFront(u32),
    FromBack(u32),
}

impl std::fmt::Display for ElemIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ElemIndex::FromFront(n) => write!(f, "{n}"),
            ElemIndex::FromBack(n) => write!(f, "-{n}"),
        }
    }
}

/// Angular-separation kind. `DR` is unoriented (arguments canonically
/// ordered at interning); `DPhi`/`DEta` are oriented per PHASE0 OPEN-2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AngKind {
    DPhi,
    DEta,
    DR,
}

impl AngKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AngKind::DPhi => "dphi",
            AngKind::DEta => "deta",
            AngKind::DR => "dR",
        }
    }

    /// Oriented = argument order is part of the identity.
    #[must_use]
    pub fn oriented(self) -> bool {
        !matches!(self, AngKind::DR)
    }
}

/// A particle-valued reference (argument to angular separations and
/// external functions).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ParticleRef {
    /// A specific element of a collection.
    Elem {
        coll: CollectionId,
        index: ElemIndex,
    },
    /// The whole collection (unindexed / underscore-all reference).
    Whole(CollectionId),
    /// The per-event missing-momentum vector (MET family).
    Met,
    /// A composite-block binder slot (`take leptons l1, l2`); identity is
    /// the (collection, binder-name) pair — never merged across names.
    Binder { coll: CollectionId, name: Symbol },
    /// The implicit *outer* element of an object-block filter, used as a
    /// particle inside a reducer body (`reject any(dR(this, X)) < 0.4`).
    /// Interpret-only: the analyzer keeps reducer bodies opaque (P1).
    ThisElem,
    /// The current iteration element of the innermost reducer (`any`/`all`/
    /// `min`/`max`/`sum`) body. Interpret-only (P1).
    ReduceElem,
    /// A 4-vector sum of particle references (`l1 + l2`), canonically
    /// flattened and operand-sorted at construction so association and
    /// argument order do not create distinct identities. Build via
    /// [`ParticleRef::sum`]; never construct the variant directly.
    Sum(Vec<ParticleRef>),
}

impl ParticleRef {
    /// Build a canonical 4-vector sum: nested `Sum`s are flattened and the
    /// operands are sorted (`ParticleRef` derives `Ord`), so `l0+(l1+l2)`,
    /// `(l0+l1)+l2` and `l2+l1+l0` all intern to the same identity.
    #[must_use]
    pub fn sum(parts: impl IntoIterator<Item = ParticleRef>) -> ParticleRef {
        let mut flat = Vec::new();
        for p in parts {
            match p {
                ParticleRef::Sum(inner) => flat.extend(inner),
                other => flat.push(other),
            }
        }
        flat.sort();
        ParticleRef::Sum(flat)
    }
}

/// Source of a per-event scalar.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ScalarSource {
    /// A property of the event MET vector; a bare MET-family value is its
    /// `.pt` magnitude.
    MetProp(PropId),
    /// A named per-event scalar (`scalarHT`, ...).
    EventVar(Symbol),
    /// A trigger flag (∈ {0,1}).
    Trigger(Symbol),
}

/// An argument of an opaque external function (interned exactly).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum QuantityArg {
    /// Canonical numeral text (exact, finite-checked downstream).
    Num(String),
    Quantity(QuantityId),
    Particle(ParticleRef),
    Collection(CollectionId),
    /// Unindexed per-element property of a collection (`jets.pt`).
    CollProp {
        coll: CollectionId,
        prop: PropId,
    },
    /// Canonical rendering of an argument we cannot type further. Exact
    /// structural text over already-resolved ids — identical text means
    /// identical resolution, so interning cannot over-merge.
    Opaque(String),
}

/// A typed event quantity (SPEC_ARCHITECTURE §4).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Quantity {
    EventScalar(ScalarSource),
    Size(CollectionId),
    ElemProp {
        coll: CollectionId,
        index: ElemIndex,
        prop: PropId,
    },
    AngularSep {
        kind: AngKind,
        a: ParticleRef,
        b: ParticleRef,
        /// `DR` unoriented; `DPhi`/`DEta` oriented (PHASE0 OPEN-2).
        oriented: bool,
    },
    /// Opaque but interned: same name + same args = same quantity.
    ExternalFn {
        name: Symbol,
        args: Vec<QuantityArg>,
    },
}

/// Sort direction of a [`Collection::Sorted`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SortDir {
    Ascend,
    Descend,
}

/// Sort key of a [`Collection::Sorted`]. `Prop(pt)` + [`SortDir::Descend`]
/// over a provably pt-descending source is the *only* shape the analyzer
/// may canonicalize to an alias of the source (P2); every other key/dir is
/// opaque (size/existence-only). The key is the interner's identity, so two
/// sorts by different properties never unify.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SortKey {
    /// Sort by a single per-element property (the interpreter re-sorts by it).
    Prop(PropId),
    /// A key not reducible to one per-element property; the interpreter keeps
    /// source order and any indexed access is diagnosed `Unsupported`. The
    /// string is the canonical render (interning identity).
    Opaque(String),
}

/// How a composite enumerates tuples (`take comb`/`disjoint`/`cartesian`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CombKind {
    /// Ordered product including cross-collection repeats (`cartesian`).
    Cartesian,
    /// Unordered pairs of value-distinct elements (`disjoint`, USER ANSWER 4).
    Disjoint,
}

/// A collection's defining structure.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Collection {
    /// Detector-level input collection (canonical base name).
    Base(Symbol),
    /// Object block with cuts: a *different* identity than its parent,
    /// forever; relations to the parent are derived facts.
    Filtered {
        parent: CollectionId,
        pred: ElemPredId,
    },
    /// Concatenation of parts (order is part of the identity).
    Union(Vec<CollectionId>),
    /// A re-sorted permutation of `source` (`take sort(coll, key, dir)`). Same
    /// element *set* as the source; the per-index order is the key's. Never an
    /// index-ordering fact unless P2's exact pt-descending alias gate fires.
    Sorted {
        source: CollectionId,
        key: SortKey,
        dir: SortDir,
    },
    /// A contiguous half-open sub-range `source[start..end]` (`coll[a:b]`).
    /// `end == None` means "through the end".
    Slice {
        source: CollectionId,
        start: u32,
        end: Option<u32>,
    },
    /// Combinatorial composite (COMB / multi-binder blocks): a collection of
    /// *tuples* over `parts`. `kind` selects the enumeration; `members` is the
    /// per-slot binder (name, source) in slot order; `candidate` is an
    /// optional 4-vector candidate built from the binders
    /// (`candidate ll = l1 + l2`); `cuts` are per-tuple filters (interned
    /// predicate ids over the tuple). Interpret-only in P1.
    Combination {
        parts: Vec<CollectionId>,
        kind: CombKind,
        members: Vec<CompositeBinder>,
        candidate: Option<CompositeCandidate>,
        cuts: Vec<ElemPredId>,
    },
    /// A projection of a [`Collection::Combination`] onto one axis
    /// (`X->ll` candidate, `X->l1` member): one element per surviving tuple.
    CombProject {
        comb: CollectionId,
        axis: CombAxis,
    },
}

/// One binder slot of a composite block: its name and the source collection
/// its element ranges over.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompositeBinder {
    pub name: Symbol,
    pub source: CollectionId,
}

/// Which axis of a composite a projection selects.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CombAxis {
    /// A binder member slot, by name (`X->l1`).
    Member(Symbol),
    /// The candidate 4-vector (`X->ll`), by name.
    Candidate(Symbol),
}

/// A composite candidate definition (`candidate ll = l1 + l2`): the binder
/// name it is bound to and the 4-vector sum it denotes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompositeCandidate {
    pub name: Symbol,
    /// The candidate 4-vector (a `ParticleRef::Sum` over the tuple binders).
    pub vector: ParticleRef,
}

/// Structural interner for collections, quantities and properties.
#[derive(Debug, Default)]
pub struct QuantityTable {
    colls: Vec<Collection>,
    coll_ids: HashMap<Collection, CollectionId>,
    quants: Vec<Quantity>,
    quant_ids: HashMap<Quantity, QuantityId>,
    props: Vec<(String, String)>, // (identity key, display)
    prop_ids: HashMap<String, PropId>,
}

impl QuantityTable {
    pub fn intern_collection(&mut self, c: Collection) -> CollectionId {
        if let Some(&id) = self.coll_ids.get(&c) {
            return id;
        }
        let id = CollectionId(u32::try_from(self.colls.len()).expect("collection id overflow"));
        self.coll_ids.insert(c.clone(), id);
        self.colls.push(c);
        id
    }

    pub fn intern_quantity(&mut self, q: Quantity) -> QuantityId {
        if let Some(&id) = self.quant_ids.get(&q) {
            return id;
        }
        let id = QuantityId(u32::try_from(self.quants.len()).expect("quantity id overflow"));
        self.quant_ids.insert(q.clone(), id);
        self.quants.push(q);
        id
    }

    /// The id of an already-interned quantity, or `None` if it was never
    /// interned. O(1) lookup via the interner map — no scan of the table.
    #[must_use]
    pub fn quantity_id(&self, q: &Quantity) -> Option<QuantityId> {
        self.quant_ids.get(q).copied()
    }

    /// Intern an angular separation, canonically ordering the operands of
    /// unoriented kinds so `dR(a,b)` and `dR(b,a)` are the SAME quantity
    /// by construction, while oriented kinds keep argument order.
    pub fn intern_angular(&mut self, kind: AngKind, a: ParticleRef, b: ParticleRef) -> QuantityId {
        let (a, b) = if !kind.oriented() && b < a {
            (b, a)
        } else {
            (a, b)
        };
        self.intern_quantity(Quantity::AngularSep {
            kind,
            a,
            b,
            oriented: kind.oriented(),
        })
    }

    /// Intern a property by its canonical identity key, keeping `display`
    /// for human output (first-wins).
    pub fn intern_prop(&mut self, key: &str, display: &str) -> PropId {
        if let Some(&id) = self.prop_ids.get(key) {
            return id;
        }
        let id = PropId(u32::try_from(self.props.len()).expect("prop id overflow"));
        self.prop_ids.insert(key.to_owned(), id);
        self.props.push((key.to_owned(), display.to_owned()));
        id
    }

    #[must_use]
    pub fn prop_display(&self, id: PropId) -> &str {
        &self.props[id.0 as usize].1
    }

    #[must_use]
    pub fn prop_key(&self, id: PropId) -> &str {
        &self.props[id.0 as usize].0
    }

    #[must_use]
    pub fn collection(&self, id: CollectionId) -> &Collection {
        &self.colls[id.0 as usize]
    }

    #[must_use]
    pub fn quantity(&self, id: QuantityId) -> &Quantity {
        &self.quants[id.0 as usize]
    }

    #[must_use]
    pub fn collections(&self) -> &[Collection] {
        &self.colls
    }

    #[must_use]
    pub fn quantities(&self) -> &[Quantity] {
        &self.quants
    }

    /// Is `c` provably pT-descending? ORD/IDOM/EPRED index-ordering facts ride
    /// on this predicate; it is the **single source of truth** shared by the
    /// axiom emitter and the resolver's sort-alias gate (plan §risk 5 — no
    /// second copy may diverge).
    ///
    /// True only for a base collection, a `Filtered` chain rooted at a base
    /// (filtering preserves source order), a `Slice` of a pT-descending source
    /// (a contiguous sub-sequence of a sorted list stays sorted), and a
    /// descending pT `Sorted` of a pT-descending source (the identity
    /// permutation — the sole alias shape). `pt_key` is the canonical pT
    /// property key (`ext.prop_canon("pt").0`). Everything else — a non-pT or
    /// ascending sort, a `Union`, a `Combination`/projection — is `false`
    /// (the sound posture: only ever weakens to POSSIBLY, never a false PROVEN).
    #[must_use]
    pub fn pt_ordered(&self, c: CollectionId, pt_key: &str) -> bool {
        match self.collection(c) {
            Collection::Base(_) => true,
            Collection::Filtered { parent, .. } => self.pt_ordered(*parent, pt_key),
            Collection::Slice { source, .. } => self.pt_ordered(*source, pt_key),
            Collection::Sorted { source, key, dir } => {
                *dir == SortDir::Descend
                    && matches!(key, SortKey::Prop(p) if self.prop_key(*p) == pt_key)
                    && self.pt_ordered(*source, pt_key)
            }
            Collection::Union(_)
            | Collection::Combination { .. }
            | Collection::CombProject { .. } => false,
        }
    }
}
