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

/// Element index within an ordered collection. 0-based (PHASE0 OPEN-3);
/// `FromBack` is reserved — `[-n]` is diagnosed as `Unsupported` until
/// OPEN-3 is resolved.
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
    /// Combinatorial composite (COMB / multi-binder blocks).
    Combination { parts: Vec<CollectionId> },
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
}
