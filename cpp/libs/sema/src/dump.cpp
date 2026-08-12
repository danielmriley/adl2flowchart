#include "adl2/sema/dump.hpp"

#include "adl2/syntax/dump.hpp"

#include <algorithm>
#include <sstream>

namespace adl2::sema {
namespace {

struct RenderCtx {
  const SymbolTable* symbols = nullptr;
  const QuantityTable* table = nullptr;
  const std::vector<std::vector<Symbol>>* coll_names = nullptr;
  const std::vector<Symbol>* region_names = nullptr;

  std::string coll(CollectionId id) const {
    std::string name;
    if (id.id < coll_names->size() && !(*coll_names)[id.id].empty()) {
      name = symbols->display((*coll_names)[id.id].front());
    } else {
      const Collection& c = table->collection(id);
      if (c.kind == CollectionKind::Base) {
        name = symbols->display(c.base);
      }
    }
    if (!name.empty()) return id.to_string() + "#" + name;
    return id.to_string();
  }

  std::string particle(const ParticleRef& p) const {
    switch (p.kind) {
      case ParticleKind::Elem:
        return coll(p.coll) + "[" + p.index.to_string() + "]";
      case ParticleKind::Whole:
        return coll(p.coll) + "[*]";
      case ParticleKind::Met:
        return "MET";
      case ParticleKind::Binder:
        return coll(p.coll) + "@" + symbols->display(p.name);
      case ParticleKind::ThisElem:
        return "this";
      case ParticleKind::ReduceElem:
        return "@elem";
      case ParticleKind::Sum: {
        std::string out = "(";
        for (std::size_t i = 0; i < p.parts.size(); ++i) {
          if (i) out += " + ";
          out += particle(p.parts[i]);
        }
        out += ")";
        return out;
      }
    }
    return "?";
  }

  std::string quantity(const Quantity& q) const {
    switch (q.kind) {
      case QuantityKind::EventScalar:
        switch (q.scalar.kind) {
          case ScalarSourceKind::MetProp:
            return "MET." + table->prop_display(q.scalar.prop);
          case ScalarSourceKind::EventVar:
            return "evt." + symbols->display(q.scalar.name);
          case ScalarSourceKind::Trigger:
            return "trig(" + symbols->display(q.scalar.name) + ")";
        }
        break;
      case QuantityKind::Size:
        return "size(" + coll(q.coll) + ")";
      case QuantityKind::ElemProp:
        return coll(q.coll) + "[" + q.index.to_string() + "]." +
               table->prop_display(q.prop);
      case QuantityKind::AngularSep:
        return std::string(ang_kind_str(q.ang)) + "(" + particle(q.a) + ", " +
               particle(q.b) + ")";
      case QuantityKind::ExternalFn: {
        std::string out = symbols->display(q.name) + "(";
        for (std::size_t i = 0; i < q.args.size(); ++i) {
          if (i) out += ", ";
          out += arg(q.args[i]);
        }
        out += ")";
        return out;
      }
      case QuantityKind::Present:
        return "defined(" + quantity(table->quantity(q.inner)) + ")";
    }
    return "?";
  }

  std::string arg(const QuantityArg& a) const {
    switch (a.kind) {
      case QuantityArgKind::Num:
      case QuantityArgKind::Opaque:
        return a.text;
      case QuantityArgKind::Quantity:
        return quantity(table->quantity(a.qid));
      case QuantityArgKind::Particle:
        return particle(a.particle);
      case QuantityArgKind::Collection:
        return coll(a.coll);
      case QuantityArgKind::CollProp:
        return coll(a.coll) + "[*]." + table->prop_display(a.prop);
    }
    return "?";
  }

