#include "adl2/analysis/analysis.hpp"
#include "adl2/sema/sema.hpp"

#include <iostream>
#include <string>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::PairReport;
using adl2::analysis::Report;
using adl2::analysis::SolverChoice;
using adl2::analysis::VerdictKind;
using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::analyze_str;

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

const PairReport* find_pair(const Report& r, const char* a, const char* b) {
  for (const auto& p : r.pairwise) {
    if (p.a == a && p.b == b) return &p;
    if (p.a == b && p.b == a) return &p;
  }
  return nullptr;
}

const char* kSrc =
    "object jets\n"
    "  take Jet\n"
    "region High\n"
    "  select jets[0].pt > 100\n"
    "region Low\n"
    "  select jets[0].pt < 50\n";

void test_library_default_leaves_gates_off() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  CHECK(!r.sampling.has_value());
  CHECK(!r.refute.has_value());
  const PairReport* hl = find_pair(r, "High", "Low");
  CHECK(hl != nullptr);
  if (hl) CHECK(hl->kind == VerdictKind::ProvenDisjoint);
}

void test_gates_on_do_not_demote_true_disjoint() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.sample_gate = 64;
  opts.refute_gate = true;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  CHECK(r.sampling.has_value());
  CHECK(r.refute.has_value());
  if (r.sampling) {
    CHECK(r.sampling->events > 64);
    CHECK(r.sampling->refutations == 0);
  }
  if (r.refute) {
    CHECK(r.refute->probes > 0);
    CHECK(r.refute->refutations == 0);
  }
  const PairReport* hl = find_pair(r, "High", "Low");
  CHECK(hl != nullptr);
  if (hl) CHECK(hl->kind == VerdictKind::ProvenDisjoint);
  std::string human = r.render_default({});
  CHECK(human.find("sampling gate") != std::string::npos);
  CHECK(human.find("refute gate") != std::string::npos);
  CHECK(human.find("0 sampling · 0 adversarial") != std::string::npos);
}

void test_met_cut_constants_reach_sample_battery() {
  ExtDecls ext = ExtDecls::legacy();
  const char* src =
      "region RA\n"
      "  select MET > 0.5\n"
      "region RB\n"
      "  select MET < 100\n";
  Hir hir = analyze_str(src, "t.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.sample_gate = 8;
  opts.refute_gate = true;
  Report r = adl2::analysis::analyze_hir(hir, src, ext, opts);
  CHECK(r.sampling.has_value());
  CHECK(r.refute.has_value());
  if (r.sampling) CHECK(r.sampling->refutations == 0);
  if (r.refute) CHECK(r.refute->refutations == 0);
}

}  // namespace

int main() {
  test_library_default_leaves_gates_off();
  test_gates_on_do_not_demote_true_disjoint();
  test_met_cut_constants_reach_sample_battery();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
