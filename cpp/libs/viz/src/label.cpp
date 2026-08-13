#include "label.hpp"

#include "adl2/sema/ops.hpp"
#include "adl2/sema/quantity.hpp"

#include <string>
#include <vector>

namespace adl2::viz {
namespace {

bool is_ascii_alnum(char c) {
  return (c >= '0' && c <= '9') || (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z');
}

bool is_ascii_digit(char c) { return c >= '0' && c <= '9'; }

std::string join_parts(const std::vector<std::string>& parts, const char* sep) {
  std::string out;
  for (std::size_t i = 0; i < parts.size(); ++i) {
    if (i) out += sep;
    out += parts[i];
  }
  return out;
}

}  // namespace

std::string strip_coll_ids(const std::string& s) {
  std::string out;
  out.reserve(s.size());
  std::size_t i = 0;
  while (i < s.size()) {
    const bool boundary = i == 0 || !is_ascii_alnum(s[i - 1]);
    if (boundary && s[i] == 'C') {
      std::size_t j = i + 1;
      while (j < s.size() && is_ascii_digit(s[j])) ++j;
      if (j > i + 1 && j < s.size() && s[j] == '#') {
        i = j + 1;  // skip `C<digits>#`
        continue;
      }
    }
    out.push_back(s[i]);
    ++i;
  }
  return out;
}

std::string Labeler::collection(adl2::sema::CollectionId id) const {
  const auto& hir = *hir_;
  if (id.id < hir.coll_names.size() && !hir.coll_names[id.id].empty()) {
    return hir.symbols.display(hir.coll_names[id.id].front());
  }
  const auto& c = hir.table.collection(id);
  switch (c.kind) {
    case adl2::sema::CollectionKind::Base:
      return hir.symbols.display(c.base);
    case adl2::sema::CollectionKind::Filtered:
      return "filter(" + collection(c.parent) + ")";
    case adl2::sema::CollectionKind::Union: {
      std::vector<std::string> parts;
      parts.reserve(c.parts.size());
      for (auto p : c.parts) parts.push_back(collection(p));
      return "union(" + join_parts(parts, ", ") + ")";
    }
    case adl2::sema::CollectionKind::Combination: {
      std::vector<std::string> parts;
      parts.reserve(c.parts.size());
      for (auto p : c.parts) parts.push_back(collection(p));
      return "comb(" + join_parts(parts, ", ") + ")";
    }
    case adl2::sema::CollectionKind::Sorted:
      return "sort(" + collection(c.parent) + ")";
    case adl2::sema::CollectionKind::Slice: {
      std::string out = collection(c.parent) + "[" + std::to_string(c.slice_start) + ":";
      if (c.slice_end) out += std::to_string(*c.slice_end);
      out += "]";
      return out;
    }
    case adl2::sema::CollectionKind::CombProject:
      return collection(c.parent) + "->" + hir.symbols.display(c.axis.name);
  }
  return "?";
}

std::string Labeler::particle(const adl2::sema::ParticleRef& p) const {
  using adl2::sema::ParticleKind;
  switch (p.kind) {
    case ParticleKind::Elem:
      return collection(p.coll) + "[" + p.index.to_string() + "]";
    case ParticleKind::Whole:
      return collection(p.coll) + "[*]";
    case ParticleKind::Met:
      return "MET";
    case ParticleKind::Binder:
      return collection(p.coll) + "@" + hir_->symbols.display(p.name);
    case ParticleKind::ThisElem:
      return "this";
    case ParticleKind::ReduceElem:
      return "elem";
    case ParticleKind::Sum: {
      std::vector<std::string> parts;
      parts.reserve(p.parts.size());
      for (const auto& part : p.parts) parts.push_back(particle(part));
      return "(" + join_parts(parts, " + ") + ")";
    }
  }
  return "?";
}

std::string Labeler::quantity(const adl2::sema::Quantity& q) const {
  using adl2::sema::QuantityKind;
  using adl2::sema::ScalarSourceKind;
  const auto& t = hir_->table;
  switch (q.kind) {
    case QuantityKind::EventScalar:
      switch (q.scalar.kind) {
        case ScalarSourceKind::MetProp:
          return "MET." + t.prop_display(q.scalar.prop);
        case ScalarSourceKind::EventVar:
          return "evt." + hir_->symbols.display(q.scalar.name);
        case ScalarSourceKind::Trigger:
          return "trig(" + hir_->symbols.display(q.scalar.name) + ")";
      }
      break;
    case QuantityKind::Size:
      return "size(" + collection(q.coll) + ")";
    case QuantityKind::ElemProp:
      return collection(q.coll) + "[" + q.index.to_string() + "]." + t.prop_display(q.prop);
    case QuantityKind::AngularSep:
      return std::string(adl2::sema::ang_kind_str(q.ang)) + "(" + particle(q.a) + ", " +
             particle(q.b) + ")";
    case QuantityKind::ExternalFn: {
      std::vector<std::string> args;
      args.reserve(q.args.size());
      for (const auto& a : q.args) args.push_back(arg(a));
      return hir_->symbols.display(q.name) + "(" + join_parts(args, ", ") + ")";
    }
    case QuantityKind::Present:
      return "defined(" + quantity(t.quantity(q.inner)) + ")";
  }
  return "?";
}

std::string Labeler::arg(const adl2::sema::QuantityArg& a) const {
  using adl2::sema::QuantityArgKind;
  const auto& t = hir_->table;
  switch (a.kind) {
    case QuantityArgKind::Num:
      return a.text;
    case QuantityArgKind::Opaque:
      return strip_coll_ids(a.text);
    case QuantityArgKind::Quantity:
      return quantity(t.quantity(a.qid));
    case QuantityArgKind::Particle:
      return particle(a.particle);
    case QuantityArgKind::Collection:
      return collection(a.coll);
    case QuantityArgKind::CollProp:
      return collection(a.coll) + "[*]." + t.prop_display(a.prop);
  }
  return "?";
}

std::string Labeler::node(const adl2::sema::HNode& n) const {
  using Kind = adl2::sema::HNode::Kind;
  const auto& t = hir_->table;
  switch (n.kind) {
    case Kind::Num:
      return n.text;
    case Kind::Bool:
      return n.bool_val ? "true" : "false";
    case Kind::Quantity:
      return quantity(t.quantity(n.qid));
    case Kind::ElemSelfProp:
      return "this." + t.prop_display(n.prop);
    case Kind::ReduceProp:
      return "elem." + t.prop_display(n.prop);
    case Kind::Reduce:
      return std::string(adl2::sema::reduce_kind_str(n.reduce)) + "(" + collection(n.coll) +
             ": " + (n.a ? node(*n.a) : "?") + ")";
    case Kind::ScalarMinMax: {
      std::vector<std::string> inner;
      inner.reserve(n.items.size());
      for (const auto& a : n.items) inner.push_back(node(a));
      return std::string(adl2::sema::reduce_kind_str(n.reduce)) + "(" +
             join_parts(inner, ", ") + ")";
    }
    case Kind::CollProp:
      return collection(n.coll) + "[*]." + t.prop_display(n.prop);
    case Kind::Particle:
      return particle(n.particle);
    case Kind::CollValue:
      return collection(n.coll);
    case Kind::Neg:
      return "(- " + (n.a ? node(*n.a) : "?") + ")";
    case Kind::Not:
      return "(not " + (n.a ? node(*n.a) : "?") + ")";
    case Kind::Binary:
      return "(" + (n.a ? node(*n.a) : "?") + " " + adl2::sema::arith_op_str(n.arith) + " " +
             (n.b ? node(*n.b) : "?") + ")";
    case Kind::And:
      return joined(n.items, " and ");
    case Kind::Or:
      return joined(n.items, " or ");
    case Kind::Cmp:
      return "(" + (n.a ? node(*n.a) : "?") + " " + adl2::sema::cmp_op_str(n.cmp) + " " +
             (n.b ? node(*n.b) : "?") + ")";
    case Kind::Band: {
      const char* op = n.band == adl2::sema::BandKind::In ? "[]" : "][";
      return "(" + (n.a ? node(*n.a) : "?") + " " + op + " " + n.lo + " " + n.hi + ")";
    }
    case Kind::Ternary:
      if (n.c) {
        return "(" + (n.a ? node(*n.a) : "?") + " ? " + (n.b ? node(*n.b) : "?") + " : " +
               node(*n.c) + ")";
      }
      return "(" + (n.a ? node(*n.a) : "?") + " ? " + (n.b ? node(*n.b) : "?") + " : true)";
    case Kind::Abs:
      return "abs(" + (n.a ? node(*n.a) : "?") + ")";
    case Kind::RegionPred: {
      std::string name = "?";
      if (n.region_index < hir_->region_name_order.size()) {
        name = hir_->symbols.display(hir_->region_name_order[n.region_index]);
      }
      return "region:" + name;
    }
    case Kind::Unsupported:
      if (!n.tag.is_in_fragment()) {
        return "<unsupported: " + n.tag.reason + ">";
      }
      return "<unsupported>";
  }
  return "?";
}

std::string Labeler::joined(const std::vector<adl2::sema::HNode>& v, const char* sep) const {
  std::vector<std::string> parts;
  parts.reserve(v.size());
  for (const auto& n : v) parts.push_back(node(n));
  return "(" + join_parts(parts, sep) + ")";
}

}  // namespace adl2::viz
