#include "adl2/sema/merge.hpp"

#include "adl2/sema/dump.hpp"
#include "adl2/sema/quantity.hpp"

#include <memory>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

namespace adl2::sema {
namespace {

struct Memo {
  std::unordered_map<std::uint32_t, CollectionId> coll;
  std::unordered_map<std::uint32_t, QuantityId> quant;
  std::unordered_map<std::uint32_t, PropId> prop;
  std::unordered_map<std::uint32_t, ElemPredId> pred;
};

struct Merger {
  std::uint32_t unit_ord = 0;
  SymbolTable symbols;
  QuantityTable table;
  std::vector<std::vector<Symbol>> coll_names;
  ElemPredInterner elem_preds;
  std::vector<HirRegion> regions;
  std::vector<Symbol> region_name_order;
  std::vector<bool> histolist_regions;

  void add_unit(const Hir& src, const std::string& unit_label);
  Symbol remap_sym(const Hir& src, Symbol s);
  PropId remap_prop(const Hir& src, Memo& memo, PropId p);
  CollectionId remap_coll(const Hir& src, Memo& memo, CollectionId c);
  std::vector<CollectionId> remap_colls(const Hir& src, Memo& memo,
                                        const std::vector<CollectionId>& cs);
  ElemPredId remap_pred(const Hir& src, Memo& memo, ElemPredId p);
  QuantityId remap_quant(const Hir& src, Memo& memo, QuantityId q);
  ScalarSource remap_scalar(const Hir& src, Memo& memo, const ScalarSource& s);
  SortKey remap_sort_key(const Hir& src, Memo& memo, SortKey key);
  CombAxis remap_axis(const Hir& src, CombAxis axis);
  ParticleRef remap_particle(const Hir& src, Memo& memo, const ParticleRef& p);
  QuantityArg remap_arg(const Hir& src, Memo& memo, const QuantityArg& a);
  HNode remap_node(const Hir& src, Memo& memo, std::size_t region_base, const HNode& n);
  HirRegionStmt remap_stmt(const Hir& src, Memo& memo, std::size_t region_base,
                           const HirRegionStmt& s);
};

Symbol Merger::remap_sym(const Hir& src, Symbol s) { return symbols.intern(src.symbols.key(s)); }

PropId Merger::remap_prop(const Hir& src, Memo& memo, PropId p) {
  auto it = memo.prop.find(p.id);
  if (it != memo.prop.end()) return it->second;
  PropId id = table.intern_prop(src.table.prop_key(p), src.table.prop_display(p));
  memo.prop.emplace(p.id, id);
  return id;
}

std::vector<CollectionId> Merger::remap_colls(const Hir& src, Memo& memo,
                                              const std::vector<CollectionId>& cs) {
  std::vector<CollectionId> out;
  out.reserve(cs.size());
  for (auto c : cs) out.push_back(remap_coll(src, memo, c));
  return out;
}

ElemPredId Merger::remap_pred(const Hir& src, Memo& memo, ElemPredId p) {
  auto it = memo.pred.find(p.id);
  if (it != memo.pred.end()) return it->second;
  HNode node = remap_node(src, memo, 0, src.elem_preds[p.id].node);
  std::string render =
      render_node_raw(symbols, table, coll_names, region_name_order, node);
  ElemPredId id = elem_preds.intern(std::move(node), std::move(render));
  memo.pred.emplace(p.id, id);
  return id;
}

ParticleRef Merger::remap_particle(const Hir& src, Memo& memo, const ParticleRef& p) {
  switch (p.kind) {
    case ParticleKind::Elem:
      return ParticleRef::elem(remap_coll(src, memo, p.coll), p.index);
    case ParticleKind::Whole:
      return ParticleRef::whole(remap_coll(src, memo, p.coll));
    case ParticleKind::Met:
      return ParticleRef::met();
    case ParticleKind::Binder:
      return ParticleRef::binder(remap_coll(src, memo, p.coll), remap_sym(src, p.name));
    case ParticleKind::ThisElem:
      return ParticleRef::this_elem();
    case ParticleKind::ReduceElem:
      return ParticleRef::reduce_elem();
    case ParticleKind::Sum: {
      std::vector<ParticleRef> parts;
      parts.reserve(p.parts.size());
      for (const auto& q : p.parts) parts.push_back(remap_particle(src, memo, q));
      return ParticleRef::sum(std::move(parts));
    }
  }
  return ParticleRef::met();
}

QuantityArg Merger::remap_arg(const Hir& src, Memo& memo, const QuantityArg& a) {
  switch (a.kind) {
    case QuantityArgKind::Num:
      return QuantityArg::num(a.text);
    case QuantityArgKind::Quantity:
      return QuantityArg::quantity(remap_quant(src, memo, a.qid));
    case QuantityArgKind::Particle:
      return QuantityArg::particle_arg(remap_particle(src, memo, a.particle));
    case QuantityArgKind::Collection:
      return QuantityArg::collection(remap_coll(src, memo, a.coll));
    case QuantityArgKind::CollProp:
      return QuantityArg::coll_prop(remap_coll(src, memo, a.coll), remap_prop(src, memo, a.prop));
    case QuantityArgKind::Opaque:
      return QuantityArg::opaque(std::to_string(unit_ord) + "\x01" + a.text);
  }
  return QuantityArg::opaque(a.text);
}

ScalarSource Merger::remap_scalar(const Hir& src, Memo& memo, const ScalarSource& s) {
  switch (s.kind) {
    case ScalarSourceKind::MetProp:
      return ScalarSource::met_prop(remap_prop(src, memo, s.prop));
    case ScalarSourceKind::EventVar:
      return ScalarSource::event_var(remap_sym(src, s.name));
    case ScalarSourceKind::Trigger:
      return ScalarSource::trigger(remap_sym(src, s.name));
  }
  return s;
}

SortKey Merger::remap_sort_key(const Hir& src, Memo& memo, SortKey key) {
  if (key.kind == SortKeyKind::Opaque) {
    return SortKey::of_opaque(std::to_string(unit_ord) + "\x01" + key.opaque);
  }
  return SortKey::of_prop(remap_prop(src, memo, key.prop));
}

CombAxis Merger::remap_axis(const Hir& src, CombAxis axis) {
  Symbol n = remap_sym(src, axis.name);
  return axis.kind == CombAxisKind::Member ? CombAxis::member(n) : CombAxis::candidate(n);
}

QuantityId Merger::remap_quant(const Hir& src, Memo& memo, QuantityId q) {
  auto it = memo.quant.find(q.id);
  if (it != memo.quant.end()) return it->second;
  const Quantity& qq = src.table.quantity(q);
  QuantityId id;
  if (qq.kind == QuantityKind::AngularSep) {
    ParticleRef a = remap_particle(src, memo, qq.a);
    ParticleRef b = remap_particle(src, memo, qq.b);
    id = table.intern_angular(qq.ang, std::move(a), std::move(b));
  } else {
    Quantity neu;
    switch (qq.kind) {
      case QuantityKind::EventScalar:
        neu = Quantity::event_scalar(remap_scalar(src, memo, qq.scalar));
        break;
      case QuantityKind::Size:
        neu = Quantity::size(remap_coll(src, memo, qq.coll));
        break;
      case QuantityKind::ElemProp:
        neu = Quantity::elem_prop(remap_coll(src, memo, qq.coll), qq.index,
                                  remap_prop(src, memo, qq.prop));
        break;
      case QuantityKind::AngularSep:
        break;
      case QuantityKind::ExternalFn: {
        std::vector<QuantityArg> args;
        args.reserve(qq.args.size());
        for (const auto& a : qq.args) args.push_back(remap_arg(src, memo, a));
        neu = Quantity::external_fn(remap_sym(src, qq.name), std::move(args));
        break;
      }
      case QuantityKind::Present:
        neu = Quantity::present(remap_quant(src, memo, qq.inner));
        break;
    }
    id = table.intern_quantity(std::move(neu));
  }
  memo.quant.emplace(q.id, id);
  return id;
}

CollectionId Merger::remap_coll(const Hir& src, Memo& memo, CollectionId c) {
  auto it = memo.coll.find(c.id);
  if (it != memo.coll.end()) return it->second;
  const Collection& col = src.table.collection(c);
  Collection neu;
  switch (col.kind) {
    case CollectionKind::Base:
      neu = Collection::of_base(remap_sym(src, col.base));
      break;
    case CollectionKind::Filtered:
      neu = Collection::filtered(remap_coll(src, memo, col.parent), remap_pred(src, memo, col.pred));
      break;
    case CollectionKind::Union:
      neu = Collection::of_union(remap_colls(src, memo, col.parts));
      break;
    case CollectionKind::Combination: {
      auto parts = remap_colls(src, memo, col.parts);
      std::vector<CompositeBinder> members;
      members.reserve(col.members.size());
      for (const auto& m : col.members) {
        CompositeBinder b;
        b.name = remap_sym(src, m.name);
        b.source = remap_coll(src, memo, m.source);
        members.push_back(b);
      }
      std::optional<CompositeCandidate> cand;
      if (col.candidate) {
        CompositeCandidate cc;
        cc.name = remap_sym(src, col.candidate->name);
        cc.vector = remap_particle(src, memo, col.candidate->vector);
        cand = std::move(cc);
      }
      std::vector<ElemPredId> cuts;
      cuts.reserve(col.cuts.size());
      for (auto p : col.cuts) cuts.push_back(remap_pred(src, memo, p));
      neu = Collection::combination(std::move(parts), col.comb_kind, std::move(members),
                                    std::move(cand), std::move(cuts));
      break;
    }
    case CollectionKind::Sorted:
      neu = Collection::sorted(remap_coll(src, memo, col.parent),
                               remap_sort_key(src, memo, col.sort_key), col.sort_dir);
      break;
    case CollectionKind::Slice:
      neu = Collection::slice(remap_coll(src, memo, col.parent), col.slice_start, col.slice_end);
      break;
    case CollectionKind::CombProject:
      neu = Collection::comb_project(remap_coll(src, memo, col.parent), remap_axis(src, col.axis));
      break;
  }
  std::size_t before = table.collections().size();
  CollectionId id = table.intern_collection(std::move(neu));
  std::vector<Symbol> names;
  if (c.id < src.coll_names.size()) {
    for (Symbol s : src.coll_names[c.id]) names.push_back(remap_sym(src, s));
  }
  if (id.id == before) {
    coll_names.push_back(std::move(names));
  } else {
    auto& slot = coll_names[id.id];
    for (Symbol s : names) {
      bool seen = false;
      for (Symbol t : slot) {
        if (t == s) {
          seen = true;
          break;
        }
      }
      if (!seen) slot.push_back(s);
    }
  }
  memo.coll.emplace(c.id, id);
  return id;
}

HNode Merger::remap_node(const Hir& src, Memo& memo, std::size_t region_base, const HNode& n) {
  HNode out = n;
  switch (n.kind) {
    case HNode::Kind::Num:
    case HNode::Kind::Bool:
    case HNode::Kind::Unsupported:
      break;
    case HNode::Kind::Quantity:
      out.qid = remap_quant(src, memo, n.qid);
      break;
    case HNode::Kind::ElemSelfProp:
    case HNode::Kind::ReduceProp:
      out.prop = remap_prop(src, memo, n.prop);
      break;
    case HNode::Kind::Reduce:
      out.coll = remap_coll(src, memo, n.coll);
      if (n.a) out.a = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.a));
      break;
    case HNode::Kind::CollProp:
      out.coll = remap_coll(src, memo, n.coll);
      out.prop = remap_prop(src, memo, n.prop);
      break;
    case HNode::Kind::ScalarMinMax:
      out.items.clear();
      for (const auto& it : n.items) out.items.push_back(remap_node(src, memo, region_base, it));
      break;
    case HNode::Kind::Particle:
      out.particle = remap_particle(src, memo, n.particle);
      break;
    case HNode::Kind::CollValue:
      out.coll = remap_coll(src, memo, n.coll);
      break;
    case HNode::Kind::Neg:
    case HNode::Kind::Not:
    case HNode::Kind::Abs:
      if (n.a) out.a = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.a));
      break;
    case HNode::Kind::Binary:
    case HNode::Kind::Cmp:
      if (n.a) out.a = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.a));
      if (n.b) out.b = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.b));
      break;
    case HNode::Kind::And:
    case HNode::Kind::Or:
      out.items.clear();
      for (const auto& it : n.items) out.items.push_back(remap_node(src, memo, region_base, it));
      break;
    case HNode::Kind::Band:
      if (n.a) out.a = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.a));
      break;
    case HNode::Kind::Ternary:
      if (n.a) out.a = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.a));
      if (n.b) out.b = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.b));
      if (n.c) out.c = std::make_unique<HNode>(remap_node(src, memo, region_base, *n.c));
      break;
    case HNode::Kind::RegionPred:
      out.region_index = region_base + n.region_index;
      break;
  }
  return out;
}

