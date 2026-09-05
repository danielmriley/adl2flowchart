#include "adl2/sema/dump.hpp"
#include "adl2/sema/hir.hpp"
#include "adl2/sema/ops.hpp"

#include <algorithm>
#include <cstdint>
#include <optional>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace adl2::sema {
namespace {

constexpr std::size_t CUTS_MAX = 64;

const std::vector<Symbol>& names_of(const Hir& hir, CollectionId id) {
  static const std::vector<Symbol> kEmpty;
  return id.id < hir.coll_names.size() ? hir.coll_names[id.id] : kEmpty;
}

std::size_t utf8_chars(const std::string& s) {
  std::size_t n = 0;
  for (unsigned char c : s) {
    if ((c & 0xC0) != 0x80) ++n;
  }
  return n;
}

std::string pad_right(const std::string& s, std::size_t w) {
  std::size_t n = utf8_chars(s);
  if (n >= w) return s;
  return s + std::string(w - n, ' ');
}

std::string ellipsize(const std::string& s, std::size_t max) {
  if (utf8_chars(s) <= max) return s;
  std::string t;
  std::size_t n = 0;
  for (std::size_t i = 0; i < s.size();) {
    unsigned char c = static_cast<unsigned char>(s[i]);
    std::size_t len = 1;
    if ((c & 0x80) == 0) len = 1;
    else if ((c & 0xE0) == 0xC0) len = 2;
    else if ((c & 0xF0) == 0xE0) len = 3;
    else len = 4;
    if (n + 1 >= max) break;
    t.append(s, i, len);
    i += len;
    ++n;
  }
  t += "…";
  return t;
}

struct Style {
  bool on = false;
  std::string wrap(const char* code, const std::string& s) const {
    if (!on) return s;
    return std::string("\x1b[") + code + "m" + s + "\x1b[0m";
  }
  std::string head(const std::string& s) const { return wrap("1", s); }
  std::string fragment(bool exact, const std::string& s) const {
    return wrap(exact ? "32" : "33", s);
  }
};

struct Row {
  std::string name;
  std::string chain;
  std::string cuts;
  std::string fragment;
  bool exact = true;
  std::vector<std::string> facts;
};

std::string collapse_names(const Hir& hir, const std::vector<Symbol>& names) {
  std::string out;
  for (std::size_t i = 0; i < names.size(); ++i) {
    if (i) out += " = ";
    out += hir.symbols.display(names[i]);
  }
  return out;
}

std::string coll_short(const Hir& hir, CollectionId id) {
  if (id.id < hir.coll_names.size() && !hir.coll_names[id.id].empty()) {
    return hir.symbols.display(hir.coll_names[id.id].front());
  }
  const Collection& c = hir.table.collection(id);
  if (c.kind == CollectionKind::Base) return hir.symbols.display(c.base);
  return id.to_string();
}

std::string join_parts(const Hir& hir, const std::vector<CollectionId>& parts) {
  std::string out;
  for (std::size_t i = 0; i < parts.size(); ++i) {
    if (i) out += " + ";
    out += coll_short(hir, parts[i]);
  }
  return out;
}

std::string link_name(const Hir& hir, CollectionId id, const std::vector<Symbol>& names) {
  if (names.empty()) return id.to_string();
  return collapse_names(hir, names);
}

std::string base_chain(const Hir& hir, CollectionId id) {
  std::vector<std::string> links;
  CollectionId cur = id;
  for (;;) {
    const std::vector<Symbol>& names = names_of(hir, cur);
    const Collection& coll = hir.table.collection(cur);
    if (coll.kind == CollectionKind::Filtered) {
      links.push_back(link_name(hir, cur, names));
      cur = coll.parent;
      continue;
    }
    if (coll.kind == CollectionKind::Base) {
      links.push_back(names.empty() ? hir.symbols.display(coll.base)
                                    : collapse_names(hir, names));
      break;
    }
    if (coll.kind == CollectionKind::Sorted || coll.kind == CollectionKind::Slice) {
      links.push_back(link_name(hir, cur, names));
      cur = coll.parent;
      continue;
    }
    if (coll.kind == CollectionKind::CombProject) {
      links.push_back(link_name(hir, cur, names));
      cur = coll.parent;
      continue;
    }
    if (coll.kind == CollectionKind::Union) {
      links.push_back(link_name(hir, cur, names));
      links.push_back(join_parts(hir, coll.parts));
      break;
    }
    if (coll.kind == CollectionKind::Combination) {
      links.push_back(link_name(hir, cur, names));
      links.push_back(join_parts(hir, coll.parts));
      break;
    }
    links.push_back(link_name(hir, cur, names));
    break;
  }
  std::string out;
  for (std::size_t i = 0; i < links.size(); ++i) {
    if (i) out += " <- ";
    out += links[i];
  }
  return out;
}

std::string unresolved_name(const std::string& reason) {
  const std::string p = "unresolved identifier `";
  if (reason.compare(0, p.size(), p) != 0) return {};
  if (reason.size() < p.size() + 1 || reason.back() != '`') return {};
  return reason.substr(p.size(), reason.size() - p.size() - 1);
}

std::string unsupported_term(const Fragment& tag) {
  if (tag.in_fragment) return "<?>";
  auto n = unresolved_name(tag.reason);
  if (!n.empty()) return n + "?";
  return "<" + tag.reason + ">";
}

std::string simplify_opaque(const std::string& text) {
  const std::string p = "<unsupported: ";
  if (text.compare(0, p.size(), p) == 0 && text.size() > p.size() && text.back() == '>') {
    std::string inner = text.substr(p.size(), text.size() - p.size() - 1);
    auto n = unresolved_name(inner);
    if (!n.empty()) return n + "?";
  }
  return text;
}

std::string render_particle(const Hir& hir, const ParticleRef& p);
std::string render_quantity(const Hir& hir, const Quantity& q);
std::string render_clause(const Hir& hir, const HNode& n);
std::string render_term(const Hir& hir, const HNode& n);

std::string render_arg(const Hir& hir, const QuantityArg& a) {
  switch (a.kind) {
    case QuantityArgKind::Num:
    case QuantityArgKind::Opaque:
      return simplify_opaque(a.text);
    case QuantityArgKind::Quantity:
      return render_quantity(hir, hir.table.quantity(a.qid));
    case QuantityArgKind::Particle:
      return render_particle(hir, a.particle);
    case QuantityArgKind::Collection:
      return coll_short(hir, a.coll);
    case QuantityArgKind::CollProp:
      return coll_short(hir, a.coll) + "." + hir.table.prop_display(a.prop);
  }
  return "?";
}

std::string render_particle(const Hir& hir, const ParticleRef& p) {
  switch (p.kind) {
    case ParticleKind::Elem:
      return coll_short(hir, p.coll) + "[" + p.index.to_string() + "]";
    case ParticleKind::Whole:
      return coll_short(hir, p.coll);
    case ParticleKind::Met:
      return "MET";
    case ParticleKind::Binder:
      return coll_short(hir, p.coll) + "@" + hir.symbols.display(p.name);
    case ParticleKind::ThisElem:
      return "this";
    case ParticleKind::ReduceElem:
      return "elem";
    case ParticleKind::Sum: {
      std::string out = "(";
      for (std::size_t i = 0; i < p.parts.size(); ++i) {
        if (i) out += " + ";
        out += render_particle(hir, p.parts[i]);
      }
      out += ")";
      return out;
    }
  }
  return "?";
}

std::string render_quantity(const Hir& hir, const Quantity& q) {
  switch (q.kind) {
    case QuantityKind::EventScalar:
      if (q.scalar.kind == ScalarSourceKind::MetProp) {
        return std::string("MET.") + hir.table.prop_display(q.scalar.prop);
      }
      if (q.scalar.kind == ScalarSourceKind::EventVar) {
        return hir.symbols.display(q.scalar.name);
      }
      return std::string("trig(") + hir.symbols.display(q.scalar.name) + ")";
    case QuantityKind::Size:
      return "size(" + coll_short(hir, q.coll) + ")";
    case QuantityKind::ElemProp:
      return coll_short(hir, q.coll) + "[" + q.index.to_string() + "]." +
             hir.table.prop_display(q.prop);
    case QuantityKind::AngularSep:
      return std::string(ang_kind_str(q.ang)) + "(" + render_particle(hir, q.a) + ", " +
             render_particle(hir, q.b) + ")";
    case QuantityKind::ExternalFn: {
      std::string out = hir.symbols.display(q.name) + "(";
      for (std::size_t i = 0; i < q.args.size(); ++i) {
        if (i) out += ", ";
        out += render_arg(hir, q.args[i]);
      }
      out += ")";
      return out;
    }
    case QuantityKind::Present:
      return "defined(" + render_quantity(hir, hir.table.quantity(q.inner)) + ")";
  }
  return "?";
}

std::string render_term(const Hir& hir, const HNode& n) {
  switch (n.kind) {
    case HNode::Kind::Num:
      return n.text;
    case HNode::Kind::Bool:
      return n.bool_val ? "true" : "false";
    case HNode::Kind::ElemSelfProp:
      return hir.table.prop_display(n.prop);
    case HNode::Kind::ReduceProp:
      return hir.table.prop_display(n.prop);
    case HNode::Kind::Reduce:
      return std::string(reduce_kind_str(n.reduce)) + "(" + coll_short(hir, n.coll) + ": " +
             (n.a ? render_clause(hir, *n.a) : "?") + ")";
    case HNode::Kind::CollProp:
      return coll_short(hir, n.coll) + "." + hir.table.prop_display(n.prop);
    case HNode::Kind::ScalarMinMax: {
      std::string out = std::string(reduce_kind_str(n.reduce)) + "(";
      for (std::size_t i = 0; i < n.items.size(); ++i) {
        if (i) out += ", ";
        out += render_term(hir, n.items[i]);
      }
      out += ")";
      return out;
    }
    case HNode::Kind::Quantity:
      return render_quantity(hir, hir.table.quantity(n.qid));
    case HNode::Kind::Abs:
      return "|" + (n.a ? render_term(hir, *n.a) : "?") + "|";
    case HNode::Kind::Neg:
      return "-" + (n.a ? render_term(hir, *n.a) : "?");
    case HNode::Kind::Binary:
      return (n.a ? render_term(hir, *n.a) : "?") + std::string(" ") + arith_op_str(n.arith) +
             " " + (n.b ? render_term(hir, *n.b) : "?");
    case HNode::Kind::Cmp:
    case HNode::Kind::And:
    case HNode::Kind::Or:
    case HNode::Kind::Not:
    case HNode::Kind::Band:
    case HNode::Kind::Ternary:
      return "(" + render_clause(hir, n) + ")";
    case HNode::Kind::Particle:
      return render_particle(hir, n.particle);
    case HNode::Kind::CollValue:
      return coll_short(hir, n.coll);
    case HNode::Kind::RegionPred:
      return "<region>";
    case HNode::Kind::Unsupported:
      return unsupported_term(n.tag);
  }
  return "?";
}

std::string render_clause(const Hir& hir, const HNode& n) {
  switch (n.kind) {
    case HNode::Kind::Cmp:
      return (n.a ? render_term(hir, *n.a) : "?") + std::string(" ") + cmp_op_str(n.cmp) +
             " " + (n.b ? render_term(hir, *n.b) : "?");
    case HNode::Kind::Not:
      return "not " + (n.a ? render_clause(hir, *n.a) : "?");
    case HNode::Kind::Or: {
      std::string out = "(";
      for (std::size_t i = 0; i < n.items.size(); ++i) {
        if (i) out += " or ";
        out += render_clause(hir, n.items[i]);
      }
      out += ")";
      return out;
    }
    case HNode::Kind::And: {
      std::string out = "(";
      for (std::size_t i = 0; i < n.items.size(); ++i) {
        if (i) out += " and ";
        out += render_clause(hir, n.items[i]);
      }
      out += ")";
      return out;
    }
    case HNode::Kind::Band: {
      const char* op = n.band == BandKind::In ? "in" : "out";
      return (n.a ? render_term(hir, *n.a) : "?") + std::string(" ") + op + " [" + n.lo +
             ", " + n.hi + "]";
    }
    case HNode::Kind::Ternary:
      if (n.c) {
        return (n.a ? render_clause(hir, *n.a) : "?") + " ? " +
               (n.b ? render_clause(hir, *n.b) : "?") + " : " + render_clause(hir, *n.c);
      }
      return (n.a ? render_clause(hir, *n.a) : "?") + " ? " +
             (n.b ? render_clause(hir, *n.b) : "?");
    default:
      return render_term(hir, n);
  }
}

void flatten_conj(const Hir& hir, const HNode& n, std::vector<std::string>& out) {
  if (n.kind == HNode::Kind::And) {
    for (const auto& c : n.items) flatten_conj(hir, c, out);
    return;
  }
  out.push_back(render_clause(hir, n));
}

std::pair<std::string, bool> element_cuts(const Hir& hir, const Collection& coll) {
  if (coll.kind != CollectionKind::Filtered) return {"—", true};
  const ElemPred& pred = hir.elem_pred(coll.pred);
  std::vector<std::string> parts;
  flatten_conj(hir, pred.node, parts);
  bool exact = !pred.node.has_unsupported();
  std::string text = parts.empty() ? "(all)" : parts[0];
  for (std::size_t i = 1; i < parts.size(); ++i) {
    text += ", ";
    text += parts[i];
  }
  return {text, exact};
}

std::optional<std::string> object_tag(const Hir& hir, CollectionId id) {
  for (const auto& o : hir.objects) {
    if (o.coll == id && !o.tag.in_fragment) return o.tag.reason;
  }
  return std::nullopt;
}

std::vector<std::string> derived_facts(const Hir& hir, CollectionId id, const Collection& coll) {
  switch (coll.kind) {
    case CollectionKind::Filtered:
      return {"size(" + coll_short(hir, id) + ") ≤ size(" + coll_short(hir, coll.parent) +
              ")  (subset of parent)"};
    case CollectionKind::Union: {
      std::string sum;
      for (std::size_t i = 0; i < coll.parts.size(); ++i) {
        if (i) sum += " + ";
        sum += "size(" + coll_short(hir, coll.parts[i]) + ")";
      }
      return {"size(" + coll_short(hir, id) + ") = " + sum +
              "  (disjoint parts ⇒ exact; else ≤)"};
    }
    case CollectionKind::Combination: {
      std::string names;
      for (std::size_t i = 0; i < coll.parts.size(); ++i) {
        if (i) names += ", ";
        names += coll_short(hir, coll.parts[i]);
      }
      return {std::string(comb_kind_debug(coll.comb_kind)) + " combination of " + names};
    }
    case CollectionKind::Sorted:
      return {"size(" + coll_short(hir, id) + ") = size(" + coll_short(hir, coll.parent) +
              ")  (permutation of source)"};
    case CollectionKind::Slice:
      return {"size(" + coll_short(hir, id) + ") ≤ size(" + coll_short(hir, coll.parent) +
              ")  (contiguous sub-range)"};
    case CollectionKind::CombProject:
      return {"size(" + coll_short(hir, id) + ") = size(" + coll_short(hir, coll.parent) +
              ")  (" + coll.axis.debug() + " axis)"};
    case CollectionKind::Base:
      return {};
  }
  return {};
}

std::optional<Row> build_row(const Hir& hir, CollectionId id, const Collection& coll) {
  const std::vector<Symbol>& names = names_of(hir, id);
  if (names.empty() && coll.kind == CollectionKind::Base) return std::nullopt;
  Row row;
  row.name = collapse_names(hir, names);
  row.chain = base_chain(hir, id);
  auto cuts = element_cuts(hir, coll);
  row.cuts = cuts.first;
  auto tag = object_tag(hir, id);
  row.exact = cuts.second && !tag;
  if (tag) {
    row.fragment = "partial: " + *tag;
  } else if (row.exact) {
    row.fragment = "exact";
  } else {
    row.fragment = "partial: cut out of fragment";
  }
  row.facts = derived_facts(hir, id, coll);
  return row;
}

}  // namespace

