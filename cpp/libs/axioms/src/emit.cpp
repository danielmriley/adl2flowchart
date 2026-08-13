#include "adl2/axioms/axioms.hpp"

#include "elem_pred.hpp"

#include "adl2/formula/dump.hpp"
#include "adl2/formula/lin.hpp"

#include <algorithm>
#include <map>
#include <set>
#include <sstream>
#include <utility>

namespace adl2::axioms {
namespace {

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::sema::AngKind;
using adl2::sema::Collection;
using adl2::sema::CollectionId;
using adl2::sema::CollectionKind;
using adl2::sema::CombKind;
using adl2::sema::ElemIndex;
using adl2::sema::ElemIndexKind;
using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::ParticleKind;
using adl2::sema::ParticleRef;
using adl2::sema::PropId;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::Rat;
using adl2::sema::ScalarSourceKind;

QFormula qatom(const std::vector<std::pair<double, QuantityId>>& terms, Rel rel, double k) {
  auto r = [](double v) {
    return *Rat::from_decimal_f64(v);
  };
  std::vector<LinAtom::Term> ts;
  for (auto& t : terms) ts.emplace_back(r(t.first), t.second);
  return QFormula::of_atom(LinAtom::make(std::move(ts), rel, r(k)));
}

QFormula qand(std::vector<QFormula> v) {
  if (v.empty()) return QFormula::ttrue();
  if (v.size() == 1) return std::move(v[0]);
  return QFormula::of_and(std::move(v));
}
QFormula qor(std::vector<QFormula> v) {
  if (v.empty()) return QFormula::ffalse();
  if (v.size() == 1) return std::move(v[0]);
  return QFormula::of_or(std::move(v));
}

void collect_qs(const QFormula& f, std::set<QuantityId>& out) {
  switch (f.kind) {
    case QFormula::Kind::Atom:
      for (const auto& t : f.atom.terms()) out.insert(t.second);
      break;
    case QFormula::Kind::And:
    case QFormula::Kind::Or:
      for (const auto& x : f.items) collect_qs(x, out);
      break;
    default:
      break;
  }
}

std::string dump_qf(const QFormula& f) { return adl2::formula::dump_qformula(f); }

struct Emit {
  Hir* hir;
  const ExtDecls* ext;
  std::string pt_key;
  std::vector<std::string> nneg_prop_keys;
  std::vector<AxiomInstance> out;

  void push(AxiomId id, QFormula f, std::string d) {
    AxiomInstance inst;
    inst.id = id;
    inst.formula = std::move(f);
    inst.description = std::move(d);
    out.push_back(std::move(inst));
  }

  std::string label(QuantityId q) { return quantity_label(*hir, q); }

  bool pt_ordered(CollectionId c) { return hir->table.pt_ordered(c, pt_key); }

  std::pair<QFormula, std::string> guarded_fact(const std::vector<QuantityId>& qs,
                                                QFormula fact) {
    std::map<CollectionId, std::uint32_t> floors;
    std::vector<QuantityId> absent;
    for (auto q : qs) {
      hir->table.existence_floor(q, floors);
      if (hir->table.may_be_absent(q) &&
          std::find(absent.begin(), absent.end(), q) == absent.end())
        absent.push_back(q);
    }
    if (floors.empty() && absent.empty()) return {std::move(fact), ""};
    std::vector<QFormula> parts;
    std::string prefix;
    for (const auto& kv : floors) {
      auto sq = hir->table.intern_quantity(Quantity::size(kv.first));
      parts.push_back(qatom({{1.0, sq}}, Rel::Le, static_cast<double>(kv.second)));
      prefix += "size(" + collection_label(*hir, kv.first) + ") > " +
                std::to_string(kv.second) + " ∧ ";
    }
    for (auto q : absent) {
      auto p = hir->table.intern_quantity(Quantity::present(q));
      parts.push_back(qatom({{1.0, p}}, Rel::Lt, 1.0));
      prefix += "defined(" + label(q) + ") ∧ ";
    }
    if (prefix.size() >= 4) prefix.resize(prefix.size() - 4);  // " ∧ "
    prefix += "⇒ ";
    parts.push_back(std::move(fact));
    return {qor(std::move(parts)), prefix};
  }

  std::pair<QFormula, std::string> guarded(
      const std::vector<std::pair<double, QuantityId>>& terms, Rel rel, double k) {
    std::vector<QuantityId> qs;
    for (auto& t : terms) qs.push_back(t.second);
    return guarded_fact(qs, qatom(terms, rel, k));
  }

