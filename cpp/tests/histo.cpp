#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <cmath>
#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

using namespace adl2::sema;
using adl2::interp::Event;
using adl2::interp::Hist1D;
using adl2::interp::HistoSet;
using adl2::interp::Interp;

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

Event must_event(const std::string& json) {
  auto ext = ExtDecls::legacy();
  adl2::interp::EventError err;
  auto e = adl2::interp::parse_event(json, ext, err);
  if (!e) {
    std::cerr << "parse_event failed: " << err.to_string() << "\n";
    std::abort();
  }
  return *e;
}

std::string met_event(double met) {
  return "{\"MET\": {\"pt\": " + std::to_string(met) + ", \"phi\": 0.0}}";
}

Hir hir_of(const char* src) {
  auto h = analyze_str(src, "test.adl", ExtDecls::legacy());
  if (has_errors(h.diags)) {
    std::cerr << "unexpected resolve errors\n";
    std::abort();
  }
  return h;
}

HistoSet run_histos(const Hir& h, const std::vector<Event>& evs) {
  auto ext = ExtDecls::legacy();
  Interp interp(h, ext);
  auto set = HistoSet::make(h);
  for (const auto& ev : evs) {
    auto results = interp.run_event(ev);
    set.fill_event(interp, ev, results);
  }
  return set;
}

void test_hist1d_fill() {
  auto h = Hist1D::make(4, 10.0, 50.0);
  for (double x : {5.0, 10.0, 19.999, 25.0, 49.9, 50.0, 75.0}) h.fill(x, 2.0);
  CHECK(h.entries == 7);
  CHECK(h.underflow_w == 2.0 && h.underflow_w2 == 4.0);
  CHECK(h.overflow_w == 4.0 && h.overflow_w2 == 8.0);
  CHECK(h.sumw.size() == 4);
  CHECK(h.sumw[0] == 4.0 && h.sumw[1] == 2.0 && h.sumw[2] == 0.0 && h.sumw[3] == 2.0);
  CHECK(h.tsumw == 8.0);
  CHECK(h.tsumw2 == 16.0);
}

void test_region_weight() {
  const char* src =
      "region SR\n  select MET >= 0\n  weight lumi 2.0\n  histo hmet, \"met\", 4, 10, 50, MET\n";
  auto h = hir_of(src);
  auto set = run_histos(h, {must_event(met_event(5)), must_event(met_event(10)),
                            must_event(met_event(25)), must_event(met_event(50))});
  CHECK(set.histos.size() == 1);
  CHECK(set.histos[0].name == "hmet" && set.histos[0].region == "SR");
  const auto* h1 = set.histos[0].h1();
  CHECK(h1 && h1->entries == 4);
  CHECK(h1 && h1->sumw.size() == 4 && h1->sumw[0] == 2.0 && h1->sumw[1] == 2.0);
  CHECK(h1 && h1->underflow_w == 2.0 && h1->overflow_w == 2.0);
  CHECK(h1 && h1->tsumw == 4.0);
  CHECK(h1 && h1->tsumwx == 2.0 * 10.0 + 2.0 * 25.0);
  CHECK(set.diagnostics().empty());
}

void test_rejected_no_fill() {
  const char* src = "region SR\n  select MET > 100\n  histo hmet, \"met\", 4, 0, 400, MET\n";
  auto h = hir_of(src);
  auto set = run_histos(h, {must_event(met_event(50)), must_event(met_event(150))});
  const auto* h1 = set.histos[0].h1();
  CHECK(h1 && h1->entries == 1);
  CHECK(h1 && h1->sumw.size() == 4 && h1->sumw[1] == 1.0);
}

void test_non_numeric_weight() {
  const char* src =
      "region SR\n  select MET >= 0\n  weight wtab someFunc(MET)\n  histo hmet, \"met\", 2, 0, 100, "
      "MET\n";
  auto h = hir_of(src);
  auto set = run_histos(h, {must_event(met_event(25))});
  const auto* h1 = set.histos[0].h1();
  CHECK(h1 && h1->sumw.size() == 2 && h1->sumw[0] == 1.0);
  auto diags = set.diagnostics();
  CHECK(diags.size() == 1);
  CHECK(diags[0].find("weight `wtab`") != std::string::npos);
  CHECK(diags[0].find("treated as 1.0") != std::string::npos);
}

void test_skip_out_of_fragment() {
  const char* src =
      "region SR\n  select MET >= 0\n  histo hbad, \"bad\", 4, 0, 100, fancyFn(MET, 3)\n  "
      "histo hmet, \"met\", 4, 0, 100, MET\n";
  auto h = hir_of(src);
  auto set = run_histos(h, {must_event(met_event(25))});
  CHECK(set.histos.size() == 1);
  CHECK(set.histos[0].name == "hmet");
  auto diags = set.diagnostics();
  CHECK(diags.size() == 1);
  CHECK(diags[0].find("hbad") != std::string::npos);
  CHECK(diags[0].find("histogram skipped") != std::string::npos);
}

void test_histolist() {
  const char* src =
      "histoList hl\n  histo hmet, \"met\", 2, 0, 100, MET\n"
      "region SR\n  select MET >= 0\n  hl\n";
  auto h = hir_of(src);
  auto set = run_histos(h, {must_event(met_event(25))});
  CHECK(set.histos.size() == 1);
  CHECK(set.histos[0].name == "hmet");
  CHECK(set.histos[0].region == "SR");
  const auto* h1 = set.histos[0].h1();
  CHECK(h1 && h1->entries == 1);
}

void test_json_deterministic() {
  const char* src =
      "region SR\n  select MET >= 0\n  weight lumi 2.0\n  histo hmet, \"met\", 4, 10, 50, MET\n";
  auto h = hir_of(src);
  auto a = run_histos(h, {must_event(met_event(25))});
  auto b = run_histos(h, {must_event(met_event(25))});
  CHECK(a.to_json(false) == b.to_json(false));
  CHECK(a.to_json(true) == b.to_json(true));
  auto j = a.to_json(false);
  CHECK(j.find("\"version\":2") != std::string::npos);
  CHECK(j.find("\"type\":\"h1\"") != std::string::npos);
}

}  // namespace

int main() {
  test_hist1d_fill();
  test_region_weight();
  test_rejected_no_fill();
  test_non_numeric_weight();
  test_skip_out_of_fragment();
  test_histolist();
  test_json_deterministic();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails ? 1 : 0;
}
