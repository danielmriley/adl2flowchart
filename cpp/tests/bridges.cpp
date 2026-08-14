#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>
#include <vector>

using namespace adl2::sema;
using adl2::interp::Event;
using adl2::interp::HistoSet;
using adl2::interp::Interp;
using adl2::interp::csv_files;
using adl2::interp::file_stem;
using adl2::interp::make_histos_c;
using adl2::interp::object_path;
using adl2::interp::root_name;
using adl2::interp::svg_files;
using adl2::interp::to_root_py;

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

std::string read_all(const std::filesystem::path& p) {
  std::ifstream in(p);
  return std::string((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
}

std::string insta_body(const std::filesystem::path& p) {
  std::string all = read_all(p);
  auto first = all.find("---\n");
  if (first == std::string::npos) return all;
  auto second = all.find("---\n", first + 4);
  if (second == std::string::npos) return all;
  return all.substr(second + 4);
}

std::filesystem::path repo_root() {
  return std::filesystem::path(__FILE__).parent_path().parent_path().parent_path();
}

Hir hir_of(const char* src) {
  auto h = analyze_str(src, "test.adl", ExtDecls::legacy());
  if (has_errors(h.diags)) {
    std::cerr << "unexpected resolve errors\n";
    std::abort();
  }
  return h;
}

HistoSet run_histos(const Hir& h, const std::string& jsonl) {
  auto ext = ExtDecls::legacy();
  std::vector<Event> events;
  adl2::interp::EventError err;
  if (!adl2::interp::read_jsonl(jsonl, ext, events, err)) {
    std::cerr << "read_jsonl failed: " << err.to_string() << "\n";
    std::abort();
  }
  Interp interp(h, ext);
  auto set = HistoSet::make(h);
  for (const auto& ev : events) {
    auto results = interp.run_event(ev);
    set.fill_event(interp, ev, results);
  }
  return set;
}

void test_names() {
  CHECK(root_name("SR", "hmet") == "SR_hmet");
  CHECK(root_name("a/b", "h") == "a_b_h");
  CHECK(object_path("SR", "hmet", true) == "SR_hmet");
  CHECK(object_path("SR", "hmet", false) == "SR/hmet");
  CHECK(object_path("a/b", "h", false) == "a_b/h");
  CHECK(file_stem("SR", "h met") == "SR_h_met");
}

void test_flow_bins() {
  const char* src =
      "region SR\n  select MET > 10\n  weight lumi 2.0\n  histo hmet, \"met\", 2, 0, 100, MET\n";
  const char* jsonl =
      "{\"MET\": {\"pt\": 25, \"phi\": 0.0}}\n"
      "{\"MET\": {\"pt\": 75, \"phi\": 0.0}}\n"
      "{\"MET\": {\"pt\": 5, \"phi\": 0.0}}\n"
      "{\"MET\": {\"pt\": 250, \"phi\": 0.0}}\n";
  auto h = hir_of(src);
  auto set = run_histos(h, jsonl);
  std::string c = make_histos_c(set, false);
  CHECK(c.find("f->mkdir(\"SR\");") != std::string::npos);
  CHECK(c.find("f->cd(\"SR\");") != std::string::npos);
  CHECK(c.find("new TH1D(\"hmet\"") != std::string::npos);
  CHECK(c.find("h->SetBinContent(3, 2.0);") != std::string::npos);
  CHECK(c.find("h->SetBinError(1, 2.0);") != std::string::npos);
  CHECK(c.find("h->SetEntries(3);") != std::string::npos);
  CHECK(c.find("Double_t stats[4] = {4.0, 8.0, 200.0, 12500.0};") != std::string::npos);

  auto svgs = svg_files(set);
  CHECK(svgs.size() == 1);
  CHECK(svgs[0].first == "SR_hmet.svg");
  CHECK(svgs[0].second.find("overflow=2") != std::string::npos);

  std::string flat = make_histos_c(set, true);
  CHECK(flat.find("new TH1D(\"SR_hmet\"") != std::string::npos);
  CHECK(flat.find("f->mkdir") == std::string::npos);
}

void test_ex02_snapshots() {
  auto root = repo_root();
  auto adl_path = root / "examples/tutorials/ex02_histograms.adl";
  auto events_path = root / "cpp/tests/fixtures/ex02_events.jsonl";
  auto snap = root / "cpp/tests/fixtures/bridges";
  if (!std::filesystem::exists(adl_path) || !std::filesystem::exists(events_path)) {
    std::cerr << "SKIP: ex02 fixtures not found\n";
    CHECK(true);
    return;
  }
  std::string src = read_all(adl_path);
  std::string jsonl = read_all(events_path);
  auto h = analyze_str(src, "ex02_histograms.adl", ExtDecls::legacy());
  CHECK(!has_errors(h.diags));
  auto set = run_histos(h, jsonl);

  auto csvs = csv_files(set);
  auto svgs = svg_files(set);
  auto find_file = [](const std::vector<std::pair<std::string, std::string>>& files,
                      const std::string& name) -> std::string {
    for (const auto& f : files) {
      if (f.first == name) return f.second;
    }
    return {};
  };

  std::string want_csv = insta_body(snap / "cli__bridges_ex02_csv_hnjets.snap");
  std::string got_csv = find_file(csvs, "baseline_hnjets.csv");
  CHECK(!got_csv.empty());
  CHECK(got_csv == want_csv);

  std::string want_eta = insta_body(snap / "cli__bridges_ex02_csv_hjet1eta.snap");
  CHECK(find_file(csvs, "baseline_hjet1eta.csv") == want_eta);

  std::string want_svg = insta_body(snap / "cli__bridges_ex02_svg_hnjets.snap");
  CHECK(find_file(svgs, "baseline_hnjets.svg") == want_svg);

  std::string want_c = insta_body(snap / "cli__bridges_ex02_make_histos_c.snap");
  CHECK(make_histos_c(set, false) == want_c);

  std::string want_py = insta_body(snap / "cli__bridges_ex02_to_root_py.snap");
  CHECK(to_root_py(set, false) == want_py);
}

}  // namespace

int main() {
  test_names();
  test_flow_bins();
  test_ex02_snapshots();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails ? 1 : 0;
}