  std::map<CollectionId, std::vector<std::pair<std::uint32_t, QuantityId>>> elem_pt(
      const std::vector<QuantityId>& qs, bool back) {
    std::map<CollectionId, std::vector<std::pair<std::uint32_t, QuantityId>>> by;
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::ElemProp) continue;
      if (hir->table.prop_key(qq.prop) != pt_key) continue;
      if (!pt_ordered(qq.coll)) continue;
      bool is_back = qq.index.kind == ElemIndexKind::FromBack;
      if (is_back != back) continue;
      by[qq.coll].push_back({qq.index.n, q});
    }
    for (auto& kv : by) {
      std::sort(kv.second.begin(), kv.second.end());
      kv.second.erase(std::unique(kv.second.begin(), kv.second.end()), kv.second.end());
    }
    return by;
  }

  void ord(const std::vector<QuantityId>& qs) {
    auto front = elem_pt(qs, false);
    for (auto& kv : front) {
      auto& idx = kv.second;
      for (std::size_t a = 0; a < idx.size(); ++a) {
        for (std::size_t b = a + 1; b < idx.size(); ++b) {
          if (idx[a].first < idx[b].first) {
            auto [f, g] = guarded({{1.0, idx[a].second}, {-1.0, idx[b].second}}, Rel::Ge, 0);
            push(AxiomId::Ord, std::move(f),
                 g + label(idx[a].second) + " >= " + label(idx[b].second));
          }
        }
      }
    }
    auto back = elem_pt(qs, true);
    for (auto& kv : back) {
      auto& idx = kv.second;
      for (std::size_t a = 0; a < idx.size(); ++a) {
        for (std::size_t b = a + 1; b < idx.size(); ++b) {
          if (idx[a].first < idx[b].first) {
            auto [f, g] = guarded({{1.0, idx[b].second}, {-1.0, idx[a].second}}, Rel::Ge, 0);
            push(AxiomId::Ord, std::move(f),
                 g + label(idx[b].second) + " >= " + label(idx[a].second));
          }
        }
      }
    }
    for (auto& fk : front) {
      auto it = back.find(fk.first);
      if (it == back.end()) continue;
      for (auto& fi : fk.second) {
        for (auto& bi : it->second) {
          if (fi.first == 0 || bi.first == 1) {
            auto [f, g] = guarded({{1.0, fi.second}, {-1.0, bi.second}}, Rel::Ge, 0);
            push(AxiomId::Ord, std::move(f),
                 g + label(fi.second) + " >= " + label(bi.second));
          }
        }
      }
    }
  }