  std::string node(const HNode& n) const {
    switch (n.kind) {
      case HNode::Kind::Num:
        return n.text;
      case HNode::Kind::Bool:
        return n.bool_val ? "true" : "false";
      case HNode::Kind::Quantity:
        return quantity(table->quantity(n.qid));
      case HNode::Kind::ElemSelfProp:
        return "this." + table->prop_display(n.prop);
      case HNode::Kind::ReduceProp:
        return "@elem." + table->prop_display(n.prop);
      case HNode::Kind::Reduce: {
        std::string s;
        if (n.has_slice) {
          s = "[" + std::to_string(n.slice_start) + ":";
          if (n.slice_end) s += std::to_string(*n.slice_end);
          s += "]";
        }
        return std::string(reduce_kind_str(n.reduce)) + "(" + coll(n.coll) + s +
               " of " + (n.a ? node(*n.a) : "?") + ")";
      }
      case HNode::Kind::CollProp:
        return coll(n.coll) + "[*]." + table->prop_display(n.prop);
      case HNode::Kind::ScalarMinMax: {
        std::string out = std::string(reduce_kind_str(n.reduce)) + "(";
        for (std::size_t i = 0; i < n.items.size(); ++i) {
          if (i) out += ", ";
          out += node(n.items[i]);
        }
        out += ")";
        return out;
      }
      case HNode::Kind::Particle:
        return particle(n.particle);
      case HNode::Kind::CollValue:
        return coll(n.coll);
      case HNode::Kind::Neg:
        return "(- " + (n.a ? node(*n.a) : "?") + ")";
      case HNode::Kind::Not:
        return "(not " + (n.a ? node(*n.a) : "?") + ")";
      case HNode::Kind::Binary:
        return "(" + (n.a ? node(*n.a) : "?") + " " + arith_op_str(n.arith) +
               " " + (n.b ? node(*n.b) : "?") + ")";
      case HNode::Kind::And:
        return joined(n.items, " and ");
      case HNode::Kind::Or:
        return joined(n.items, " or ");
      case HNode::Kind::Cmp:
        return "(" + (n.a ? node(*n.a) : "?") + " " + cmp_op_str(n.cmp) + " " +
               (n.b ? node(*n.b) : "?") + ")";
      case HNode::Kind::Band: {
        const char* op = n.band == BandKind::In ? "[]" : "][";
        return "(" + (n.a ? node(*n.a) : "?") + " " + op + " " + n.lo + " " +
               n.hi + ")";
      }
      case HNode::Kind::Ternary:
        if (n.c) {
          return "(" + (n.a ? node(*n.a) : "?") + " ? " +
                 (n.b ? node(*n.b) : "?") + " : " + node(*n.c) + ")";
        }
        return "(" + (n.a ? node(*n.a) : "?") + " ? " +
               (n.b ? node(*n.b) : "?") + " : true)";
      case HNode::Kind::Abs:
        return "abs(" + (n.a ? node(*n.a) : "?") + ")";
      case HNode::Kind::RegionPred: {
        std::string name = "?";
        if (n.region_index < region_names->size()) {
          name = symbols->display((*region_names)[n.region_index]);
        }
        return "region:" + name;
      }
      case HNode::Kind::Unsupported:
        if (!n.tag.in_fragment) {
          return "<unsupported: " + n.tag.reason + ">";
        }
        return "<unsupported>";
    }
    return "?";
  }

