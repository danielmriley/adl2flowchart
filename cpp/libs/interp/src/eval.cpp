#include "adl2/interp/eval.hpp"

#include "adl2/sema/num.hpp"
#include "adl2/sema/quantity.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <map>
#include <optional>
#include <sstream>
#include <utility>

namespace adl2::interp {
namespace {

using adl2::sema::AngKind;
using adl2::sema::ArithOp;
using adl2::sema::BandKind;
using adl2::sema::CmpOp;
using adl2::sema::Collection;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::CombAxisKind;
using adl2::sema::CombKind;
using adl2::sema::ElemIndex;
using adl2::sema::ElemIndexKind;
using adl2::sema::ElemPredId;
using adl2::sema::HNode;
using adl2::sema::Hir;
using adl2::sema::HirRegionStmt;
using adl2::sema::MET_FAMILY_KEY;
using adl2::sema::NumVal;
using adl2::sema::ParticleKind;
using adl2::sema::ParticleRef;
using adl2::sema::PropId;
using adl2::sema::Quantity;
using adl2::sema::QuantityArg;
using adl2::sema::QuantityArgKind;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::Rat;
using adl2::sema::ReduceKind;
using adl2::sema::ScalarSourceKind;
using adl2::sema::SortDir;
using adl2::sema::SortKeyKind;
using adl2::sema::Span;
using adl2::sema::Symbol;
using HKind = HNode::Kind;

constexpr double kPi = 3.14159265358979323846;

double rem_euclid(double a, double b) {
  double r = std::fmod(a, b);
  if (r < 0) r += b;
  return r;
}

std::optional<std::size_t> elem_pos(ElemIndex index, std::size_t len) {
  if (index.kind == ElemIndexKind::FromFront) {
    auto i = static_cast<std::size_t>(index.n);
    return i < len ? std::optional<std::size_t>(i) : std::nullopt;
  }
  auto k = static_cast<std::size_t>(index.n);
  if (k >= 1 && k <= len) return len - k;
  return std::nullopt;
}

EvalError mkerr(Span span, std::string reason,
                EvalErrorKind kind = EvalErrorKind::OutOfFragment) {
  EvalError e;
  e.span = span;
  e.reason = std::move(reason);
  e.kind = kind;
  return e;
}

bool cmp_num(CmpOp op, const NumVal& a, const NumVal& b) {
  if (a.kind == NumVal::Kind::Exact && b.kind == NumVal::Kind::Exact) {
    const Rat &x = a.exact, &y = b.exact;
    switch (op) {
      case CmpOp::Gt: return x > y;
      case CmpOp::Lt: return x < y;
      case CmpOp::Ge: return x >= y;
      case CmpOp::Le: return x <= y;
      case CmpOp::Eq: return x == y;
      case CmpOp::Ne:
      case CmpOp::ApproxEq: return !(x == y);
    }
  }
  double x = a.to_f64(), y = b.to_f64();
  switch (op) {
    case CmpOp::Gt: return x > y;
    case CmpOp::Lt: return x < y;
    case CmpOp::Ge: return x >= y;
    case CmpOp::Le: return x <= y;
    case CmpOp::Eq: return x == y;
    case CmpOp::Ne:
    case CmpOp::ApproxEq: return x != y;
  }
  return false;
}

bool band_ok(BandKind kind, const NumVal& v, const Rat& lo, const Rat& hi) {
  if (v.kind == NumVal::Kind::Exact) {
    return kind == BandKind::In ? (lo <= v.exact && v.exact <= hi)
                                : (v.exact <= lo || v.exact >= hi);
  }
  double x = v.approx, lf = lo.to_f64(), hf = hi.to_f64();
  return kind == BandKind::In ? (lf <= x && x <= hf) : (x <= lf || x >= hf);
}

std::optional<Rat> parse_rat(const std::string& text) {
  try {
    return Rat::from_decimal_f64(std::stod(text));
  } catch (...) {
    return std::nullopt;
  }
}

struct Angles {
  std::optional<double> eta, phi;
};
struct LV {
  double px = 0, py = 0, pz = 0, e = 0;
  static LV ptetaphim(double pt, double eta, double phi, double m) {
    LV v;
    v.px = pt * std::cos(phi);
    v.py = pt * std::sin(phi);
    v.pz = pt * std::sinh(eta);
    v.e = std::sqrt(v.px * v.px + v.py * v.py + v.pz * v.pz + m * m);
    return v;
  }
  static LV transverse(double pt, double phi) {
    LV v;
    v.px = pt * std::cos(phi);
    v.py = pt * std::sin(phi);
    v.e = pt;
    return v;
  }
  LV operator+(LV o) const { return LV{px + o.px, py + o.py, pz + o.pz, e + o.e}; }
  std::optional<NumVal> mass() const {
    return NumVal::from_f64(std::sqrt(std::max(0.0, e * e - (px * px + py * py + pz * pz))));
  }
  std::optional<NumVal> pt() const { return NumVal::from_f64(std::hypot(px, py)); }
  std::optional<NumVal> phi() const { return NumVal::from_f64(std::atan2(py, px)); }
  std::optional<NumVal> eta() const {
    return NumVal::from_f64(std::asinh(pz / std::hypot(px, py)));
  }
  std::optional<NumVal> energy() const { return NumVal::from_f64(e); }
};

struct CombTuple {
  std::map<Symbol, EventObject> binders;
  std::optional<EventObject> candidate;
};

enum class NR { Val, NV, Err };
struct NRes {
  NR k = NR::Val;
  NumVal val;
  NonValue nv;
  EvalError err;
  static NRes of(NumVal v) {
    NRes r;
    r.val = std::move(v);
    return r;
  }
  static NRes nv_(NonValueKind kind, std::string d = {}) {
    NRes r;
    r.k = NR::NV;
    r.nv.kind = kind;
    r.nv.detail = std::move(d);
    return r;
  }
  static NRes err_(EvalError e) {
    NRes r;
    r.k = NR::Err;
    r.err = std::move(e);
    return r;
  }
};

struct BRes {
  bool hard = false;
  bool pass = false;
  EvalError err;
  static BRes ok(bool v) {
    BRes r;
    r.pass = v;
    return r;
  }
  static BRes err_(EvalError e) {
    BRes r;
    r.hard = true;
    r.err = std::move(e);
    return r;
  }
};

NRes approx(double v) {
  if (auto n = NumVal::from_f64(v)) return NRes::of(*n);
  return NRes::nv_(NonValueKind::NonFinite);
}
NRes arith(ArithOp op, NumVal a, NumVal b) {
  if (auto v = adl2::sema::bin_arith(op, std::move(a), std::move(b))) return NRes::of(*v);
  return NRes::nv_(NonValueKind::NonFinite);
}

std::vector<std::vector<std::size_t>> enumerate_tuples(CombKind kind,
                                                       const std::vector<CollectionId>& parts,
                                                       const std::vector<std::size_t>& sizes) {
  if (parts.empty()) return {};
  std::vector<std::vector<std::size_t>> tuples = {{}};
  for (auto n : sizes) {
    std::vector<std::vector<std::size_t>> next;
    for (auto& t : tuples) {
      for (std::size_t i = 0; i < n; ++i) {
        auto nt = t;
        nt.push_back(i);
        next.push_back(std::move(nt));
      }
    }
    tuples = std::move(next);
  }
  if (kind == CombKind::Disjoint) {
    bool same = parts.size() >= 2;
    for (std::size_t i = 1; i < parts.size(); ++i)
      if (!(parts[i] == parts[0])) same = false;
    if (same) {
      std::vector<std::vector<std::size_t>> kept;
      for (auto& t : tuples) {
        bool inc = true;
        for (std::size_t i = 1; i < t.size(); ++i)
          if (!(t[i - 1] < t[i])) inc = false;
        if (inc) kept.push_back(std::move(t));
      }
      return kept;
    }
  }
  return tuples;
}

struct Ev {
  const Interp* it;
  const Event* event;
  std::map<CollectionId, std::vector<EventObject>> colls;
  std::map<CollectionId, std::vector<CombTuple>> combs;
  std::map<std::size_t, BRes> region_cache;
  std::vector<EventObject> reduce_stack;
  std::map<Symbol, EventObject> binder_env;