  void pres(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      if (hir->table.quantity(q).kind != QuantityKind::Present) continue;
      auto f = qand({qatom({{1.0, q}}, Rel::Ge, 0), qatom({{1.0, q}}, Rel::Le, 1)});
      push(AxiomId::Pres, std::move(f), "0 <= " + label(q) + " <= 1");
    }
  }

  void pdef(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::Present) continue;
      auto inner = qq.inner;
      std::map<CollectionId, std::uint32_t> floors;
      hir->table.existence_floor(inner, floors);
      AngKind k;
      CollectionId a, b;
      if (hir->table.whole_pair_legs(inner, k, a, b)) {
        floors.emplace(a, 0);
        floors.emplace(b, 0);
      }
      if (floors.empty()) continue;
      std::vector<QFormula> exists;
      std::string suffix;
      for (const auto& kv : floors) {
        auto sq = hir->table.intern_quantity(Quantity::size(kv.first));
        exists.push_back(qatom({{1.0, sq}}, Rel::Gt, static_cast<double>(kv.second)));
        suffix += "size(" + collection_label(*hir, kv.first) + ") > " +
                  std::to_string(kv.second) + " ∧ ";
      }
      if (suffix.size() >= 4) suffix.resize(suffix.size() - 4);
      auto all = exists.size() == 1 ? std::move(exists[0]) : qand(std::move(exists));
      auto f = qor({qatom({{1.0, q}}, Rel::Lt, 1), std::move(all)});
      push(AxiomId::Pdef, std::move(f), "defined(" + label(inner) + ") ⇒ " + suffix);
    }
  }

  void sz0(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      if (hir->table.quantity(q).kind != QuantityKind::Size) continue;
      push(AxiomId::Sz0, qatom({{1.0, q}}, Rel::Ge, 0), label(q) + " >= 0");
    }
  }

  void sub(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::Size) continue;
      const auto& c = hir->table.collection(qq.coll);
      if (c.kind != CollectionKind::Filtered) continue;
      auto qp = hir->table.intern_quantity(Quantity::size(c.parent));
      auto f = qatom({{1.0, q}, {-1.0, qp}}, Rel::Le, 0);
      push(AxiomId::Sub, std::move(f), label(q) + " <= " + label(qp));
    }
  }

  void uni(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::Size) continue;
      const auto& c = hir->table.collection(qq.coll);
      if (c.kind != CollectionKind::Union) continue;
      std::vector<QuantityId> parts;
      for (auto p : c.parts) parts.push_back(hir->table.intern_quantity(Quantity::size(p)));
      for (auto p : parts) {
        auto f = qatom({{1.0, q}, {-1.0, p}}, Rel::Ge, 0);
        push(AxiomId::Uni, std::move(f), label(q) + " >= " + label(p));
      }
      std::vector<LinAtom::Term> ts;
      ts.emplace_back(Rat::one(), q);
      for (auto p : parts) ts.emplace_back(Rat::from_i64(-1), p);
      auto f = QFormula::of_atom(LinAtom::make(std::move(ts), Rel::Le, Rat::zero()));
      std::string sum;
      for (std::size_t i = 0; i < parts.size(); ++i) {
        if (i) sum += " + ";
        sum += label(parts[i]);
      }
      push(AxiomId::Uni, std::move(f), label(q) + " <= " + sum);
    }
  }

  void nneg(const std::vector<QuantityId>& qs) {
    static const char* extfn[] = {"pt", "m", "mass", "e", "energy", "dr", "sqrt"};
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      bool nonneg = false;
      if (qq.kind == QuantityKind::ElemProp) {
        auto key = hir->table.prop_key(qq.prop);
        for (const auto& k : nneg_prop_keys) {
          if (k == key) nonneg = true;
        }
      } else if (qq.kind == QuantityKind::EventScalar &&
                 qq.scalar.kind == ScalarSourceKind::MetProp) {
        nonneg = hir->table.prop_key(qq.scalar.prop) == ext->prop_canon("pt").first;
      } else if (qq.kind == QuantityKind::EventScalar &&
                 qq.scalar.kind == ScalarSourceKind::EventVar) {
        nonneg = ext->is_event_scalar(hir->symbols.key(qq.scalar.name));
      } else if (qq.kind == QuantityKind::AngularSep && qq.ang == AngKind::DR) {
        nonneg = true;
      } else if (qq.kind == QuantityKind::ExternalFn) {
        auto key = hir->symbols.key(qq.name);
        for (auto* s : extfn) {
          if (key == s) nonneg = true;
        }
      }
      if (nonneg) {
        auto [f, g] = guarded({{1.0, q}}, Rel::Ge, 0);
        push(AxiomId::Nneg, std::move(f), g + label(q) + " >= 0");
      }
    }
  }

  void trig(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::ExternalFn) continue;
      auto key = hir->symbols.key(qq.name);
      if (key != "cos" && key != "sin") continue;
      auto fact = qand({qatom({{1.0, q}}, Rel::Le, 1), qatom({{1.0, q}}, Rel::Ge, -1)});
      auto [f, g] = guarded_fact({q}, std::move(fact));
      push(AxiomId::Trig, std::move(f), g + "-1 <= " + label(q) + " <= 1");
    }
  }

  void dphi(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::AngularSep || qq.ang != AngKind::DPhi) continue;
      auto fact = qand({qatom({{1.0, q}}, Rel::Le, PI_UPPER),
                        qatom({{1.0, q}}, Rel::Ge, -PI_UPPER)});
      auto [f, g] = guarded_fact({q}, std::move(fact));
      push(AxiomId::Dphi, std::move(f), g + "-pi <= " + label(q) + " <= pi");
    }
  }

  void tag(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      bool is_tag = false;
      if (qq.kind == QuantityKind::ElemProp)
        is_tag = ext->is_tag_property(hir->table.prop_key(qq.prop));
      else if (qq.kind == QuantityKind::EventScalar &&
               qq.scalar.kind == ScalarSourceKind::Trigger)
        is_tag = true;
      if (!is_tag) continue;
      auto fact = qor({qatom({{1.0, q}}, Rel::Eq, 0), qatom({{1.0, q}}, Rel::Eq, 1)});
      auto [f, g] = guarded_fact({q}, std::move(fact));
      push(AxiomId::Tag, std::move(f), g + label(q) + " in {0, 1}");
    }
  }

  void twin(const std::vector<QuantityId>& qs) {
    std::set<QuantityId> set(qs.begin(), qs.end());
    std::vector<QuantityId> angs;
    for (auto q : qs) {
      if (hir->table.quantity(q).kind == QuantityKind::AngularSep) angs.push_back(q);
    }
    for (std::size_t i = 0; i < angs.size(); ++i) {
      for (std::size_t j = i + 1; j < angs.size(); ++j) {
        auto q1 = angs[i], q2 = angs[j];
        const auto& a = hir->table.quantity(q1);
        const auto& b = hir->table.quantity(q2);
        if (a.ang != b.ang || !a.oriented) continue;
        if (!(a.a == b.b && a.b == b.a)) continue;
        if (hir->table.has_unindexed_leg(q1) || hir->table.has_unindexed_leg(q2)) continue;
        auto fact = qor({qatom({{1.0, q1}, {-1.0, q2}}, Rel::Eq, 0),
                         qatom({{1.0, q1}, {1.0, q2}}, Rel::Eq, 0)});
        auto [f, g] = guarded_fact({q1, q2}, std::move(fact));
        push(AxiomId::Twin, std::move(f), g + label(q1) + " = +/- " + label(q2));
      }
    }
    (void)set;
  }

  void idom(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::ElemProp) continue;
      if (hir->table.prop_key(qq.prop) != pt_key) continue;
      if (qq.index.kind != ElemIndexKind::FromFront) continue;
      const auto& c = hir->table.collection(qq.coll);
      if (c.kind != CollectionKind::Filtered) continue;
      if (!pt_ordered(qq.coll) || !pt_ordered(c.parent)) continue;
      auto qp = hir->table.intern_quantity(
          Quantity::elem_prop(c.parent, qq.index, qq.prop));
      auto [f, g] = guarded({{1.0, q}, {-1.0, qp}}, Rel::Le, 0);
      push(AxiomId::Idom, std::move(f), g + label(q) + " <= " + label(qp));
    }
  }

  void szslice(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::Size) continue;
      const auto& c = hir->table.collection(qq.coll);
      if (c.kind != CollectionKind::Slice) continue;
      auto qp = hir->table.intern_quantity(Quantity::size(c.parent));
      push(AxiomId::Szslice, qatom({{1.0, q}}, Rel::Ge, 0), label(q) + " >= 0");
      push(AxiomId::Szslice, qatom({{1.0, q}, {-1.0, qp}}, Rel::Le, 0),
           label(q) + " <= " + label(qp));
      if (c.slice_end && *c.slice_end >= c.slice_start) {
        double w = static_cast<double>(*c.slice_end - c.slice_start);
        push(AxiomId::Szslice, qatom({{1.0, q}}, Rel::Le, w),
             label(q) + " <= " + std::to_string(static_cast<int>(w)));
      }
    }
  }

  void szperm(const std::vector<QuantityId>& qs) {
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::Size) continue;
      const auto& c = hir->table.collection(qq.coll);
      if (c.kind != CollectionKind::Sorted) continue;
      auto qp = hir->table.intern_quantity(Quantity::size(c.parent));
      auto f = qatom({{1.0, q}, {-1.0, qp}}, Rel::Eq, 0);
      push(AxiomId::Szperm, std::move(f), label(q) + " = " + label(qp));
    }
  }

  void comb_size(const std::vector<QuantityId>& qs) {
    // Mirrors Rust `Emit::comb_size`. Catalog statement and emitter must
    // agree: cross-source empty-factor and cuts-free cartesian lower bound
    // are emitted here (P3 follow-up; previously the catalog overclaimed).
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::Size) continue;
      const auto& c = hir->table.collection(qq.coll);
      if (c.kind == CollectionKind::CombProject) {
        auto qk = hir->table.intern_quantity(Quantity::size(c.parent));
        push(AxiomId::CombSize, qatom({{1.0, q}, {-1.0, qk}}, Rel::Eq, 0),
             label(q) + " = " + label(qk));
        continue;
      }
      if (c.kind != CollectionKind::Combination) continue;
      push(AxiomId::CombSize, qatom({{1.0, q}}, Rel::Ge, 0), label(q) + " >= 0");

      std::vector<QuantityId> part_sizes;
      part_sizes.reserve(c.parts.size());
      for (auto p : c.parts) {
        part_sizes.push_back(hir->table.intern_quantity(Quantity::size(p)));
      }
      bool same = c.parts.size() >= 2;
      for (std::size_t i = 1; i < c.parts.size(); ++i) {
        if (!(c.parts[i] == c.parts[0])) same = false;
      }
      if (c.comb_kind == CombKind::Disjoint && same && !c.parts.empty()) {
        auto qs0 = part_sizes[0];
        // size(C) < 2 => size(K) = 0  ≡  size(C) >= 2 ∨ size(K) = 0
        auto f = qor({qatom({{1.0, qs0}}, Rel::Ge, 2), qatom({{1.0, q}}, Rel::Eq, 0)});
        push(AxiomId::CombSize, std::move(f),
             label(qs0) + " < 2 => " + label(q) + " = 0");
      } else {
        // Cross-source / cartesian: any factor empty => size(K) = 0.
        for (auto qp : part_sizes) {
          auto f = qor({qatom({{1.0, qp}}, Rel::Ge, 1), qatom({{1.0, q}}, Rel::Eq, 0)});
          push(AxiomId::CombSize, std::move(f),
               label(qp) + " = 0 => " + label(q) + " = 0");
        }
        // all-parts-nonempty => size(K) >= 1 only for a bare cartesian
        // (no per-tuple cuts, no candidate). Cross-source disjoint is
        // excluded: value-distinctness can drop the sole pair.
        if (c.comb_kind == CombKind::Cartesian && c.cuts.empty() &&
            !c.candidate.has_value() && !part_sizes.empty()) {
          std::vector<QFormula> alts;
          alts.reserve(part_sizes.size() + 1);
          for (auto qp : part_sizes) {
            alts.push_back(qatom({{1.0, qp}}, Rel::Le, 0));
          }
          alts.push_back(qatom({{1.0, q}}, Rel::Ge, 1));
          push(AxiomId::CombSize, qor(std::move(alts)),
               "all parts >= 1 => " + label(q) + " >= 1");
        }
      }
    }
  }

  void epred(const std::vector<QuantityId>& qs) {
    std::set<std::pair<CollectionId, std::uint32_t>> targets;
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::ElemProp) continue;
      if (qq.index.kind != ElemIndexKind::FromFront) continue;
      if (hir->table.collection(qq.coll).kind != CollectionKind::Filtered) continue;
      targets.insert({qq.coll, qq.index.n});
    }
    for (auto [coll, i] : targets) {
      const auto& c = hir->table.collection(coll);
      if (c.kind != CollectionKind::Filtered) continue;
      const auto& pred_node = hir->elem_pred(c.pred).node;
      auto pred_f = encode_elem_pred(hir->table, pred_node, coll, i);
      if (!pred_f) continue;
      auto size_q = hir->table.intern_quantity(Quantity::size(coll));
      auto guard = qatom({{1.0, size_q}}, Rel::Le, static_cast<double>(i));
      auto f = QFormula::of_or({std::move(guard), std::move(*pred_f)});
      std::string cl = collection_label(*hir, coll);
      push(AxiomId::Epred, std::move(f),
           "size(" + cl + ") > " + std::to_string(i) + " => filter predicate holds for " + cl +
               "[" + std::to_string(i) + "]");
    }
  }

  void epres(const std::vector<QuantityId>& qs) {
    std::set<std::pair<CollectionId, std::uint32_t>> targets;
    for (auto q : qs) {
      const auto& qq = hir->table.quantity(q);
      if (qq.kind != QuantityKind::ElemProp) continue;
      if (qq.index.kind != ElemIndexKind::FromFront) continue;
      if (hir->table.collection(qq.coll).kind != CollectionKind::Filtered) continue;
      targets.insert({qq.coll, qq.index.n});
    }
    for (auto [coll, i] : targets) {
      const auto& c = hir->table.collection(coll);
      if (c.kind != CollectionKind::Filtered) continue;
      const auto& pred_node = hir->elem_pred(c.pred).node;
      std::set<PropId> props;
      collect_self_props(pred_node, props);
      auto size_q = hir->table.intern_quantity(Quantity::size(coll));
      for (auto prop : props) {
        if (!requires_present(pred_node, prop)) continue;
        auto q = hir->table.intern_quantity(
            Quantity::elem_prop(coll, ElemIndex::from_front(i), prop));
        auto p = hir->table.intern_quantity(Quantity::present(q));
        auto guard = qatom({{1.0, size_q}}, Rel::Le, static_cast<double>(i));
        auto fact = qatom({{1.0, p}}, Rel::Ge, 1.0);
        auto f = QFormula::of_or({std::move(guard), std::move(fact)});
        push(AxiomId::Epres, std::move(f),
             "size(" + collection_label(*hir, coll) + ") > " + std::to_string(i) +
                 " => defined(" + label(q) + ")");
      }
    }
  }
};

}  // namespace

