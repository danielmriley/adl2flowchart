#include "adl2/formula/encode.hpp"

#include "adl2/sema/num.hpp"

#include <algorithm>
#include <cmath>
#include <map>
#include <optional>
#include <set>
#include <sstream>
#include <utility>
#include <vector>

namespace adl2::formula {
namespace {

using adl2::sema::Absence;
using adl2::sema::ArithOp;
using adl2::sema::BandKind;
using adl2::sema::CmpOp;
using adl2::sema::Collection;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::CombKind;
using adl2::sema::CompositeBinder;
using adl2::sema::ElemIndex;
using adl2::sema::ElemPredId;
using adl2::sema::HNode;
using adl2::sema::Hir;
using adl2::sema::HirRegionStmt;
using adl2::sema::NumVal;
using adl2::sema::ParticleKind;
using adl2::sema::ParticleRef;
using adl2::sema::Quantity;
using adl2::sema::QuantityArg;
using adl2::sema::QuantityArgKind;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::QuantityTable;
using adl2::sema::Rat;
using adl2::sema::ReduceKind;
using adl2::sema::ScalarSourceKind;
using adl2::sema::Span;
using adl2::sema::SymbolTable;

constexpr const char* MIN_PAIR_SHAPE =
    "angular separation over an unindexed collection outside "
    "`dR(A, B) <rel> <value>`: with ONE unindexed leg the interpreter has no "
    "value for it at all, and with two it reads the plain min-pair value here "
    "rather than the operator-scoped pair fold a leaf would mean";
constexpr const char* MIN_PAIR_THRESHOLD =
    "`>`/`>=`/`!=` against an unindexed angular separation needs a "
    "CONSTANT threshold: the pair fold reads the other operand first and "
    "soft-falses the whole cut when it has no value, which the vacuous "
    "empty-product / `+inf` readings cannot see";
constexpr const char* NOT_FAITHFUL =
    "approximate expression whose f64 arithmetic an exact fold cannot reproduce";
constexpr const char* MIXED_EDGE =
    "comparison mixes an exact event quantity with an approximate value "
    "(the interpreter rounds the exact side at the comparison edge)";

enum class DualKind { Open1, Any, All };
enum class MinPairKind { No, Fold, OutOfShape };
enum class Edge { Exact, F64, Unmodelable };
enum class LinErrKind { NonLinear, NonFinite, BadLiteral };

struct LinErr {
  LinErrKind kind = LinErrKind::NonLinear;
  std::string why;
  static LinErr nonlinear(std::string w) {
    LinErr e;
    e.kind = LinErrKind::NonLinear;
    e.why = std::move(w);
    return e;
  }
  static LinErr nonfinite() {
    LinErr e;
    e.kind = LinErrKind::NonFinite;
    return e;
  }
  static LinErr badlit() {
    LinErr e;
    e.kind = LinErrKind::BadLiteral;
    return e;
  }
};

/// Sparse linear combination. Terms stay sorted by QuantityId so dump
/// order matches the old `std::map` / smash2 `BTreeMap`. Zero coefficients
/// are kept (smash2 `combine`); `LinAtom::make` drops them.
struct LinExpr {
  std::vector<std::pair<QuantityId, Rat>> terms;
  Rat k;
  std::set<QuantityId> mentioned;