std::string object_table(const Hir& hir, bool color) {
  Style st;
  st.on = color;
  std::vector<Row> rows;
  const auto& colls = hir.table.collections();
  for (std::size_t i = 0; i < colls.size(); ++i) {
    CollectionId id{static_cast<std::uint32_t>(i)};
    if (auto r = build_row(hir, id, colls[i])) rows.push_back(std::move(*r));
  }
  std::ostringstream out;
  out << st.head("== objects ==") << "\n";
  if (rows.empty()) {
    out << "  (no collections)\n";
    return out.str();
  }
  for (auto& r : rows) r.cuts = ellipsize(r.cuts, CUTS_MAX);
  std::size_t name_w = 0, chain_w = 0, cuts_w = 0;
  for (const auto& r : rows) {
    name_w = std::max(name_w, utf8_chars(r.name));
    chain_w = std::max(chain_w, utf8_chars(r.chain));
    cuts_w = std::max(cuts_w, utf8_chars(r.cuts));
  }
  for (const auto& r : rows) {
    out << "  " << pad_right(r.name, name_w) << "  " << pad_right(r.chain, chain_w) << "  "
        << pad_right(r.cuts, cuts_w) << "  " << st.fragment(r.exact, r.fragment) << "\n";
    for (const auto& fact : r.facts) {
      out << "  " << pad_right("", name_w) << "    " << fact << "\n";
    }
  }
  return out.str();
}

}  // namespace adl2::sema