  EvalError oof(Span s, std::string m) const { return mkerr(s, std::move(m)); }
  EvalError miss(Span s, std::string m) const {
    return mkerr(s, std::move(m), EvalErrorKind::MissingEventData);
  }

  std::string coll_label(CollectionId id) const {
    const auto& names = it->hir().coll_names;
    if (id.id < names.size() && !names[id.id].empty())
      return it->hir().symbols.display(names[id.id].front());
    const auto& c = it->hir().table.collection(id);
    if (c.kind == CollectionKind::Base) return it->hir().symbols.display(c.base);
    return id.to_string();
  }

  NRes obj_prop(const EventObject& obj, PropId prop) const {
    const auto& key = it->hir().table.prop_key(prop);
    if (const Rat* v = obj.get(key)) return NRes::of(NumVal::from_exact(*v));
    return NRes::nv_(NonValueKind::MissingProperty, it->hir().table.prop_display(prop));
  }

  Angles obj_ang(const EventObject& obj) const {
    Angles a;
    if (const Rat* e = obj.get(it->eta_key())) a.eta = e->to_f64();
    if (const Rat* p = obj.get(it->phi_key())) a.phi = p->to_f64();
    return a;
  }

  std::optional<LV> obj_lv(const EventObject& obj, NonValue& nv) const {
    auto get = [&](const std::string& k, const char* n) -> std::optional<double> {
      if (const Rat* v = obj.get(k)) return v->to_f64();
      nv.kind = NonValueKind::MissingProperty;
      nv.detail = n;
      return std::nullopt;
    };
    auto pt = get(it->pt_key(), "pt");
    if (!pt) return std::nullopt;
    auto eta = get(it->eta_key(), "eta");
    if (!eta) return std::nullopt;
    auto phi = get(it->phi_key(), "phi");
    if (!phi) return std::nullopt;
    auto m = get(it->mass_key(), "mass");
    if (!m) return std::nullopt;
    return LV::ptetaphim(*pt, *eta, *phi, *m);
  }

  bool kin_eq(const EventObject& a, const EventObject& b) const {
    const std::string* ks[4] = {&it->pt_key(), &it->eta_key(), &it->phi_key(), &it->mass_key()};
    for (auto* k : ks) {
      const Rat* va = a.get(*k);
      const Rat* vb = b.get(*k);
      if (bool(va) != bool(vb)) return false;
      if (va && !(*va == *vb)) return false;
    }
    return true;
  }

