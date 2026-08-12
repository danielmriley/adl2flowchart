#include "adl2/sema/hir.hpp"

namespace adl2::sema {

namespace {

std::unique_ptr<HNode> clone_ptr(const std::unique_ptr<HNode>& p) {
  if (!p) return nullptr;
  return std::make_unique<HNode>(*p);
}

bool eq_ptr(const std::unique_ptr<HNode>& a, const std::unique_ptr<HNode>& b) {
  if (!a && !b) return true;
  if (!a || !b) return false;
  return *a == *b;
}

}  // namespace

HNode::HNode(const HNode& o)
    : kind(o.kind),
      span(o.span),
      tag(o.tag),
      text(o.text),
      bool_val(o.bool_val),
      qid(o.qid),
      prop(o.prop),
      coll(o.coll),
      particle(o.particle),
      arith(o.arith),
      cmp(o.cmp),
      band(o.band),
      reduce(o.reduce),
      has_slice(o.has_slice),
      slice_start(o.slice_start),
      slice_end(o.slice_end),
      region_index(o.region_index),
      lo(o.lo),
      hi(o.hi),
      items(o.items),
      a(clone_ptr(o.a)),
      b(clone_ptr(o.b)),
      c(clone_ptr(o.c)) {}

HNode& HNode::operator=(const HNode& o) {
  if (this == &o) return *this;
  kind = o.kind;
  span = o.span;
  tag = o.tag;
  text = o.text;
  bool_val = o.bool_val;
  qid = o.qid;
  prop = o.prop;
  coll = o.coll;
  particle = o.particle;
  arith = o.arith;
  cmp = o.cmp;
  band = o.band;
  reduce = o.reduce;
  has_slice = o.has_slice;
  slice_start = o.slice_start;
  slice_end = o.slice_end;
  region_index = o.region_index;
  lo = o.lo;
  hi = o.hi;
  items = o.items;
  a = clone_ptr(o.a);
  b = clone_ptr(o.b);
  c = clone_ptr(o.c);
  return *this;
}

bool HNode::has_unsupported() const {
  if (!tag.is_in_fragment()) return true;
  for (const HNode* ch : children()) {
    if (ch->has_unsupported()) return true;
  }
  return false;
}

std::vector<const HNode*> HNode::children() const {
  std::vector<const HNode*> v;
  switch (kind) {
    case Kind::Neg:
    case Kind::Not:
    case Kind::Abs:
    case Kind::Band:
    case Kind::Reduce:
      if (a) v.push_back(a.get());
      break;
    case Kind::Binary:
    case Kind::Cmp:
      if (a) v.push_back(a.get());
      if (b) v.push_back(b.get());
      break;
    case Kind::And:
    case Kind::Or:
    case Kind::ScalarMinMax:
      for (const auto& n : items) v.push_back(&n);
      break;
    case Kind::Ternary:
      if (a) v.push_back(a.get());
      if (b) v.push_back(b.get());
      if (c) v.push_back(c.get());
      break;
    default:
      break;
  }
  return v;
}

bool HNode::operator==(const HNode& o) const {
  if (kind != o.kind) return false;
  if (!(span == o.span)) return false;
  if (tag.in_fragment != o.tag.in_fragment || tag.reason != o.tag.reason) {
    return false;
  }
  switch (kind) {
    case Kind::Num:
      return text == o.text;
    case Kind::Bool:
      return bool_val == o.bool_val;
    case Kind::Quantity:
      return qid == o.qid;
    case Kind::ElemSelfProp:
    case Kind::ReduceProp:
      return prop == o.prop;
    case Kind::Reduce:
      return reduce == o.reduce && coll == o.coll && has_slice == o.has_slice &&
             slice_start == o.slice_start && slice_end == o.slice_end &&
             eq_ptr(a, o.a);
    case Kind::CollProp:
      return coll == o.coll && prop == o.prop;
    case Kind::ScalarMinMax:
      return reduce == o.reduce && items == o.items;
    case Kind::Particle:
      return particle == o.particle;
    case Kind::CollValue:
      return coll == o.coll;
    case Kind::Neg:
    case Kind::Not:
    case Kind::Abs:
      return eq_ptr(a, o.a);
    case Kind::Binary:
      return arith == o.arith && eq_ptr(a, o.a) && eq_ptr(b, o.b);
    case Kind::And:
    case Kind::Or:
      return items == o.items;
    case Kind::Cmp:
      return cmp == o.cmp && eq_ptr(a, o.a) && eq_ptr(b, o.b);
    case Kind::Band:
      return band == o.band && lo == o.lo && hi == o.hi && eq_ptr(a, o.a);
    case Kind::Ternary:
      return eq_ptr(a, o.a) && eq_ptr(b, o.b) && eq_ptr(c, o.c);
    case Kind::RegionPred:
      return region_index == o.region_index;
    case Kind::Unsupported:
      return true;
  }
  return false;
}

ElemPredId ElemPredInterner::intern(HNode node, std::string render) {
  ElemPredId id{static_cast<std::uint32_t>(preds_.size())};
  if (node.has_unsupported()) {
    preds_.push_back(ElemPred{std::move(node), std::move(render)});
    return id;
  }
  auto it = by_render_.find(render);
  if (it != by_render_.end()) return it->second;
  by_render_.emplace(render, id);
  preds_.push_back(ElemPred{std::move(node), std::move(render)});
  return id;
}

std::optional<CollectionId> Hir::collection_of(const std::string& name) const {
  Symbol sym;
  if (!symbols.lookup(name, sym)) return std::nullopt;
  for (const auto& o : objects) {
    if (o.name == sym) return o.coll;
  }
  for (std::size_t i = 0; i < coll_names.size(); ++i) {
    for (Symbol s : coll_names[i]) {
      if (s == sym) {
        return CollectionId{static_cast<std::uint32_t>(i)};
      }
    }
  }
  return std::nullopt;
}

const HirDefine* Hir::define(const std::string& name) const {
  Symbol sym;
  if (!symbols.lookup(name, sym)) return nullptr;
  for (const auto& d : defines) {
    if (d.name == sym) return &d;
  }
  return nullptr;
}

const HirRegion* Hir::region(const std::string& name) const {
  Symbol sym;
  if (!symbols.lookup(name, sym)) return nullptr;
  for (const auto& r : regions) {
    if (r.name == sym) return &r;
  }
  return nullptr;
}

}  // namespace adl2::sema
