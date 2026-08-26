#include "adl2/analysis/interval.hpp"

#include "adl2/formula/lin.hpp"

#include <sstream>
#include <utility>

namespace adl2::analysis {
namespace {

using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::sema::QuantityId;
using adl2::sema::Rat;
using adl2::solver::AssertName;

std::string rat_display(const Rat& r) {
  std::ostringstream os;
  os << r.to_f64();
  return os.str();
}

}  // namespace

void Iv::tighten_lo(Bound b) {
  bool better = !lo.has_value() || b.value > lo->value ||
                (b.value == lo->value && b.strict && !lo->strict);
  if (better) lo = std::move(b);
}

void Iv::tighten_hi(Bound b) {
  bool better = !hi.has_value() || b.value < hi->value ||
                (b.value == hi->value && b.strict && !hi->strict);
  if (better) hi = std::move(b);
}

const Bound* tightest(const Bound* a, const Bound* b, bool lower) {
  if (!a && !b) return nullptr;
  if (a && !b) return a;
  if (!a && b) return b;
  bool strictly_tighter = lower ? (a->value > b->value) : (a->value < b->value);
  bool wins = strictly_tighter || (a->value == b->value && a->strict && !b->strict);
  return wins ? a : b;
}

std::optional<std::pair<const Bound*, const Bound*>> Iv::refutation(
    const Iv& other) const {
  const Bound* lo_b = tightest(lo ? &*lo : nullptr, other.lo ? &*other.lo : nullptr, true);
  const Bound* hi_b = tightest(hi ? &*hi : nullptr, other.hi ? &*other.hi : nullptr, false);
  if (!lo_b || !hi_b) return std::nullopt;
  bool empty = lo_b->value > hi_b->value ||
               (lo_b->value == hi_b->value && (lo_b->strict || hi_b->strict));
  if (!empty) return std::nullopt;
  return std::make_pair(lo_b, hi_b);
}

std::string Iv::human() const {
  const char* lo_b = (lo && lo->strict) ? "(" : "[";
  const char* hi_b = (hi && hi->strict) ? ")" : "]";
  std::string lo_s = lo ? rat_display(lo->value) : "-inf";
  std::string hi_s = hi ? rat_display(hi->value) : "inf";
  return std::string(lo_b) + lo_s + ", " + hi_s + hi_b;
}

std::string SelfEmpty::human() const {
  if (kind == Kind::ConstFalse) return "a cut is constant-false";
  return "quantity " + q.to_string() + " constrained to the empty interval " + iv.human();
}

std::vector<RefutingPart> SelfEmpty::parts() const {
  if (kind == Kind::ConstFalse) return {RefutingPart::whole(name)};
  auto ref = iv.refutation(iv);
  if (!ref) return {};
  return {part_of(*ref->first), part_of(*ref->second)};
}

RefutingPart part_of(const Bound& b) { return RefutingPart::conjunct(b.src, b.atom); }

void IntervalMap::add_over(const AssertName& src, const adl2::formula::Over& o) {
  spine(src, o.qformula());
}

void IntervalMap::spine(const AssertName& src, const QFormula& f) {
  switch (f.kind) {
    case QFormula::Kind::True:
      return;
    case QFormula::Kind::False:
      if (!falsified) falsified = src;
      return;
    case QFormula::Kind::And:
      for (const auto& p : f.items) spine(src, p);
      return;
    case QFormula::Kind::Or:
      // Disjunctive structure leaves the spine; ignoring it is sound
      // (we only ever DROP necessary conditions).
      return;
    case QFormula::Kind::Atom: {
      const LinAtom& a = f.atom;
      if (a.terms().size() != 1) return;
      const Rat& c = a.terms()[0].first;
      QuantityId q = a.terms()[0].second;
      if (c.is_zero()) return;
      auto bound = a.constant().checked_div(c);
      if (!bound) return;
      Rel rel = c.is_negative() ? adl2::formula::rel_flipped(a.rel()) : a.rel();
      auto mk = [&](Rat value, bool strict) {
        Bound b;
        b.value = std::move(value);
        b.strict = strict;
        b.src = src;
        b.atom = a;
        return b;
      };
      Iv& iv = by_quantity[q];
      switch (rel) {
        case Rel::Lt:
          iv.tighten_hi(mk(std::move(*bound), true));
          break;
        case Rel::Le:
          iv.tighten_hi(mk(std::move(*bound), false));
          break;
        case Rel::Gt:
          iv.tighten_lo(mk(std::move(*bound), true));
          break;
        case Rel::Ge:
          iv.tighten_lo(mk(std::move(*bound), false));
          break;
        case Rel::Eq:
          iv.tighten_lo(mk(*bound, false));
          iv.tighten_hi(mk(std::move(*bound), false));
          break;
        case Rel::Ne:
          break;
      }
      return;
    }
  }
}

std::optional<SelfEmpty> IntervalMap::self_empty() const {
  if (falsified) return SelfEmpty::const_false(*falsified);
  for (const auto& kv : by_quantity) {
    if (kv.second.is_empty()) {
      return SelfEmpty::empty_interval(kv.first, kv.second);
    }
  }
  return std::nullopt;
}

bool IntervalMap::pins_present(QuantityId present_id) const {
  auto it = by_quantity.find(present_id);
  if (it == by_quantity.end() || !it->second.lo) return false;
  return it->second.lo->value >= Rat::one();
}

}  // namespace adl2::analysis
