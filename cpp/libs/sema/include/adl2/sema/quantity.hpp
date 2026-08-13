#pragma once

/// Typed Quantity/Collection identity (SPEC_ARCHITECTURE §4).
/// Identity is structural and interned — never a string key.

#include "adl2/sema/intern.hpp"

#include <cstdint>
#include <map>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::sema {

struct CollectionId {
  std::uint32_t id = 0;
  bool operator==(CollectionId o) const { return id == o.id; }
  bool operator!=(CollectionId o) const { return id != o.id; }
  bool operator<(CollectionId o) const { return id < o.id; }
  std::string to_string() const { return "C" + std::to_string(id); }
};

struct QuantityId {
  std::uint32_t id = 0;
  bool operator==(QuantityId o) const { return id == o.id; }
  bool operator!=(QuantityId o) const { return id != o.id; }
  bool operator<(QuantityId o) const { return id < o.id; }
  std::string to_string() const { return "Q" + std::to_string(id); }
};

struct ElemPredId {
  std::uint32_t id = 0;
  bool operator==(ElemPredId o) const { return id == o.id; }
  bool operator!=(ElemPredId o) const { return id != o.id; }
  bool operator<(ElemPredId o) const { return id < o.id; }
  std::string to_string() const { return "P" + std::to_string(id); }
};

struct PropId {
  std::uint32_t id = 0;
  bool operator==(PropId o) const { return id == o.id; }
  bool operator!=(PropId o) const { return id != o.id; }
  bool operator<(PropId o) const { return id < o.id; }
  std::string to_string() const { return "prop" + std::to_string(id); }
  /// Rust Debug: `PropId(n)`.
  std::string debug() const { return "PropId(" + std::to_string(id) + ")"; }
};

/// Largest front index a SOURCE expression can resolve to. `u32::MAX` is
/// reserved as reconciliation's generic-element sentinel.
constexpr std::uint32_t MAX_SOURCE_ELEM_INDEX = 0xFFFFFFFEu;

enum class ElemIndexKind : std::uint8_t { FromFront, FromBack };

struct ElemIndex {
  ElemIndexKind kind = ElemIndexKind::FromFront;
  std::uint32_t n = 0;

  static ElemIndex from_front(std::uint32_t i) {
    ElemIndex e;
    e.kind = ElemIndexKind::FromFront;
    e.n = i;
    return e;
  }
  static ElemIndex from_back(std::uint32_t k) {
    ElemIndex e;
    e.kind = ElemIndexKind::FromBack;
    e.n = k;
    return e;
  }

  bool operator==(const ElemIndex& o) const { return kind == o.kind && n == o.n; }
  bool operator!=(const ElemIndex& o) const { return !(*this == o); }
  bool operator<(const ElemIndex& o) const {
    if (kind != o.kind) return kind < o.kind;
    return n < o.n;
  }
  std::string to_string() const {
    return kind == ElemIndexKind::FromBack ? ("-" + std::to_string(n))
                                           : std::to_string(n);
  }
};

enum class AngKind : std::uint8_t { DPhi, DEta, DR };

inline const char* ang_kind_str(AngKind k) {
  switch (k) {
    case AngKind::DPhi: return "dphi";
    case AngKind::DEta: return "deta";
    case AngKind::DR: return "dR";
  }
  return "?";
}

inline bool ang_kind_oriented(AngKind k) { return k != AngKind::DR; }

enum class ParticleKind : std::uint8_t {
  Elem,
  Whole,
  Met,
  Binder,
  ThisElem,
  ReduceElem,
  Sum
};

struct ParticleRef {
  ParticleKind kind = ParticleKind::Met;
  CollectionId coll;
  ElemIndex index;
  Symbol name;
  std::vector<ParticleRef> parts;  // Sum

  static ParticleRef elem(CollectionId c, ElemIndex i) {
    ParticleRef p;
    p.kind = ParticleKind::Elem;
    p.coll = c;
    p.index = i;
    return p;
  }
  static ParticleRef whole(CollectionId c) {
    ParticleRef p;
    p.kind = ParticleKind::Whole;
    p.coll = c;
    return p;
  }
  static ParticleRef met() {
    ParticleRef p;
    p.kind = ParticleKind::Met;
    return p;
  }
  static ParticleRef binder(CollectionId c, Symbol n) {
    ParticleRef p;
    p.kind = ParticleKind::Binder;
    p.coll = c;
    p.name = n;
    return p;
  }
  static ParticleRef this_elem() {
    ParticleRef p;
    p.kind = ParticleKind::ThisElem;
    return p;
  }
  static ParticleRef reduce_elem() {
    ParticleRef p;
    p.kind = ParticleKind::ReduceElem;
    return p;
  }
  /// Flatten nested Sums and sort operands (canonical identity).
  static ParticleRef sum(std::vector<ParticleRef> in);

