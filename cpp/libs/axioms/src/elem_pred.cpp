#include "elem_pred.hpp"

#include "adl2/formula/lin.hpp"
#include "adl2/sema/ops.hpp"
#include "adl2/sema/quantity.hpp"

#include <map>
#include <string>
#include <utility>
#include <vector>

namespace adl2::axioms {
namespace {

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::formula::rel_eval;
using adl2::formula::rel_flipped;
using adl2::sema::ArithOp;
using adl2::sema::BandKind;
using adl2::sema::CmpOp;
using adl2::sema::CollectionId;
using adl2::sema::ElemIndex;
using adl2::sema::HNode;
using adl2::sema::PropId;
using adl2::sema::Quantity;
using adl2::sema::QuantityArgKind;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::QuantityTable;
using adl2::sema::Rat;
using HKind = HNode::Kind;

struct PredLin {
  std::map<QuantityId, Rat> terms;
  Rat k;
  static PredLin constant(Rat c) {
    PredLin p;
    p.k = std::move(c);
    return p;
  }
  PredLin sub(const PredLin& o) const {
    PredLin out = *this;
    out.k = out.k - o.k;
    for (const auto& kv : o.terms) {
      auto it = out.terms.find(kv.first);
      if (it == out.terms.end())
        out.terms.emplace(kv.first, Rat::zero() - kv.second);
      else
        it->second = it->second - kv.second;
    }
    return out;
  }
  PredLin scale(const Rat& c) const {
    PredLin out;
    out.k = k * c;
    for (const auto& kv : terms) out.terms.emplace(kv.first, kv.second * c);
    return out;
  }
};

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

QFormula lin_atom(std::map<QuantityId, Rat> terms, Rel rel, Rat k) {
  std::vector<LinAtom::Term> ts;
  for (auto& kv : terms) {
    if (!kv.second.is_zero()) ts.emplace_back(std::move(kv.second), kv.first);
  }
  if (ts.empty()) {
    return rel_eval(rel, Rat::zero(), k) ? QFormula::ttrue() : QFormula::ffalse();
  }
  return QFormula::of_atom(LinAtom::make(std::move(ts), rel, std::move(k)));
}

bool is_power_of_two_rat(const Rat& r) {
  if (r.is_zero()) return false;
  auto p = r.abs().to_parts();
  auto is_pow2 = [](const std::string& s) -> bool {
    try {
      unsigned long long n = std::stoull(s);
      return n != 0 && (n & (n - 1)) == 0;
    } catch (...) {
      return false;
    }
  };
  return (p.numerator == "1" && is_pow2(p.denominator)) ||
         (p.denominator == "1" && is_pow2(p.numerator));
}

std::optional<PredLin> lin_pred(QuantityTable& table, const HNode& node, CollectionId coll,
                                std::uint32_t index);

std::optional<QFormula> encode_pred_exact(QuantityTable& table, const HNode& node,
                                          CollectionId coll, std::uint32_t index);

std::optional<QFormula> abs_pred(QuantityTable& table, const HNode& inner, Rel rel, const Rat& c,
                                 CollectionId coll, std::uint32_t index) {
  if (c.is_negative()) {
    switch (rel) {
      case Rel::Lt:
      case Rel::Le:
      case Rel::Eq:
        return QFormula::ffalse();
      case Rel::Gt:
      case Rel::Ge:
      case Rel::Ne:
        return QFormula::ttrue();
    }
  }
  auto e = lin_pred(table, inner, coll, index);
  if (!e) return std::nullopt;
  Rat hi = c - e->k;
  Rat lo = (Rat::zero() - c) - e->k;
  auto bound = [&](Rel r, const Rat& k) {
    return lin_atom(e->terms, r, k);
  };
  switch (rel) {
    case Rel::Lt:
      return QFormula::of_and({bound(Rel::Lt, hi), bound(Rel::Gt, lo)});
    case Rel::Le:
      return QFormula::of_and({bound(Rel::Le, hi), bound(Rel::Ge, lo)});
    case Rel::Gt:
      return QFormula::of_or({bound(Rel::Gt, hi), bound(Rel::Lt, lo)});
    case Rel::Ge:
      return QFormula::of_or({bound(Rel::Ge, hi), bound(Rel::Le, lo)});
    case Rel::Eq:
      return QFormula::of_or({bound(Rel::Eq, hi), bound(Rel::Eq, lo)});
    case Rel::Ne:
      return QFormula::of_and({bound(Rel::Ne, hi), bound(Rel::Ne, lo)});
  }
  return std::nullopt;
}

std::optional<QFormula> clear_ratio(QuantityTable& table, const HNode& ratio_side,
                                    const HNode& other_side, Rel rel, CollectionId coll,
                                    std::uint32_t index) {
  if (ratio_side.kind != HKind::Binary || ratio_side.arith != ArithOp::Div) return std::nullopt;
  auto d = lin_pred(table, *ratio_side.b, coll, index);
  if (!d || !d->terms.empty()) return std::nullopt;
  if (d->k.is_zero()) return QFormula::ffalse();
  if (!is_power_of_two_rat(d->k)) return std::nullopt;
  auto l = lin_pred(table, *ratio_side.a, coll, index);
  auto r = lin_pred(table, other_side, coll, index);
  if (!l || !r) return std::nullopt;
  PredLin rd = r->scale(d->k);
  PredLin e = l->sub(rd);
  Rel use = d->k.is_negative() ? rel_flipped(rel) : rel;
  Rat k = Rat::zero() - e.k;
  return lin_atom(std::move(e.terms), use, std::move(k));
}

std::optional<PredLin> lin_pred(QuantityTable& table, const HNode& node, CollectionId coll,
                                std::uint32_t index) {
  if (!node.tag.in_fragment) return std::nullopt;
  switch (node.kind) {
    case HKind::Num: {
      try {
        auto r = Rat::from_decimal_f64(std::stod(node.text));
        if (!r) return std::nullopt;
        return PredLin::constant(std::move(*r));
      } catch (...) {
        return std::nullopt;
      }
    }
    case HKind::ElemSelfProp: {
      auto q = table.intern_quantity(
          Quantity::elem_prop(coll, ElemIndex::from_front(index), node.prop));
      PredLin p;
      p.terms.emplace(q, Rat::one());
      p.k = Rat::zero();
      return p;
    }
    case HKind::Quantity: {
      const auto& qq = table.quantity(node.qid);
      if (qq.kind == QuantityKind::ExternalFn) {
        for (const auto& a : qq.args) {
          if (a.kind != QuantityArgKind::Opaque) continue;
          const auto& s = a.text;
          if (s.find("<unsupported:") != std::string::npos ||
              s.compare(0, 5, "this.") == 0 || s.compare(0, 6, "@elem.") == 0 ||
              s.find('@') != std::string::npos) {
            return std::nullopt;
          }
        }
      }
      PredLin p;
      p.terms.emplace(node.qid, Rat::one());
      p.k = Rat::zero();
      return p;
    }
    case HKind::Neg: {
      auto inner = lin_pred(table, *node.a, coll, index);
      if (!inner) return std::nullopt;
      return inner->scale(Rat::from_i64(-1));
    }
    case HKind::Binary: {
      auto l = lin_pred(table, *node.a, coll, index);
      auto r = lin_pred(table, *node.b, coll, index);
      if (!l || !r) return std::nullopt;
      switch (node.arith) {
        case ArithOp::Add: {
          PredLin out = *l;
          out.k = out.k + r->k;
          for (const auto& kv : r->terms) {
            auto it = out.terms.find(kv.first);
            if (it == out.terms.end())
              out.terms.emplace(kv.first, kv.second);
            else
              it->second = it->second + kv.second;
          }
          return out;
        }
        case ArithOp::Sub:
          return l->sub(*r);
        case ArithOp::Mul:
          if (l->terms.empty()) return r->scale(l->k);
          if (r->terms.empty()) return l->scale(r->k);
          return std::nullopt;
        case ArithOp::Div:
          if (!r->terms.empty() || r->k.is_zero()) return std::nullopt;
          if (l->terms.empty()) {
            auto d = l->k.checked_div(r->k);
            if (!d) return std::nullopt;
            return PredLin::constant(std::move(*d));
          }
          // var / const: deferred to clear_ratio at the comparison level.
          return std::nullopt;
        case ArithOp::Pow:
          return std::nullopt;
      }
      return std::nullopt;
    }
    default:
      return std::nullopt;
  }
}

std::optional<QFormula> encode_pred_exact(QuantityTable& table, const HNode& node,
                                          CollectionId coll, std::uint32_t index) {
  if (!node.tag.in_fragment) return std::nullopt;
  switch (node.kind) {
    case HKind::Bool:
      return node.bool_val ? QFormula::ttrue() : QFormula::ffalse();
    case HKind::And: {
      std::vector<QFormula> parts;
      parts.reserve(node.items.size());
      for (const auto& p : node.items) {
        auto e = encode_pred_exact(table, p, coll, index);
        if (!e) return std::nullopt;
        parts.push_back(std::move(*e));
      }
      return QFormula::of_and(std::move(parts));
    }
    case HKind::Or: {
      std::vector<QFormula> parts;
      parts.reserve(node.items.size());
      for (const auto& p : node.items) {
        auto e = encode_pred_exact(table, p, coll, index);
        if (!e) return std::nullopt;
        parts.push_back(std::move(*e));
      }
      return QFormula::of_or(std::move(parts));
    }
    case HKind::Not: {
      auto inner = encode_pred_exact(table, *node.a, coll, index);
      if (!inner) return std::nullopt;
      return inner->qnot();
    }
    case HKind::Cmp: {
      Rel rel = rel_of(node.cmp);
      if (auto a = clear_ratio(table, *node.a, *node.b, rel, coll, index)) return a;
      if (auto a = clear_ratio(table, *node.b, *node.a, rel_flipped(rel), coll, index)) return a;
      if (node.a->kind == HKind::Abs) {
        auto r = lin_pred(table, *node.b, coll, index);
        if (r && r->terms.empty()) return abs_pred(table, *node.a->a, rel, r->k, coll, index);
      }
      if (node.b->kind == HKind::Abs) {
        auto l = lin_pred(table, *node.a, coll, index);
        if (l && l->terms.empty())
          return abs_pred(table, *node.b->a, rel_flipped(rel), l->k, coll, index);
      }
      auto l = lin_pred(table, *node.a, coll, index);
      auto r = lin_pred(table, *node.b, coll, index);
      if (!l || !r) return std::nullopt;
      PredLin diff = l->sub(*r);
      Rat k = Rat::zero() - diff.k;
      return lin_atom(std::move(diff.terms), rel, std::move(k));
    }
    case HKind::Band: {
      auto e = lin_pred(table, *node.a, coll, index);
      if (!e) return std::nullopt;
      std::optional<Rat> lo, hi;
      try {
        lo = Rat::from_decimal_f64(std::stod(node.lo));
        hi = Rat::from_decimal_f64(std::stod(node.hi));
      } catch (...) {
        return std::nullopt;
      }
      if (!lo || !hi) return std::nullopt;
      Rel lo_rel, hi_rel;
      bool combine_and = true;
      if (node.band == BandKind::In) {
        lo_rel = Rel::Ge;
        hi_rel = Rel::Le;
        combine_and = true;
      } else {
        lo_rel = Rel::Le;
        hi_rel = Rel::Ge;
        combine_and = false;
      }
      Rat lo_k = *lo - e->k;
      Rat hi_k = *hi - e->k;
      auto lo_b = lin_atom(e->terms, lo_rel, std::move(lo_k));
      auto hi_b = lin_atom(std::move(e->terms), hi_rel, std::move(hi_k));
      if (combine_and) return QFormula::of_and({std::move(lo_b), std::move(hi_b)});
      return QFormula::of_or({std::move(lo_b), std::move(hi_b)});
    }
    default:
      return std::nullopt;
  }
}

std::vector<const HNode*> pred_children(const HNode& node) {
  switch (node.kind) {
    case HKind::Neg:
    case HKind::Not:
    case HKind::Abs:
      return node.a ? std::vector<const HNode*>{node.a.get()} : std::vector<const HNode*>{};
    case HKind::Binary:
    case HKind::Cmp: {
      std::vector<const HNode*> v;
      if (node.a) v.push_back(node.a.get());
      if (node.b) v.push_back(node.b.get());
      return v;
    }
    case HKind::And:
    case HKind::Or: {
      std::vector<const HNode*> v;
      for (const auto& p : node.items) v.push_back(&p);
      return v;
    }
    case HKind::Band:
      return node.a ? std::vector<const HNode*>{node.a.get()} : std::vector<const HNode*>{};
    case HKind::ScalarMinMax: {
      std::vector<const HNode*> v;
      for (const auto& p : node.items) v.push_back(&p);
      return v;
    }
    case HKind::Ternary: {
      std::vector<const HNode*> v;
      if (node.a) v.push_back(node.a.get());
      if (node.b) v.push_back(node.b.get());
      if (node.c) v.push_back(node.c.get());
      return v;
    }
    default:
      return {};
  }
}

bool mentions_prop(const HNode& node, PropId prop) {
  if (node.kind == HKind::ElemSelfProp && node.prop == prop) return true;
  for (const HNode* c : pred_children(node)) {
    if (mentions_prop(*c, prop)) return true;
  }
  return false;
}

bool absorbing_arith(const HNode& node) {
  switch (node.kind) {
    case HKind::Num:
    case HKind::Bool:
    case HKind::ElemSelfProp:
      return true;
    case HKind::Neg:
    case HKind::Abs:
      return node.a && absorbing_arith(*node.a);
    case HKind::Binary:
      return (node.arith == ArithOp::Add || node.arith == ArithOp::Sub ||
              node.arith == ArithOp::Mul || node.arith == ArithOp::Div) &&
             node.a && node.b && absorbing_arith(*node.a) && absorbing_arith(*node.b);
    default:
      return false;
  }
}

}  // namespace

std::optional<adl2::formula::QFormula> encode_elem_pred(adl2::sema::QuantityTable& table,
                                                        const adl2::sema::HNode& node,
                                                        adl2::sema::CollectionId coll,
                                                        std::uint32_t index) {
  if (node.kind == HKind::And && node.tag.is_in_fragment()) {
    std::vector<QFormula> kept;
    for (const auto& p : node.items) {
      if (auto e = encode_pred_exact(table, p, coll, index)) kept.push_back(std::move(*e));
    }
    if (kept.empty()) return std::nullopt;
    return QFormula::of_and(std::move(kept));
  }
  return encode_pred_exact(table, node, coll, index);
}

void collect_self_props(const adl2::sema::HNode& node, std::set<adl2::sema::PropId>& out) {
  if (node.kind == HKind::ElemSelfProp) out.insert(node.prop);
  for (const HNode* c : pred_children(node)) collect_self_props(*c, out);
}

bool requires_present(const adl2::sema::HNode& pred, adl2::sema::PropId prop) {
  switch (pred.kind) {
    case HKind::And:
      for (const auto& p : pred.items) {
        if (requires_present(p, prop)) return true;
      }
      return false;
    case HKind::Or:
      if (pred.items.empty()) return false;
      for (const auto& p : pred.items) {
        if (!requires_present(p, prop)) return false;
      }
      return true;
    case HKind::Cmp: {
      bool mentions = (pred.a && mentions_prop(*pred.a, prop)) ||
                      (pred.b && mentions_prop(*pred.b, prop));
      return mentions && pred.a && pred.b && absorbing_arith(*pred.a) && absorbing_arith(*pred.b);
    }
    case HKind::Band:
      return pred.a && mentions_prop(*pred.a, prop) && absorbing_arith(*pred.a);
    default:
      return false;
  }
}

}  // namespace adl2::axioms