  BRes region(std::size_t idx);
  BRes truth(const HNode& n, const EventObject* elem);
  NRes num(const HNode& n, const EventObject* elem);
  Tri region3(std::size_t idx);
  Tri truth3(const HNode& n, const EventObject* elem);
  NRes num3(const HNode& n, const EventObject* elem);
  Tri reduce_bool3(ReduceKind kind, CollectionId coll, const HNode& body,
                   const EventObject* elem);
  NRes reduce_num3(ReduceKind kind, CollectionId coll, const HNode& body,
                   const EventObject* elem);
  NRes quantity(QuantityId q, Span span, const EventObject* elem);
  const std::vector<EventObject>* materialize(CollectionId id, EvalError& err);
  NRes angular(AngKind k, const ParticleRef& a, const ParticleRef& b, Span span,
               const EventObject* elem);
  std::optional<bool> whole_cmp(CmpOp op, const HNode& lhs, const HNode& rhs, Span span,
                                const EventObject* elem, EvalError& err);
  std::optional<LV> lorentz(const ParticleRef& p, Span span, const EventObject* elem, NonValue& nv,
                            EvalError& err, bool& hard);
  std::optional<Angles> angles(const ParticleRef& p, Span span, const EventObject* elem,
                               NonValue& nv, EvalError& err, bool& hard);
  const std::vector<CombTuple>* comb_tuples(CollectionId id, EvalError& err);
  BRes reduce_bool(ReduceKind kind, CollectionId coll, const HNode& body, const EventObject* elem);
  NRes reduce_num(ReduceKind kind, CollectionId coll, const HNode& body, const EventObject* elem);
  NRes event_scalar_q(const Quantity& q, Span span);
  NRes arg_num(const QuantityArg& arg, Span span, const EventObject* elem);
  std::optional<double> whole_angular_min(AngKind kind, CollectionId a, CollectionId b, Span span,
                                          const EventObject* elem, EvalError& err);
  std::vector<BinOutcome> region_bins(std::size_t idx);
};

BRes Ev::region(std::size_t idx) {
  auto itc = region_cache.find(idx);
  if (itc != region_cache.end()) return itc->second;
  const auto& reg = it->hir().regions[idx];
  BRes result = BRes::ok(true);
  for (const auto& stmt : reg.stmts) {
    BRes o;
    switch (stmt.kind) {
      case HirRegionStmt::Kind::Select:
      case HirRegionStmt::Kind::Trigger:
        o = truth(stmt.node, nullptr);
        break;
      case HirRegionStmt::Kind::Reject: {
        o = truth(stmt.node, nullptr);
        if (!o.hard) o.pass = !o.pass;
        break;
      }
      case HirRegionStmt::Kind::Inherit:
        o = region(stmt.region);
        break;
      case HirRegionStmt::Kind::Bin:
      case HirRegionStmt::Kind::BinCond:
        continue;
      case HirRegionStmt::Kind::NonMembership:
        if (!stmt.tag.in_fragment) {
          o = BRes::err_(oof(stmt.span, "cannot evaluate region: " + stmt.tag.reason));
          break;
        }
        continue;
    }
    if (o.hard) {
      result = o;
      break;
    }
    if (!o.pass) {
      result = BRes::ok(false);
      break;
    }
  }
  region_cache[idx] = result;
  return result;
}

Tri Ev::region3(std::size_t idx) {
  // Do not consult region_cache: Kleene short-circuit rules differ from
  // two-valued cutflow (False over Unknown; never abort on the first error).
  std::optional<EvalError> unknown;
  const auto& reg = it->hir().regions[idx];
  for (const auto& stmt : reg.stmts) {
    Tri t;
    switch (stmt.kind) {
      case HirRegionStmt::Kind::Select:
      case HirRegionStmt::Kind::Trigger:
        t = truth3(stmt.node, nullptr);
        break;
      case HirRegionStmt::Kind::Reject:
        t = truth3(stmt.node, nullptr).tnot();
        break;
      case HirRegionStmt::Kind::Inherit:
        t = region3(stmt.region);
        break;
      case HirRegionStmt::Kind::Bin:
      case HirRegionStmt::Kind::BinCond:
        continue;
      case HirRegionStmt::Kind::NonMembership:
        if (!stmt.tag.in_fragment) {
          t = Tri::unknown(oof(stmt.span, "cannot evaluate region: " + stmt.tag.reason));
          break;
        }
        continue;
    }
    if (t.kind == TriKind::True) {
    } else if (t.kind == TriKind::False) {
      return Tri::ffalse();
    } else if (!unknown) {
      unknown = t.err;
    }
  }
  return unknown ? Tri::unknown(*unknown) : Tri::ttrue();
}

BRes Ev::reduce_bool(ReduceKind kind, CollectionId coll, const HNode& body,
                     const EventObject* elem) {
  EvalError e;
  const auto* objs = materialize(coll, e);
  if (!objs) return BRes::err_(e);
  for (const auto& obj : *objs) {
    reduce_stack.push_back(obj);
    BRes t = truth(body, elem);
    reduce_stack.pop_back();
    if (t.hard) return t;
    if (kind == ReduceKind::Any && t.pass) return BRes::ok(true);
    if (kind == ReduceKind::All && !t.pass) return BRes::ok(false);
  }
  return BRes::ok(kind == ReduceKind::All);
}

NRes Ev::reduce_num(ReduceKind kind, CollectionId coll, const HNode& body,
                    const EventObject* elem) {
  EvalError e;
  const auto* objs = materialize(coll, e);
  if (!objs) return NRes::err_(e);
  NumVal sum = NumVal::from_exact(Rat::zero());
  std::optional<NumVal> acc;
  for (const auto& obj : *objs) {
    reduce_stack.push_back(obj);
    NRes v = num(body, elem);
    reduce_stack.pop_back();
    if (v.k == NR::Err) return v;
    if (v.k == NR::NV) return v;
    auto s = adl2::sema::bin_arith(ArithOp::Add, sum, v.val);
    if (!s) return NRes::nv_(NonValueKind::NonFinite);
    sum = *s;
    if (!acc) acc = v.val;
    else if (kind == ReduceKind::Min) acc = adl2::sema::num_min(*acc, v.val);
    else if (kind == ReduceKind::Max) acc = adl2::sema::num_max(*acc, v.val);
  }
  if (kind == ReduceKind::Sum) return NRes::of(sum);
  if (!acc) return NRes::nv_(NonValueKind::EmptyReduction, adl2::sema::reduce_kind_str(kind));
  return NRes::of(*acc);
}

BRes Ev::truth(const HNode& n, const EventObject* elem) {
  if (!n.tag.in_fragment) return BRes::err_(oof(n.span, n.tag.reason));
  switch (n.kind) {
    case HKind::Bool:
      return BRes::ok(n.bool_val);
    case HKind::Not: {
      auto t = truth(*n.a, elem);
      if (!t.hard) t.pass = !t.pass;
      return t;
    }
    case HKind::And:
      for (const auto& p : n.items) {
        auto t = truth(p, elem);
        if (t.hard || !t.pass) return t;
      }
      return BRes::ok(true);
    case HKind::Or:
      for (const auto& p : n.items) {
        auto t = truth(p, elem);
        if (t.hard || t.pass) return t;
      }
      return BRes::ok(false);
    case HKind::Cmp: {
      EvalError e;
      if (auto r = whole_cmp(n.cmp, *n.a, *n.b, n.span, elem, e)) return BRes::ok(*r);
      if (!e.reason.empty()) return BRes::err_(e);
      auto a = num(*n.a, elem);
      if (a.k == NR::Err) return BRes::err_(a.err);
      auto b = num(*n.b, elem);
      if (b.k == NR::Err) return BRes::err_(b.err);
      if (a.k == NR::NV || b.k == NR::NV) return BRes::ok(false);
      return BRes::ok(cmp_num(n.cmp, a.val, b.val));
    }
    case HKind::Band: {
      auto v = num(*n.a, elem);
      if (v.k == NR::Err) return BRes::err_(v.err);
      if (v.k == NR::NV) return BRes::ok(false);
      auto lo = parse_rat(n.lo);
      auto hi = parse_rat(n.hi);
      if (!lo || !hi) return BRes::err_(oof(n.span, "malformed numeric literal"));
      return BRes::ok(band_ok(n.band, v.val, *lo, *hi));
    }
    case HKind::Ternary:
      if (auto g = truth(*n.a, elem); g.hard) return g;
      else if (g.pass) return truth(*n.b, elem);
      else if (n.c) return truth(*n.c, elem);
      else return BRes::ok(true);
    case HKind::RegionPred:
      return region(n.region_index);
    case HKind::Reduce:
      if (adl2::sema::reduce_kind_is_boolean(n.reduce))
        return reduce_bool(n.reduce, n.coll, *n.a, elem);
      // fallthrough numeric-as-pred
      [[fallthrough]];
    case HKind::Num:
    case HKind::Quantity:
    case HKind::ElemSelfProp:
    case HKind::ReduceProp:
    case HKind::Neg:
    case HKind::Abs:
    case HKind::ScalarMinMax:
    case HKind::Binary: {
      auto v = num(n, elem);
      if (v.k == NR::Err) return BRes::err_(v.err);
      if (v.k == NR::NV) return BRes::ok(false);
      return BRes::ok(v.val.is_nonzero());
    }
    case HKind::CollProp:
      return BRes::err_(oof(
          n.span, "unindexed per-element cut at region level is ambiguous (OPEN-1 unresolved)"));
    default:
      return BRes::err_(oof(n.span, "expression is outside the checked fragment"));
  }
}

NRes Ev::num(const HNode& n, const EventObject* elem) {
  if (!n.tag.in_fragment) return NRes::err_(oof(n.span, n.tag.reason));
  switch (n.kind) {
    case HKind::Num: {
      auto r = parse_rat(n.text);
      if (!r) return NRes::err_(oof(n.span, "malformed numeric literal `" + n.text + "`"));
      return NRes::of(NumVal::from_exact(*r));
    }
    case HKind::Bool:
      return NRes::of(NumVal::from_exact(n.bool_val ? Rat::one() : Rat::zero()));
    case HKind::Quantity:
      return quantity(n.qid, n.span, elem);
    case HKind::ElemSelfProp:
      if (!elem)
        return NRes::err_(oof(n.span, "implicit element property used outside an object block"));
      return obj_prop(*elem, n.prop);
    case HKind::ReduceProp:
      if (reduce_stack.empty())
        return NRes::err_(oof(n.span, "reducer element property used outside a reducer body"));
      return obj_prop(reduce_stack.back(), n.prop);
    case HKind::Reduce:
      if (!adl2::sema::reduce_kind_is_boolean(n.reduce))
        return reduce_num(n.reduce, n.coll, *n.a, elem);
      {
        auto t = truth(n, elem);
        if (t.hard) return NRes::err_(t.err);
        return NRes::of(NumVal::from_exact(t.pass ? Rat::one() : Rat::zero()));
      }
    case HKind::Neg: {
      auto a = num(*n.a, elem);
      if (a.k != NR::Val) return a;
      return NRes::of(a.val.negated());
    }
    case HKind::Abs: {
      auto a = num(*n.a, elem);
      if (a.k != NR::Val) return a;
      return NRes::of(a.val.abs());
    }
    case HKind::Binary: {
      auto a = num(*n.a, elem);
      if (a.k == NR::Err) return a;
      auto b = num(*n.b, elem);
      if (b.k == NR::Err) return b;
      if (a.k == NR::NV) return a;
      if (b.k == NR::NV) return b;
      return arith(n.arith, a.val, b.val);
    }
    case HKind::ScalarMinMax: {
      std::optional<NumVal> acc;
      for (const auto& a : n.items) {
        auto v = num(a, elem);
        if (v.k != NR::Val) return v;
        if (!acc) acc = v.val;
        else if (n.reduce == ReduceKind::Min) acc = adl2::sema::num_min(*acc, v.val);
        else acc = adl2::sema::num_max(*acc, v.val);
      }
      if (!acc) return NRes::nv_(NonValueKind::EmptyReduction, adl2::sema::reduce_kind_str(n.reduce));
      return NRes::of(*acc);
    }
    case HKind::Ternary: {
      auto g = truth(*n.a, elem);
      if (g.hard) return NRes::err_(g.err);
      if (g.pass) return num(*n.b, elem);
      if (n.c) return num(*n.c, elem);
      return NRes::of(NumVal::from_exact(Rat::one()));
    }
    case HKind::Not:
    case HKind::And:
    case HKind::Or:
    case HKind::Cmp:
    case HKind::Band:
    case HKind::RegionPred: {
      auto t = truth(n, elem);
      if (t.hard) return NRes::err_(t.err);
      return NRes::of(NumVal::from_exact(t.pass ? Rat::one() : Rat::zero()));
    }
    case HKind::CollProp:
      return NRes::err_(oof(
          n.span, "unindexed per-element cut at region level is ambiguous (OPEN-1 unresolved)"));
    default:
      return NRes::err_(oof(n.span, "expression is outside the checked fragment"));
  }
}

Tri Ev::reduce_bool3(ReduceKind kind, CollectionId coll, const HNode& body,
                     const EventObject* elem) {
  EvalError e;
  const auto* objs = materialize(coll, e);
  if (!objs) return Tri::unknown(e);
  std::optional<EvalError> unknown;
  for (const auto& obj : *objs) {
    reduce_stack.push_back(obj);
    Tri t = truth3(body, elem);
    reduce_stack.pop_back();
    if (kind == ReduceKind::Any && t.kind == TriKind::True) return Tri::ttrue();
    if (kind == ReduceKind::All && t.kind == TriKind::False) return Tri::ffalse();
    if (t.kind == TriKind::Unknown && !unknown) unknown = t.err;
  }
  if (unknown) return Tri::unknown(*unknown);
  return Tri::from_bool(kind == ReduceKind::All);
}

NRes Ev::reduce_num3(ReduceKind kind, CollectionId coll, const HNode& body,
                     const EventObject* elem) {
  EvalError e;
  const auto* objs = materialize(coll, e);
  if (!objs) return NRes::err_(e);
  NumVal sum = NumVal::from_exact(Rat::zero());
  std::optional<NumVal> acc;
  bool any = false;
  for (const auto& obj : *objs) {
    reduce_stack.push_back(obj);
    NRes v = num3(body, elem);
    reduce_stack.pop_back();
    if (v.k == NR::Err) return v;
    if (v.k == NR::NV) return v;  // soft non-value is absorbing
    auto s = adl2::sema::bin_arith(ArithOp::Add, sum, v.val);
    if (!s) return NRes::nv_(NonValueKind::NonFinite);
    sum = *s;
    any = true;
    if (!acc) acc = v.val;
    else if (kind == ReduceKind::Min) acc = adl2::sema::num_min(*acc, v.val);
    else if (kind == ReduceKind::Max) acc = adl2::sema::num_max(*acc, v.val);
  }
  if (kind == ReduceKind::Sum) return NRes::of(sum);
  if (kind == ReduceKind::Min || kind == ReduceKind::Max) {
    if (any) return NRes::of(*acc);
    return NRes::nv_(NonValueKind::EmptyReduction, adl2::sema::reduce_kind_str(kind));
  }
  return NRes::of(sum);
}

Tri Ev::truth3(const HNode& n, const EventObject* elem) {
  if (!n.tag.in_fragment) return Tri::unknown(oof(n.span, n.tag.reason));
  switch (n.kind) {
    case HKind::Bool:
      return Tri::from_bool(n.bool_val);
    case HKind::Not:
      return truth3(*n.a, elem).tnot();
    case HKind::And: {
      std::optional<EvalError> unknown;
      for (const auto& p : n.items) {
        Tri t = truth3(p, elem);
        if (t.kind == TriKind::False) return Tri::ffalse();
        if (t.kind == TriKind::Unknown && !unknown) unknown = t.err;
      }
      return unknown ? Tri::unknown(*unknown) : Tri::ttrue();
    }
    case HKind::Or: {
      std::optional<EvalError> unknown;
      for (const auto& p : n.items) {
        Tri t = truth3(p, elem);
        if (t.kind == TriKind::True) return Tri::ttrue();
        if (t.kind == TriKind::Unknown && !unknown) unknown = t.err;
      }
      return unknown ? Tri::unknown(*unknown) : Tri::ffalse();
    }
    case HKind::Ternary: {
      Tri g = truth3(*n.a, elem);
      if (g.kind == TriKind::True) return truth3(*n.b, elem);
      if (g.kind == TriKind::False) return n.c ? truth3(*n.c, elem) : Tri::ttrue();
      Tri then_t = truth3(*n.b, elem);
      Tri else_t = n.c ? truth3(*n.c, elem) : Tri::ttrue();
      if (then_t.kind == TriKind::False && else_t.kind == TriKind::False) return Tri::ffalse();
      if (then_t.kind == TriKind::True && else_t.kind == TriKind::True) return Tri::ttrue();
      return Tri::unknown(g.err);
    }
    case HKind::RegionPred:
      return region3(n.region_index);
    case HKind::Reduce:
      if (adl2::sema::reduce_kind_is_boolean(n.reduce))
        return reduce_bool3(n.reduce, n.coll, *n.a, elem);
      {
        auto v = num3(n, elem);
        if (v.k == NR::Err) return Tri::unknown(v.err);
        if (v.k == NR::NV) return Tri::ffalse();
        return Tri::from_bool(v.val.is_nonzero());
      }
    case HKind::Cmp: {
      EvalError e;
      if (auto r = whole_cmp(n.cmp, *n.a, *n.b, n.span, elem, e)) return Tri::from_bool(*r);
      if (!e.reason.empty()) return Tri::unknown(e);
      auto a = num3(*n.a, elem);
      auto b = num3(*n.b, elem);
      // §4.4: soft non-value is ABSORBING — decidable False even if the
      // other operand is a blocking Unknown.
      if (a.k == NR::NV || b.k == NR::NV) return Tri::ffalse();
      if (a.k == NR::Err) return Tri::unknown(a.err);
      if (b.k == NR::Err) return Tri::unknown(b.err);
      return Tri::from_bool(cmp_num(n.cmp, a.val, b.val));
    }
    case HKind::Band: {
      auto v = num3(*n.a, elem);
      if (v.k == NR::Err) return Tri::unknown(v.err);
      if (v.k == NR::NV) return Tri::ffalse();
      auto lo = parse_rat(n.lo);
      auto hi = parse_rat(n.hi);
      if (!lo) return Tri::unknown(oof(n.span, "malformed numeric literal `" + n.lo + "`"));
      if (!hi) return Tri::unknown(oof(n.span, "malformed numeric literal `" + n.hi + "`"));
      return Tri::from_bool(band_ok(n.band, v.val, *lo, *hi));
    }
    case HKind::Num:
    case HKind::Quantity:
    case HKind::ElemSelfProp:
    case HKind::ReduceProp:
    case HKind::Neg:
    case HKind::Abs:
    case HKind::ScalarMinMax:
    case HKind::Binary: {
      auto v = num3(n, elem);
      if (v.k == NR::Err) return Tri::unknown(v.err);
      if (v.k == NR::NV) return Tri::ffalse();
      return Tri::from_bool(v.val.is_nonzero());
    }
    case HKind::CollProp:
    case HKind::Particle:
    case HKind::CollValue:
    case HKind::Unsupported: {
      auto t = truth(n, elem);
      if (t.hard) return Tri::unknown(t.err);
      return Tri::from_bool(t.pass);
    }
  }
  return Tri::unknown(oof(n.span, "expression is outside the checked fragment"));
}

NRes Ev::num3(const HNode& n, const EventObject* elem) {
  if (!n.tag.in_fragment) return NRes::err_(oof(n.span, n.tag.reason));
  switch (n.kind) {
    case HKind::Neg: {
      auto a = num3(*n.a, elem);
      if (a.k != NR::Val) return a;
      return NRes::of(a.val.negated());
    }
    case HKind::Abs: {
      auto a = num3(*n.a, elem);
      if (a.k != NR::Val) return a;
      return NRes::of(a.val.abs());
    }
    case HKind::Binary: {
      auto a = num3(*n.a, elem);
      auto b = num3(*n.b, elem);
      // §4.4: soft non-value is ABSORBING in arithmetic — checked before
      // blocking Err so `softNV * opaque > k` stays a decidable False.
      if (a.k == NR::NV) return a;
      if (b.k == NR::NV) return b;
      if (a.k == NR::Err) return a;
      if (b.k == NR::Err) return b;
      return arith(n.arith, a.val, b.val);
    }
    case HKind::ScalarMinMax: {
      std::optional<NumVal> acc;
      for (const auto& a : n.items) {
        auto v = num3(a, elem);
        if (v.k == NR::Err) return v;
        if (v.k == NR::NV) return v;
        if (!acc) acc = v.val;
        else if (n.reduce == ReduceKind::Min) acc = adl2::sema::num_min(*acc, v.val);
        else acc = adl2::sema::num_max(*acc, v.val);
      }
      if (!acc) return NRes::nv_(NonValueKind::EmptyReduction, adl2::sema::reduce_kind_str(n.reduce));
      return NRes::of(*acc);
    }
    case HKind::Ternary: {
      Tri g = truth3(*n.a, elem);
      if (g.kind == TriKind::True) return num3(*n.b, elem);
      if (g.kind == TriKind::False) {
        if (n.c) return num3(*n.c, elem);
        return NRes::of(NumVal::from_exact(Rat::one()));
      }
      auto then_v = num3(*n.b, elem);
      if (then_v.k == NR::Err) return then_v;
      NRes else_v = n.c ? num3(*n.c, elem) : NRes::of(NumVal::from_exact(Rat::one()));
      if (else_v.k == NR::Err) return else_v;
      if (then_v.k == NR::Val && else_v.k == NR::Val && then_v.val == else_v.val) return then_v;
      if (then_v.k == NR::NV && else_v.k == NR::NV) return then_v;
      return NRes::err_(g.err);
    }
    case HKind::Not:
    case HKind::And:
    case HKind::Or:
    case HKind::Cmp:
    case HKind::Band:
    case HKind::RegionPred: {
      Tri t = truth3(n, elem);
      if (t.kind == TriKind::Unknown) return NRes::err_(t.err);
      return NRes::of(NumVal::from_exact(t.kind == TriKind::True ? Rat::one() : Rat::zero()));
    }
    case HKind::Reduce:
      if (adl2::sema::reduce_kind_is_boolean(n.reduce)) {
        Tri t = truth3(n, elem);
        if (t.kind == TriKind::Unknown) return NRes::err_(t.err);
        return NRes::of(NumVal::from_exact(t.kind == TriKind::True ? Rat::one() : Rat::zero()));
      }
      return reduce_num3(n.reduce, n.coll, *n.a, elem);
    case HKind::Num:
    case HKind::Bool:
    case HKind::Quantity:
    case HKind::ElemSelfProp:
    case HKind::ReduceProp:
    case HKind::CollProp:
    case HKind::Particle:
    case HKind::CollValue:
    case HKind::Unsupported:
      return num(n, elem);
  }
  return NRes::err_(oof(n.span, "expression is outside the checked fragment"));
}

NRes Ev::event_scalar_q(const Quantity& q, Span span) {
  switch (q.scalar.kind) {
    case ScalarSourceKind::MetProp: {
      if (event->met.empty()) return NRes::err_(miss(span, "event has no MET vector"));
      const auto& key = it->hir().table.prop_key(q.scalar.prop);
      auto itm = event->met.find(key);
      if (itm == event->met.end())
        return NRes::err_(miss(
            span, "event MET has no `" + it->hir().table.prop_display(q.scalar.prop) + "` component"));
      return NRes::of(NumVal::from_exact(itm->second));
    }
    case ScalarSourceKind::EventVar: {
      const auto& key = it->hir().symbols.key(q.scalar.name);
      auto itm = event->scalars.find(key);
      if (itm == event->scalars.end())
        return NRes::err_(
            miss(span, "event has no scalar `" + it->hir().symbols.display(q.scalar.name) + "`"));
      return NRes::of(NumVal::from_exact(itm->second));
    }
    case ScalarSourceKind::Trigger: {
      const auto& key = it->hir().symbols.key(q.scalar.name);
      auto itm = event->triggers.find(key);
      if (itm == event->triggers.end())
        return NRes::err_(miss(
            span, "event has no trigger flag `" + it->hir().symbols.display(q.scalar.name) + "`"));
      return NRes::of(NumVal::from_exact(itm->second));
    }
  }
  return NRes::err_(oof(span, "unknown event scalar"));
}

NRes Ev::quantity(QuantityId q, Span span, const EventObject* elem) {
  AngKind ak;
  CollectionId ca, cb;
  if (it->hir().table.whole_pair_legs(q, ak, ca, cb)) {
    EvalError e;
    auto m = whole_angular_min(ak, ca, cb, span, elem, e);
    if (!e.reason.empty()) return NRes::err_(e);
    if (!m) return NRes::nv_(NonValueKind::EmptyReduction, "min pairwise separation");
    return approx(*m);
  }
  const auto& qq = it->hir().table.quantity(q);
  switch (qq.kind) {
    case QuantityKind::EventScalar:
      return event_scalar_q(qq, span);
    case QuantityKind::Size: {
      EvalError e;
      const auto* objs = materialize(qq.coll, e);
      if (!objs) return NRes::err_(e);
      return NRes::of(NumVal::from_exact(Rat::from_i64(static_cast<std::int64_t>(objs->size()))));
    }
    case QuantityKind::ElemProp: {
      EvalError e;
      const auto* objs = materialize(qq.coll, e);
      if (!objs) return NRes::err_(e);
      if (auto pos = elem_pos(qq.index, objs->size())) return obj_prop((*objs)[*pos], qq.prop);
      return NRes::nv_(NonValueKind::MissingElement,
                       coll_label(qq.coll) + "[" + qq.index.to_string() + "]");
    }
    case QuantityKind::AngularSep:
      return angular(qq.ang, qq.a, qq.b, span, elem);
    case QuantityKind::Present: {
      auto inner = quantity(qq.inner, span, elem);
      bool present = inner.k == NR::Val;
      return NRes::of(NumVal::from_exact(present ? Rat::one() : Rat::zero()));
    }
    case QuantityKind::ExternalFn: {
      const auto& fname = it->hir().symbols.key(qq.name);
      if (qq.args.size() == 1 && qq.args[0].kind == QuantityArgKind::Particle) {
        auto getter = [&](const LV& lv) -> std::optional<NumVal> {
          if (fname == "mass" || fname == "m") return lv.mass();
          if (fname == "pt") return lv.pt();
          if (fname == "eta") return lv.eta();
          if (fname == "phi") return lv.phi();
          if (fname == "e" || fname == "energy") return lv.energy();
          return std::nullopt;
        };
        if (fname == "mass" || fname == "m" || fname == "pt" || fname == "eta" || fname == "phi" ||
            fname == "e" || fname == "energy") {
          NonValue nv;
          EvalError err;
          bool hard = false;
          auto lv = lorentz(qq.args[0].particle, span, elem, nv, err, hard);
          if (hard) return NRes::err_(err);
          if (!lv) {
            NRes r;
            r.k = NR::NV;
            r.nv = nv;
            return r;
          }
          if (auto v = getter(*lv)) return NRes::of(*v);
          return NRes::nv_(NonValueKind::NonFinite);
        }
      }
      if (qq.args.size() == 1) {
        auto fn = [&](double x) -> std::optional<double> {
          if (fname == "sqrt") return std::sqrt(x);
          if (fname == "cos") return std::cos(x);
          if (fname == "sin") return std::sin(x);
          if (fname == "tan") return std::tan(x);
          if (fname == "log") return std::log(x);
          return std::nullopt;
        };
        if (auto f = fn(0); fname == "sqrt" || fname == "cos" || fname == "sin" || fname == "tan" ||
                            fname == "log") {
          (void)f;
          NRes a = arg_num(qq.args[0], span, elem);
          if (a.k != NR::Val) return a;
          double x = a.val.to_f64();
          double y = 0;
          if (fname == "sqrt") y = std::sqrt(x);
          else if (fname == "cos") y = std::cos(x);
          else if (fname == "sin") y = std::sin(x);
          else if (fname == "tan") y = std::tan(x);
          else y = std::log(x);
          return approx(y);
        }
      }
      return NRes::err_(
          oof(span, "external function `" + fname + "` has no reference interpretation"));
    }
  }
  return NRes::err_(oof(span, "unhandled quantity"));
}

NRes Ev::arg_num(const QuantityArg& arg, Span span, const EventObject* elem) {
  if (arg.kind == QuantityArgKind::Num) {
    auto r = parse_rat(arg.text);
    if (!r) return NRes::err_(oof(span, "malformed numeric literal `" + arg.text + "`"));
    return NRes::of(NumVal::from_exact(*r));
  }
  if (arg.kind == QuantityArgKind::Quantity) return quantity(arg.qid, span, elem);
  return NRes::err_(oof(span, "function argument is outside the checked fragment"));
}

NRes Ev::angular(AngKind kind, const ParticleRef& a, const ParticleRef& b, Span span,
                 const EventObject* elem) {
  NonValue nv;
  EvalError err;
  bool hard = false;
  auto pa = angles(a, span, elem, nv, err, hard);
  if (hard) return NRes::err_(err);
  if (!pa) {
    NRes r;
    r.k = NR::NV;
    r.nv = nv;
    return r;
  }
  auto pb = angles(b, span, elem, nv, err, hard);
  if (hard) return NRes::err_(err);
  if (!pb) {
    NRes r;
    r.k = NR::NV;
    r.nv = nv;
    return r;
  }
  auto dphi = [&]() -> NRes {
    if (pa->phi && pb->phi) return approx(wrap_dphi(*pa->phi - *pb->phi));
    return NRes::nv_(NonValueKind::MissingProperty, "phi");
  };
  auto deta = [&]() -> NRes {
    if (pa->eta && pb->eta) return approx(*pa->eta - *pb->eta);
    return NRes::nv_(NonValueKind::MissingProperty, "eta");
  };
  if (kind == AngKind::DPhi) return dphi();
  if (kind == AngKind::DEta) return deta();
  auto de = deta();
  auto dp = dphi();
  if (de.k == NR::NV) return de;
  if (dp.k == NR::NV) return dp;
  if (de.k == NR::Err) return de;
  if (dp.k == NR::Err) return dp;
  return approx(std::hypot(de.val.to_f64(), dp.val.to_f64()));
}

std::optional<double> Ev::whole_angular_min(AngKind kind, CollectionId a, CollectionId b, Span span,
                                            const EventObject* elem, EvalError& err) {
  const auto* oa = materialize(a, err);
  if (!oa) return std::nullopt;
  const auto* ob = materialize(b, err);
  if (!ob) return std::nullopt;
  std::optional<double> min;
  for (std::size_t i = 0; i < oa->size(); ++i) {
    for (std::size_t j = 0; j < ob->size(); ++j) {
      ParticleRef pa = ParticleRef::elem(a, ElemIndex::from_front(static_cast<std::uint32_t>(i)));
      ParticleRef pb = ParticleRef::elem(b, ElemIndex::from_front(static_cast<std::uint32_t>(j)));
      auto d = angular(kind, pa, pb, span, elem);
      if (d.k == NR::Err) {
        err = d.err;
        return std::nullopt;
      }
      if (d.k == NR::Val) {
        double x = d.val.to_f64();
        min = min ? std::min(*min, x) : x;
      }
    }
  }
  err = EvalError{};
  return min;
}

std::optional<bool> Ev::whole_cmp(CmpOp op, const HNode& lhs, const HNode& rhs, Span span,
                                  const EventObject* elem, EvalError& err) {
  err = EvalError{};
  QuantityId q{};
  CmpOp use = op;
  const HNode* other = nullptr;
  auto is_whole = [&](const HNode& n) -> std::optional<QuantityId> {
    if (n.kind != HKind::Quantity) return std::nullopt;
    AngKind k;
    CollectionId a, b;
    if (it->hir().table.whole_pair_legs(n.qid, k, a, b)) return n.qid;
    return std::nullopt;
  };
  if (auto w = is_whole(lhs)) {
    q = *w;
    other = &rhs;
  } else if (auto w = is_whole(rhs)) {
    q = *w;
    use = adl2::sema::cmp_op_flipped(op);
    other = &lhs;
  } else {
    return std::nullopt;
  }
  AngKind kind;
  CollectionId a, b;
  if (!it->hir().table.whole_pair_legs(q, kind, a, b)) return std::nullopt;
  auto c = num(*other, elem);
  if (c.k == NR::Err) {
    err = c.err;
    return std::nullopt;
  }
  if (c.k == NR::NV) return false;
  const auto* oa = materialize(a, err);
  if (!oa) return std::nullopt;
  const auto* ob = materialize(b, err);
  if (!ob) return std::nullopt;
  auto pair = [&](std::size_t i, std::size_t j) {
    ParticleRef pa = ParticleRef::elem(a, ElemIndex::from_front(static_cast<std::uint32_t>(i)));
    ParticleRef pb = ParticleRef::elem(b, ElemIndex::from_front(static_cast<std::uint32_t>(j)));
    return angular(kind, pa, pb, span, elem);
  };
  if (use == CmpOp::Gt || use == CmpOp::Ge || use == CmpOp::Lt || use == CmpOp::Le) {
    bool forall = use == CmpOp::Gt || use == CmpOp::Ge;
    if (oa->empty() || ob->empty()) return forall;
    for (std::size_t i = 0; i < oa->size(); ++i) {
      for (std::size_t j = 0; j < ob->size(); ++j) {
        auto d = pair(i, j);
        if (d.k == NR::Err) {
          err = d.err;
          return std::nullopt;
        }
        bool holds = d.k == NR::Val && cmp_num(use, d.val, c.val);
        if (forall && !holds) return false;
        if (!forall && holds) return true;
      }
    }
    return forall;
  }
  auto min = whole_angular_min(kind, a, b, span, elem, err);
  if (!err.reason.empty()) return std::nullopt;
  double m = min.value_or(std::numeric_limits<double>::infinity());
  auto approx_v = NumVal::from_f64(m);
  if (!approx_v) approx_v = NumVal::from_f64(0);
  return cmp_num(use, *approx_v, c.val);
}

std::optional<LV> Ev::lorentz(const ParticleRef& p, Span span, const EventObject* elem, NonValue& nv,
                              EvalError& err, bool& hard) {
  hard = false;
  switch (p.kind) {
    case ParticleKind::Sum: {
      std::optional<LV> acc;
      for (const auto& part : p.parts) {
        auto lv = lorentz(part, span, elem, nv, err, hard);
        if (hard) return std::nullopt;
        if (!lv) return std::nullopt;
        acc = acc ? *acc + *lv : *lv;
      }
      if (!acc) {
        hard = true;
        err = oof(span, "empty 4-vector sum");
        return std::nullopt;
      }
      return acc;
    }
    case ParticleKind::Elem: {
      const auto* objs = materialize(p.coll, err);
      if (!objs) {
        hard = true;
        return std::nullopt;
      }
      if (auto pos = elem_pos(p.index, objs->size())) return obj_lv((*objs)[*pos], nv);
      nv.kind = NonValueKind::MissingElement;
      nv.detail = coll_label(p.coll) + "[" + p.index.to_string() + "]";
      return std::nullopt;
    }
    case ParticleKind::ReduceElem:
      if (reduce_stack.empty()) {
        hard = true;
        err = oof(span, "reducer element used outside a reducer body");
        return std::nullopt;
      }
      return obj_lv(reduce_stack.back(), nv);
    case ParticleKind::ThisElem:
      if (!elem) {
        hard = true;
        err = oof(span, "`this` used outside an object block");
        return std::nullopt;
      }
      return obj_lv(*elem, nv);
    case ParticleKind::Met: {
      if (event->met.empty()) {
        hard = true;
        err = oof(span, "event has no MET vector");
        return std::nullopt;
      }
      auto pt = event->met.find(it->pt_key());
      auto phi = event->met.find(it->phi_key());
      if (pt == event->met.end() || phi == event->met.end()) {
        nv.kind = NonValueKind::MissingProperty;
        nv.detail = "MET pt/phi";
        return std::nullopt;
      }
      return LV::transverse(pt->second.to_f64(), phi->second.to_f64());
    }
    case ParticleKind::Binder: {
      auto itb = binder_env.find(p.name);
      if (itb == binder_env.end()) {
        hard = true;
        err = oof(span, "composite binder used outside a tuple environment");
        return std::nullopt;
      }
      return obj_lv(itb->second, nv);
    }
    case ParticleKind::Whole:
      hard = true;
      err = oof(span, "4-vector over an unindexed collection is unsupported");
      return std::nullopt;
  }
  hard = true;
  err = oof(span, "unhandled particle");
  return std::nullopt;
}

std::optional<Angles> Ev::angles(const ParticleRef& p, Span span, const EventObject* elem,
                                 NonValue& nv, EvalError& err, bool& hard) {
  hard = false;
  switch (p.kind) {
    case ParticleKind::Elem: {
      const auto* objs = materialize(p.coll, err);
      if (!objs) {
        hard = true;
        return std::nullopt;
      }
      if (auto pos = elem_pos(p.index, objs->size())) return obj_ang((*objs)[*pos]);
      nv.kind = NonValueKind::MissingElement;
      nv.detail = coll_label(p.coll) + "[" + p.index.to_string() + "]";
      return std::nullopt;
    }
    case ParticleKind::Met: {
      if (event->met.empty()) {
        hard = true;
        err = oof(span, "event has no MET vector");
        return std::nullopt;
      }
      Angles a;
      auto phi = event->met.find(it->phi_key());
      if (phi != event->met.end()) a.phi = phi->second.to_f64();
      return a;
    }
    case ParticleKind::ReduceElem:
      if (reduce_stack.empty()) {
        hard = true;
        err = oof(span, "reducer element used outside a reducer body");
        return std::nullopt;
      }
      return obj_ang(reduce_stack.back());
    case ParticleKind::ThisElem:
      if (!elem) {
        hard = true;
        err = oof(span, "`this` used outside an object block");
        return std::nullopt;
      }
      return obj_ang(*elem);
    case ParticleKind::Sum: {
      auto lv = lorentz(p, span, elem, nv, err, hard);
      if (hard || !lv) return std::nullopt;
      auto eta = lv->eta();
      auto phi = lv->phi();
      if (!eta || !phi) {
        nv.kind = NonValueKind::NonFinite;
        return std::nullopt;
      }
      return Angles{eta->to_f64(), phi->to_f64()};
    }
    case ParticleKind::Whole:
      hard = true;
      err = oof(span,
                "angular separation over an unindexed collection is ambiguous (OPEN-1 unresolved)");
      return std::nullopt;
    case ParticleKind::Binder: {
      auto itb = binder_env.find(p.name);
      if (itb == binder_env.end()) {
        hard = true;
        err = oof(span, "composite binder used outside a tuple environment");
        return std::nullopt;
      }
      return obj_ang(itb->second);
    }
  }
  hard = true;
  err = oof(span, "unhandled particle");
  return std::nullopt;
}

const std::vector<CombTuple>* Ev::comb_tuples(CollectionId id, EvalError& err) {
  auto itc = combs.find(id);
  if (itc != combs.end()) return &itc->second;
  const auto& c = it->hir().table.collection(id);
  if (c.kind != CollectionKind::Combination) {
    err = oof(Span{}, "internal: comb_tuples on a non-composite");
    return nullptr;
  }
  std::vector<const std::vector<EventObject>*> slots;
  for (auto p : c.parts) {
    auto* s = materialize(p, err);
    if (!s) return nullptr;
    slots.push_back(s);
  }
  std::vector<std::size_t> sizes;
  for (auto* s : slots) sizes.push_back(s->size());
  auto idxs = enumerate_tuples(c.comb_kind, c.parts, sizes);
  bool drop_eq = c.comb_kind == CombKind::Disjoint;
  std::vector<CombTuple> out;
  for (const auto& ix : idxs) {
    if (drop_eq) {
      bool skip = false;
      for (std::size_t a = 0; a < ix.size() && !skip; ++a)
        for (std::size_t b = a + 1; b < ix.size(); ++b)
          if (kin_eq((*slots[a])[ix[a]], (*slots[b])[ix[b]])) skip = true;
      if (skip) continue;
    }
    CombTuple t;
    for (std::size_t slot = 0; slot < ix.size(); ++slot)
      t.binders[c.members[slot].name] = (*slots[slot])[ix[slot]];
    if (c.candidate) {
      auto prev = binder_env;
      binder_env = t.binders;
      NonValue nv;
      bool hard = false;
      auto lv = lorentz(c.candidate->vector, Span{}, nullptr, nv, err, hard);
      binder_env = std::move(prev);
      if (hard) return nullptr;
      if (!lv) continue;
      auto pt = lv->pt(), eta = lv->eta(), phi = lv->phi(), mass = lv->mass();
      if (!pt || !eta || !phi || !mass) continue;
      EventObject o;
      auto put = [&](const std::string& k, double v) {
        if (auto r = Rat::from_decimal_f64(v)) o.props[k] = *r;
      };
      put(it->pt_key(), pt->to_f64());
      put(it->eta_key(), eta->to_f64());
      put(it->phi_key(), phi->to_f64());
      put(it->mass_key(), mass->to_f64());
      t.candidate = std::move(o);
    }
    bool pass = true;
    for (auto pred : c.cuts) {
      auto prev = binder_env;
      binder_env = t.binders;
      auto tr = truth(it->hir().elem_pred(pred).node, nullptr);
      binder_env = std::move(prev);
      if (tr.hard) {
        err = tr.err;
        return nullptr;
      }
      if (!tr.pass) {
        pass = false;
        break;
      }
    }
    if (pass) out.push_back(std::move(t));
  }
  auto [itn, _] = combs.emplace(id, std::move(out));
  return &itn->second;
}

const std::vector<EventObject>* Ev::materialize(CollectionId id, EvalError& err) {
  auto itc = colls.find(id);
  if (itc != colls.end()) return &itc->second;
  const auto& c = it->hir().table.collection(id);
  std::vector<EventObject> objs;
  switch (c.kind) {
    case CollectionKind::Base: {
      const auto& key = it->hir().symbols.key(c.base);
      if (key == MET_FAMILY_KEY) {
        err = oof(Span{}, "the MET family is an event vector, not an object list");
        return nullptr;
      }
      auto ite = event->collections.find(key);
      if (ite != event->collections.end()) objs = ite->second;
      break;
    }
    case CollectionKind::Filtered: {
      auto* src = materialize(c.parent, err);
      if (!src) return nullptr;
      const HNode& pred = it->hir().elem_pred(c.pred).node;
      for (const auto& obj : *src) {
        auto t = truth(pred, &obj);
        if (t.hard) {
          err = t.err;
          return nullptr;
        }
        if (t.pass) objs.push_back(obj);
      }
      break;
    }
    case CollectionKind::Union:
      for (auto p : c.parts) {
        auto* src = materialize(p, err);
        if (!src) return nullptr;
        objs.insert(objs.end(), src->begin(), src->end());
      }
      break;
    case CollectionKind::Sorted: {
      auto* src = materialize(c.parent, err);
      if (!src) return nullptr;
      objs = *src;
      if (c.sort_key.kind == SortKeyKind::Prop) {
        std::string pkey = it->hir().table.prop_key(c.sort_key.prop);
        std::stable_sort(objs.begin(), objs.end(), [&](const EventObject& a, const EventObject& b) {
          const Rat* va = a.get(pkey);
          const Rat* vb = b.get(pkey);
          int ord = 0;
          if (va && vb) {
            if (*va < *vb) ord = -1;
            else if (*vb < *va) ord = 1;
          } else if (va && !vb)
            ord = -1;
          else if (!va && vb)
            ord = 1;
          if (c.sort_dir == SortDir::Descend) ord = -ord;
          return ord < 0;
        });
      }
      break;
    }
    case CollectionKind::Slice: {
      auto* src = materialize(c.parent, err);
      if (!src) return nullptr;
      std::size_t lo = std::min(static_cast<std::size_t>(c.slice_start), src->size());
      std::size_t hi = c.slice_end ? std::clamp(static_cast<std::size_t>(*c.slice_end), lo, src->size())
                                   : src->size();
      objs.assign(src->begin() + static_cast<std::ptrdiff_t>(lo),
                  src->begin() + static_cast<std::ptrdiff_t>(hi));
      break;
    }
    case CollectionKind::Combination: {
      auto* ts = comb_tuples(id, err);
      if (!ts) return nullptr;
      for (const auto& t : *ts) objs.push_back(t.candidate.value_or(EventObject{}));
      break;
    }
    case CollectionKind::CombProject: {
      auto* ts = comb_tuples(c.parent, err);
      if (!ts) return nullptr;
      for (const auto& t : *ts) {
        if (c.axis.kind == CombAxisKind::Candidate)
          objs.push_back(t.candidate.value_or(EventObject{}));
        else {
          auto itb = t.binders.find(c.axis.name);
          objs.push_back(itb == t.binders.end() ? EventObject{} : itb->second);
        }
      }
      break;
    }
  }
  auto [itn, _] = colls.emplace(id, std::move(objs));
  return &itn->second;
}

std::vector<BinOutcome> Ev::region_bins(std::size_t idx) {
  std::vector<BinOutcome> bins;
  for (const auto& stmt : it->hir().regions[idx].stmts) {
    if (stmt.kind == HirRegionStmt::Kind::Bin) {
      BinOutcome b;
      b.kind = BinOutcomeKind::Boundary;
      b.label = stmt.label;
      std::vector<double> edges;
      bool bad = false;
      for (const auto& t : stmt.edges) {
        try {
          edges.push_back(std::stod(t));
        } catch (...) {
          b.kind = BinOutcomeKind::Failed;
          b.reason = "malformed numeric literal `" + t + "`";
          bad = true;
          break;
        }
      }
      if (!bad) {
        auto v = num(stmt.node, nullptr);
        if (v.k == NR::Err) {
          b.kind = BinOutcomeKind::Failed;
          b.reason = v.err.reason;
        } else if (v.k == NR::NV) {
          b.value = std::nullopt;
          b.bin = std::nullopt;
        } else {
          double f = v.val.to_f64();
          b.value = f;
          b.bin = assign_bin(f, edges);
        }
      }
      bins.push_back(std::move(b));
    } else if (stmt.kind == HirRegionStmt::Kind::BinCond) {
      BinOutcome b;
      b.label = stmt.label;
      auto t = truth(stmt.node, nullptr);
      if (t.hard) {
        b.kind = BinOutcomeKind::Failed;
        b.reason = t.err.reason;
      } else {
        b.kind = BinOutcomeKind::Cond;
        b.member = t.pass;
      }
      bins.push_back(std::move(b));
    }
  }
  return bins;
}

}  // namespace

double wrap_dphi(double d) { return rem_euclid(d + kPi, 2.0 * kPi) - kPi; }

std::optional<std::size_t> assign_bin(double v, const std::vector<double>& edges) {
  if (edges.empty() || !std::isfinite(v)) return std::nullopt;
  auto last = edges.size() - 1;
  if (v >= edges[last]) return last;
  for (std::size_t i = 0; i < last; ++i) {
    if (edges[i] <= v && v < edges[i + 1]) return i;
  }
  return std::nullopt;
}

std::string format_region_text(const RegionResult& r) {
  if (!r.pass) return "ERROR: " + r.error;
  if (!*r.pass) return "fail";
  std::string s = "PASS";
  for (const auto& b : r.bins) {
    std::string label = b.label.value_or("bin");
    if (b.kind == BinOutcomeKind::Boundary) {
      if (b.bin) s += " [" + label + "=" + std::to_string(*b.bin) + "]";
      else s += " [" + label + "=underflow]";
    } else if (b.kind == BinOutcomeKind::Cond) {
      s += " [" + label + "=" + (b.member ? "true" : "false") + "]";
    } else {
      s += " [" + label + ": error " + b.reason + "]";
    }
  }
  return s;
}

Interp::Interp(const Hir& hir, const adl2::sema::ExtDecls& ext)
    : hir_(&hir),
      ext_(&ext),
      eta_key_(ext.prop_canon("eta").first),
      phi_key_(ext.prop_canon("phi").first),
      pt_key_(ext.prop_canon("pt").first),
      mass_key_(ext.prop_canon("mass").first) {}

std::optional<bool> Interp::eval_region_by_name(const std::string& name, const Event& event,
                                                EvalError& err) const {
  adl2::sema::Symbol sym;
  if (!hir_->symbols.lookup(name, sym)) {
    err = mkerr(Span{}, "no region named `" + name + "`");
    return std::nullopt;
  }
  std::optional<std::size_t> idx;
  for (std::size_t i = 0; i < hir_->regions.size(); ++i) {
    if (hir_->regions[i].name == sym) {
      idx = i;
      break;
    }
  }
  if (!idx) {
    err = mkerr(Span{}, "no region named `" + name + "`");
    return std::nullopt;
  }
  Ev ev{this, &event, {}, {}, {}, {}, {}};
  auto r = ev.region(*idx);
  if (r.hard) {
    err = r.err;
    return std::nullopt;
  }
  return r.pass;
}

namespace {
std::optional<bool> tri_to_opt(const Tri& t, EvalError& err) {
  if (t.kind == TriKind::Unknown) {
    err = t.err;
    return std::nullopt;
  }
  return t.kind == TriKind::True;
}
}  // namespace

std::optional<bool> Interp::eval_region_membership(const std::string& name, const Event& event,
                                                   EvalError& err) const {
  adl2::sema::Symbol sym;
  if (!hir_->symbols.lookup(name, sym)) {
    err = mkerr(Span{}, "no region named `" + name + "`");
    return std::nullopt;
  }
  std::optional<std::size_t> idx;
  for (std::size_t i = 0; i < hir_->regions.size(); ++i) {
    if (hir_->regions[i].name == sym) {
      idx = i;
      break;
    }
  }
  if (!idx) {
    err = mkerr(Span{}, "no region named `" + name + "`");
    return std::nullopt;
  }
  Ev ev{this, &event, {}, {}, {}, {}, {}};
  return tri_to_opt(ev.region3(*idx), err);
}

std::optional<bool> Interp::eval_region_membership_idx(std::size_t idx, const Event& event,
                                                       EvalError& err) const {
  if (idx >= hir_->regions.size()) {
    err = mkerr(Span{}, "region index " + std::to_string(idx) + " out of range");
    return std::nullopt;
  }
  Ev ev{this, &event, {}, {}, {}, {}, {}};
  return tri_to_opt(ev.region3(idx), err);
}

std::vector<RegionResult> Interp::run_event(const Event& event) const {
  Ev ev{this, &event, {}, {}, {}, {}, {}};
  std::vector<RegionResult> out;
  out.reserve(hir_->regions.size());
  for (std::size_t i = 0; i < hir_->regions.size(); ++i) {
    RegionResult rr;
    rr.name = hir_->symbols.display(hir_->regions[i].name);
    auto r = ev.region(i);
    if (r.hard) {
      rr.error = r.err.reason;
    } else {
      rr.pass = r.pass;
      if (r.pass) rr.bins = ev.region_bins(i);
    }
    out.push_back(std::move(rr));
  }
  return out;
}

int module_anchor() { return 3; }

}  // namespace adl2::interp
