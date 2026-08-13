#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <cmath>
#include <cstdlib>
#include <iostream>
#include <string>
#include <vector>

using namespace adl2::sema;
using adl2::interp::BinFlowKind;
using adl2::interp::Counts;
using adl2::interp::CutflowSet;
using adl2::interp::Event;
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
    std::cerr << "parse_event failed: " << err.to_string() << " json=" << json << "\n";
    std::abort();
  }
  return *e;
}

std::string met_event(double met) {
  return "{\"MET\": {\"pt\": " + std::to_string(met) + ", \"phi\": 0.0}}";
}

std::string met_weighted(double met, double w) {
  return "{\"MET\": {\"pt\": " + std::to_string(met) + ", \"phi\": 0.0}, \"weight\": " +
         std::to_string(w) + "}";
}

Hir hir_of(const char* src) {
  auto h = analyze_str(src, "test.adl", ExtDecls::legacy());
  if (has_errors(h.diags)) {
    std::cerr << "unexpected resolve errors\n";
    std::abort();
  }
  return h;
}

CutflowSet run_cutflow(const char* src, const Hir& h, const std::vector<Event>& evs) {
  auto ext = ExtDecls::legacy();
  Interp interp(h, ext);
  auto set = CutflowSet::make(h, src);
  for (const auto& ev : evs) {
    auto [results, traces] = interp.run_event_traced(ev);
    set.record_event(ev, results, traces);
  }
  return set;
}

Counts counts(std::uint64_t raw, double sumw, double sumw2) {
  Counts c;
  c.raw = raw;
  c.sumw = sumw;
  c.sumw2 = sumw2;
  return c;
}

void test_json_f64() {
  CHECK(adl2::interp::json_f64(3.0) == "3.0");
  CHECK(adl2::interp::json_f64(2.5) == "2.5");
  CHECK(adl2::interp::json_f64(1.25) == "1.25");
  CHECK(adl2::interp::json_f64(0.0) == "0.0");
  CHECK(adl2::interp::json_f64(std::sqrt(2.0)) == "1.4142135623730951");
}

void test_select_and_reject() {
  const char* src = "region SR\n  select MET > 100\n  reject MET > 300\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {must_event(met_event(50)), must_event(met_event(150)),
                            must_event(met_event(350)), must_event(met_event(200))};
  auto set = run_cutflow(src, h, evs);
  CHECK((set.total() == counts(4, 4.0, 4.0)));
  CHECK(set.regions().size() == 1);
  const auto& flow = set.regions()[0];
  CHECK(flow.name == "SR");
  CHECK(flow.steps.size() == 3);
  CHECK(std::string(flow.steps[0].kind) == "all");
  CHECK(std::string(flow.steps[1].kind) == "select");
  CHECK(std::string(flow.steps[2].kind) == "reject");
  CHECK(flow.steps[1].label == "select MET > 100");
  CHECK(flow.steps[2].label == "reject MET > 300");
  CHECK((flow.steps[0].counts == counts(4, 4.0, 4.0)));
  CHECK((flow.steps[1].counts == counts(3, 3.0, 3.0)));
  CHECK((flow.steps[2].counts == counts(2, 2.0, 2.0)));
  CHECK(flow.steps[0].errors == 0 && flow.steps[1].errors == 0 && flow.steps[2].errors == 0);
  CHECK(set.diagnostics().empty());
}

void test_inherit_one_step() {
  const char* src =
      "region presel\n  select MET > 100\n  reject MET > 400\n"
      "region SR\n  presel\n  select MET > 200\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {must_event(met_event(50)), must_event(met_event(150)),
                            must_event(met_event(250)), must_event(met_event(450))};
  auto set = run_cutflow(src, h, evs);
  CHECK(set.regions().size() == 2);
  const auto& presel = set.regions()[0];
  CHECK(presel.steps[1].counts.raw == 3);
  CHECK(presel.steps[2].counts.raw == 2);
  const auto& sr = set.regions()[1];
  CHECK(sr.steps.size() == 3);
  CHECK(std::string(sr.steps[1].kind) == "inherit");
  CHECK(sr.steps[1].label == "presel");
  CHECK(sr.steps[1].counts.raw == 2);
  CHECK(sr.steps[2].counts.raw == 1);
}

