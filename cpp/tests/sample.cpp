#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <cmath>
#include <cstdlib>
#include <iostream>
#include <limits>
#include <string>
#include <vector>

using namespace adl2::sema;
using adl2::interp::Event;
using adl2::interp::absence_family;
using adl2::interp::battery;
using adl2::interp::battery_with_cuts;
using adl2::interp::expand_cut_boundaries;
using adl2::interp::MAX_CUT_CONSTANTS;

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

double next_up(double v) { return std::nextafter(v, std::numeric_limits<double>::infinity()); }
double next_down(double v) { return std::nextafter(v, -std::numeric_limits<double>::infinity()); }

void test_battery_deterministic() {
  ExtDecls ext = ExtDecls::legacy();
  auto a = battery(ext, 32);
  auto b = battery(ext, 32);
  CHECK(a.size() == 32 + absence_family().size());
  CHECK(a.size() == b.size());
  for (std::size_t i = 0; i < a.size(); ++i) {
    auto ia = a[i].collections.find("jet");
    auto ib = b[i].collections.find("jet");
    std::size_t na = ia == a[i].collections.end() ? 0 : ia->second.size();
    std::size_t nb = ib == b[i].collections.end() ? 0 : ib->second.size();
    CHECK(na == nb);
  }
}

void test_battery_covers_boundary_and_empty() {
  ExtDecls ext = ExtDecls::legacy();
  auto events = battery(ext, 64);
  std::size_t empties = 0;
  for (const auto& e : events) {
    auto it = e.collections.find("jet");
    if (it == e.collections.end() || it->second.empty()) ++empties;
  }
  CHECK(empties >= 1);
  auto want_pt = Rat::from_decimal_f64(30.0);
  CHECK(want_pt.has_value());
  bool boundary_pt = false;
  for (const auto& e : events) {
    auto it = e.collections.find("jet");
    if (it == e.collections.end()) continue;
    for (const auto& j : it->second) {
      const Rat* p = j.get("ptof");
      if (p && want_pt && *p == *want_pt) boundary_pt = true;
    }
  }
  CHECK(boundary_pt);
}

void test_cut_constant_met_boundaries() {
  ExtDecls ext = ExtDecls::legacy();
  auto pt_key = ext.prop_canon("pt").first;
  double c = 0.5;
  auto events = battery_with_cuts(ext, 8, {c});
  double want[3] = {c, next_up(c), next_down(c)};
  for (double w : want) {
    auto want_r = Rat::from_decimal_f64(w);
    CHECK(want_r.has_value());
    bool hit = false;
    for (const auto& e : events) {
      auto it = e.met.find(pt_key);
      if (it != e.met.end() && want_r && it->second == *want_r) hit = true;
    }
    CHECK(hit);
  }
}

void test_battery_inside_axiom_domain() {
  ExtDecls ext = ExtDecls::legacy();
  auto pt_key = ext.prop_canon("pt").first;
  auto e_key = ext.prop_canon("e").first;
  std::vector<double> cuts = {-5.0, -0.5, 0.0, 0.3, 30.0, -1e6};
  auto events = battery_with_cuts(ext, 64, cuts);
  CHECK(!events.empty());
  Rat zero = Rat::zero();
  for (const auto& e : events) {
    for (const auto& kv : e.collections) {
      for (const auto& o : kv.second) {
        for (const auto& p : o.props) {
          if (p.first == pt_key || p.first == e_key) {
            CHECK(p.second >= zero);
          }
          if (p.first == "btag" || p.first == "ctag" || p.first == "tautag") {
            CHECK(p.second.is_zero() || p.second == Rat::one());
          }
        }
      }
    }
    auto mit = e.met.find(pt_key);
    if (mit != e.met.end()) CHECK(mit->second >= zero);
    for (const auto& s : e.scalars) {
      if (ext.is_event_scalar(s.first)) CHECK(s.second >= zero);
    }
    for (const auto& t : e.triggers) {
      CHECK(t.second.is_zero() || t.second == Rat::one());
    }
  }
}

void test_absence_family_reach() {
  ExtDecls ext = ExtDecls::legacy();
  auto events = battery(ext, 64);
  auto pt_key = ext.prop_canon("pt").first;
  bool jet_without_btag = false;
  bool ele_without_pt = false;
  bool met_less = false;
  bool ht_less = false;
  bool trig_less = false;
  for (const auto& e : events) {
    auto jit = e.collections.find("jet");
    if (jit != e.collections.end() && !jit->second.empty()) {
      bool all = true;
      for (const auto& j : jit->second) {
        if (j.get("btag")) all = false;
      }
      if (all) jet_without_btag = true;
    }
    auto eit = e.collections.find("electron");
    if (eit != e.collections.end() && !eit->second.empty()) {
      bool all = true;
      for (const auto& x : eit->second) {
        if (x.get(pt_key)) all = false;
      }
      if (all) ele_without_pt = true;
    }
    if (e.met.empty()) met_less = true;
    if (e.scalars.find("HT") == e.scalars.end()) ht_less = true;
    if (e.triggers.empty()) trig_less = true;
  }
  CHECK(jet_without_btag);
  CHECK(ele_without_pt);
  CHECK(met_less);
  CHECK(ht_less);
  CHECK(trig_less);
}

void test_expand_cut_boundaries_capped() {
  std::vector<double> many;
  for (int i = 0; i < 100; ++i) many.push_back(static_cast<double>(i));
  auto b = expand_cut_boundaries(many);
  CHECK(b.size() <= MAX_CUT_CONSTANTS * 3);
  for (std::size_t i = 1; i < b.size(); ++i) CHECK(b[i - 1] < b[i]);
}

}  // namespace

int main() {
  test_battery_deterministic();
  test_battery_covers_boundary_and_empty();
  test_cut_constant_met_boundaries();
  test_battery_inside_axiom_domain();
  test_absence_family_reach();
  test_expand_cut_boundaries_capped();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
