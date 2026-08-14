#include "adl2/analysis/refute.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <iostream>
#include <limits>
#include <string>
#include <vector>

using namespace adl2::sema;
using adl2::analysis::MAX_REFUTE_PROBES;
using adl2::analysis::probe_events;
using adl2::analysis::probe_scalars;
using adl2::interp::absence_family;

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

bool contains(const std::vector<double>& v, double x) {
  return std::find(v.begin(), v.end(), x) != v.end();
}

void test_probe_scalars_families() {
  auto c4 = probe_scalars({0.5, 1.0});
  CHECK(contains(c4, 0.5000000000000001));
  auto c5 = probe_scalars({0.3, 1.0, 3.3333333333333335});
  CHECK(contains(c5, 3.3333333333333335));
  auto ce14 = probe_scalars({0.3, 0.1, 0.03});
  double cleared = 0.1 * 0.3;
  double up = std::nextafter(cleared, std::numeric_limits<double>::infinity());
  bool hit = false;
  for (double v : ce14) {
    if (v == up) hit = true;
  }
  CHECK(hit);
}

void test_probe_events_c4_met() {
  ExtDecls ext = ExtDecls::legacy();
  auto pt_key = ext.prop_canon("pt").first;
  double w = 0.5000000000000001;
  auto events = probe_events(ext, {0.5, 1.0});
  bool hit = false;
  for (const auto& e : events) {
    auto it = e.met.find(pt_key);
    if (it != e.met.end() && it->second.to_f64() == w) hit = true;
  }
  CHECK(hit);
}

void test_probe_events_capped() {
  ExtDecls ext = ExtDecls::legacy();
  std::vector<double> many;
  for (int i = 0; i < 40; ++i) many.push_back(static_cast<double>(i));
  auto events = probe_events(ext, many);
  CHECK(events.size() <= MAX_REFUTE_PROBES + absence_family().size());
  CHECK(!events.empty());
}

void test_absence_survives_saturated_budget() {
  ExtDecls ext = ExtDecls::legacy();
  std::vector<double> many;
  for (int i = 0; i < 40; ++i) many.push_back(static_cast<double>(i));
  auto events = probe_events(ext, many);
  bool met_less = false;
  bool jet_no_btag = false;
  for (const auto& e : events) {
    if (e.met.empty()) met_less = true;
    auto jit = e.collections.find("jet");
    if (jit != e.collections.end() && !jit->second.empty()) {
      bool all = true;
      for (const auto& j : jit->second) {
        if (j.get("btag")) all = false;
      }
      if (all) jet_no_btag = true;
    }
  }
  CHECK(met_less);
  CHECK(jet_no_btag);
}

void test_every_cut_anchor_survives() {
  ExtDecls ext = ExtDecls::legacy();
  auto pt_key = ext.prop_canon("pt").first;
  std::vector<double> cuts = {30.0, 100.0, 250.0, 800.0};
  auto events = probe_events(ext, cuts);
  for (double c : cuts) {
    auto want = Rat::from_decimal_f64(c);
    CHECK(want.has_value());
    bool hit = false;
    for (const auto& e : events) {
      auto it = e.met.find(pt_key);
      if (it != e.met.end() && want && it->second == *want) hit = true;
    }
    CHECK(hit);
  }
}

void test_negative_anchors_clamped() {
  ExtDecls ext = ExtDecls::legacy();
  auto pt_key = ext.prop_canon("pt").first;
  auto scalars = probe_scalars({-5.0, 0.0});
  for (double v : scalars) CHECK(v >= 0.0);
  for (const auto& e : probe_events(ext, {-5.0, -1.0, 0.0})) {
    auto mit = e.met.find(pt_key);
    if (mit != e.met.end()) CHECK(!mit->second.is_negative());
    for (const auto& kv : e.collections) {
      for (const auto& o : kv.second) {
        const Rat* pt = o.get(pt_key);
        if (pt) CHECK(!pt->is_negative());
      }
    }
  }
}

}  // namespace

int main() {
  test_probe_scalars_families();
  test_probe_events_c4_met();
  test_probe_events_capped();
  test_absence_survives_saturated_budget();
  test_every_cut_anchor_survives();
  test_negative_anchors_clamped();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