void test_trigger_step() {
  const char* src = "region SR\n  trigger mu_trig\n  select MET > 100\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {
      must_event("{\"MET\": {\"pt\": 150, \"phi\": 0.0}, \"triggers\": {\"mu_trig\": 1}}"),
      must_event("{\"MET\": {\"pt\": 150, \"phi\": 0.0}, \"triggers\": {\"mu_trig\": 0}}"),
      must_event("{\"MET\": {\"pt\": 50, \"phi\": 0.0}, \"triggers\": {\"mu_trig\": 1}}"),
  };
  auto set = run_cutflow(src, h, evs);
  const auto& flow = set.regions()[0];
  CHECK(std::string(flow.steps[1].kind) == "trigger");
  CHECK(flow.steps[1].label == "trigger mu_trig");
  CHECK(flow.steps[1].counts.raw == 2);
  CHECK(flow.steps[2].counts.raw == 1);
}

void test_hard_error_counts() {
  const char* src = "region SR\n  select MET > 100\n  select HT > 50\n  select MET > 200\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {must_event(met_event(150)), must_event(met_event(250)),
                            must_event(met_event(50))};
  auto set = run_cutflow(src, h, evs);
  const auto& flow = set.regions()[0];
  CHECK(flow.steps[1].counts.raw == 2);
  CHECK(flow.steps[2].counts.raw == 0);
  CHECK(flow.steps[2].errors == 2);
  CHECK(flow.steps[3].counts.raw == 0);
  CHECK(flow.steps[3].errors == 0);
}

void test_out_of_fragment_skipped() {
  const char* src =
      "region SR\n  select MET > 100\n  sort MET\n"
      "region OK\n  select MET > 100\n";
  auto h = hir_of(src);
  auto set = run_cutflow(src, h, {must_event(met_event(150))});
  CHECK(set.regions().size() == 1);
  CHECK(set.regions()[0].name == "OK");
  CHECK(set.diagnostics().size() == 1);
  CHECK(set.diagnostics()[0].find("region `SR`") != std::string::npos);
  CHECK(set.diagnostics()[0].find("cutflow skipped") != std::string::npos);
}

void test_positional_weights() {
  const char* src = "region SR\n  select MET > 10\n  weight lumi 2.0\n  select MET > 100\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {must_event(met_weighted(50, 2)), must_event(met_weighted(150, 0)),
                            must_event(met_weighted(200, 3)), must_event(met_event(5))};
  auto set = run_cutflow(src, h, evs);
  CHECK((set.total() == counts(4, 6.0, 14.0)));
  const auto& flow = set.regions()[0];
  CHECK((flow.steps[0].counts == counts(4, 6.0, 14.0)));
  CHECK(flow.steps[1].label == "select MET > 10");
  CHECK((flow.steps[1].counts == counts(3, 5.0, 13.0)));
  CHECK((flow.steps[2].counts == counts(2, 6.0, 36.0)));
  CHECK(!flow.steps[0].weighted_incomplete && !flow.steps[1].weighted_incomplete &&
        !flow.steps[2].weighted_incomplete);
}

void test_non_numeric_weight() {
  const char* src =
      "region SR\n  select MET > 10\n  weight wtab someFunc(MET)\n  select MET > 100\n";
  auto h = hir_of(src);
  auto set = run_cutflow(src, h, {must_event(met_weighted(150, 2))});
  const auto& flow = set.regions()[0];
  CHECK(!flow.steps[0].weighted_incomplete);
  CHECK(!flow.steps[1].weighted_incomplete);
  CHECK(flow.steps[2].weighted_incomplete);
  CHECK((flow.steps[2].counts == counts(1, 2.0, 4.0)));
  auto json = set.to_json(false);
  std::size_t n = 0;
  for (std::size_t p = 0; (p = json.find("\"weighted_incomplete\":true", p)) != std::string::npos;
       p += 1)
    ++n;
  CHECK(n == 1);
}

