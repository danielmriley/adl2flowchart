#include "adl2/analysis/interval.hpp"
#include "adl2/analysis/report.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/formula/lin.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"
#include "adl2/solver/solver.hpp"

#include <iostream>
#include <string>
#include <vector>

using adl2::analysis::Bound;
using adl2::analysis::IntervalMap;
using adl2::analysis::Iv;
using adl2::analysis::Presence;
using adl2::analysis::RefutingPart;
using adl2::analysis::SelfEmpty;
using adl2::formula::Formula;
using adl2::formula::LinAtom;
using adl2::formula::Over;
using adl2::formula::Rel;
using adl2::sema::QuantityId;
using adl2::sema::Rat;
using adl2::solver::AssertName;

namespace {

int g_fails = 0;
int g_pass = 0;

void check(bool cond, const char* expr, const char* file, int line) {
  if (cond) {
    ++g_pass;
  } else {
    ++g_fails;
    std::cerr << "FAIL " << file << ":" << line << "  " << expr << "\n";
  }
}

#define CHECK(cond) check(static_cast<bool>(cond), #cond, __FILE__, __LINE__)

Over over(Formula f) { return f.over(); }

AssertName name(const char* s) { return AssertName::make(s); }

Formula atom_f64(std::uint32_t q, Rel rel, double k) {
  return Formula::of_atom(
      LinAtom::single(QuantityId{q}, rel, *Rat::from_decimal_f64(k)));
}

Formula atom_i64(std::uint32_t q, Rel rel, std::int64_t k) {
  return Formula::of_atom(LinAtom::single(QuantityId{q}, rel, Rat::from_i64(k)));
}

IntervalMap map_of(const char* src, Formula f) {
  IntervalMap m;
  auto o = over(std::move(f));
  m.add_over(name(src), o);
  return m;
}

void test_spine_intervals_and_disjointness() {
  auto a = map_of("A", Formula::of_and({atom_f64(0, Rel::Gt, 100.0),
                                       atom_f64(0, Rel::Lt, 200.0)}));
  auto b = map_of("B", atom_f64(0, Rel::Gt, 300.0));
  auto total = [](QuantityId) { return Presence::total(); };
  CHECK(a.disjoint_with(b, total).has_value());
  CHECK(!a.self_empty().has_value());

  auto c = map_of("C", atom_f64(0, Rel::Ge, 200.0));
  CHECK(a.disjoint_with(c, total).has_value());
  auto d = map_of("D", atom_f64(0, Rel::Le, 100.0));
  CHECK(a.disjoint_with(d, total).has_value());
}

void test_refutation_reports_both_originating_atoms() {
  auto a = map_of("A", Formula::of_and({atom_f64(0, Rel::Gt, 100.0),
                                       atom_f64(0, Rel::Lt, 200.0)}));
  auto b = map_of("B", atom_f64(0, Rel::Gt, 300.0));
  auto d = a.disjoint_with(b, [](QuantityId) { return Presence::total(); });
  CHECK(d.has_value());
  if (!d) return;
  CHECK(d->parts.size() == 2);
  CHECK(d->parts[0].src() == name("B"));
  CHECK(d->parts[1].src() == name("A"));
}

void test_or_branches_are_ignored_soundly() {
  auto a = map_of("A", Formula::of_or({atom_f64(0, Rel::Lt, 100.0),
                                      atom_f64(0, Rel::Gt, 500.0)}));
  CHECK(a.by_quantity.empty());
}

void test_negative_coefficient_flips() {
  // -2 q <= -400  ⇔  q >= 200
  auto a = map_of(
      "A", Formula::of_atom(LinAtom::make(
               {{Rat::from_i64(-2), QuantityId{0}}}, Rel::Le, Rat::from_i64(-400))));
  auto it = a.by_quantity.find(QuantityId{0});
  CHECK(it != a.by_quantity.end());
  CHECK(it->second.lo.has_value());
  if (it->second.lo) {
    CHECK(it->second.lo->value == Rat::from_i64(200));
    CHECK(it->second.lo->strict == false);
  }
}

void test_self_empty_detection_and_provenance() {
  auto a = map_of(
      "A", Formula::of_and({atom_f64(0, Rel::Gt, 5.0), atom_f64(0, Rel::Lt, 5.0)}));
  auto e = a.self_empty();
  CHECK(e.has_value());
  if (e) {
    CHECK(e->parts().size() == 2);
    CHECK(e->human().find("quantity ") == 0);
  }

  auto f = map_of("F", Formula::ffalse());
  auto ef = f.self_empty();
  CHECK(ef.has_value());
  if (ef) {
    CHECK(ef->parts() == std::vector<RefutingPart>{RefutingPart::whole(name("F"))});
    CHECK(ef->human() == "a cut is constant-false");
  }
}

void test_empty_open_interval_2_2() {
  // Empty (2,2): q > 2 ∧ q < 2.
  auto a = map_of(
      "A", Formula::of_and({atom_i64(0, Rel::Gt, 2), atom_i64(0, Rel::Lt, 2)}));
  CHECK(a.self_empty().has_value());
  auto it = a.by_quantity.find(QuantityId{0});
  CHECK(it != a.by_quantity.end());
  CHECK(it->second.is_empty());
}

void test_disjoint_gt2_vs_le2() {
  auto total = [](QuantityId) { return Presence::total(); };
  auto gt = map_of("A", atom_i64(0, Rel::Gt, 2));
  auto le = map_of("B", atom_i64(0, Rel::Le, 2));
  CHECK(gt.disjoint_with(le, total).has_value());

  // Touching closed bounds intersect.
  auto ge = map_of("C", atom_i64(0, Rel::Ge, 2));
  CHECK(!ge.disjoint_with(le, total).has_value());
}

void test_rational_0_1_bounds() {
  auto tenth = Rat::from_decimal_f64(0.1);
  CHECK(tenth.has_value());
  if (tenth) {
    auto p = tenth->to_parts();
    CHECK(p.numerator == "1");
    CHECK(p.denominator == "10");
    CHECK(!p.negative);
  }
  auto total = [](QuantityId) { return Presence::total(); };
  auto a = map_of("A", atom_f64(0, Rel::Gt, 0.1));
  auto b = map_of("B", atom_f64(0, Rel::Le, 0.1));
  CHECK(a.disjoint_with(b, total).has_value());
  auto it = a.by_quantity.find(QuantityId{0});
  CHECK(it != a.by_quantity.end() && it->second.lo.has_value());
  if (it != a.by_quantity.end() && it->second.lo) {
    auto p = it->second.lo->value.to_parts();
    CHECK(p.numerator == "1" && p.denominator == "10");
    CHECK(it->second.lo->strict);
  }
}

void test_tightest_strict_wins_ties() {
  Bound closed;
  closed.value = Rat::from_i64(2);
  closed.strict = false;
  closed.src = name("C");
  Bound openb;
  openb.value = Rat::from_i64(2);
  openb.strict = true;
  openb.src = name("O");
  const Bound* lo = adl2::analysis::tightest(&closed, &openb, true);
  CHECK(lo == &openb);
  const Bound* hi = adl2::analysis::tightest(&closed, &openb, false);
  CHECK(hi == &openb);
}

void test_presence_guard() {
  auto p = [](QuantityId q) {
    if (q.id == 0) return Presence::of_indicator(QuantityId{9});
    return Presence::total();
  };
  auto a = map_of("A", Formula::of_and({atom_i64(9, Rel::Ge, 1), atom_i64(0, Rel::Gt, 10)}));
  auto b = map_of("B", Formula::of_and({atom_i64(9, Rel::Ge, 1), atom_i64(0, Rel::Lt, 5)}));
  CHECK(a.disjoint_with(b, p).has_value());

  auto c = map_of("C", atom_i64(0, Rel::Lt, 5));
  CHECK(!a.disjoint_with(c, p).has_value());

  auto unpinned = [](QuantityId) { return Presence::unpinned(); };
  CHECK(!a.disjoint_with(b, unpinned).has_value());

  auto d = map_of("D", atom_i64(1, Rel::Gt, 10));
  auto e = map_of("E", atom_i64(1, Rel::Lt, 5));
  CHECK(d.disjoint_with(e, p).has_value());
}

void test_fail_on_parse() {
  adl2::analysis::FailOn f;
  std::string err;
  CHECK(adl2::analysis::FailOn::parse("overlap,empty", f, err));
  CHECK(f.overlap && f.empty && !f.gap && !f.non_exact);
  CHECK(adl2::analysis::FailOn::parse("non-exact", f, err));
  CHECK(f.non_exact);
  CHECK(!adl2::analysis::FailOn::parse("bogus", f, err));
  CHECK(adl2::analysis::FailOn::parse("", f, err));
  CHECK(!f.overlap && !f.gap && !f.empty && !f.non_exact && !f.unknown);
  CHECK(adl2::analysis::SCHEMA_VERSION == 4);
}

}  // namespace

int main() {
  test_spine_intervals_and_disjointness();
  test_refutation_reports_both_originating_atoms();
  test_or_branches_are_ignored_soundly();
  test_negative_coefficient_flips();
  test_self_empty_detection_and_provenance();
  test_empty_open_interval_2_2();
  test_disjoint_gt2_vs_le2();
  test_rational_0_1_bounds();
  test_tightest_strict_wins_ties();
  test_presence_guard();
  test_fail_on_parse();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