std::string collection_label(const Hir& hir, CollectionId c) {
  if (c.id < hir.coll_names.size() && !hir.coll_names[c.id].empty())
    return hir.symbols.display(hir.coll_names[c.id].front());
  const auto& col = hir.table.collection(c);
  if (col.kind == CollectionKind::Base) return hir.symbols.display(col.base);
  return c.to_string();
}

std::string quantity_label(const Hir& hir, QuantityId q) {
  const auto& qq = hir.table.quantity(q);
  switch (qq.kind) {
    case QuantityKind::EventScalar:
      if (qq.scalar.kind == ScalarSourceKind::MetProp)
        return "MET." + hir.table.prop_display(qq.scalar.prop);
      if (qq.scalar.kind == ScalarSourceKind::EventVar)
        return hir.symbols.display(qq.scalar.name);
      return "trig(" + hir.symbols.display(qq.scalar.name) + ")";
    case QuantityKind::Size:
      return "size(" + collection_label(hir, qq.coll) + ")";
    case QuantityKind::ElemProp:
      return collection_label(hir, qq.coll) + "[" + qq.index.to_string() + "]." +
             hir.table.prop_display(qq.prop);
    case QuantityKind::AngularSep:
      return std::string(adl2::sema::ang_kind_str(qq.ang)) + "(...)";
    case QuantityKind::ExternalFn:
      return hir.symbols.display(qq.name) + "(...)";
    case QuantityKind::Present:
      return "defined(" + quantity_label(hir, qq.inner) + ")";
  }
  return q.to_string();
}