void test_boundary_bins() {
  const char* src = "region SR\n  select MET > 100\n  bin MET 200 300 500\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {must_event(met_event(50)), must_event(met_event(150)),
                            must_event(met_event(250)), must_event(met_event(350)),
                            must_event(met_event(550))};
  auto set = run_cutflow(src, h, evs);
  const auto& flow = set.regions()[0];
  CHECK(flow.bins.size() == 1);
  const auto& b = flow.bins[0];
  CHECK(b.kind == BinFlowKind::Boundary);
  CHECK(b.edges.size() == 3);
  CHECK(b.edges[0] == "200" && b.edges[1] == "300" && b.edges[2] == "500");
  CHECK(b.bins.size() == 3);
  CHECK((b.bins[0] == counts(1, 1.0, 1.0)));
  CHECK((b.bins[1] == counts(1, 1.0, 1.0)));
  CHECK((b.bins[2] == counts(1, 1.0, 1.0)));
  CHECK((b.out == counts(1, 1.0, 1.0)));
  CHECK(b.failed == 0);
}

void test_boolean_bins() {
  const char* src = "region SR\n  select MET > 100\n  bin \"hi\" MET > 300\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {must_event(met_event(150)), must_event(met_event(350)),
                            must_event(met_event(50))};
  auto set = run_cutflow(src, h, evs);
  const auto& b = set.regions()[0].bins[0];
  CHECK(b.kind == BinFlowKind::Cond);
  CHECK(b.label && *b.label == "hi");
  CHECK((b.yes == counts(1, 1.0, 1.0)));
  CHECK((b.no == counts(1, 1.0, 1.0)));
  CHECK(b.failed == 0);
}

void test_canonical_json() {
  const char* src = "region SR\n  select MET > 100\n  weight lumi 2.0\n  reject MET > 300\n";
  auto h = hir_of(src);
  std::vector<Event> evs = {must_event(met_weighted(150, 0.5)), must_event(met_event(350)),
                            must_event(met_event(50))};
  auto a = run_cutflow(src, h, evs);
  auto b = run_cutflow(src, h, evs);
  CHECK(a.to_json(true) == b.to_json(true));
  CHECK(a.to_json(false) == b.to_json(false));
  CHECK(a.text_table() == b.text_table());
  CHECK(a.to_json(false) ==
        "{\"version\":1,"
        "\"total\":{\"raw\":3,\"sumw\":2.5,\"sumw2\":2.25},"
        "\"regions\":[{\"name\":\"SR\",\"steps\":["
        "{\"kind\":\"all\",\"label\":\"all\",\"raw\":3,\"sumw\":2.5,\"sumw2\":2.25,\"errors\":0},"
        "{\"kind\":\"select\",\"label\":\"select MET > 100\",\"raw\":2,\"sumw\":1.5,\"sumw2\":1.25,"
        "\"errors\":0},"
        "{\"kind\":\"reject\",\"label\":\"reject MET > 300\",\"raw\":1,\"sumw\":1.0,\"sumw2\":1.0,"
        "\"errors\":0}"
        "],\"bins\":[]}]}");
}

void test_text_table() {
  const char* src = "region SR\n  select MET > 100\n";
  auto h = hir_of(src);
  auto set = run_cutflow(src, h, {must_event(met_event(150)), must_event(met_event(50))});
  auto table = set.text_table();
  CHECK(table.find("cutflow: SR\n") == 0);
  CHECK(table.find("sumw +- err") != std::string::npos);
  CHECK(table.find("100.00%") != std::string::npos);
  CHECK(table.find("50.00%") != std::string::npos);
  CHECK(table.find("2.0 +- 1.4142135623730951") != std::string::npos);
}

}  // namespace

int main() {
  test_json_f64();
  test_select_and_reject();
  test_inherit_one_step();
  test_trigger_step();
  test_hard_error_counts();
  test_out_of_fragment_skipped();
  test_positional_weights();
  test_non_numeric_weight();
  test_boundary_bins();
  test_boolean_bins();
  test_canonical_json();
  test_text_table();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails ? 1 : 0;
}