HirRegionStmt Merger::remap_stmt(const Hir& src, Memo& memo, std::size_t region_base,
                                 const HirRegionStmt& s) {
  HirRegionStmt out = s;
  switch (s.kind) {
    case HirRegionStmt::Kind::Select:
    case HirRegionStmt::Kind::Reject:
    case HirRegionStmt::Kind::Trigger:
    case HirRegionStmt::Kind::Bin:
    case HirRegionStmt::Kind::BinCond:
      out.node = remap_node(src, memo, region_base, s.node);
      break;
    case HirRegionStmt::Kind::Inherit:
      out.region = region_base + s.region;
      break;
    case HirRegionStmt::Kind::NonMembership:
      break;
  }
  return out;
}

void Merger::add_unit(const Hir& src, const std::string& unit_label) {
  Memo memo;
  std::size_t region_base = region_name_order.size();
  for (std::size_t i = 0; i < src.regions.size(); ++i) {
    const auto& region = src.regions[i];
    std::vector<HirRegionStmt> stmts;
    stmts.reserve(region.stmts.size());
    for (const auto& s : region.stmts) stmts.push_back(remap_stmt(src, memo, region_base, s));
    std::string orig = src.symbols.display(region.name);
    Symbol name = symbols.intern(unit_label + "::" + orig);
    HirRegion r;
    r.name = name;
    r.stmts = std::move(stmts);
    r.span = region.span;
    regions.push_back(std::move(r));
    region_name_order.push_back(name);
    bool hl = i < src.histolist_regions.size() ? src.histolist_regions[i] : false;
    histolist_regions.push_back(hl);
  }
}

}  // namespace