  std::string joined(const std::vector<HNode>& v, const char* sep) const {
    std::string out = "(";
    for (std::size_t i = 0; i < v.size(); ++i) {
      if (i) out += sep;
      out += node(v[i]);
    }
    out += ")";
    return out;
  }
};

RenderCtx render_ctx(const Hir& hir) {
  RenderCtx rc;
  rc.symbols = &hir.symbols;
  rc.table = &hir.table;
  rc.coll_names = &hir.coll_names;
  rc.region_names = &hir.region_name_order;
  return rc;
}

std::string option_u32_debug(const std::optional<std::uint32_t>& o) {
  if (!o) return "None";
  return "Some(" + std::to_string(*o) + ")";
}

}  // namespace

std::string render_node(const Hir& hir, const HNode& node) {
  return render_ctx(hir).node(node);
}

std::string render_node_raw(const SymbolTable& symbols, const QuantityTable& table,
                            const std::vector<std::vector<Symbol>>& coll_names,
                            const std::vector<Symbol>& region_names,
                            const HNode& node) {
  RenderCtx rc;
  rc.symbols = &symbols;
  rc.table = &table;
  rc.coll_names = &coll_names;
  rc.region_names = &region_names;
  return rc.node(node);
}

std::string collection_ref(const Hir& hir, CollectionId id) {
  return render_ctx(hir).coll(id);
}

std::string quantity_table_dump(const Hir& hir) {
  auto rc = render_ctx(hir);
  std::ostringstream out;
  out << "unit: " << hir.unit << "\n";
  out << "== collections ==\n";
  for (std::size_t i = 0; i < hir.table.collections().size(); ++i) {
    CollectionId id{static_cast<std::uint32_t>(i)};
    const Collection& coll = hir.table.collections()[i];
    std::vector<std::string> names;
    if (i < hir.coll_names.size()) {
      for (Symbol s : hir.coll_names[i]) names.push_back(hir.symbols.display(s));
    }
    std::string structure;
    switch (coll.kind) {
      case CollectionKind::Base:
        structure = "Base(" + hir.symbols.display(coll.base) + ")";
        break;
      case CollectionKind::Filtered:
        structure = "Filtered(parent=" + rc.coll(coll.parent) +
                    ", pred=" + coll.pred.to_string() + ")";
        break;
      case CollectionKind::Union: {
        std::string parts;
        for (std::size_t j = 0; j < coll.parts.size(); ++j) {
          if (j) parts += ", ";
          parts += rc.coll(coll.parts[j]);
        }
        structure = "Union(" + parts + ")";
        break;
      }
      case CollectionKind::Combination: {
        std::string parts;
        for (std::size_t j = 0; j < coll.parts.size(); ++j) {
          if (j) parts += ", ";
          parts += rc.coll(coll.parts[j]);
        }
        structure = "Combination[" + std::string(comb_kind_debug(coll.comb_kind)) +
                    "](" + parts + ")";
        break;
      }
      case CollectionKind::Sorted:
        structure = "Sorted(" + rc.coll(coll.parent) + ", " +
                    coll.sort_key.debug() + ", " + sort_dir_debug(coll.sort_dir) +
                    ")";
        break;
      case CollectionKind::Slice:
        structure = "Slice(" + rc.coll(coll.parent) + ", " +
                    std::to_string(coll.slice_start) + ".." +
                    option_u32_debug(coll.slice_end) + ")";
        break;
      case CollectionKind::CombProject:
        structure = "CombProject(" + rc.coll(coll.parent) + ", " +
                    coll.axis.debug() + ")";
        break;
    }
    std::string names_s;
    if (!names.empty()) {
      names_s = "  names=[";
      for (std::size_t j = 0; j < names.size(); ++j) {
        if (j) names_s += ", ";
        names_s += names[j];
      }
      names_s += "]";
    }
    out << id.to_string() << " = " << structure << names_s << "\n";
  }

  out << "== element predicates ==\n";
  for (std::size_t i = 0; i < hir.elem_preds.size(); ++i) {
    out << "P" << i << " = " << hir.elem_preds[i].render << "\n";
  }

  out << "== quantities ==\n";
  std::vector<std::string> lines;
  for (const auto& q : hir.table.quantities()) {
    const char* variant = "extfn  ";
    switch (q.kind) {
      case QuantityKind::EventScalar:
        variant = "scalar ";
        break;
      case QuantityKind::Size:
        variant = "size   ";
        break;
      case QuantityKind::ElemProp:
        variant = "elem   ";
        break;
      case QuantityKind::AngularSep:
        variant = q.oriented ? "ang(or)" : "ang    ";
        break;
      case QuantityKind::ExternalFn:
        variant = "extfn  ";
        break;
      case QuantityKind::Present:
        variant = "present";
        break;
    }
    lines.push_back(std::string(variant) + " " + rc.quantity(q));
  }
  std::sort(lines.begin(), lines.end());
  for (const auto& line : lines) out << line << "\n";
  return out.str();
}

std::string hir_dump(const Hir& hir) {
  auto rc = render_ctx(hir);
  std::ostringstream out;
  out << "unit: " << hir.unit << "\n";
  out << "== objects ==\n";
  for (const auto& obj : hir.objects) {
    std::string line = "object " + hir.symbols.display(obj.name) + " -> " +
                       rc.coll(obj.coll);
    if (obj.pure_alias_of) {
      line += "  (pure rename of " + rc.coll(*obj.pure_alias_of) + ")";
    }
    if (!obj.tag.in_fragment) {
      line += "  [unsupported: " + obj.tag.reason + "]";
    }
    out << line << "\n";
  }

  out << "== defines ==\n";
  for (const auto& def : hir.defines) {
    out << "define " << hir.symbols.display(def.name) << " ["
        << define_kind_str(def.kind) << "] = " << rc.node(def.body) << "\n";
  }

  out << "== regions ==\n";
  for (const auto& region : hir.regions) {
    out << "region " << hir.symbols.display(region.name) << "\n";
    for (const auto& stmt : region.stmts) {
      std::string line;
      switch (stmt.kind) {
        case HirRegionStmt::Kind::Select:
          line = "select " + rc.node(stmt.node);
          break;
        case HirRegionStmt::Kind::Reject:
          line = "reject " + rc.node(stmt.node);
          break;
        case HirRegionStmt::Kind::Inherit: {
          std::string name = "?";
          if (stmt.region < hir.region_name_order.size()) {
            name = hir.symbols.display(hir.region_name_order[stmt.region]);
          }
          line = "inherit " + name;
          break;
        }
        case HirRegionStmt::Kind::Trigger:
          line = "trigger " + rc.node(stmt.node);
          break;
        case HirRegionStmt::Kind::Bin: {
          std::string label;
          if (stmt.label) {
            label = " " + adl2::syntax::rust_debug_str(*stmt.label);
          }
          std::string edges;
          for (std::size_t i = 0; i < stmt.edges.size(); ++i) {
            if (i) edges += ", ";
            edges += stmt.edges[i];
          }
          line = "bin" + label + " " + rc.node(stmt.node) + " edges=[" + edges +
                 "]";
          break;
        }
        case HirRegionStmt::Kind::BinCond: {
          std::string label;
          if (stmt.label) {
            label = " " + adl2::syntax::rust_debug_str(*stmt.label);
          }
          line = "bin" + label + " " + rc.node(stmt.node);
          break;
        }
        case HirRegionStmt::Kind::NonMembership:
          if (stmt.tag.in_fragment) {
            line = std::string("(") + stmt.nm_kind + ": no membership effect)";
          } else {
            line = std::string("(") + stmt.nm_kind +
                   ": unsupported: " + stmt.tag.reason + ")";
          }
          break;
      }
      out << "  " << line << "\n";
    }
  }

  out << "== diagnostics ==\n";
  for (const auto& d : hir.diags) {
    out << severity_str(d.severity) << ": " << d.message << "\n";
  }
  return out.str();
}

}  // namespace adl2::sema