AxiomSet emit_axioms(Hir& hir, const ExtDecls& ext, const std::set<QuantityId>& quantities) {
  std::set<QuantityId> qs = quantities;
  std::set<std::string> seen;
  std::vector<AxiomInstance> instances;
  for (int round = 0; round < 32; ++round) {
    std::vector<QuantityId> snap(qs.begin(), qs.end());
    Emit em;
    em.hir = &hir;
    em.ext = &ext;
    em.pt_key = ext.prop_canon("pt").first;
    em.nneg_prop_keys = {ext.prop_canon("pt").first, ext.prop_canon("e").first};
    em.ord(snap);
    em.sz0(snap);
    em.sub(snap);
    em.uni(snap);
    em.nneg(snap);
    em.trig(snap);
    em.dphi(snap);
    em.tag(snap);
    em.twin(snap);
    em.epred(snap);
    em.epres(snap);
    em.idom(snap);
    em.szslice(snap);
    em.szperm(snap);
    em.comb_size(snap);
    em.pres(snap);
    em.pdef(snap);
    bool grew = false;
    for (auto& inst : em.out) {
      std::string key = std::string(axiom_id_str(inst.id)) + "|" + dump_qf(inst.formula);
      if (!seen.insert(key).second) continue;
      collect_qs(inst.formula, qs);
      grew = true;
      instances.push_back(std::move(inst));
    }
    if (!grew) break;
  }
  std::sort(instances.begin(), instances.end(), [](const AxiomInstance& a, const AxiomInstance& b) {
    if (a.id != b.id) return static_cast<int>(a.id) < static_cast<int>(b.id);
    return a.description < b.description;
  });
  AxiomSet set;
  set.instances = std::move(instances);
  return set;
}

std::string dump_axioms(const Hir& hir, const AxiomSet& set) {
  std::ostringstream os;
  os << "unit: " << hir.unit << "\n";
  os << "axioms " << set.instances.size() << "\n";
  for (const auto& inst : set.instances) {
    os << axiom_id_str(inst.id) << " " << inst.description << "\n";
    os << "  " << dump_qf(inst.formula) << "\n";
  }
  return os.str();
}

}  // namespace adl2::axioms