  bool operator==(const ParticleRef& o) const;
  bool operator!=(const ParticleRef& o) const { return !(*this == o); }
  bool operator<(const ParticleRef& o) const;
};

enum class ScalarSourceKind : std::uint8_t { MetProp, EventVar, Trigger };

struct ScalarSource {
  ScalarSourceKind kind = ScalarSourceKind::EventVar;
  PropId prop;
  Symbol name;

  static ScalarSource met_prop(PropId p) {
    ScalarSource s;
    s.kind = ScalarSourceKind::MetProp;
    s.prop = p;
    return s;
  }
  static ScalarSource event_var(Symbol n) {
    ScalarSource s;
    s.kind = ScalarSourceKind::EventVar;
    s.name = n;
    return s;
  }
  static ScalarSource trigger(Symbol n) {
    ScalarSource s;
    s.kind = ScalarSourceKind::Trigger;
    s.name = n;
    return s;
  }

  bool operator==(const ScalarSource& o) const {
    if (kind != o.kind) return false;
    if (kind == ScalarSourceKind::MetProp) return prop == o.prop;
    return name == o.name;
  }
  bool operator<(const ScalarSource& o) const {
    if (kind != o.kind) return kind < o.kind;
    if (kind == ScalarSourceKind::MetProp) return prop < o.prop;
    return name < o.name;
  }
};

enum class QuantityArgKind : std::uint8_t {
  Num,
  Quantity,
  Particle,
  Collection,
  CollProp,
  Opaque
};

struct QuantityArg {
  QuantityArgKind kind = QuantityArgKind::Opaque;
  std::string text;  // Num / Opaque
  QuantityId qid;
  ParticleRef particle;
  CollectionId coll;
  PropId prop;

  static QuantityArg num(std::string t) {
    QuantityArg a;
    a.kind = QuantityArgKind::Num;
    a.text = std::move(t);
    return a;
  }
  static QuantityArg quantity(QuantityId q) {
    QuantityArg a;
    a.kind = QuantityArgKind::Quantity;
    a.qid = q;
    return a;
  }
  static QuantityArg particle_arg(ParticleRef p) {
    QuantityArg a;
    a.kind = QuantityArgKind::Particle;
    a.particle = std::move(p);
    return a;
  }
  static QuantityArg collection(CollectionId c) {
    QuantityArg a;
    a.kind = QuantityArgKind::Collection;
    a.coll = c;
    return a;
  }
  static QuantityArg coll_prop(CollectionId c, PropId p) {
    QuantityArg a;
    a.kind = QuantityArgKind::CollProp;
    a.coll = c;
    a.prop = p;
    return a;
  }
  static QuantityArg opaque(std::string t) {
    QuantityArg a;
    a.kind = QuantityArgKind::Opaque;
    a.text = std::move(t);
    return a;
  }

  bool operator==(const QuantityArg& o) const;
  bool operator<(const QuantityArg& o) const;
};

enum class QuantityKind : std::uint8_t {
  EventScalar,
  Size,
  ElemProp,
  AngularSep,
  ExternalFn,
  Present
};

struct Quantity {
  QuantityKind kind = QuantityKind::Size;
  ScalarSource scalar;
  CollectionId coll;
  ElemIndex index;
  PropId prop;
  AngKind ang = AngKind::DR;
  ParticleRef a;
  ParticleRef b;
  bool oriented = false;
  Symbol name;
  std::vector<QuantityArg> args;
  QuantityId inner;  // Present