  static LinExpr constant(Rat c) {
    LinExpr e;
    e.k = std::move(c);
    return e;
  }
  static LinExpr quantity(QuantityId q) {
    LinExpr e;
    e.terms.emplace_back(q, Rat::one());
    e.mentioned.insert(q);
    return e;
  }
  void add_term(QuantityId q, Rat c) {
    auto it = std::lower_bound(terms.begin(), terms.end(), q,
                               [](const std::pair<QuantityId, Rat>& t, QuantityId id) {
                                 return t.first < id;
                               });
    if (it != terms.end() && it->first == q) {
      it->second = it->second + c;
    } else {
      terms.insert(it, {q, std::move(c)});
    }
  }
  LinExpr combine(const LinExpr& o, bool negate) const {
    LinExpr out = *this;
    for (const auto& kv : o.terms) {
      Rat c = negate ? -kv.second : kv.second;
      out.add_term(kv.first, std::move(c));
    }
    out.k = negate ? (out.k - o.k) : (out.k + o.k);
    out.mentioned.insert(o.mentioned.begin(), o.mentioned.end());
    return out;
  }
  LinExpr sub(const LinExpr& o) const { return combine(o, true); }
  LinExpr scale(const Rat& c) const {
    LinExpr out;
    out.k = k * c;
    out.mentioned = mentioned;
    out.terms.reserve(terms.size());
    for (const auto& kv : terms) out.terms.emplace_back(kv.first, kv.second * c);
    return out;
  }
};

std::optional<Rat> parse_rat(const std::string& s) {
  try {
    std::size_t idx = 0;
    double v = std::stod(s, &idx);
    if (idx != s.size()) return std::nullopt;
    return Rat::from_decimal_f64(v);
  } catch (...) {
    return std::nullopt;
  }
}

Rel rel_of(CmpOp op) {
  switch (op) {
    case CmpOp::Gt: return Rel::Gt;
    case CmpOp::Lt: return Rel::Lt;
    case CmpOp::Ge: return Rel::Ge;
    case CmpOp::Le: return Rel::Le;
    case CmpOp::Eq: return Rel::Eq;
    case CmpOp::Ne:
    case CmpOp::ApproxEq: return Rel::Ne;
  }
  return Rel::Eq;
}

std::vector<const HNode*> hnode_children(const HNode& node) { return node.children(); }

std::optional<std::pair<Span, std::string>> first_unsupported(const HNode& node) {
  if (!node.tag.is_in_fragment()) return std::make_pair(node.span, node.tag.reason);
  for (const HNode* c : hnode_children(node)) {
    if (auto u = first_unsupported(*c)) return u;
  }
  return std::nullopt;
}

void collect_collprops(const HNode& node, std::set<CollectionId>& out) {
  if (node.kind == HNode::Kind::CollProp) out.insert(node.coll);
  for (const HNode* c : hnode_children(node)) collect_collprops(*c, out);
}

void collect_quantities(const HNode& node, std::set<QuantityId>& out) {
  if (node.kind == HNode::Kind::Quantity) out.insert(node.qid);
  for (const HNode* c : hnode_children(node)) collect_quantities(*c, out);
}

bool quantity_is_exact(const QuantityTable& table, QuantityId q) {
  auto k = table.quantity(q).kind;
  return k == QuantityKind::EventScalar || k == QuantityKind::Size ||
         k == QuantityKind::ElemProp;
}

bool is_const_tree(const HNode& node) {
  switch (node.kind) {
    case HNode::Kind::Num: return true;
    case HNode::Kind::Neg:
    case HNode::Kind::Abs:
      return node.a && is_const_tree(*node.a);
    case HNode::Kind::Binary:
      return node.a && node.b && is_const_tree(*node.a) && is_const_tree(*node.b);
    case HNode::Kind::ScalarMinMax:
      for (const auto& a : node.items) {
        if (!is_const_tree(a)) return false;
      }
      return true;
    default: return false;
  }
}

bool is_exact_valued(const HNode& node, const QuantityTable& table) {
  switch (node.kind) {
    case HNode::Kind::Num: return true;
    case HNode::Kind::Quantity: return quantity_is_exact(table, node.qid);
    case HNode::Kind::ElemSelfProp:
    case HNode::Kind::ReduceProp:
    case HNode::Kind::CollProp: return true;
    case HNode::Kind::Neg:
    case HNode::Kind::Abs:
      return node.a && is_exact_valued(*node.a, table);
    case HNode::Kind::Binary:
      if (node.arith == ArithOp::Pow) return false;
      return node.a && node.b && is_exact_valued(*node.a, table) &&
             is_exact_valued(*node.b, table);
    case HNode::Kind::ScalarMinMax:
      for (const auto& a : node.items) {
        if (!is_exact_valued(a, table)) return false;
      }
      return true;
    case HNode::Kind::Bool:
    case HNode::Kind::Not:
    case HNode::Kind::And:
    case HNode::Kind::Or:
    case HNode::Kind::Cmp:
    case HNode::Kind::Band:
    case HNode::Kind::RegionPred: return true;
    case HNode::Kind::Reduce:
      return reduce_kind_is_boolean(node.reduce) ||
             (node.a && is_exact_valued(*node.a, table));
    case HNode::Kind::Ternary:
      return node.b && is_exact_valued(*node.b, table) &&
             (!node.c || is_exact_valued(*node.c, table));
    default: return false;
  }
}

Edge edge_mode(const HNode& node, const QuantityTable& table) {
  return is_exact_valued(node, table) ? Edge::Exact : Edge::F64;
}

bool is_power_of_two_rat(const Rat& r) {
  if (r.is_zero()) return false;
  auto p = r.abs().to_parts();
  auto pow2 = [](const std::string& s) {
    try {
      unsigned long long n = std::stoull(s);
      return n != 0 && (n & (n - 1)) == 0;
    } catch (...) {
      return false;
    }
  };
  return (p.numerator == "1" && pow2(p.denominator)) ||
         (p.denominator == "1" && pow2(p.numerator));
}

LinErr const_tree_num_err;
std::optional<NumVal> const_tree_num(const HNode& node);

std::optional<NumVal> const_tree_num(const HNode& node) {
  switch (node.kind) {
    case HNode::Kind::Num: {
      auto r = parse_rat(node.text);
      if (!r) return std::nullopt;
      return NumVal::from_exact(*r);
    }
    case HNode::Kind::Neg:
      if (!node.a) return std::nullopt;
      if (auto v = const_tree_num(*node.a)) return v->negated();
      return std::nullopt;
    case HNode::Kind::Abs:
      if (!node.a) return std::nullopt;
      if (auto v = const_tree_num(*node.a)) return v->abs();
      return std::nullopt;
    case HNode::Kind::Binary: {
      if (!node.a || !node.b) return std::nullopt;
      auto a = const_tree_num(*node.a);
      auto b = const_tree_num(*node.b);
      if (!a || !b) return std::nullopt;
      return adl2::sema::bin_arith(node.arith, *a, *b);
    }
    case HNode::Kind::ScalarMinMax: {
      std::optional<NumVal> acc;
      for (const auto& a : node.items) {
        auto v = const_tree_num(a);
        if (!v) return std::nullopt;
        if (!acc) acc = *v;
        else if (node.reduce == ReduceKind::Min)
          acc = adl2::sema::num_min(*acc, *v);
        else
          acc = adl2::sema::num_max(*acc, *v);
      }
      return acc;
    }
    default: return std::nullopt;
  }
}

std::optional<Rat> const_tree_rat(const HNode& node, LinErr& err) {
  auto v = const_tree_num(node);
  if (!v) {
    if (node.kind == HNode::Kind::Num) err = LinErr::badlit();
    else err = LinErr::nonfinite();
    return std::nullopt;
  }
  if (v->kind == NumVal::Kind::Exact) return v->exact;
  auto r = Rat::from_decimal_f64(v->approx);
  if (!r) {
    err = LinErr::nonfinite();
    return std::nullopt;
  }
  return r;
}

bool is_f64_exact_approx(const HNode& node) {
  switch (node.kind) {
    case HNode::Kind::Quantity: return true;
    case HNode::Kind::Reduce:
      return !reduce_kind_is_boolean(node.reduce);
    case HNode::Kind::Neg:
    case HNode::Kind::Abs:
      return node.a && is_f64_exact_approx(*node.a);
    case HNode::Kind::Binary: {
      auto pow2_scale = [&](const HNode& side, const HNode& other) {
        if (!is_const_tree(side)) return false;
        LinErr e;
        auto c = const_tree_rat(side, e);
        return c && is_power_of_two_rat(*c) && is_f64_exact_approx(other);
      };
      if (!node.a || !node.b) return false;
      if (node.arith == ArithOp::Mul)
        return pow2_scale(*node.a, *node.b) || pow2_scale(*node.b, *node.a);
      if (node.arith == ArithOp::Div) return pow2_scale(*node.b, *node.a);
      return false;
    }
    default: return false;
  }
}

bool flattens_faithfully(const HNode& node, const QuantityTable& table) {
  return is_exact_valued(node, table) || is_f64_exact_approx(node);
}

Edge edge_mode_pair(const HNode& lhs, const HNode& rhs, const QuantityTable& table) {
  auto a = edge_mode(lhs, table);
  auto b = edge_mode(rhs, table);
  if (a == Edge::Exact && b == Edge::Exact) return Edge::Exact;
  if (a == Edge::F64 && b == Edge::F64) return Edge::F64;
  if (a == Edge::F64 && b == Edge::Exact && is_const_tree(rhs)) return Edge::F64;
  if (a == Edge::Exact && b == Edge::F64 && is_const_tree(lhs)) return Edge::F64;
  return Edge::Unmodelable;
}

std::string slice_key(bool has, std::uint32_t start, const std::optional<std::uint32_t>& end) {
  if (!has) return "";
  if (end) return "[" + std::to_string(start) + ":" + std::to_string(*end) + "]";
  return "[" + std::to_string(start) + ":]";
}

std::optional<std::string> reduce_body_key(const HNode& node) {
  if (!node.tag.is_in_fragment()) return std::nullopt;
  switch (node.kind) {
    case HNode::Kind::Num: return "#" + node.text;
    case HNode::Kind::Quantity: return "Q" + std::to_string(node.qid.id);
    case HNode::Kind::ReduceProp: return "RP" + std::to_string(node.prop.id);
    case HNode::Kind::CollProp:
      return "CP" + std::to_string(node.coll.id) + "." + std::to_string(node.prop.id);
    case HNode::Kind::Neg: {
      if (!node.a) return std::nullopt;
      auto k = reduce_body_key(*node.a);
      if (!k) return std::nullopt;
      return "(neg " + *k + ")";
    }
    case HNode::Kind::Abs: {
      if (!node.a) return std::nullopt;
      auto k = reduce_body_key(*node.a);
      if (!k) return std::nullopt;
      return "(abs " + *k + ")";
    }
    case HNode::Kind::Binary: {
      if (!node.a || !node.b) return std::nullopt;
      auto l = reduce_body_key(*node.a);
      auto r = reduce_body_key(*node.b);
      if (!l || !r) return std::nullopt;
      return std::string("(") + adl2::sema::arith_op_str(node.arith) + " " + *l +
             " " + *r + ")";
    }
    case HNode::Kind::Reduce: {
      if (!node.a) return std::nullopt;
      auto b = reduce_body_key(*node.a);
      if (!b) return std::nullopt;
      return std::string(adl2::sema::reduce_kind_str(node.reduce)) + "[C" +
             std::to_string(node.coll.id) +
             slice_key(node.has_slice, node.slice_start, node.slice_end) + " " +
             *b + "]";
    }
    case HNode::Kind::ScalarMinMax: {
      std::string s = std::string(adl2::sema::reduce_kind_str(node.reduce)) + "<";
      for (const auto& a : node.items) {
        auto k = reduce_body_key(a);
        if (!k) return std::nullopt;
        s += *k;
        s += ",";
      }
      s += ">";
      return s;
    }
    default: return std::nullopt;
  }
}

struct Encoder {
  QuantityTable* table;
  const std::vector<adl2::sema::HirRegion>* regions;
  SymbolTable* symbols;
  const std::vector<adl2::sema::ElemPred>* elem_preds = nullptr;
  DiagTable diags;
  std::vector<std::size_t> stack;

  // smash3 oracle: Size is Never. smash2_cpp had an encoder-local Hard
  // override for Size of out-of-fragment filters; that extra presence
  // guard does not appear in smash3 --dump-formula.
  Formula unknown(Span span, std::string reason) {
    return Formula::unknown(diags.push(span, std::move(reason)));
  }
  Formula simple_atom(QuantityId q, Rel rel, std::int64_t k) {
    return Formula::of_atom(LinAtom::single(q, rel, Rat::from_i64(k)));
  }
  Formula present_atom(QuantityId q) {
    auto p = table->intern_quantity(Quantity::present(q));
    return Formula::of_atom(LinAtom::single(p, Rel::Ge, Rat::one()));
  }

  Formula guard_presence(const std::vector<QuantityId>& quants, Formula inner) {
    if (!inner.is_exact()) return inner;
    std::vector<QuantityId> needed;
    for (auto q : quants) {
      if (table->may_be_absent(q)) needed.push_back(q);
    }
    if (needed.empty()) return inner;
    std::vector<Formula> parts;
    for (auto q : needed) parts.push_back(present_atom(q));
    parts.push_back(std::move(inner));
    return fand(std::move(parts));
  }
  Formula guard_presence_set(const std::set<QuantityId>& qs, Formula inner) {
    return guard_presence(std::vector<QuantityId>(qs.begin(), qs.end()), std::move(inner));
  }
  Formula guard_presence_node(const HNode& node, Formula inner) {
    std::set<QuantityId> qs;
    collect_quantities(node, qs);
    return guard_presence_set(qs, std::move(inner));
  }

  void hard_quantities(const Formula& f, std::set<QuantityId>& out) {
    switch (f.kind) {
      case Formula::Kind::True:
      case Formula::Kind::False:
      case Formula::Kind::Unknown:
        break;
      case Formula::Kind::Atom:
        for (const auto& t : f.atom.terms()) {
          if (table->absence(t.second) == Absence::Hard) out.insert(t.second);
        }
        break;
      case Formula::Kind::And:
      case Formula::Kind::Or:
        for (const auto& p : f.items) hard_quantities(p, out);
        break;
      case Formula::Kind::Dual:
        hard_quantities(*f.plus, out);
        hard_quantities(*f.minus, out);
        break;
    }
  }

  static bool is_presence_atom(const QuantityTable& table, const Formula& f) {
    if (f.kind != Formula::Kind::Atom) return false;
    if (f.atom.terms().size() != 1) return false;
    return table.quantity(f.atom.terms()[0].second).kind == QuantityKind::Present;
  }

