#pragma once

/// Interval fast path (SPEC_ANALYSIS §2): a cheap, sound heuristic over the
/// unconditional And-spine of a region's over-projection. Only single-quantity
/// atoms reachable without crossing an `Or` contribute. Bounds are exact `Rat`
/// (`k/c`); never f64. Port of Rust `adl-analysis::interval`.

#include "adl2/formula/formula.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"
#include "adl2/solver/assert_name.hpp"

#include <map>
#include <optional>
#include <string>
#include <utility>
#include <vector>

namespace adl2::analysis {

/// One side of an interval, with the atom that set it.
struct Bound {
  adl2::sema::Rat value;
  bool strict = false;
  adl2::solver::AssertName src;
  adl2::formula::LinAtom atom;

  bool operator==(const Bound& o) const {
    return value == o.value && strict == o.strict && src == o.src && atom == o.atom;
  }
  bool operator!=(const Bound& o) const { return !(*this == o); }
};

/// A named formula (or one conjunct of it) participating in an interval
/// refutation.
struct RefutingPart {
  enum class Kind { Whole, Conjunct };
  Kind kind = Kind::Whole;
  adl2::solver::AssertName src_name;
  adl2::formula::LinAtom atom;

  static RefutingPart whole(adl2::solver::AssertName n) {
    RefutingPart p;
    p.kind = Kind::Whole;
    p.src_name = std::move(n);
    return p;
  }
  static RefutingPart conjunct(adl2::solver::AssertName n, adl2::formula::LinAtom a) {
    RefutingPart p;
    p.kind = Kind::Conjunct;
    p.src_name = std::move(n);
    p.atom = std::move(a);
    return p;
  }

  const adl2::solver::AssertName& src() const { return src_name; }

  bool operator==(const RefutingPart& o) const {
    if (kind != o.kind || !(src_name == o.src_name)) return false;
    if (kind == Kind::Conjunct) return atom == o.atom;
    return true;
  }
  bool operator!=(const RefutingPart& o) const { return !(*this == o); }
};

/// A (possibly open) interval constraint on one quantity. Absent bounds are
/// ±∞. Bounds are EXACT rationals.
struct Iv {
  std::optional<Bound> lo;
  std::optional<Bound> hi;

  void tighten_lo(Bound b);
  void tighten_hi(Bound b);

  /// Tightest opposing bounds when `self` and `other` cannot both hold.
  std::optional<std::pair<const Bound*, const Bound*>> refutation(
      const Iv& other) const;

  bool is_empty() const { return refutation(*this).has_value(); }
  bool disjoint_from(const Iv& other) const { return refutation(other).has_value(); }
  std::string human() const;

  bool operator==(const Iv& o) const { return lo == o.lo && hi == o.hi; }
  bool operator!=(const Iv& o) const { return !(*this == o); }
};

/// The tighter of two optional bounds. `lower == true` picks the greater
/// value; otherwise the lesser. On a value tie, the strict bound wins.
const Bound* tightest(const Bound* a, const Bound* b, bool lower);

/// Why a region's own spine is unsatisfiable.
struct SelfEmpty {
  enum class Kind { ConstFalse, EmptyInterval };
  Kind kind = Kind::ConstFalse;
  adl2::solver::AssertName name;
  adl2::sema::QuantityId q;
  Iv iv;

  static SelfEmpty const_false(adl2::solver::AssertName n) {
    SelfEmpty e;
    e.kind = Kind::ConstFalse;
    e.name = std::move(n);
    return e;
  }
  static SelfEmpty empty_interval(adl2::sema::QuantityId qid, Iv interval) {
    SelfEmpty e;
    e.kind = Kind::EmptyInterval;
    e.q = qid;
    e.iv = std::move(interval);
    return e;
  }

  std::string human() const;
  std::vector<RefutingPart> parts() const;
};

/// Two regions' spines cannot intersect on one quantity.
struct IntervalDisjoint {
  adl2::sema::QuantityId q;
  Iv a;
  Iv b;
  std::vector<RefutingPart> parts;
};

/// What the caller knows about a quantity's definedness, for
/// `IntervalMap::disjoint_with`'s belt-and-braces check.
struct Presence {
  enum class Kind { Total, Indicator, Unpinned };
  Kind kind = Kind::Total;
  adl2::sema::QuantityId indicator;

  static Presence total() {
    Presence p;
    p.kind = Kind::Total;
    return p;
  }
  static Presence of_indicator(adl2::sema::QuantityId id) {
    Presence p;
    p.kind = Kind::Indicator;
    p.indicator = id;
    return p;
  }
  static Presence unpinned() {
    Presence p;
    p.kind = Kind::Unpinned;
    return p;
  }
};

/// Per-region interval summary from the And-spine of over-projections.
struct IntervalMap {
  std::map<adl2::sema::QuantityId, Iv> by_quantity;
  std::optional<adl2::solver::AssertName> falsified;

  /// Fold an over-projection's And-spine into the map. Takes `Over` so only
  /// supersets feed the interval layer.
  void add_over(const adl2::solver::AssertName& src, const adl2::formula::Over& o);

  std::optional<SelfEmpty> self_empty() const;

  /// First quantity on which the two regions' spines cannot intersect.
  /// `presence(q)` must report Total / Indicator / Unpinned for `q`.
  template <typename PresenceFn>
  std::optional<IntervalDisjoint> disjoint_with(const IntervalMap& other,
                                                PresenceFn presence) const;

 private:
  void spine(const adl2::solver::AssertName& src, const adl2::formula::QFormula& f);
  bool pins_present(adl2::sema::QuantityId present_id) const;
};

RefutingPart part_of(const Bound& b);

template <typename PresenceFn>
std::optional<IntervalDisjoint> IntervalMap::disjoint_with(
    const IntervalMap& other, PresenceFn presence) const {
  for (const auto& kv : by_quantity) {
    auto it = other.by_quantity.find(kv.first);
    if (it == other.by_quantity.end()) continue;
    auto ref = kv.second.refutation(it->second);
    if (!ref) continue;
    Presence p = presence(kv.first);
    if (p.kind == Presence::Kind::Indicator) {
      if (!(pins_present(p.indicator) && other.pins_present(p.indicator))) continue;
    } else if (p.kind == Presence::Kind::Unpinned) {
      continue;
    }
    IntervalDisjoint d;
    d.q = kv.first;
    d.a = kv.second;
    d.b = it->second;
    d.parts.push_back(part_of(*ref->first));
    d.parts.push_back(part_of(*ref->second));
    return d;
  }
  return std::nullopt;
}

}  // namespace adl2::analysis