  static Quantity event_scalar(ScalarSource s) {
    Quantity q;
    q.kind = QuantityKind::EventScalar;
    q.scalar = std::move(s);
    return q;
  }
  static Quantity size(CollectionId c) {
    Quantity q;
    q.kind = QuantityKind::Size;
    q.coll = c;
    return q;
  }
  static Quantity elem_prop(CollectionId c, ElemIndex i, PropId p) {
    Quantity q;
    q.kind = QuantityKind::ElemProp;
    q.coll = c;
    q.index = i;
    q.prop = p;
    return q;
  }
  static Quantity angular(AngKind k, ParticleRef aa, ParticleRef bb, bool ori) {
    Quantity q;
    q.kind = QuantityKind::AngularSep;
    q.ang = k;
    q.a = std::move(aa);
    q.b = std::move(bb);
    q.oriented = ori;
    return q;
  }
  static Quantity external_fn(Symbol n, std::vector<QuantityArg> a) {
    Quantity q;
    q.kind = QuantityKind::ExternalFn;
    q.name = n;
    q.args = std::move(a);
    return q;
  }
  static Quantity present(QuantityId inner) {
    Quantity q;
    q.kind = QuantityKind::Present;
    q.inner = inner;
    return q;
  }

  bool operator==(const Quantity& o) const;
  bool operator<(const Quantity& o) const;
};

enum class Absence : std::uint8_t { Never, Soft, Hard };

inline bool absence_possible(Absence a) { return a != Absence::Never; }

enum class SortDir : std::uint8_t { Ascend, Descend };

inline const char* sort_dir_debug(SortDir d) {
  return d == SortDir::Ascend ? "Ascend" : "Descend";
}

enum class SortKeyKind : std::uint8_t { Prop, Opaque };

struct SortKey {
  SortKeyKind kind = SortKeyKind::Opaque;
  PropId prop;
  std::string opaque;

  static SortKey of_prop(PropId p) {
    SortKey k;
    k.kind = SortKeyKind::Prop;
    k.prop = p;
    return k;
  }
  static SortKey of_opaque(std::string s) {
    SortKey k;
    k.kind = SortKeyKind::Opaque;
    k.opaque = std::move(s);
    return k;
  }

  bool operator==(const SortKey& o) const {
    if (kind != o.kind) return false;
    if (kind == SortKeyKind::Prop) return prop == o.prop;
    return opaque == o.opaque;
  }
  bool operator<(const SortKey& o) const {
    if (kind != o.kind) return kind < o.kind;
    if (kind == SortKeyKind::Prop) return prop < o.prop;
    return opaque < o.opaque;
  }
  /// Rust Debug.
  std::string debug() const;
};

enum class CombKind : std::uint8_t { Cartesian, Disjoint };

inline const char* comb_kind_debug(CombKind k) {
  return k == CombKind::Disjoint ? "Disjoint" : "Cartesian";
}

struct CompositeBinder {
  Symbol name;
  CollectionId source;
  bool operator==(const CompositeBinder& o) const {
    return name == o.name && source == o.source;
  }
  bool operator<(const CompositeBinder& o) const {
    if (!(name == o.name)) return name < o.name;
    return source < o.source;
  }
};

enum class CombAxisKind : std::uint8_t { Member, Candidate };

struct CombAxis {
  CombAxisKind kind = CombAxisKind::Member;
  Symbol name;
  static CombAxis member(Symbol n) {
    CombAxis a;
    a.kind = CombAxisKind::Member;
    a.name = n;
    return a;
  }
  static CombAxis candidate(Symbol n) {
    CombAxis a;
    a.kind = CombAxisKind::Candidate;
    a.name = n;
    return a;
  }
  bool operator==(const CombAxis& o) const {
    return kind == o.kind && name == o.name;
  }
  bool operator<(const CombAxis& o) const {
    if (kind != o.kind) return kind < o.kind;
    return name < o.name;
  }
  std::string debug() const {
    std::string k = kind == CombAxisKind::Member ? "Member" : "Candidate";
    return k + "(Symbol(" + std::to_string(name.id) + "))";
  }
};

struct CompositeCandidate {
  Symbol name;
  ParticleRef vector;
  bool operator==(const CompositeCandidate& o) const {
    return name == o.name && vector == o.vector;
  }
  bool operator<(const CompositeCandidate& o) const {
    if (!(name == o.name)) return name < o.name;
    return vector < o.vector;
  }
};

enum class CollectionKind : std::uint8_t {
  Base,
  Filtered,
  Union,
  Sorted,
  Slice,
  Combination,
  CombProject
};

