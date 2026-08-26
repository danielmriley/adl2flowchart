#include "adl2/sema/quantity.hpp"

#include <algorithm>
#include <tuple>

namespace adl2::sema {

ParticleRef ParticleRef::sum(std::vector<ParticleRef> in) {
  std::vector<ParticleRef> flat;
  for (auto& p : in) {
    if (p.kind == ParticleKind::Sum) {
      for (auto& inner : p.parts) flat.push_back(std::move(inner));
    } else {
      flat.push_back(std::move(p));
    }
  }
  std::sort(flat.begin(), flat.end());
  ParticleRef out;
  out.kind = ParticleKind::Sum;
  out.parts = std::move(flat);
  return out;
}

bool ParticleRef::operator==(const ParticleRef& o) const {
  if (kind != o.kind) return false;
  switch (kind) {
    case ParticleKind::Elem:
      return coll == o.coll && index == o.index;
    case ParticleKind::Whole:
      return coll == o.coll;
    case ParticleKind::Met:
    case ParticleKind::ThisElem:
    case ParticleKind::ReduceElem:
      return true;
    case ParticleKind::Binder:
      return coll == o.coll && name == o.name;
    case ParticleKind::Sum:
      return parts == o.parts;
  }
  return false;
}

bool ParticleRef::operator<(const ParticleRef& o) const {
  if (kind != o.kind) return kind < o.kind;
  switch (kind) {
    case ParticleKind::Elem:
      if (!(coll == o.coll)) return coll < o.coll;
      return index < o.index;
    case ParticleKind::Whole:
      return coll < o.coll;
    case ParticleKind::Met:
    case ParticleKind::ThisElem:
    case ParticleKind::ReduceElem:
      return false;
    case ParticleKind::Binder:
      if (!(coll == o.coll)) return coll < o.coll;
      return name < o.name;
    case ParticleKind::Sum:
      return parts < o.parts;
  }
  return false;
}

bool QuantityArg::operator==(const QuantityArg& o) const {
  if (kind != o.kind) return false;
  switch (kind) {
    case QuantityArgKind::Num:
    case QuantityArgKind::Opaque:
      return text == o.text;
    case QuantityArgKind::Quantity:
      return qid == o.qid;
    case QuantityArgKind::Particle:
      return particle == o.particle;
    case QuantityArgKind::Collection:
      return coll == o.coll;
    case QuantityArgKind::CollProp:
      return coll == o.coll && prop == o.prop;
  }
  return false;
}

bool QuantityArg::operator<(const QuantityArg& o) const {
  if (kind != o.kind) return kind < o.kind;
  switch (kind) {
    case QuantityArgKind::Num:
    case QuantityArgKind::Opaque:
      return text < o.text;
    case QuantityArgKind::Quantity:
      return qid < o.qid;
    case QuantityArgKind::Particle:
      return particle < o.particle;
    case QuantityArgKind::Collection:
      return coll < o.coll;
    case QuantityArgKind::CollProp:
      if (!(coll == o.coll)) return coll < o.coll;
      return prop < o.prop;
  }
  return false;
}

bool Quantity::operator==(const Quantity& o) const {
  if (kind != o.kind) return false;
  switch (kind) {
    case QuantityKind::EventScalar:
      return scalar == o.scalar;
    case QuantityKind::Size:
      return coll == o.coll;
    case QuantityKind::ElemProp:
      return coll == o.coll && index == o.index && prop == o.prop;
    case QuantityKind::AngularSep:
      return ang == o.ang && a == o.a && b == o.b && oriented == o.oriented;
    case QuantityKind::ExternalFn:
      return name == o.name && args == o.args;
    case QuantityKind::Present:
      return inner == o.inner;
  }
  return false;
}

bool Quantity::operator<(const Quantity& o) const {
  if (kind != o.kind) return kind < o.kind;
  switch (kind) {
    case QuantityKind::EventScalar:
      return scalar < o.scalar;
    case QuantityKind::Size:
      return coll < o.coll;
    case QuantityKind::ElemProp:
      if (!(coll == o.coll)) return coll < o.coll;
      if (!(index == o.index)) return index < o.index;
      return prop < o.prop;
    case QuantityKind::AngularSep:
      if (ang != o.ang) return ang < o.ang;
      if (!(a == o.a)) return a < o.a;
      if (!(b == o.b)) return b < o.b;
      return oriented < o.oriented;
    case QuantityKind::ExternalFn:
      if (!(name == o.name)) return name < o.name;
      return args < o.args;
    case QuantityKind::Present:
      return inner < o.inner;
  }
  return false;
}

std::string SortKey::debug() const {
  if (kind == SortKeyKind::Prop) {
    return "Prop(" + prop.debug() + ")";
  }
  // Rust Debug for String: `"..."` with escapes.
  std::string out = "Opaque(\"";
  for (unsigned char c : opaque) {
    if (c == '\\' || c == '"') {
      out.push_back('\\');
      out.push_back(static_cast<char>(c));
    } else {
      out.push_back(static_cast<char>(c));
    }
  }
  out += "\")";
  return out;
}

bool Collection::operator==(const Collection& o) const {
  if (kind != o.kind) return false;
  switch (kind) {
    case CollectionKind::Base:
      return base == o.base;
    case CollectionKind::Filtered:
      return parent == o.parent && pred == o.pred;
    case CollectionKind::Union:
      return parts == o.parts;
    case CollectionKind::Sorted:
      return parent == o.parent && sort_key == o.sort_key &&
             sort_dir == o.sort_dir;
    case CollectionKind::Slice:
      return parent == o.parent && slice_start == o.slice_start &&
             slice_end == o.slice_end;
    case CollectionKind::Combination:
      return parts == o.parts && comb_kind == o.comb_kind &&
             members == o.members && candidate == o.candidate && cuts == o.cuts;
    case CollectionKind::CombProject:
      return parent == o.parent && axis == o.axis;
  }
  return false;
}

bool Collection::operator<(const Collection& o) const {
  if (kind != o.kind) return kind < o.kind;
  switch (kind) {
    case CollectionKind::Base:
      return base < o.base;
    case CollectionKind::Filtered:
      if (!(parent == o.parent)) return parent < o.parent;
      return pred < o.pred;
    case CollectionKind::Union:
      return parts < o.parts;
    case CollectionKind::Sorted:
      if (!(parent == o.parent)) return parent < o.parent;
      if (!(sort_key == o.sort_key)) return sort_key < o.sort_key;
      return sort_dir < o.sort_dir;
    case CollectionKind::Slice:
      if (!(parent == o.parent)) return parent < o.parent;
      if (slice_start != o.slice_start) return slice_start < o.slice_start;
      if (slice_end.has_value() != o.slice_end.has_value()) {
        return slice_end.has_value() < o.slice_end.has_value();
      }
      return slice_end.has_value() && *slice_end < *o.slice_end;
    case CollectionKind::Combination: {
      if (!(parts == o.parts)) return parts < o.parts;
      if (comb_kind != o.comb_kind) return comb_kind < o.comb_kind;
      if (!(members == o.members)) return members < o.members;
      bool ch = candidate.has_value();
      bool oh = o.candidate.has_value();
      if (ch != oh) return ch < oh;
      if (ch && !(*candidate == *o.candidate)) return *candidate < *o.candidate;
      return cuts < o.cuts;
    }
    case CollectionKind::CombProject:
      if (!(parent == o.parent)) return parent < o.parent;
      return axis < o.axis;
  }
  return false;
}

CollectionId QuantityTable::intern_collection(Collection c) {
  auto it = coll_ids_.find(c);
  if (it != coll_ids_.end()) return it->second;
  CollectionId id{static_cast<std::uint32_t>(colls_.size())};
  coll_ids_.emplace(c, id);
  colls_.push_back(std::move(c));
  return id;
}

QuantityId QuantityTable::intern_quantity(Quantity q) {
  auto it = quant_ids_.find(q);
  if (it != quant_ids_.end()) return it->second;
  QuantityId id{static_cast<std::uint32_t>(quants_.size())};
  quant_ids_.emplace(q, id);
  quants_.push_back(std::move(q));
  return id;
}

std::optional<QuantityId> QuantityTable::quantity_id(const Quantity& q) const {
  auto it = quant_ids_.find(q);
  if (it == quant_ids_.end()) return std::nullopt;
  return it->second;
}

QuantityId QuantityTable::intern_angular(AngKind kind, ParticleRef a,
                                         ParticleRef b) {
  if (!ang_kind_oriented(kind) && b < a) {
    std::swap(a, b);
  }
  return intern_quantity(
      Quantity::angular(kind, std::move(a), std::move(b), ang_kind_oriented(kind)));
}

PropId QuantityTable::intern_prop(const std::string& key,
                                  const std::string& display) {
  auto it = prop_ids_.find(key);
  if (it != prop_ids_.end()) return it->second;
  PropId id{static_cast<std::uint32_t>(props_.size())};
  prop_ids_.emplace(key, id);
  props_.emplace_back(key, display);
  return id;
}

bool QuantityTable::pt_ordered(CollectionId c, const std::string& pt_key) const {
  const Collection& col = collection(c);
  switch (col.kind) {
    case CollectionKind::Base:
      return true;
    case CollectionKind::Filtered:
    case CollectionKind::Slice:
      return pt_ordered(col.parent, pt_key);
    case CollectionKind::Sorted:
      return col.sort_dir == SortDir::Descend &&
             col.sort_key.kind == SortKeyKind::Prop &&
             this->prop_key(col.sort_key.prop) == pt_key &&
             pt_ordered(col.parent, pt_key);
    case CollectionKind::Union:
    case CollectionKind::Combination:
    case CollectionKind::CombProject:
      return false;
  }
  return false;
}

Absence QuantityTable::absence(QuantityId q) const {
  switch (quantity(q).kind) {
    case QuantityKind::Size:
    case QuantityKind::Present:
      return Absence::Never;
    case QuantityKind::EventScalar:
      return Absence::Hard;
    case QuantityKind::ElemProp:
    case QuantityKind::AngularSep:
    case QuantityKind::ExternalFn:
      return Absence::Soft;
  }
  return Absence::Soft;
}

bool QuantityTable::whole_pair_legs(QuantityId q, AngKind& kind, CollectionId& a,
                                    CollectionId& b) const {
  const Quantity& qq = quantity(q);
  if (qq.kind != QuantityKind::AngularSep) return false;
  if (qq.a.kind != ParticleKind::Whole || qq.b.kind != ParticleKind::Whole) {
    return false;
  }
  kind = qq.ang;
  a = qq.a.coll;
  b = qq.b.coll;
  return true;
}

bool QuantityTable::has_unindexed_leg(QuantityId q) const {
  const Quantity& qq = quantity(q);
  if (qq.kind != QuantityKind::AngularSep) return false;
  return qq.a.kind == ParticleKind::Whole || qq.b.kind == ParticleKind::Whole;
}

void QuantityTable::existence_floor(
    QuantityId q, std::map<CollectionId, std::uint32_t>& out) const {
  auto need = [&](CollectionId coll, std::uint32_t i) {
    auto it = out.find(coll);
    if (it == out.end()) out.emplace(coll, i);
    else if (i > it->second) it->second = i;
  };
  auto floor_need = [](const ElemIndex& index) -> std::optional<std::uint32_t> {
    if (index.kind == ElemIndexKind::FromFront) return index.n;
    if (index.n >= 1) return index.n - 1;
    return std::nullopt;
  };
  const Quantity& qq = quantity(q);
  switch (qq.kind) {
    case QuantityKind::ElemProp:
      if (auto n = floor_need(qq.index)) need(qq.coll, *n);
      break;
    case QuantityKind::AngularSep:
      for (const ParticleRef* p : {&qq.a, &qq.b}) {
        if (p->kind == ParticleKind::Elem) {
          if (auto n = floor_need(p->index)) need(p->coll, *n);
        }
      }
      break;
    case QuantityKind::Present:
      existence_floor(qq.inner, out);
      break;
    default:
      break;
  }
}

bool QuantityTable::filter_chain(CollectionId id, Symbol& base,
                                 std::vector<ElemPredId>& preds) const {
  preds.clear();
  CollectionId cur = id;
  for (;;) {
    const Collection& c = collection(cur);
    if (c.kind == CollectionKind::Base) {
      std::reverse(preds.begin(), preds.end());
      base = c.base;
      return true;
    }
    if (c.kind == CollectionKind::Filtered) {
      preds.push_back(c.pred);
      cur = c.parent;
      continue;
    }
    return false;
  }
}

std::vector<std::pair<CollectionId, CollectionId>>
QuantityTable::reconciliation_candidates() const {
  std::map<Symbol, std::vector<CollectionId>> by_base;
  for (std::size_t i = 0; i < colls_.size(); ++i) {
    if (colls_[i].kind != CollectionKind::Filtered) continue;
    CollectionId id{static_cast<std::uint32_t>(i)};
    Symbol base;
    std::vector<ElemPredId> preds;
    if (filter_chain(id, base, preds)) by_base[base].push_back(id);
  }
  std::vector<std::pair<CollectionId, CollectionId>> out;
  for (const auto& kv : by_base) {
    const auto& ids = kv.second;
    for (std::size_t a = 0; a < ids.size(); ++a) {
      for (std::size_t b = a + 1; b < ids.size(); ++b) {
        out.emplace_back(ids[a], ids[b]);
      }
    }
  }
  return out;
}

}  // namespace adl2::sema