  static Formula drop_absent(const Formula& f, const std::set<QuantityId>& present) {
    if (f.kind == Formula::Kind::Atom) {
      const auto& ts = f.atom.terms();
      if (ts.size() == 1 && present.count(ts[0].second) && f.atom.rel() == Rel::Lt &&
          ts[0].first.is_one() && f.atom.constant().is_one()) {
        return Formula::ffalse();
      }
      return f;
    }
    if (f.kind == Formula::Kind::And) {
      std::vector<Formula> v;
      for (const auto& p : f.items) v.push_back(drop_absent(p, present));
      return fand(std::move(v));
    }
    if (f.kind == Formula::Kind::Or) {
      std::vector<Formula> v;
      for (const auto& p : f.items) v.push_back(drop_absent(p, present));
      return forr(std::move(v));
    }
    if (f.kind == Formula::Kind::Dual) {
      return Formula::dual(drop_absent(*f.plus, present), drop_absent(*f.minus, present),
                           f.diag);
    }
    return f;
  }

  std::set<QuantityId> forced_by_falsity(const Formula& f) {
    switch (f.kind) {
      case Formula::Kind::Atom: {
        for (const auto& t : f.atom.terms()) {
          if (table->absence(t.second) == Absence::Soft) return {};
        }
        std::set<QuantityId> s;
        for (const auto& t : f.atom.terms()) s.insert(t.second);
        return s;
      }
      case Formula::Kind::Or: {
        std::set<QuantityId> s;
        for (const auto& p : f.items) {
          auto x = forced_by_falsity(p);
          s.insert(x.begin(), x.end());
        }
        return s;
      }
      case Formula::Kind::And: {
        std::optional<std::set<QuantityId>> acc;
        for (const auto& p : f.items) {
          if (is_presence_atom(*table, p)) continue;
          auto s = forced_by_falsity(p);
          if (!acc) acc = std::move(s);
          else {
            std::set<QuantityId> inter;
            for (auto q : *acc) {
              if (s.count(q)) inter.insert(q);
            }
            acc = std::move(inter);
            if (acc->empty()) break;
          }
        }
        return acc.value_or(std::set<QuantityId>{});
      }
      default:
        return {};
    }
  }

  Formula negate(Formula f) {
    std::set<QuantityId> hard;
    hard_quantities(f, hard);
    if (hard.empty()) return f.fnot();
    auto forced_all = forced_by_falsity(f);
    std::set<QuantityId> forced;
    for (auto q : forced_all) {
      if (hard.count(q)) forced.insert(q);
    }
    auto n = f.fnot();
    std::set<QuantityId> both, all;
    for (auto q : forced) both.insert(table->intern_quantity(Quantity::present(q)));
    for (auto q : hard) all.insert(table->intern_quantity(Quantity::present(q)));
    auto guarded = [&](const std::set<QuantityId>& present) {
      std::vector<Formula> parts;
      for (auto p : present)
        parts.push_back(Formula::of_atom(LinAtom::single(p, Rel::Ge, Rat::one())));
      parts.push_back(drop_absent(n, present));
      return fand(std::move(parts));
    };
    if (both == all) return guarded(both);
    auto plus = guarded(both);
    auto minus = guarded(all);
    auto why = diags.push(
        Span{},
        "negation over an event-level datum the interpreter may never read "
        "(one decidably-false conjunct of an `and`, or a reducer over an "
        "empty collection, settles the negation without it): the superset "
        "requires only the data every false-route reads, the subset requires "
        "all of it");
    return Formula::dual(std::move(plus), std::move(minus), why);
  }