struct Collection {
  CollectionKind kind = CollectionKind::Base;
  Symbol base;
  CollectionId parent;  // Filtered parent / Sorted source / Slice source / CombProject comb
  ElemPredId pred;
  std::vector<CollectionId> parts;  // Union / Combination
  SortKey sort_key;
  SortDir sort_dir = SortDir::Descend;
  std::uint32_t slice_start = 0;
  std::optional<std::uint32_t> slice_end;
  CombKind comb_kind = CombKind::Cartesian;
  std::vector<CompositeBinder> members;
  std::optional<CompositeCandidate> candidate;
  std::vector<ElemPredId> cuts;
  CombAxis axis;

  static Collection of_base(Symbol s) {
    Collection c;
    c.kind = CollectionKind::Base;
    c.base = s;
    return c;
  }
  static Collection filtered(CollectionId parent, ElemPredId pred) {
    Collection c;
    c.kind = CollectionKind::Filtered;
    c.parent = parent;
    c.pred = pred;
    return c;
  }
  static Collection of_union(std::vector<CollectionId> parts) {
    Collection c;
    c.kind = CollectionKind::Union;
    c.parts = std::move(parts);
    return c;
  }
  static Collection sorted(CollectionId src, SortKey key, SortDir dir) {
    Collection c;
    c.kind = CollectionKind::Sorted;
    c.parent = src;
    c.sort_key = std::move(key);
    c.sort_dir = dir;
    return c;
  }
  static Collection slice(CollectionId src, std::uint32_t start,
                          std::optional<std::uint32_t> end) {
    Collection c;
    c.kind = CollectionKind::Slice;
    c.parent = src;
    c.slice_start = start;
    c.slice_end = end;
    return c;
  }
  static Collection combination(std::vector<CollectionId> parts, CombKind kind,
                                std::vector<CompositeBinder> members,
                                std::optional<CompositeCandidate> cand,
                                std::vector<ElemPredId> cuts) {
    Collection c;
    c.kind = CollectionKind::Combination;
    c.parts = std::move(parts);
    c.comb_kind = kind;
    c.members = std::move(members);
    c.candidate = std::move(cand);
    c.cuts = std::move(cuts);
    return c;
  }
  static Collection comb_project(CollectionId comb, CombAxis axis) {
    Collection c;
    c.kind = CollectionKind::CombProject;
    c.parent = comb;
    c.axis = std::move(axis);
    return c;
  }

  bool operator==(const Collection& o) const;
  bool operator<(const Collection& o) const;
};

class QuantityTable {
 public:
  CollectionId intern_collection(Collection c);
  QuantityId intern_quantity(Quantity q);
  std::optional<QuantityId> quantity_id(const Quantity& q) const;
  QuantityId intern_angular(AngKind kind, ParticleRef a, ParticleRef b);
  PropId intern_prop(const std::string& key, const std::string& display);

  const std::string& prop_display(PropId id) const { return props_[id.id].second; }
  const std::string& prop_key(PropId id) const { return props_[id.id].first; }
  const Collection& collection(CollectionId id) const { return colls_[id.id]; }
  const Quantity& quantity(QuantityId id) const { return quants_[id.id]; }
  const std::vector<Collection>& collections() const { return colls_; }
  const std::vector<Quantity>& quantities() const { return quants_; }

  bool pt_ordered(CollectionId c, const std::string& pt_key) const;
  Absence absence(QuantityId q) const;
  bool may_be_absent(QuantityId q) const { return absence_possible(absence(q)); }

  /// `(kind, A, B)` of an angular separation between two WHOLE collections.
  bool whole_pair_legs(QuantityId q, AngKind& kind, CollectionId& a,
                       CollectionId& b) const;
  /// Angular separation with an unindexed (Whole) leg.
  bool has_unindexed_leg(QuantityId q) const;
  /// Element-existence floors: `size(C) > n` required for `q` to exist.
  void existence_floor(QuantityId q, std::map<CollectionId, std::uint32_t>& out) const;

  /// Flatten a pure filter chain to (base symbol, preds base-down).
  bool filter_chain(CollectionId id, Symbol& base,
                    std::vector<ElemPredId>& preds) const;

 private:
  std::vector<Collection> colls_;
  std::map<Collection, CollectionId> coll_ids_;
  std::vector<Quantity> quants_;
  std::map<Quantity, QuantityId> quant_ids_;
  std::vector<std::pair<std::string, std::string>> props_;  // (key, display)
  std::map<std::string, PropId> prop_ids_;
};

}  // namespace adl2::sema