Hir merge_hirs(const std::vector<const Hir*>& units) {
  Merger m;
  std::unordered_map<std::string, std::size_t> seen;
  std::unordered_set<std::string> taken;
  std::vector<std::string> labels;
  labels.reserve(units.size());
  for (const Hir* h : units) {
    std::string lk = SymbolTable::ascii_lower(h->unit);
    std::size_t n = ++seen[lk];
    std::string label = n == 1 ? h->unit : h->unit + "#" + std::to_string(n);
    while (!taken.insert(SymbolTable::ascii_lower(label)).second) {
      ++n;
      seen[lk] = n;
      label = h->unit + "#" + std::to_string(n);
    }
    labels.push_back(std::move(label));
  }
  for (std::size_t i = 0; i < units.size(); ++i) {
    m.unit_ord = static_cast<std::uint32_t>(i);
    m.add_unit(*units[i], labels[i]);
  }
  Hir out;
  for (std::size_t i = 0; i < units.size(); ++i) {
    if (i) out.unit += " + ";
    out.unit += units[i]->unit;
  }
  out.symbols = std::move(m.symbols);
  out.table = std::move(m.table);
  out.coll_names = std::move(m.coll_names);
  out.elem_preds = m.elem_preds.into_preds();
  out.regions = std::move(m.regions);
  out.region_name_order = std::move(m.region_name_order);
  out.histolist_regions = std::move(m.histolist_regions);
  return out;
}

}  // namespace adl2::sema
