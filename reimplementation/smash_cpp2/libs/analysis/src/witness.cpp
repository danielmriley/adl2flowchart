#include "adl2/analysis/witness.hpp"
#include "adl2/analysis/analysis.hpp"

#include "adl2/interp/eval.hpp"
#include "adl2/interp/event.hpp"
#include "adl2/sema/ext.hpp"
#include "adl2/sema/ops.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <limits>
#include <locale>
#include <map>
#include <optional>
#include <set>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

namespace adl2::analysis {
namespace {

using adl2::interp::Event;
using adl2::interp::EventObject;
using adl2::interp::EvalError;
using adl2::sema::AngKind;
using adl2::sema::ArithOp;
using adl2::sema::BandKind;
using adl2::sema::CmpOp;
using adl2::sema::Collection;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::CombKind;
using adl2::sema::ElemIndex;
using adl2::sema::ElemIndexKind;
using adl2::sema::ExtDecls;
using adl2::sema::Fragment;
using adl2::sema::HNode;
using adl2::sema::Hir;
using adl2::sema::HirRegionStmt;
using adl2::sema::MET_FAMILY_KEY;
using adl2::sema::ParticleKind;
using adl2::sema::ParticleRef;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::Rat;
using adl2::sema::ScalarSourceKind;
using adl2::sema::Symbol;
using adl2::sema::SymbolTable;
using adl2::solver::Model;

/// Finite f64 → shortest-decimal Rat (angular/default fill edge).
Rat rat_f64(double v) {
  auto r = Rat::from_decimal_f64(std::isfinite(v) ? v : 0.0);
  return r ? *r : Rat::zero();
}

struct CollPlan {
  std::vector<std::pair<CollectionId, std::uint64_t>> family;
};

using PropMap = std::map<std::string, Rat>;

void json_escape(std::string& out, const std::string& s) {
  out.push_back('"');
  for (unsigned char c : s) {
    switch (c) {
      case '"':
        out += "\\\"";
        break;
      case '\\':
        out += "\\\\";
        break;
      case '\b':
        out += "\\b";
        break;
      case '\f':
        out += "\\f";
        break;
      case '\n':
        out += "\\n";
        break;
      case '\r':
        out += "\\r";
        break;
      case '\t':
        out += "\\t";
        break;
      default:
        if (c < 0x20) {
          char buf[8];
          std::snprintf(buf, sizeof(buf), "\\u%04x", static_cast<unsigned>(c));
          out += buf;
        } else {
          out.push_back(static_cast<char>(c));
        }
        break;
    }
  }
  out.push_back('"');
}

/// serde_json number text (`50.0`, `0.0`, shortest round-trip), as smash3's
/// `num(v)` writes the diagnostic event. Non-finite → 0.0.
void json_number(std::string& out, double v) {
  if (!std::isfinite(v)) v = 0.0;
  out += adl2::interp::json_f64(v);
}

std::string json_object(const std::map<std::string, std::string>& fields) {
  std::string out = "{";
  bool first = true;
  for (const auto& kv : fields) {
    if (!first) out += ',';
    first = false;
    json_escape(out, kv.first);
    out += ':';
    out += kv.second;
  }
  out += '}';
  return out;
}

std::string json_array(const std::vector<std::string>& items) {
  std::string out = "[";
  for (std::size_t i = 0; i < items.size(); ++i) {
    if (i) out += ',';
    out += items[i];
  }
  out += ']';
  return out;
}

std::string num_json(double v) {
  std::string s;
  json_number(s, v);
  return s;
}

std::string uppercase_first(std::string s) {
  if (!s.empty() && s[0] >= 'a' && s[0] <= 'z') {
    s[0] = static_cast<char>(s[0] - 'a' + 'A');
  }
  return s;
}

std::optional<std::string> backtick(const std::string& s) {
  auto start = s.find('`');
  if (start == std::string::npos) return std::nullopt;
  auto end = s.find('`', start + 1);
  if (end == std::string::npos) return std::nullopt;
  return s.substr(start + 1, end - start - 1);
}

bool starts_with(const std::string& s, const char* pfx) {
  std::size_t n = std::char_traits<char>::length(pfx);
  return s.size() >= n && s.compare(0, n, pfx) == 0;
}

std::string event_to_json(const Event& event) {
  std::map<std::string, std::string> root;
  for (const auto& kv : event.collections) {
    std::vector<std::string> arr;
    arr.reserve(kv.second.size());
    for (const auto& o : kv.second) {
      std::map<std::string, std::string> m;
      for (const auto& p : o.props) m[p.first] = num_json(p.second.to_f64());
      arr.push_back(json_object(m));
    }
    root[uppercase_first(kv.first)] = json_array(arr);
  }
  if (!event.met.empty()) {
    std::map<std::string, std::string> met;
    for (const auto& p : event.met) met[p.first] = num_json(p.second.to_f64());
    root["MET"] = json_object(met);
  }
  for (const auto& p : event.scalars) {
    if (root.find(p.first) == root.end()) root[p.first] = num_json(p.second.to_f64());
  }
  if (!event.triggers.empty()) {
    std::map<std::string, std::string> trig;
    for (const auto& p : event.triggers) trig[p.first] = num_json(p.second.to_f64());
    root["triggers"] = json_object(trig);
  }
  return json_object(root);
}

bool patch_missing_event(Event& event, const std::string& reason) {
  if (starts_with(reason, "event has no scalar ")) {
    auto name = backtick(reason);
    if (!name) return false;
    event.scalars.emplace(*name, Rat::zero());
    return true;
  }
  if (starts_with(reason, "event has no trigger flag ")) {
    auto name = backtick(reason);
    if (!name) return false;
    event.triggers.emplace(*name, Rat::zero());
    return true;
  }
  if (reason == "event has no MET vector" || starts_with(reason, "event MET has no ")) {
    event.met.emplace("pt", Rat::zero());
    event.met.emplace("phi", Rat::zero());
    if (auto c = backtick(reason)) event.met.emplace(*c, Rat::zero());
    return true;
  }
  return false;
}

std::uint32_t depth(const Hir& hir, CollectionId c) {
  const Collection& col = hir.table.collection(c);
  if (col.kind == CollectionKind::Filtered) return depth(hir, col.parent) + 1;
  return 0;
}

std::optional<CollectionId> base_of(const Hir& hir, CollectionId c) {
  const Collection& col = hir.table.collection(c);
  switch (col.kind) {
    case CollectionKind::Base:
      return c;
    case CollectionKind::Filtered:
      return base_of(hir, col.parent);
    case CollectionKind::Sorted:
    case CollectionKind::Slice:
      return base_of(hir, col.parent);
    case CollectionKind::Union:
    case CollectionKind::Combination:
    case CollectionKind::CombProject:
      return std::nullopt;
  }
  return std::nullopt;
}

std::string realizable(const Hir& hir, CollectionId c, std::set<CollectionId>& out) {
  const Collection& col = hir.table.collection(c);
  switch (col.kind) {
    case CollectionKind::Base:
    case CollectionKind::Filtered:
      out.insert(c);
      return {};
    case CollectionKind::Union:
      for (auto p : col.parts) {
        auto e = realizable(hir, p, out);
        if (!e.empty()) return e;
      }
      return {};
    case CollectionKind::Sorted:
    case CollectionKind::Slice:
      return realizable(hir, col.parent, out);
    case CollectionKind::Combination:
      for (auto p : col.parts) {
        auto e = realizable(hir, p, out);
        if (!e.empty()) return e;
      }
      return {};
    case CollectionKind::CombProject:
      return realizable(hir, col.parent, out);
  }
  return {};
}

std::set<CollectionId> disjoint_source_bases(const Hir& hir) {
  std::set<CollectionId> out;
  const auto& colls = hir.table.collections();
  for (std::uint32_t i = 0; i < colls.size(); ++i) {
    const Collection& c = colls[i];
    if (c.kind != CollectionKind::Combination) continue;
    if (c.comb_kind != CombKind::Disjoint) continue;
    if (c.parts.size() < 2) continue;
    bool same = true;
    for (std::size_t k = 1; k < c.parts.size(); ++k) {
      if (!(c.parts[k] == c.parts[0])) {
        same = false;
        break;
      }
    }
    if (!same) continue;
    if (auto b = base_of(hir, c.parts[0])) out.insert(*b);
  }
  return out;
}

std::optional<double> eval_num(const HNode& node, const PropMap& obj, const Model& model,
                               const Hir& hir);
void repair(const HNode& node, PropMap& obj, const std::set<std::string>& pinned, const Hir& hir);

std::optional<bool> eval_pred_opt(const HNode& node, const PropMap& obj, const Model& model,
                                  const Hir& hir) {
  if (!node.tag.in_fragment) return std::nullopt;
  switch (node.kind) {
    case HNode::Kind::Bool:
      return node.bool_val;
    case HNode::Kind::And: {
      bool all = true;
      for (const auto& p : node.items) {
        auto v = eval_pred_opt(p, obj, model, hir);
        if (v && !*v) return false;
        if (!v) all = false;
      }
      if (all) return true;
      return std::nullopt;
    }
    case HNode::Kind::Or: {
      bool any_unknown = false;
      for (const auto& p : node.items) {
        auto v = eval_pred_opt(p, obj, model, hir);
        if (v && *v) return true;
        if (!v) any_unknown = true;
      }
      if (any_unknown) return std::nullopt;
      return false;
    }
    case HNode::Kind::Not:
      if (!node.a) return std::nullopt;
      if (auto v = eval_pred_opt(*node.a, obj, model, hir)) return !*v;
      return std::nullopt;
    case HNode::Kind::Cmp: {
      if (!node.a || !node.b) return std::nullopt;
      auto l = eval_num(*node.a, obj, model, hir);
      auto r = eval_num(*node.b, obj, model, hir);
      if (!l || !r) return std::nullopt;
      switch (node.cmp) {
        case CmpOp::Gt:
          return *l > *r;
        case CmpOp::Lt:
          return *l < *r;
        case CmpOp::Ge:
          return *l >= *r;
        case CmpOp::Le:
          return *l <= *r;
        case CmpOp::Eq:
          return *l == *r;
        case CmpOp::Ne:
        case CmpOp::ApproxEq:
          return *l != *r;
      }
      return std::nullopt;
    }
    case HNode::Kind::Band: {
      if (!node.a) return std::nullopt;
      auto v = eval_num(*node.a, obj, model, hir);
      if (!v) return std::nullopt;
      double lo, hi;
      try {
        lo = std::stod(node.lo);
        hi = std::stod(node.hi);
      } catch (...) {
        return std::nullopt;
      }
      if (node.band == BandKind::In) return lo <= *v && *v <= hi;
      return *v <= lo || *v >= hi;
    }
    default:
      return std::nullopt;
  }
}

std::optional<double> eval_num(const HNode& node, const PropMap& obj, const Model& model,
                               const Hir& hir) {
  if (!node.tag.in_fragment) return std::nullopt;
  switch (node.kind) {
    case HNode::Kind::Num:
      try {
        return std::stod(node.text);
      } catch (...) {
        return std::nullopt;
      }
    case HNode::Kind::ElemSelfProp: {
      auto it = obj.find(hir.table.prop_key(node.prop));
      if (it == obj.end()) return std::nullopt;
      return it->second.to_f64();
    }
    case HNode::Kind::Quantity:
      return model.get_f64(node.qid);
    case HNode::Kind::Neg: {
      if (!node.a) return std::nullopt;
      auto v = eval_num(*node.a, obj, model, hir);
      if (!v) return std::nullopt;
      return -*v;
    }
    case HNode::Kind::Abs: {
      if (!node.a) return std::nullopt;
      auto v = eval_num(*node.a, obj, model, hir);
      if (!v) return std::nullopt;
      return std::fabs(*v);
    }
    case HNode::Kind::Binary: {
      if (!node.a || !node.b) return std::nullopt;
      auto l = eval_num(*node.a, obj, model, hir);
      auto r = eval_num(*node.b, obj, model, hir);
      if (!l || !r) return std::nullopt;
      double v = 0;
      switch (node.arith) {
        case ArithOp::Add:
          v = *l + *r;
          break;
        case ArithOp::Sub:
          v = *l - *r;
          break;
        case ArithOp::Mul:
          v = *l * *r;
          break;
        case ArithOp::Div:
          v = *l / *r;
          break;
        case ArithOp::Pow:
          v = std::pow(*l, *r);
          break;
      }
      return std::isfinite(v) ? std::optional<double>(v) : std::nullopt;
    }
    default:
      return std::nullopt;
  }
}

void repair_prop(PropMap& obj, const std::set<std::string>& pinned, const std::string& key,
                 CmpOp op, double k) {
  if (pinned.count(key)) return;
  auto it = obj.find(key);
  bool satisfied = false;
  if (it != obj.end()) {
    double v = it->second.to_f64();
    switch (op) {
      case CmpOp::Gt:
        satisfied = v > k;
        break;
      case CmpOp::Lt:
        satisfied = v < k;
        break;
      case CmpOp::Ge:
        satisfied = v >= k;
        break;
      case CmpOp::Le:
        satisfied = v <= k;
        break;
      case CmpOp::Eq:
        satisfied = v == k;
        break;
      case CmpOp::Ne:
      case CmpOp::ApproxEq:
        satisfied = v != k;
        break;
    }
  }
  if (satisfied) return;
  double v = k;
  switch (op) {
    case CmpOp::Gt:
      v = k + 1.0;
      break;
    case CmpOp::Ge:
    case CmpOp::Eq:
      v = k;
      break;
    case CmpOp::Lt:
      v = k - 1.0;
      break;
    case CmpOp::Le:
      v = k;
      break;
    case CmpOp::Ne:
    case CmpOp::ApproxEq:
      v = k + 1.0;
      break;
  }
  obj[key] = rat_f64(v);
}

void repair(const HNode& node, PropMap& obj, const std::set<std::string>& pinned, const Hir& hir) {
  switch (node.kind) {
    case HNode::Kind::And:
      for (const auto& p : node.items) repair(p, obj, pinned, hir);
      break;
    case HNode::Kind::Cmp: {
      if (!node.a || !node.b) return;
      const HNode* lhs = node.a.get();
      const HNode* rhs = node.b.get();
      if (lhs->kind == HNode::Kind::ElemSelfProp && rhs->kind == HNode::Kind::Num) {
        double k;
        try {
          k = std::stod(rhs->text);
        } catch (...) {
          return;
        }
        repair_prop(obj, pinned, hir.table.prop_key(lhs->prop), node.cmp, k);
      } else if (lhs->kind == HNode::Kind::Num && rhs->kind == HNode::Kind::ElemSelfProp) {
        double k;
        try {
          k = std::stod(lhs->text);
        } catch (...) {
          return;
        }
        repair_prop(obj, pinned, hir.table.prop_key(rhs->prop), adl2::sema::cmp_op_flipped(node.cmp),
                    k);
      }
      break;
    }
    case HNode::Kind::Band: {
      if (!node.a || node.a->kind != HNode::Kind::ElemSelfProp) return;
      if (node.band != BandKind::In) return;
      double lo, hi;
      try {
        lo = std::stod(node.lo);
        hi = std::stod(node.hi);
      } catch (...) {
        return;
      }
      std::string key = hir.table.prop_key(node.a->prop);
      if (!pinned.count(key)) obj[key] = rat_f64((lo + hi) / 2.0);
      break;
    }
    default:
      break;
  }
}

enum class LocKind { Met, Obj };
struct Loc {
  LocKind kind = LocKind::Met;
  CollectionId base;
  std::size_t index = 0;
  static Loc met() { return Loc{}; }
  static Loc obj(CollectionId b, std::size_t i) {
    Loc l;
    l.kind = LocKind::Obj;
    l.base = b;
    l.index = i;
    return l;
  }
};

constexpr double kPi = 3.14159265358979323846;

double eta_for_exact_gap(double other, double v) {
  auto fixpoint = [&](double start) {
    double x = start;
    for (int n = 0; n < 4; ++n) {
      double got = std::fabs(x - other);
      if (got == v || !std::isfinite(v - got)) break;
      if (x >= other)
        x += (v - got);
      else
        x -= (v - got);
    }
    return x;
  };
  double plus = fixpoint(other + v);
  if (std::fabs(plus - other) == v) return plus;
  double minus = fixpoint(other - v);
  if (std::fabs(minus - other) == v) return minus;
  return plus;
}

void realize_dr(double v, const Loc& la, const Loc& lb, const std::string& eta_key,
                const std::string& phi_key, std::map<CollectionId, std::vector<PropMap>>& built) {
  if (!std::isfinite(v) || v < 0.0) return;
  if (la.kind != LocKind::Obj || lb.kind != LocKind::Obj) return;

  // `present=false`: element absent. `present && !set`: key unset (free).
  struct Slot {
    bool present = false;
    bool set = false;
    double v = 0;
  };
  auto get = [&](CollectionId base, std::size_t i, const std::string& key) -> Slot {
    Slot s;
    auto it = built.find(base);
    if (it == built.end() || i >= it->second.size()) return s;
    s.present = true;
    auto pit = it->second[i].find(key);
    if (pit == it->second[i].end()) return s;
    s.set = true;
    s.v = pit->second.to_f64();
    return s;
  };
  auto set = [&](CollectionId base, std::size_t i, const std::string& key, double val) {
    auto it = built.find(base);
    if (it == built.end() || i >= it->second.size()) return;
    it->second[i][key] = rat_f64(val);
  };

  auto eta_a = get(la.base, la.index, eta_key);
  auto eta_b = get(lb.base, lb.index, eta_key);
  auto phi_a = get(la.base, la.index, phi_key);
  auto phi_b = get(lb.base, lb.index, phi_key);
  if (!eta_a.present || !eta_b.present || !phi_a.present || !phi_b.present) return;

  if (eta_a.set && eta_b.set) {
    double ea = eta_a.v;
    double eb = eta_b.v;
    double de = ea - eb;
    double rem2 = v * v - de * de;
    if (rem2 < 0.0) return;
    double dphi = std::sqrt(rem2);
    if (!std::isfinite(dphi) || dphi > kPi) return;
    CollectionId free_base;
    std::size_t free_i = 0;
    double pinned = 0;
    bool pinned_is_a = false;
    if (phi_a.set && phi_b.set) return;
    if (phi_a.set && !phi_b.set) {
      free_base = lb.base;
      free_i = lb.index;
      pinned = phi_a.v;
      pinned_is_a = true;
    } else if (!phi_a.set && phi_b.set) {
      free_base = la.base;
      free_i = la.index;
      pinned = phi_b.v;
      pinned_is_a = false;
    } else {
      set(lb.base, lb.index, phi_key, 0.0);
      free_base = la.base;
      free_i = la.index;
      pinned = 0.0;
      pinned_is_a = false;
    }
    double val = pinned_is_a ? pinned - dphi : pinned + dphi;
    for (int n = 0; n < 4; ++n) {
      double d = pinned_is_a ? pinned - val : val - pinned;
      double got = std::hypot(de, adl2::interp::wrap_dphi(d));
      if (got == v || !std::isfinite(v - got) || d < 0.0 || d > kPi) break;
      val = pinned_is_a ? val - (v - got) : val + (v - got);
    }
    set(free_base, free_i, phi_key, val);
    return;
  }

  double phi_target = 0.0;
  if (phi_a.set && phi_b.set) {
    if (phi_a.v != phi_b.v) return;
    phi_target = phi_a.v;
  } else if (phi_a.set) {
    phi_target = phi_a.v;
  } else if (phi_b.set) {
    phi_target = phi_b.v;
  }

  if (eta_a.set && eta_b.set) {
    // handled above
  } else if (!eta_a.set && eta_b.set) {
    set(la.base, la.index, eta_key, eta_for_exact_gap(eta_b.v, v));
  } else if (eta_a.set && !eta_b.set) {
    set(lb.base, lb.index, eta_key, eta_for_exact_gap(eta_a.v, v));
  } else {
    set(lb.base, lb.index, eta_key, 0.0);
    set(la.base, la.index, eta_key, v);
  }
  if (!phi_a.set) set(la.base, la.index, phi_key, phi_target);
  if (!phi_b.set) set(lb.base, lb.index, phi_key, phi_target);
}

void realize_angulars(const Hir& hir, const ExtDecls& ext, const Model& model,
                      const std::set<QuantityId>& mentioned,
                      std::map<CollectionId, std::vector<PropMap>>& built,
                      std::map<std::string, Rat>& met) {
  std::string phi_key = ext.prop_canon("phi").first;
  std::string eta_key = ext.prop_canon("eta").first;

  auto loc_of = [&](const ParticleRef& p) -> std::optional<Loc> {
    if (p.kind == ParticleKind::Met) return Loc::met();
    if (p.kind == ParticleKind::Elem && p.index.kind == ElemIndexKind::FromFront) {
      auto b = base_of(hir, p.coll);
      if (!b) return std::nullopt;
      return Loc::obj(*b, static_cast<std::size_t>(p.index.n));
    }
    return std::nullopt;
  };

  for (const auto& kv : model.values()) {
    QuantityId q = kv.first;
    if (!mentioned.count(q)) continue;
    const Quantity& qq = hir.table.quantity(q);
    if (qq.kind != QuantityKind::AngularSep) continue;
    auto la = loc_of(qq.a);
    auto lb = loc_of(qq.b);
    if (!la || !lb) continue;
    if (qq.ang == AngKind::DR) {
      realize_dr(kv.second.to_f64(), *la, *lb, eta_key, phi_key, built);
      continue;
    }
    const std::string& key = qq.ang == AngKind::DPhi ? phi_key : eta_key;
    if (qq.ang == AngKind::DEta && (la->kind == LocKind::Met || lb->kind == LocKind::Met)) {
      continue;
    }
    struct Slot {
      bool present = false;
      bool set = false;
      double v = 0;
    };
    auto read = [&](const Loc& loc) -> Slot {
      Slot s;
      if (loc.kind == LocKind::Met) {
        s.present = true;
        auto it = met.find(key);
        if (it == met.end()) return s;
        s.set = true;
        s.v = it->second.to_f64();
        return s;
      }
      auto it = built.find(loc.base);
      if (it == built.end() || loc.index >= it->second.size()) return s;
      s.present = true;
      auto pit = it->second[loc.index].find(key);
      if (pit == it->second[loc.index].end()) return s;
      s.set = true;
      s.v = pit->second.to_f64();
      return s;
    };
    auto write = [&](const Loc& loc, double value) {
      if (loc.kind == LocKind::Met) {
        met[key] = rat_f64(value);
        return;
      }
      auto it = built.find(loc.base);
      if (it == built.end() || loc.index >= it->second.size()) return;
      it->second[loc.index][key] = rat_f64(value);
    };
    auto cur_a = read(*la);
    auto cur_b = read(*lb);
    if (!cur_a.present || !cur_b.present) continue;
    auto realized = [&](double x, double other, bool flip) {
      double d = flip ? other - x : x - other;
      return qq.ang == AngKind::DPhi ? adl2::interp::wrap_dphi(d) : d;
    };
    double target = kv.second.to_f64();
    auto correct = [&](double x, double other, bool flip) {
      for (int n = 0; n < 4; ++n) {
        double got = realized(x, other, flip);
        if (got == target || !std::isfinite(target - got)) break;
        x += flip ? got - target : target - got;
      }
      return x;
    };
    if (cur_a.set && cur_b.set) {
      continue;
    } else if (!cur_a.set && cur_b.set) {
      write(*la, correct(target + cur_b.v, cur_b.v, false));
    } else if (cur_a.set && !cur_b.set) {
      write(*lb, correct(cur_a.v - target, cur_a.v, true));
    } else {
      write(*lb, 0.0);
      write(*la, correct(target, 0.0, false));
    }
  }
}

/// Which statements of region `idx` the realized event fails (smash3
/// `failing_stmts`): every membership statement is evaluated on its own,
/// two-valued, so the reader sees each culprit rather than the first.
std::string failing_stmts(const Hir& hir, const adl2::interp::Interp& interp, std::size_t idx,
                          const Event& event) {
  if (idx >= hir.regions.size()) return "region not found";
  using SK = adl2::sema::HirRegionStmt::Kind;
  std::vector<std::string> out;
  const auto& stmts = hir.regions[idx].stmts;
  for (std::size_t i = 0; i < stmts.size(); ++i) {
    const auto& stmt = stmts[i];
    if (stmt.kind != SK::Select && stmt.kind != SK::Trigger && stmt.kind != SK::Reject) continue;
    EvalError err;
    auto v = interp.eval_bool(stmt.node, event, err);
    if (!v) {
      out.push_back("stmt " + std::to_string(i) + " errors: " + err.reason);
      continue;
    }
    bool pass = stmt.kind == SK::Reject ? !*v : *v;
    if (!pass) out.push_back("stmt " + std::to_string(i) + " fails");
  }
  if (out.empty()) return "no single failing statement (inheritance?)";
  std::string joined;
  for (std::size_t i = 0; i < out.size(); ++i) {
    if (i) joined += "; ";
    joined += out[i];
  }
  return joined;
}

struct BuildOk {
  Event event;
  std::string json;
};

std::optional<BuildOk> build_event(const Hir& hir, const ExtDecls& ext, const Model& model,
                                   const std::set<QuantityId>& mentioned, std::string& err) {
  Symbol met_sym{};
  bool has_met = hir.symbols.lookup(MET_FAMILY_KEY, met_sym);
  auto is_met_base = [&](CollectionId c) -> bool {
    if (!has_met) return false;
    const Collection& col = hir.table.collection(c);
    return col.kind == CollectionKind::Base && col.base == met_sym;
  };

  std::set<CollectionId> needed;
  std::map<CollectionId, std::uint64_t> sizes;
  std::map<std::pair<CollectionId, std::uint32_t>, std::vector<std::pair<std::string, Rat>>>
      elem_pins;
  std::map<std::pair<CollectionId, std::uint32_t>, std::set<std::string>> elem_absent;

  for (const auto& kv : model.values()) {
    const Quantity& q = hir.table.quantity(kv.first);
    if (q.kind != QuantityKind::Present) continue;
    if (!mentioned.count(kv.first) || kv.second >= Rat::one()) continue;
    const Quantity& inner = hir.table.quantity(q.inner);
    if (inner.kind == QuantityKind::ElemProp && inner.index.kind == ElemIndexKind::FromFront) {
      elem_absent[{inner.coll, inner.index.n}].insert(hir.table.prop_key(inner.prop));
    }
  }

  std::set<CollectionId> size_pinned;
  for (const auto& kv : model.values()) {
    const Quantity& q = hir.table.quantity(kv.first);
    if (q.kind != QuantityKind::Size) continue;
    if (is_met_base(q.coll)) continue;
    auto e = realizable(hir, q.coll, needed);
    if (!e.empty()) {
      err = e;
      return std::nullopt;
    }
    double n = std::max(0.0, std::round(kv.second.to_f64()));
    if (n > static_cast<double>(MAX_REALIZED)) {
      err = "collection size " + std::to_string(n) + " exceeds the realizer cap";
      return std::nullopt;
    }
    auto& slot = sizes[q.coll];
    slot = std::max(slot, static_cast<std::uint64_t>(n));
    size_pinned.insert(q.coll);
  }

  for (const auto& kv : model.values()) {
    const Quantity& q = hir.table.quantity(kv.first);
    if (q.kind == QuantityKind::ElemProp) {
      if (q.index.kind != ElemIndexKind::FromFront) continue;
      if (is_met_base(q.coll)) continue;
      auto e = realizable(hir, q.coll, needed);
      if (!e.empty()) {
        err = e;
        return std::nullopt;
      }
      if (mentioned.count(kv.first) && !size_pinned.count(q.coll)) {
        auto& slot = sizes[q.coll];
        slot = std::max(slot, static_cast<std::uint64_t>(q.index.n) + 1);
      }
      elem_pins[{q.coll, q.index.n}].emplace_back(hir.table.prop_key(q.prop), kv.second);
    } else if (q.kind == QuantityKind::AngularSep) {
      const ParticleRef* ps[2] = {&q.a, &q.b};
      for (const ParticleRef* p : ps) {
        if (p->kind != ParticleKind::Elem || is_met_base(p->coll)) continue;
        auto e = realizable(hir, p->coll, needed);
        if (!e.empty()) {
          err = e;
          return std::nullopt;
        }
        if (mentioned.count(kv.first) && !size_pinned.count(p->coll) &&
            p->index.kind == ElemIndexKind::FromFront) {
          auto& slot = sizes[p->coll];
          slot = std::max(slot, static_cast<std::uint64_t>(p->index.n) + 1);
        }
      }
    }
  }

  std::map<CollectionId, std::vector<CollectionId>> families;
  for (auto c : needed) {
    auto b = base_of(hir, c);
    if (!b) continue;
    if (is_met_base(*b)) continue;
    families[*b].push_back(c);
  }
  std::map<CollectionId, CollPlan> plans;
  for (auto& kv : families) {
    auto& members = kv.second;
    std::sort(members.begin(), members.end(), [&](CollectionId a, CollectionId b) {
      auto da = depth(hir, a);
      auto db = depth(hir, b);
      if (da != db) return da < db;
      return a < b;
    });
    members.erase(std::unique(members.begin(), members.end()), members.end());
    std::uint64_t n_base = 0;
    for (auto c : members) {
      auto it = sizes.find(c);
      if (it != sizes.end()) n_base = std::max(n_base, it->second);
    }
    CollPlan plan;
    for (auto c : members) plan.family.emplace_back(c, n_base);
    plans.emplace(kv.first, std::move(plan));
  }

  std::string pt_key = ext.prop_canon("pt").first;
  std::map<CollectionId, std::vector<PropMap>> built;
  std::map<CollectionId, std::vector<std::set<std::string>>> absent_by_base;

  for (const auto& pk : plans) {
    CollectionId base = pk.first;
    const CollPlan& plan = pk.second;
    std::uint64_t n = plan.family.empty() ? 0 : plan.family.front().second;
    std::vector<PropMap> objs(static_cast<std::size_t>(n));
    std::vector<std::set<std::string>> pinned(static_cast<std::size_t>(n));
    std::vector<std::set<std::string>> omitted(static_cast<std::size_t>(n));

    for (const auto& fn : plan.family) {
      CollectionId c = fn.first;
      std::uint64_t n_c = fn.second;
      for (std::uint64_t j = 0; j < n_c && j < n; ++j) {
        if (j > std::numeric_limits<std::uint32_t>::max()) continue;
        auto it = elem_absent.find({c, static_cast<std::uint32_t>(j)});
        if (it != elem_absent.end()) {
          omitted[static_cast<std::size_t>(j)].insert(it->second.begin(), it->second.end());
        }
      }
    }

    for (const auto& fn : plan.family) {
      CollectionId c = fn.first;
      std::uint64_t n_c = fn.second;
      for (std::uint64_t j = 0; j < n_c; ++j) {
        if (j > std::numeric_limits<std::uint32_t>::max()) continue;
        auto it = elem_pins.find({c, static_cast<std::uint32_t>(j)});
        if (it == elem_pins.end()) continue;
        auto ji = static_cast<std::size_t>(j);
        for (const auto& pin : it->second) {
          if (omitted[ji].count(pin.first)) continue;
          objs[ji][pin.first] = pin.second;
          pinned[ji].insert(pin.first);
        }
      }
    }
    for (std::size_t j = 0; j < omitted.size(); ++j) {
      pinned[j].insert(omitted[j].begin(), omitted[j].end());
    }

    for (const auto& fn : plan.family) {
      const Collection& col = hir.table.collection(fn.first);
      if (col.kind != CollectionKind::Filtered) continue;
      const HNode& pred_node = hir.elem_pred(col.pred).node;
      for (std::uint64_t j = 0; j < fn.second; ++j) {
        auto ji = static_cast<std::size_t>(j);
        if (eval_pred_opt(pred_node, objs[ji], model, hir) != true) {
          repair(pred_node, objs[ji], pinned[ji], hir);
        }
      }
    }

    {
      std::optional<Rat> last;
      std::optional<Rat> first_set;
      for (const auto& o : objs) {
        auto it = o.find(pt_key);
        if (it != o.end()) {
          first_set = it->second;
          break;
        }
      }
      for (auto& o : objs) {
        auto it = o.find(pt_key);
        if (it != o.end()) {
          last = it->second;
        } else {
          Rat v = last ? *last : (first_set ? *first_set : Rat::from_i64(50));
          o[pt_key] = v;
          last = v;
        }
      }
    }
    built[base] = std::move(objs);
    absent_by_base[base] = std::move(omitted);
  }

  std::map<std::string, Rat> met_rats;
  std::vector<std::pair<std::string, Rat>> scalars;
  std::map<std::string, Rat> triggers_rats;
  Rat half = *Rat::from_decimal_f64(0.5);
  for (const auto& kv : model.values()) {
    const Quantity& q = hir.table.quantity(kv.first);
    if (q.kind != QuantityKind::EventScalar) continue;
    switch (q.scalar.kind) {
      case ScalarSourceKind::MetProp:
        met_rats[hir.table.prop_key(q.scalar.prop)] = kv.second;
        break;
      case ScalarSourceKind::EventVar:
        scalars.emplace_back(hir.symbols.key(q.scalar.name), kv.second);
        break;
      case ScalarSourceKind::Trigger: {
        Rat flag = kv.second >= half ? Rat::one() : Rat::zero();
        triggers_rats[hir.symbols.key(q.scalar.name)] = flag;
        break;
      }
    }
  }

  realize_angulars(hir, ext, model, mentioned, built, met_rats);

  auto disjoint_bases = disjoint_source_bases(hir);
  {
    std::string eta_key = ext.prop_canon("eta").first;
    std::string phi_key = ext.prop_canon("phi").first;
    std::string m_key = ext.prop_canon("m").first;
    Rat zero = Rat::zero();
    std::pair<std::string, Rat> const_defaults[] = {
        {eta_key, zero}, {phi_key, zero}, {m_key, zero},
        {"btag", zero},  {"ctag", zero},  {"tautag", zero},
    };
    for (auto& kv : built) {
      bool distinct_eta = disjoint_bases.count(kv.first) != 0;
      for (std::size_t i = 0; i < kv.second.size(); ++i) {
        auto& o = kv.second[i];
        if (distinct_eta) {
          if (!o.count(eta_key)) o[eta_key] = rat_f64(0.1 * static_cast<double>(i));
        }
        for (const auto& d : const_defaults) {
          if (!o.count(d.first)) o[d.first] = d.second;
        }
      }
    }
  }

  for (auto& kv : built) {
    auto it = absent_by_base.find(kv.first);
    if (it == absent_by_base.end()) continue;
    std::size_t n = std::min(kv.second.size(), it->second.size());
    for (std::size_t i = 0; i < n; ++i) {
      for (const auto& k : it->second[i]) kv.second[i].erase(k);
    }
  }

  {
    std::string pt = ext.prop_canon("pt").first;
    for (auto& kv : built) {
      std::stable_sort(kv.second.begin(), kv.second.end(),
                       [&](const PropMap& a, const PropMap& b) {
                         auto ia = a.find(pt);
                         auto ib = b.find(pt);
                         bool ha = ia != a.end();
                         bool hb = ib != b.end();
                         if (ha && hb) return ib->second < ia->second;
                         if (ha && !hb) return true;
                         return false;
                       });
    }
  }

  // Event (exact Rat) + diagnostic JSON keyed by the collection's display
  // spelling (smash3 phase 3; `event_to_json` re-derives keys from the
  // lowercase event map and is used only after a missing-data patch).
  Event event;
  std::map<std::string, std::string> root;
  for (auto& kv : built) {
    const Collection& col = hir.table.collection(kv.first);
    if (col.kind != CollectionKind::Base) continue;
    std::string display = hir.symbols.display(col.base);
    std::string key = SymbolTable::ascii_lower(display);
    std::vector<EventObject> arr;
    std::vector<std::string> arr_json;
    arr.reserve(kv.second.size());
    arr_json.reserve(kv.second.size());
    for (auto& o : kv.second) {
      std::map<std::string, std::string> m;
      for (const auto& p : o) m[p.first] = num_json(p.second.to_f64());
      arr_json.push_back(json_object(m));
      EventObject eo;
      eo.props = std::move(o);
      arr.push_back(std::move(eo));
    }
    root[display] = json_array(arr_json);
    event.collections[key] = std::move(arr);
  }
  if (!met_rats.empty()) {
    std::map<std::string, std::string> met;
    for (const auto& p : met_rats) met[p.first] = num_json(p.second.to_f64());
    root["MET"] = json_object(met);
    event.met = std::move(met_rats);
  }
  for (auto& p : scalars) {
    if (root.find(p.first) == root.end()) root[p.first] = num_json(p.second.to_f64());
    event.scalars.emplace(p.first, p.second);
  }
  if (!triggers_rats.empty()) {
    std::map<std::string, std::string> trig;
    for (const auto& p : triggers_rats) trig[p.first] = num_json(p.second.to_f64());
    root["triggers"] = json_object(trig);
    event.triggers = std::move(triggers_rats);
  }

  BuildOk ok;
  ok.event = std::move(event);
  ok.json = json_object(root);
  return ok;
}

}  // namespace

Validation validate_witness(const adl2::sema::Hir& hir, const adl2::sema::ExtDecls& ext,
                            const adl2::interp::Interp& interp,
                            const adl2::solver::Model& model,
                            const std::set<adl2::sema::QuantityId>& mentioned,
                            std::size_t region_a, std::size_t region_b) {
  std::string why;
  auto built = build_event(hir, ext, model, mentioned, why);
  if (!built) {
    return Validation::rejected("witness realization failed: " + why);
  }
  Event event = std::move(built->event);
  std::string json = std::move(built->json);

  for (int round = 0; round < 8; ++round) {
    std::optional<std::string> opaque;
    bool missing = false;
    std::size_t idxs[2] = {region_a, region_b};
    adl2::interp::Interp::EventEval ev(interp, event);
    for (std::size_t k = 0; k < 2; ++k) {
      std::size_t idx = idxs[k];
      std::string name = idx < hir.regions.size()
                             ? hir.symbols.display(hir.regions[idx].name)
                             : ("#" + std::to_string(idx));
      EvalError err;
      auto mem = ev.region_membership(idx, err);
      if (mem && *mem) {
        continue;
      }
      if (mem && !*mem) {
        return Validation::rejected("interpreter rejects the witness event in region " + name +
                                    " (" + failing_stmts(hir, interp, idx, event) +
                                    "); event: " + json);
      }
      if (err.reason.find("no reference interpretation") != std::string::npos) {
        opaque = "region " + name + " depends on an opaque quantity (" + err.reason + ")";
        continue;
      }
      if (patch_missing_event(event, err.reason)) {
        json = event_to_json(event);
        missing = true;
        break;
      }
      return Validation::rejected("interpreter cannot evaluate region " + name +
                                  " on the witness: " + err.reason);
    }
    if (missing) continue;
    if (opaque) return Validation::candidate(*opaque);
    return Validation::validated(json);
  }
  return Validation::rejected("witness event patching did not converge");
}

}  // namespace adl2::analysis