  Formula region(std::size_t idx, Span span);
  std::optional<Formula> stmt(const HirRegionStmt& s);
  Formula boolean(const HNode& node);
  Formula leaf(const HNode& node);
  Formula leaf_inner(const HNode& node);
  Formula guard_existence(const HNode& node, Formula inner);
  std::vector<Formula> needs_guards(const std::map<CollectionId, std::uint32_t>& needs);
  Formula build_dual(DualKind kind, QuantityId size_q, const std::vector<Formula>& instances,
                     DiagId why);
  Formula encode_reduce(ReduceKind kind, CollectionId coll, const HNode& body, Span span);
  Formula encode_static_slice_reduce(DualKind kind, CollectionId source, std::uint32_t start,
                                     std::uint32_t n, const HNode& body);
  HNode subst(const HNode& node, CollectionId coll, std::uint32_t index);
  HNode subst_reduce(const HNode& node, CollectionId coll, std::uint32_t index);
  QuantityId subst_reduce_quantity(QuantityId q, CollectionId coll, std::uint32_t index);
  Formula cmp(CmpOp op, const HNode& lhs, const HNode& rhs, Span span);
  Formula cmp_linear(CmpOp op, const HNode& lhs, const HNode& rhs, Span span);
  Formula min_pair_cmp(QuantityId q, CmpOp folded, CmpOp op, const HNode& lhs,
                       const HNode& rhs, Span span);
  MinPairKind min_pair_shape(CmpOp op, const HNode& lhs, const HNode& rhs, QuantityId& q,
                             CmpOp& folded);
  bool is_constant(const HNode& node);
  Formula empty_product(QuantityId q);
  std::optional<QuantityId> bare_min_pair(const HNode& node);
  bool mentions_unindexed(const HNode& node);
  Formula pattern(const HNode& side, Rel rel, Rat c, const std::string& why, Span span);
  Formula opaque_atom(const HNode& side, Rel rel, Rat c, const std::string& why, Span span);
  Formula cmp_node_const(const HNode& node, Rel rel, Rat c, Span span);
  Formula ratio(const HNode& whole, const HNode& num, const HNode& den, Rel rel, Rat c,
                Span span);
  Formula abs_cmp(const HNode& abs_node, const HNode& inner, Rel rel, Rat c, Span span);
  Formula band(BandKind kind, const HNode& expr, const std::string& lo, const std::string& hi,
               Span span);
  Formula lin_err(const LinErr& e, const std::string& what, Span span);
  static Rat at_edge(Edge mode, Rat k);
  Formula atom_of(std::vector<std::pair<QuantityId, Rat>> terms, Rel rel, Rat k);
  std::optional<QuantityId> intern_reduce(ReduceKind kind, CollectionId coll, const HNode& body,
                                          bool has_slice, std::uint32_t start,
                                          const std::optional<std::uint32_t>& end);
  std::optional<QuantityId> intern_opaque_scalar(const HNode& node);
  std::optional<LinExpr> lin(const HNode& node, LinErr& err);
  std::optional<LinExpr> lin_or_opaque(const HNode& node, LinErr& err);
  std::optional<LinExpr> lin_binary(ArithOp op, const HNode& lhs, const HNode& rhs, LinErr& err);
  std::optional<Formula> try_comb_existence(
      const std::vector<std::pair<QuantityId, Rat>>& terms, Rel rel, const Rat& k, Span span);
  std::vector<std::vector<std::uint32_t>> binder_index_tuples(
      const std::vector<CollectionId>& parts, CombKind kind);
  Formula encode_tuple_cuts(const std::vector<ElemPredId>& cuts,
                            const std::vector<CompositeBinder>& members,
                            const std::vector<CollectionId>& parts,
                            const std::vector<std::uint32_t>& idx);
  bool has_residual_binder(const HNode& node);
  HNode subst_binders(const HNode& node, const std::vector<CompositeBinder>& members,
                      const std::vector<CollectionId>& parts,
                      const std::vector<std::uint32_t>& idx);
  QuantityId subst_binder_quantity(QuantityId q, const std::vector<CompositeBinder>& members,
                                   const std::vector<CollectionId>& parts,
                                   const std::vector<std::uint32_t>& idx);
};

Formula Encoder::region(std::size_t idx, Span span) {
  if (idx >= regions->size()) return unknown(span, "reference to an unknown region");
  for (auto s : stack) {
    if (s == idx) return unknown(span, "region inheritance cycle");
  }
  stack.push_back(idx);
  std::vector<Formula> parts;
  for (const auto& st : (*regions)[idx].stmts) {
    if (auto f = stmt(st)) parts.push_back(std::move(*f));
  }
  stack.pop_back();
  return fand(std::move(parts));
}

std::optional<Formula> Encoder::stmt(const HirRegionStmt& s) {
  using K = HirRegionStmt::Kind;
  switch (s.kind) {
    case K::Select:
    case K::Trigger:
      return boolean(s.node);
    case K::Reject:
      if (s.node.kind == HNode::Kind::Not && s.node.a) return boolean(*s.node.a);
      return negate(boolean(s.node));
    case K::Inherit:
      return region(s.region, s.span);
    case K::Bin:
    case K::BinCond:
      return std::nullopt;
    case K::NonMembership:
      if (!s.tag.is_in_fragment()) return unknown(s.span, s.tag.reason);
      return std::nullopt;
  }
  return std::nullopt;
}

Formula Encoder::boolean(const HNode& node) {
  if (!node.tag.is_in_fragment()) return unknown(node.span, node.tag.reason);
  switch (node.kind) {
    case HNode::Kind::Bool:
      return node.bool_val ? Formula::ttrue() : Formula::ffalse();
    case HNode::Kind::And: {
      std::vector<Formula> parts;
      for (const auto& n : node.items) parts.push_back(boolean(n));
      return fand(std::move(parts));
    }
    case HNode::Kind::Or: {
      std::vector<Formula> parts;
      for (const auto& n : node.items) parts.push_back(boolean(n));
      return forr(std::move(parts));
    }
    case HNode::Kind::Not:
      if (node.a && node.a->kind == HNode::Kind::Not && node.a->a)
        return boolean(*node.a->a);
      if (node.a) return negate(boolean(*node.a));
      return unknown(node.span, "expression is not a boolean condition");
    case HNode::Kind::Cmp:
    case HNode::Kind::Band:
      return leaf(node);
    case HNode::Kind::Ternary: {
      if (!node.a || !node.b) return unknown(node.span, "expression is not a boolean condition");
      auto g = boolean(*node.a);
      auto t = boolean(*node.b);
      auto e = node.c ? boolean(*node.c) : Formula::ttrue();
      auto ng = negate(g);
      return forr({fand({std::move(g), std::move(t)}), fand({std::move(ng), std::move(e)})});
    }
    case HNode::Kind::Quantity: {
      const auto& q = table->quantity(node.qid);
      if (q.kind == QuantityKind::EventScalar &&
          q.scalar.kind == ScalarSourceKind::Trigger) {
        auto a = simple_atom(node.qid, Rel::Eq, 1);
        return guard_presence({node.qid}, std::move(a));
      }
      return unknown(node.span, "numeric quantity used as a boolean condition");
    }
    case HNode::Kind::RegionPred:
      return region(node.region_index, node.span);
    case HNode::Kind::Num:
      return unknown(node.span, "numeric literal used as a boolean condition");
    case HNode::Kind::CollProp:
      return unknown(node.span, "unindexed collection property used as a bare boolean");
    case HNode::Kind::Reduce:
      if (reduce_kind_is_boolean(node.reduce) && node.a)
        return encode_reduce(node.reduce, node.coll, *node.a, node.span);
      return unknown(node.span, std::string("`") + adl2::sema::reduce_kind_str(node.reduce) +
                                    "` reducer is not a boolean condition");
    default:
      return unknown(node.span, "expression is not a boolean condition");
  }
}

Formula Encoder::leaf(const HNode& node) {
  if (auto u = first_unsupported(node)) return unknown(u->first, u->second);
  std::set<CollectionId> colls;
  collect_collprops(node, colls);
  if (colls.empty()) {
    auto inner = leaf_inner(node);
    return guard_existence(node, std::move(inner));
  }
  if (colls.size() == 1) {
    CollectionId coll = *colls.begin();
    auto size_q = table->intern_quantity(Quantity::size(coll));
    std::vector<Formula> instances;
    for (std::uint32_t i = 0; i < OPEN1_BOUND; ++i) {
      auto inst = subst(node, coll, i);
      instances.push_back(leaf(inst));
    }
    std::ostringstream why;
    why << "unindexed collection cut: ∀/∃ reading unresolved (OPEN-1); "
           "Dual bounded expansion k="
        << OPEN1_BOUND;
    auto id = diags.push(node.span, why.str());
    return build_dual(DualKind::Open1, size_q, instances, id);
  }
  return unknown(node.span,
                 "comparison references more than one unindexed collection (OPEN-1)");
}

Formula Encoder::guard_existence(const HNode& node, Formula inner) {
  if (!inner.is_exact()) return inner;
  std::map<CollectionId, std::uint32_t> needs;
  std::set<QuantityId> qids;
  collect_quantities(node, qids);
  for (auto q : qids) table->existence_floor(q, needs);
  if (needs.empty()) return inner;
  auto parts = needs_guards(needs);
  parts.push_back(std::move(inner));
  return fand(std::move(parts));
}

std::vector<Formula> Encoder::needs_guards(
    const std::map<CollectionId, std::uint32_t>& needs) {
  std::vector<Formula> parts;
  for (const auto& kv : needs) {
    auto sq = table->intern_quantity(Quantity::size(kv.first));
    parts.push_back(simple_atom(sq, Rel::Gt, static_cast<std::int64_t>(kv.second)));
  }
  return parts;
}

Formula Encoder::leaf_inner(const HNode& node) {
  if (node.kind == HNode::Kind::Cmp && node.a && node.b)
    return cmp(node.cmp, *node.a, *node.b, node.span);
  if (node.kind == HNode::Kind::Band && node.a) {
    if (mentions_unindexed(*node.a)) return unknown(node.span, MIN_PAIR_SHAPE);
    return band(node.band, *node.a, node.lo, node.hi, node.span);
  }
  return unknown(node.span, "expression is not a comparison");
}

Formula Encoder::build_dual(DualKind kind, QuantityId size_q,
                            const std::vector<Formula>& instances, DiagId why) {
  std::int64_t k = static_cast<std::int64_t>(OPEN1_BOUND);
  auto all_within = [&]() {
    std::vector<Formula> parts;
    for (std::size_t i = 0; i < instances.size(); ++i) {
      auto guard = simple_atom(size_q, Rel::Le, static_cast<std::int64_t>(i));
      parts.push_back(forr({std::move(guard), instances[i]}));
    }
    return fand(std::move(parts));
  };
  Formula plus, minus;
  if (kind == DualKind::Open1) {
    std::vector<Formula> plus_parts;
    plus_parts.push_back(simple_atom(size_q, Rel::Eq, 0));
    for (const auto& p : instances) plus_parts.push_back(p);
    plus_parts.push_back(simple_atom(size_q, Rel::Gt, k));
    plus = forr(std::move(plus_parts));
    std::vector<Formula> minus_parts;
    minus_parts.push_back(simple_atom(size_q, Rel::Ge, 1));
    minus_parts.push_back(simple_atom(size_q, Rel::Le, k));
    minus_parts.push_back(all_within());
    minus = fand(std::move(minus_parts));
  } else if (kind == DualKind::Any) {
    std::vector<Formula> plus_parts = instances;
    plus_parts.push_back(simple_atom(size_q, Rel::Gt, k));
    plus = forr(std::move(plus_parts));
    std::vector<Formula> minus_parts;
    for (std::size_t i = 0; i < instances.size(); ++i) {
      auto guard = simple_atom(size_q, Rel::Gt, static_cast<std::int64_t>(i));
      minus_parts.push_back(fand({std::move(guard), instances[i]}));
    }
    minus = forr(std::move(minus_parts));
  } else {
    plus = all_within();
    auto empty = simple_atom(size_q, Rel::Eq, 0);
    auto lo = simple_atom(size_q, Rel::Ge, 1);
    auto hi = simple_atom(size_q, Rel::Le, k);
    auto bounded = fand({std::move(lo), std::move(hi), all_within()});
    minus = forr({std::move(empty), std::move(bounded)});
  }
  return Formula::dual(std::move(plus), std::move(minus), why);
}

Formula Encoder::encode_reduce(ReduceKind kind, CollectionId coll, const HNode& body,
                               Span span) {
  DualKind dual_kind;
  if (kind == ReduceKind::Any) dual_kind = DualKind::Any;
  else if (kind == ReduceKind::All) dual_kind = DualKind::All;
  else return unknown(span, "numeric reducer used as a boolean condition");
  const auto& c = table->collection(coll);
  if (c.kind == CollectionKind::Slice && c.slice_end) {
    std::uint32_t n = *c.slice_end > c.slice_start ? *c.slice_end - c.slice_start : 0;
    if (n > MAX_STATIC_SLICE_REDUCE) {
      std::ostringstream os;
      os << "static slice width " << n << " exceeds reducer expansion cap "
         << MAX_STATIC_SLICE_REDUCE;
      return unknown(span, os.str());
    }
    return encode_static_slice_reduce(dual_kind, c.parent, c.slice_start, n, body);
  }
  auto size_q = table->intern_quantity(Quantity::size(coll));
  std::vector<Formula> instances;
  for (std::uint32_t i = 0; i < OPEN1_BOUND; ++i) {
    auto inst = subst_reduce(body, coll, i);
    instances.push_back(boolean(inst));
  }
  std::ostringstream why;
  why << "`" << adl2::sema::reduce_kind_str(kind)
      << "` reducer: bounded expansion k=" << OPEN1_BOUND;
  return build_dual(dual_kind, size_q, instances, diags.push(span, why.str()));
}

Formula Encoder::encode_static_slice_reduce(DualKind kind, CollectionId source,
                                            std::uint32_t start, std::uint32_t n,
                                            const HNode& body) {
  auto size_q = table->intern_quantity(Quantity::size(source));
  std::vector<Formula> parts;
  for (std::uint32_t j = 0; j < n; ++j) {
    std::uint32_t abs = start + j;
    if (abs > adl2::sema::MAX_SOURCE_ELEM_INDEX) abs = adl2::sema::MAX_SOURCE_ELEM_INDEX;
    auto inst = subst_reduce(body, source, abs);
    auto p = boolean(inst);
    auto idx = static_cast<std::int64_t>(abs);
    if (kind == DualKind::Any) {
      auto guard = simple_atom(size_q, Rel::Gt, idx);
      parts.push_back(fand({std::move(guard), std::move(p)}));
    } else {
      auto guard = simple_atom(size_q, Rel::Le, idx);
      parts.push_back(forr({std::move(guard), std::move(p)}));
    }
  }
  return kind == DualKind::Any ? forr(std::move(parts)) : fand(std::move(parts));
}

HNode Encoder::subst(const HNode& node, CollectionId coll, std::uint32_t index) {
  HNode out = node;
  if (node.kind == HNode::Kind::CollProp && node.coll == coll) {
    auto q = table->intern_quantity(
        Quantity::elem_prop(coll, ElemIndex::from_front(index), node.prop));
    out = HNode::make(HNode::Kind::Quantity, node.span);
    out.tag = node.tag;
    out.qid = q;
    return out;
  }
  if (out.a) *out.a = subst(*node.a, coll, index);
  if (out.b) *out.b = subst(*node.b, coll, index);
  if (out.c) *out.c = subst(*node.c, coll, index);
  for (auto& it : out.items) it = subst(it, coll, index);
  return out;
}

HNode Encoder::subst_reduce(const HNode& node, CollectionId coll, std::uint32_t index) {
  HNode out = node;
  if (node.kind == HNode::Kind::ReduceProp) {
    auto q = table->intern_quantity(
        Quantity::elem_prop(coll, ElemIndex::from_front(index), node.prop));
    out = HNode::make(HNode::Kind::Quantity, node.span);
    out.tag = node.tag;
    out.qid = q;
    return out;
  }
  if (node.kind == HNode::Kind::Quantity) {
    out.qid = subst_reduce_quantity(node.qid, coll, index);
    return out;
  }
  if (out.a) *out.a = subst_reduce(*node.a, coll, index);
  if (out.b) *out.b = subst_reduce(*node.b, coll, index);
  if (out.c) *out.c = subst_reduce(*node.c, coll, index);
  for (auto& it : out.items) it = subst_reduce(it, coll, index);
  return out;
}

QuantityId Encoder::subst_reduce_quantity(QuantityId q, CollectionId coll,
                                          std::uint32_t index) {
  auto elem = ParticleRef::elem(coll, ElemIndex::from_front(index));
  auto subst_p = [&](const ParticleRef& p) {
    return p.kind == ParticleKind::ReduceElem ? elem : p;
  };
  const auto& qq = table->quantity(q);
  if (qq.kind == QuantityKind::AngularSep) {
    auto na = subst_p(qq.a);
    auto nb = subst_p(qq.b);
    if (na == qq.a && nb == qq.b) return q;
    return table->intern_angular(qq.ang, std::move(na), std::move(nb));
  }
  if (qq.kind == QuantityKind::ExternalFn) {
    bool changed = false;
    auto args = qq.args;
    for (auto& arg : args) {
      if (arg.kind == QuantityArgKind::Particle &&
          arg.particle.kind == ParticleKind::ReduceElem) {
        arg.particle = elem;
        changed = true;
      }
    }
    if (!changed) return q;
    return table->intern_quantity(Quantity::external_fn(qq.name, std::move(args)));
  }
  return q;
}

Formula Encoder::cmp(CmpOp op, const HNode& lhs, const HNode& rhs, Span span) {
  QuantityId q{};
  CmpOp folded = op;
  auto sh = min_pair_shape(op, lhs, rhs, q, folded);
  if (sh == MinPairKind::No) return cmp_linear(op, lhs, rhs, span);
  if (sh == MinPairKind::OutOfShape) return unknown(span, MIN_PAIR_SHAPE);
  return min_pair_cmp(q, folded, op, lhs, rhs, span);
}

MinPairKind Encoder::min_pair_shape(CmpOp op, const HNode& lhs, const HNode& rhs,
                                    QuantityId& q, CmpOp& folded) {
  bool lm = mentions_unindexed(lhs);
  bool rm = mentions_unindexed(rhs);
  if (!lm && !rm) return MinPairKind::No;
  if (lm && !rm) {
    if (auto b = bare_min_pair(lhs)) {
      q = *b;
      folded = op;
      return MinPairKind::Fold;
    }
  }
  if (rm && !lm) {
    if (auto b = bare_min_pair(rhs)) {
      q = *b;
      folded = adl2::sema::cmp_op_flipped(op);
      return MinPairKind::Fold;
    }
  }
  return MinPairKind::OutOfShape;
}

std::optional<QuantityId> Encoder::bare_min_pair(const HNode& node) {
  if (node.kind != HNode::Kind::Quantity) return std::nullopt;
  adl2::sema::AngKind k;
  CollectionId a, b;
  if (table->whole_pair_legs(node.qid, k, a, b)) return node.qid;
  return std::nullopt;
}

bool Encoder::mentions_unindexed(const HNode& node) {
  std::set<QuantityId> qs;
  collect_quantities(node, qs);
  for (auto q : qs) {
    if (table->has_unindexed_leg(q)) return true;
  }
  return false;
}

bool Encoder::is_constant(const HNode& node) {
  LinErr err;
  auto l = lin(node, err);
  return l && l->terms.empty() && l->mentioned.empty();
}

Formula Encoder::empty_product(QuantityId q) {
  adl2::sema::AngKind k;
  CollectionId a, b;
  if (!table->whole_pair_legs(q, k, a, b)) return Formula::ffalse();
  std::set<CollectionId> colls{a, b};
  std::vector<Formula> parts;
  for (auto c : colls) {
    auto sq = table->intern_quantity(Quantity::size(c));
    parts.push_back(simple_atom(sq, Rel::Le, 0));
  }
  return forr(std::move(parts));
}

Formula Encoder::min_pair_cmp(QuantityId q, CmpOp folded, CmpOp op, const HNode& lhs,
                              const HNode& rhs, Span span) {
  const HNode* other = bare_min_pair(lhs) ? &rhs : &lhs;
  bool special = folded == CmpOp::Ne || folded == CmpOp::ApproxEq || folded == CmpOp::Gt ||
                 folded == CmpOp::Ge;
  if (special && !is_constant(*other)) return unknown(span, MIN_PAIR_THRESHOLD);
  if (folded == CmpOp::Ne || folded == CmpOp::ApproxEq) {
    return cmp_linear(CmpOp::Eq, lhs, rhs, span).fnot();
  }
  if (folded == CmpOp::Gt || folded == CmpOp::Ge) {
    auto plain = cmp_linear(op, lhs, rhs, span);
    auto empty = empty_product(q);
    auto plus = forr({plain, empty});
    auto why = diags.push(
        span,
        "separation cut over two unindexed collections is a ∀ over the pair "
        "product, and a pair with no value fails it: the superset drops "
        "\"every pair has a value\" (unstatable over the min-pair quantity), "
        "the subset keeps only the vacuous empty-product case");
    return Formula::dual(std::move(plus), std::move(empty), why);
  }
  return cmp_linear(op, lhs, rhs, span);
}

Formula Encoder::cmp_linear(CmpOp op, const HNode& lhs, const HNode& rhs, Span span) {
  Rel rel = rel_of(op);
  LinErr el, er;
  auto l = lin(lhs, el);
  auto r = lin(rhs, er);
  if (l && r) {
    auto mode = edge_mode_pair(lhs, rhs, *table);
    if (mode == Edge::Unmodelable) return unknown(span, MIXED_EDGE);
    auto d = l->sub(*r);
    auto k = at_edge(mode, -d.k);
    Formula leaf;
    if (auto f = try_comb_existence(d.terms, rel, k, span)) leaf = std::move(*f);
    else leaf = atom_of(d.terms, rel, k);
    return guard_presence_set(d.mentioned, std::move(leaf));
  }
  if ((!l && el.kind == LinErrKind::NonFinite) || (!r && er.kind == LinErrKind::NonFinite))
    return Formula::ffalse();
  if ((!l && el.kind == LinErrKind::BadLiteral) || (!r && er.kind == LinErrKind::BadLiteral))
    return unknown(span, "non-finite numeric literal cannot construct an atom");
  if (!l && el.kind == LinErrKind::NonLinear && r && r->terms.empty()) {
    auto f = pattern(lhs, rel, r->k, el.why, span);
    return guard_presence_set(r->mentioned, std::move(f));
  }
  if (!r && er.kind == LinErrKind::NonLinear && l && l->terms.empty()) {
    auto f = pattern(rhs, rel_flipped(rel), l->k, er.why, span);
    return guard_presence_set(l->mentioned, std::move(f));
  }
  std::string why = l ? er.why : el.why;
  return unknown(span, "comparison is not linear arithmetic: " + why);
}

Formula Encoder::pattern(const HNode& side, Rel rel, Rat c, const std::string& why,
                         Span span) {
  auto mode = edge_mode(side, *table);
  c = at_edge(mode, std::move(c));
  if (side.kind == HNode::Kind::Binary && side.arith == ArithOp::Div && side.a && side.b)
    return ratio(side, *side.a, *side.b, rel, std::move(c), span);
  if (side.kind == HNode::Kind::Abs && side.a)
    return abs_cmp(side, *side.a, rel, std::move(c), span);
  if (side.kind == HNode::Kind::ScalarMinMax) {
    if (mode == Edge::F64) {
      for (const auto& a : side.items) {
        if (is_exact_valued(a, *table)) return unknown(span, MIXED_EDGE);
      }
    }
    bool or_branch =
        (side.reduce == ReduceKind::Min && (rel == Rel::Lt || rel == Rel::Le)) ||
        (side.reduce == ReduceKind::Max && (rel == Rel::Gt || rel == Rel::Ge));
    bool and_branch =
        (side.reduce == ReduceKind::Min && (rel == Rel::Gt || rel == Rel::Ge)) ||
        (side.reduce == ReduceKind::Max && (rel == Rel::Lt || rel == Rel::Le));
    if (or_branch || and_branch) {
      std::vector<Formula> parts, opaque;
      std::map<CollectionId, std::uint32_t> needs;
      std::set<QuantityId> present;
      for (const auto& a : side.items) {
        auto f = cmp_node_const(a, rel, c, span);
        if (f.is_exact()) {
          collect_quantities(a, present);
          std::set<QuantityId> qids;
          collect_quantities(a, qids);
          for (auto q : qids) table->existence_floor(q, needs);
        } else {
          opaque.push_back(unknown(a.span, "scalar min/max argument is opaque"));
        }
        parts.push_back(std::move(f));
      }
      auto combined = or_branch ? forr(std::move(parts)) : fand(std::move(parts));
      auto conj = needs_guards(needs);
      for (auto& o : opaque) conj.push_back(std::move(o));
      conj.push_back(std::move(combined));
      return guard_presence_set(present, fand(std::move(conj)));
    }
    (void)why;
    return unknown(span, "scalar min/max compared by equality is opaque");
  }
  return opaque_atom(side, rel, std::move(c), why, span);
}

Formula Encoder::opaque_atom(const HNode& side, Rel rel, Rat c, const std::string& why,
                             Span span) {
  if (auto q = intern_opaque_scalar(side)) {
    std::vector<std::pair<QuantityId, Rat>> terms;
    terms.emplace_back(*q, Rat::one());
    auto a = atom_of(std::move(terms), rel, std::move(c));
    return guard_presence({*q}, std::move(a));
  }
  return unknown(span, "comparison is not linear arithmetic: " + why);
}

Formula Encoder::cmp_node_const(const HNode& node, Rel rel, Rat c, Span span) {
  auto mode = edge_mode(node, *table);
  c = at_edge(mode, std::move(c));
  LinErr err;
  auto l = lin(node, err);
  if (l) {
    auto k = at_edge(mode, c - l->k);
    auto mentioned = l->mentioned;
    Formula leaf;
    if (auto f = try_comb_existence(l->terms, rel, k, span)) leaf = std::move(*f);
    else leaf = atom_of(l->terms, rel, k);
    return guard_presence_set(mentioned, std::move(leaf));
  }
  if (err.kind == LinErrKind::NonFinite) return Formula::ffalse();
  if (err.kind == LinErrKind::BadLiteral)
    return unknown(span, "non-finite numeric literal cannot construct an atom");
  return pattern(node, rel, std::move(c), err.why, span);
}

Formula Encoder::ratio(const HNode& whole, const HNode& num, const HNode& den, Rel rel,
                       Rat c, Span span) {
  if (!(is_exact_valued(num, *table) && is_exact_valued(den, *table))) {
    return opaque_atom(whole, rel, std::move(c),
                       "ratio over approximate values cannot be cleared faithfully", span);
  }
  LinErr el, ed;
  auto l = lin_or_opaque(num, el);
  if (!l) return lin_err(el, "ratio numerator is not linear", span);
  auto d = lin_or_opaque(den, ed);
  if (!d) return lin_err(ed, "ratio denominator is not linear", span);
  auto mentioned = l->mentioned;
  mentioned.insert(d->mentioned.begin(), d->mentioned.end());
  if (d->terms.empty()) {
    if (d->k.is_zero()) return Formula::ffalse();
    auto cd = d->scale(c);
    auto e = l->sub(cd);
    if (d->k.is_negative()) rel = rel_flipped(rel);
    auto k = -e.k;
    auto a = atom_of(e.terms, rel, k);
    return guard_presence_set(mentioned, std::move(a));
  }
  auto cd = d->scale(c);
  auto e = l->sub(cd);
  auto neg_d_k = -d->k;
  auto neg_e_k = -e.k;
  auto d_pos = atom_of(d->terms, Rel::Gt, neg_d_k);
  auto e_pos = atom_of(e.terms, rel, neg_e_k);
  auto d_neg = atom_of(d->terms, Rel::Lt, -d->k);
  auto e_neg = atom_of(e.terms, rel_flipped(rel), -e.k);
  auto branches = forr({fand({std::move(d_pos), std::move(e_pos)}),
                        fand({std::move(d_neg), std::move(e_neg)})});
  return guard_presence_set(mentioned, std::move(branches));
}

Formula Encoder::abs_cmp(const HNode& abs_node, const HNode& inner, Rel rel, Rat c,
                         Span span) {
  if (c.is_negative()) {
    Formula folded;
    if (rel == Rel::Lt || rel == Rel::Le || rel == Rel::Eq) folded = Formula::ffalse();
    else folded = Formula::ttrue();
    return guard_presence_node(inner, std::move(folded));
  }
  LinErr err;
  auto e = lin(inner, err);
  if (!e) {
    if (err.kind == LinErrKind::NonLinear) {
      return opaque_atom(abs_node, rel, std::move(c),
                         "absolute value of a non-exact-f64 expression", span);
    }
    return lin_err(err, "absolute value of a non-linear expression", span);
  }
  auto hi = c - e->k;
  auto lo = (-c) - e->k;
  auto upper = [&](Rel r) { return atom_of(e->terms, r, hi); };
  auto lower = [&](Rel r) { return atom_of(e->terms, r, lo); };
  Formula expansion;
  switch (rel) {
    case Rel::Lt: expansion = fand({upper(Rel::Lt), lower(Rel::Gt)}); break;
    case Rel::Le: expansion = fand({upper(Rel::Le), lower(Rel::Ge)}); break;
    case Rel::Gt: expansion = forr({upper(Rel::Gt), lower(Rel::Lt)}); break;
    case Rel::Ge: expansion = forr({upper(Rel::Ge), lower(Rel::Le)}); break;
    case Rel::Eq: expansion = forr({upper(Rel::Eq), lower(Rel::Eq)}); break;
    case Rel::Ne: expansion = fand({upper(Rel::Ne), lower(Rel::Ne)}); break;
  }
  return guard_presence_set(e->mentioned, std::move(expansion));
}

Formula Encoder::band(BandKind kind, const HNode& expr, const std::string& lo_s,
                      const std::string& hi_s, Span span) {
  auto lo = parse_rat(lo_s);
  auto hi = parse_rat(hi_s);
  if (!lo || !hi)
    return unknown(span, "non-finite numeric literal cannot construct an atom");
  auto mode = edge_mode(expr, *table);
  *lo = at_edge(mode, *lo);
  *hi = at_edge(mode, *hi);
  LinErr err;
  auto e = lin(expr, err);
  if (!e) {
    if (err.kind == LinErrKind::NonLinear) {
      Rel lo_rel = kind == BandKind::In ? Rel::Ge : Rel::Le;
      Rel hi_rel = kind == BandKind::In ? Rel::Le : Rel::Ge;
      auto lo_bound = pattern(expr, lo_rel, *lo, err.why, span);
      auto hi_bound = pattern(expr, hi_rel, *hi, err.why, span);
      auto bandf = kind == BandKind::In ? fand({std::move(lo_bound), std::move(hi_bound)})
                                        : forr({std::move(lo_bound), std::move(hi_bound)});
      return guard_presence_node(expr, std::move(bandf));
    }
    return lin_err(err, "band expression is not linear", span);
  }
  auto lo_k = *lo - e->k;
  auto hi_k = *hi - e->k;
  auto lo_bound = atom_of(e->terms, kind == BandKind::In ? Rel::Ge : Rel::Le, lo_k);
  auto hi_bound = atom_of(e->terms, kind == BandKind::In ? Rel::Le : Rel::Ge, hi_k);
  auto bandf = kind == BandKind::In ? fand({std::move(lo_bound), std::move(hi_bound)})
                                    : forr({std::move(lo_bound), std::move(hi_bound)});
  return guard_presence_set(e->mentioned, std::move(bandf));
}

Formula Encoder::lin_err(const LinErr& e, const std::string& what, Span span) {
  if (e.kind == LinErrKind::NonFinite) return Formula::ffalse();
  if (e.kind == LinErrKind::BadLiteral)
    return unknown(span, "non-finite numeric literal cannot construct an atom");
  return unknown(span, what + ": " + e.why);
}

Rat Encoder::at_edge(Edge mode, Rat k) {
  if (mode == Edge::F64) {
    if (auto r = Rat::from_f64_exact(k.to_f64())) return *r;
  }
  return k;
}

Formula Encoder::atom_of(std::vector<std::pair<QuantityId, Rat>> terms, Rel rel, Rat k) {
  if (terms.empty()) {
    return rel_eval(rel, Rat::zero(), k) ? Formula::ttrue() : Formula::ffalse();
  }
  bool int_valued = true;
  for (const auto& kv : terms) {
    if (table->quantity(kv.first).kind != QuantityKind::Size || !kv.second.is_integer()) {
      int_valued = false;
      break;
    }
  }
  if (int_valued && !k.is_integer()) {
    switch (rel) {
      case Rel::Lt:
      case Rel::Le:
        rel = Rel::Le;
        k = k.floor();
        break;
      case Rel::Gt:
      case Rel::Ge:
        rel = Rel::Ge;
        k = k.ceil();
        break;
      case Rel::Eq:
        return Formula::ffalse();
      case Rel::Ne:
        return Formula::ttrue();
    }
  }
  std::vector<LinAtom::Term> ts;
  for (auto& kv : terms) ts.emplace_back(std::move(kv.second), kv.first);
  return Formula::of_atom(LinAtom::make(std::move(ts), rel, std::move(k)));
}

std::optional<QuantityId> Encoder::intern_reduce(ReduceKind kind, CollectionId coll,
                                                 const HNode& body, bool has_slice,
                                                 std::uint32_t start,
                                                 const std::optional<std::uint32_t>& end) {
  auto body_key = reduce_body_key(body);
  if (!body_key) return std::nullopt;
  auto name = symbols->intern(std::string("reduce.") + adl2::sema::reduce_kind_str(kind));
  std::vector<adl2::sema::QuantityArg> args;
  args.push_back(adl2::sema::QuantityArg::collection(coll));
  args.push_back(adl2::sema::QuantityArg::opaque(slice_key(has_slice, start, end) + *body_key));
  return table->intern_quantity(Quantity::external_fn(name, std::move(args)));
}

std::optional<QuantityId> Encoder::intern_opaque_scalar(const HNode& node) {
  auto body_key = reduce_body_key(node);
  if (!body_key) return std::nullopt;
  auto name = symbols->intern("opaque.scalar");
  std::vector<adl2::sema::QuantityArg> args;
  args.push_back(adl2::sema::QuantityArg::opaque(*body_key));
  return table->intern_quantity(Quantity::external_fn(name, std::move(args)));
}

std::optional<LinExpr> Encoder::lin_or_opaque(const HNode& node, LinErr& err) {
  auto v = lin(node, err);
  if (v) return v;
  if (err.kind == LinErrKind::NonLinear) {
    if (auto q = intern_opaque_scalar(node)) return LinExpr::quantity(*q);
  }
  return std::nullopt;
}

std::optional<LinExpr> Encoder::lin(const HNode& node, LinErr& err) {
  if (!node.tag.is_in_fragment()) {
    err = LinErr::nonlinear(node.tag.reason);
    return std::nullopt;
  }
  if (is_const_tree(node)) {
    auto r = const_tree_rat(node, err);
    if (!r) return std::nullopt;
    return LinExpr::constant(*r);
  }
  switch (node.kind) {
    case HNode::Kind::Num: {
      auto v = parse_rat(node.text);
      if (!v) {
        err = LinErr::badlit();
        return std::nullopt;
      }
      return LinExpr::constant(*v);
    }
    case HNode::Kind::Quantity:
      return LinExpr::quantity(node.qid);
    case HNode::Kind::Neg: {
      if (!flattens_faithfully(node, *table)) {
        err = LinErr::nonlinear(NOT_FAITHFUL);
        return std::nullopt;
      }
      if (!node.a) {
        err = LinErr::nonlinear("missing operand");
        return std::nullopt;
      }
      auto a = lin(*node.a, err);
      if (!a) return std::nullopt;
      return a->scale(Rat::from_i64(-1));
    }
    case HNode::Kind::Abs:
      err = LinErr::nonlinear("absolute value (only `|E| ⋈ const` is expanded)");
      return std::nullopt;
    case HNode::Kind::Binary:
      if (!flattens_faithfully(node, *table)) {
        err = LinErr::nonlinear(NOT_FAITHFUL);
        return std::nullopt;
      }
      if (!node.a || !node.b) {
        err = LinErr::nonlinear("missing operand");
        return std::nullopt;
      }
      return lin_binary(node.arith, *node.a, *node.b, err);
    case HNode::Kind::CollProp:
      err = LinErr::nonlinear("unindexed collection property");
      return std::nullopt;
    case HNode::Kind::ElemSelfProp:
      err = LinErr::nonlinear("implicit-element property outside an object block");
      return std::nullopt;
    case HNode::Kind::ReduceProp:
      err = LinErr::nonlinear("reducer-element property is interpret-only");
      return std::nullopt;
    case HNode::Kind::Reduce:
      if (!reduce_kind_is_boolean(node.reduce) && node.a) {
        if (auto q = intern_reduce(node.reduce, node.coll, *node.a, node.has_slice,
                                   node.slice_start, node.slice_end))
          return LinExpr::quantity(*q);
        err = LinErr::nonlinear(std::string("`") + adl2::sema::reduce_kind_str(node.reduce) +
                                "` reducer body is not injectively renderable");
        return std::nullopt;
      }
      err = LinErr::nonlinear(std::string("`") + adl2::sema::reduce_kind_str(node.reduce) +
                              "` reducer value is opaque to linear arithmetic");
      return std::nullopt;
    case HNode::Kind::ScalarMinMax:
      err = LinErr::nonlinear(std::string("`") + adl2::sema::reduce_kind_str(node.reduce) +
                              "` of scalars is not linear arithmetic");
      return std::nullopt;
    case HNode::Kind::Bool:
    case HNode::Kind::Cmp:
    case HNode::Kind::And:
    case HNode::Kind::Or:
    case HNode::Kind::Not:
    case HNode::Kind::Band:
    case HNode::Kind::Ternary:
    case HNode::Kind::RegionPred:
      err = LinErr::nonlinear("boolean value used in arithmetic");
      return std::nullopt;
    default:
      err = LinErr::nonlinear("unsupported value in arithmetic");
      return std::nullopt;
  }
}

std::optional<LinExpr> Encoder::lin_binary(ArithOp op, const HNode& lhs, const HNode& rhs,
                                          LinErr& err) {
  switch (op) {
    case ArithOp::Add: {
      auto l = lin(lhs, err);
      if (!l) return std::nullopt;
      auto r = lin(rhs, err);
      if (!r) return std::nullopt;
      return l->combine(*r, false);
    }
    case ArithOp::Sub: {
      auto l = lin(lhs, err);
      if (!l) return std::nullopt;
      auto r = lin(rhs, err);
      if (!r) return std::nullopt;
      return l->sub(*r);
    }
    case ArithOp::Mul: {
      auto l = lin(lhs, err);
      if (!l) return std::nullopt;
      auto r = lin(rhs, err);
      if (!r) return std::nullopt;
      if (l->terms.empty()) return r->scale(l->k);
      if (r->terms.empty()) return l->scale(r->k);
      err = LinErr::nonlinear("product of two event quantities");
      return std::nullopt;
    }
    case ArithOp::Div: {
      auto r = lin(rhs, err);
      if (!r) return std::nullopt;
      if (!r->terms.empty()) {
        err = LinErr::nonlinear("ratio with a non-constant denominator");
        return std::nullopt;
      }
      if (r->k.is_zero()) {
        err = LinErr::nonfinite();
        return std::nullopt;
      }
      auto l = lin(lhs, err);
      if (!l) return std::nullopt;
      if (l->terms.empty()) {
        auto v = l->k.checked_div(r->k);
        if (!v) {
          err = LinErr::nonfinite();
          return std::nullopt;
        }
        return LinExpr::constant(*v);
      }
      auto inv = Rat::one().checked_div(r->k);
      if (!inv) {
        err = LinErr::nonfinite();
        return std::nullopt;
      }
      return l->scale(*inv);
    }
    case ArithOp::Pow:
      err = LinErr::nonlinear("non-constant power");
      return std::nullopt;
  }
  err = LinErr::nonlinear("unknown arithmetic");
  return std::nullopt;
}

std::optional<Formula> Encoder::try_comb_existence(
    const std::vector<std::pair<QuantityId, Rat>>& terms, Rel rel, const Rat& k, Span span) {
  if (terms.size() != 1) return std::nullopt;
  if (!terms[0].second.is_one()) return std::nullopt;
  QuantityId q = terms[0].first;
  bool forces = false;
  if (rel == Rel::Ge) forces = k >= Rat::one();
  else if (rel == Rel::Gt) forces = k >= Rat::zero();
  else if (rel == Rel::Eq) forces = k >= Rat::one();
  if (!forces) return std::nullopt;
  if (table->quantity(q).kind != QuantityKind::Size) return std::nullopt;
  CollectionId coll = table->quantity(q).coll;
  CollectionId comb_id = coll;
  const auto& c0 = table->collection(coll);
  if (c0.kind == CollectionKind::CombProject) comb_id = c0.parent;
  else if (c0.kind != CollectionKind::Combination) return std::nullopt;
  const auto& comb = table->collection(comb_id);
  if (comb.kind != CollectionKind::Combination) return std::nullopt;
  if (comb.cuts.empty() || comb.parts.empty()) return std::nullopt;
  auto tuples = binder_index_tuples(comb.parts, comb.comb_kind);
  if (tuples.empty()) return std::nullopt;
  std::vector<Formula> existence;
  for (const auto& t : tuples)
    existence.push_back(encode_tuple_cuts(comb.cuts, comb.members, comb.parts, t));
  for (auto part : comb.parts) {
    auto sq = table->intern_quantity(Quantity::size(part));
    existence.push_back(simple_atom(sq, Rel::Gt, static_cast<std::int64_t>(COMB2D_BOUND)));
  }
  auto atom = atom_of(terms, rel, k);
  if (atom.kind == Formula::Kind::False) return atom;
  auto why = diags.push(
      span,
      "composite tuple-count lower bound: 2D per-candidate-cut existence "
      "expansion (bound=" +
          std::to_string(COMB2D_BOUND) + "); under-approx kept opaque (USER ANSWER 4)");
  auto plus = fand({atom, forr(std::move(existence))});
  return Formula::dual(std::move(plus), std::move(atom), why);
}

std::vector<std::vector<std::uint32_t>> Encoder::binder_index_tuples(
    const std::vector<CollectionId>& parts, CombKind kind) {
  std::uint32_t b = COMB2D_BOUND;
  std::vector<std::vector<std::uint32_t>> tuples{{}};
  for (std::size_t p = 0; p < parts.size(); ++p) {
    std::vector<std::vector<std::uint32_t>> next;
    for (const auto& t : tuples) {
      for (std::uint32_t i = 0; i < b; ++i) {
        auto nt = t;
        nt.push_back(i);
        next.push_back(std::move(nt));
      }
    }
    tuples = std::move(next);
  }
  bool same_source = parts.size() >= 2;
  for (std::size_t i = 1; i < parts.size(); ++i) {
    if (!(parts[i] == parts[0])) same_source = false;
  }
  if (kind == CombKind::Disjoint && same_source) {
    std::vector<std::vector<std::uint32_t>> kept;
    for (auto& t : tuples) {
      bool ok = true;
      for (std::size_t i = 1; i < t.size(); ++i) {
        if (!(t[i - 1] < t[i])) ok = false;
      }
      if (ok) kept.push_back(std::move(t));
    }
    return kept;
  }
  return tuples;
}

Formula Encoder::encode_tuple_cuts(const std::vector<ElemPredId>& cuts,
                                   const std::vector<CompositeBinder>& members,
                                   const std::vector<CollectionId>& parts,
                                   const std::vector<std::uint32_t>& idx) {
  std::vector<Formula> parts_f;
  for (std::size_t slot = 0; slot < idx.size(); ++slot) {
    auto sq = table->intern_quantity(Quantity::size(parts[slot]));
    parts_f.push_back(simple_atom(sq, Rel::Gt, static_cast<std::int64_t>(idx[slot])));
  }
  for (auto cut : cuts) {
    const auto& node = (*elem_preds)[cut.id].node;
    auto inst = subst_binders(node, members, parts, idx);
    if (has_residual_binder(inst)) {
      parts_f.push_back(unknown(
          inst.span,
          "composite per-tuple cut references an opaque candidate "
          "(mass/pt of a 4-vector sum) — kept opaque (P3)"));
    } else {
      parts_f.push_back(boolean(inst));
    }
  }
  return fand(std::move(parts_f));
}

bool particle_has_binder(const ParticleRef& p) {
  if (p.kind == ParticleKind::Binder) return true;
  if (p.kind == ParticleKind::Sum) {
    for (const auto& x : p.parts) {
      if (particle_has_binder(x)) return true;
    }
  }
  return false;
}

bool Encoder::has_residual_binder(const HNode& node) {
  if (node.kind == HNode::Kind::Quantity) {
    const auto& q = table->quantity(node.qid);
    if (q.kind == QuantityKind::AngularSep)
      return particle_has_binder(q.a) || particle_has_binder(q.b);
    if (q.kind == QuantityKind::ExternalFn) {
      for (const auto& arg : q.args) {
        if (arg.kind == QuantityArgKind::Particle && particle_has_binder(arg.particle))
          return true;
      }
    }
    return false;
  }
  if (node.kind == HNode::Kind::ReduceProp || node.kind == HNode::Kind::ElemSelfProp)
    return false;
  for (const HNode* c : hnode_children(node)) {
    if (has_residual_binder(*c)) return true;
  }
  return false;
}

HNode Encoder::subst_binders(const HNode& node, const std::vector<CompositeBinder>& members,
                             const std::vector<CollectionId>& parts,
                             const std::vector<std::uint32_t>& idx) {
  HNode out = node;
  if (node.kind == HNode::Kind::Quantity) {
    out.qid = subst_binder_quantity(node.qid, members, parts, idx);
    return out;
  }
  if (out.a) *out.a = subst_binders(*node.a, members, parts, idx);
  if (out.b) *out.b = subst_binders(*node.b, members, parts, idx);
  if (out.c) *out.c = subst_binders(*node.c, members, parts, idx);
  for (auto& it : out.items) it = subst_binders(it, members, parts, idx);
  return out;
}

QuantityId Encoder::subst_binder_quantity(QuantityId q,
                                          const std::vector<CompositeBinder>& members,
                                          const std::vector<CollectionId>& parts,
                                          const std::vector<std::uint32_t>& idx) {
  auto binder_elem = [&](adl2::sema::Symbol name) -> std::optional<ParticleRef> {
    for (std::size_t slot = 0; slot < members.size(); ++slot) {
      if (members[slot].name == name && slot < parts.size() && slot < idx.size())
        return ParticleRef::elem(parts[slot], ElemIndex::from_front(idx[slot]));
    }
    return std::nullopt;
  };
  auto subst_p = [&](const ParticleRef& p) {
    if (p.kind == ParticleKind::Binder) {
      if (auto e = binder_elem(p.name)) return *e;
    }
    return p;
  };
  const auto& qq = table->quantity(q);
  if (qq.kind == QuantityKind::AngularSep) {
    auto na = subst_p(qq.a);
    auto nb = subst_p(qq.b);
    if (na == qq.a && nb == qq.b) return q;
    return table->intern_angular(qq.ang, std::move(na), std::move(nb));
  }
  if (qq.kind == QuantityKind::ExternalFn) {
    bool changed = false;
    auto args = qq.args;
    for (auto& arg : args) {
      if (arg.kind == QuantityArgKind::Particle &&
          arg.particle.kind == ParticleKind::Binder) {
        auto np = subst_p(arg.particle);
        if (!(np == arg.particle)) {
          arg.particle = np;
          changed = true;
        }
      }
    }
    if (!changed) return q;
    return table->intern_quantity(Quantity::external_fn(qq.name, std::move(args)));
  }
  return q;
}

Encoder make_encoder(Hir& hir) {
  Encoder enc;
  enc.table = &hir.table;
  enc.regions = &hir.regions;
  enc.symbols = &hir.symbols;
  enc.elem_preds = &hir.elem_preds;
  return enc;
}

}  // namespace

EncodedRegion encode_region(Hir& hir, std::size_t region) {
  std::string name;
  Span span;
  if (region < hir.regions.size()) {
    name = hir.symbols.display(hir.regions[region].name);
    span = hir.regions[region].span;
  }
  Encoder enc = make_encoder(hir);
  auto formula = enc.region(region, span);
  EncodedRegion out;
  out.region = region;
  out.name = std::move(name);
  out.formula = std::move(formula);
  out.diags = std::move(enc.diags);
  return out;
}

EncodedRegion encode_region_stmts(Hir& hir, const std::vector<HirRegionStmt>& stmts,
                                  Span span) {
  Encoder enc = make_encoder(hir);
  std::vector<Formula> parts;
  parts.reserve(stmts.size());
  for (const auto& st : stmts) {
    if (auto f = enc.stmt(st)) parts.push_back(std::move(*f));
  }
  EncodedRegion out;
  out.name = hir.symbols.display(hir.symbols.intern("__adl2_synth__"));
  out.formula = fand(std::move(parts));
  out.diags = std::move(enc.diags);
  (void)span;
  return out;
}

std::vector<EncodedRegion> encode_regions(Hir& hir) {
  std::vector<EncodedRegion> out;
  out.reserve(hir.regions.size());
  for (std::size_t i = 0; i < hir.regions.size(); ++i) out.push_back(encode_region(hir, i));
  return out;
}

}  // namespace adl2::formula
