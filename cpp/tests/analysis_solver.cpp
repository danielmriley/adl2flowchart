#include "adl2/analysis/analysis.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/solver/solver.hpp"

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

bool subset_named(const PairReport* p, const char* sub, const char* sup) {
  if (!p) return false;
  if (p->a == sub && p->b == sup) return p->subset_a_in_b;
  if (p->a == sup && p->b == sub) return p->subset_b_in_a;
  return false;
}

const char* kSrc =
    "object jets\n"
    "  take Jet\n"
    "region High\n"
    "  select jets[0].pt > 100\n"
    "region Low\n"
    "  select jets[0].pt < 50\n"
    "region Mid\n"
    "  select jets[0].pt > 30\n";

void test_solver_disjoint_and_overlap() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (analysis solver path)\n";
    CHECK(true);
    return;
  }
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  CHECK(r.solver == "smtlib-subprocess");

  const PairReport* hl = find_pair(r, "High", "Low");
  CHECK(hl != nullptr);
  if (hl) {
    CHECK(hl->kind == VerdictKind::ProvenDisjoint);
    CHECK(hl->proof_path.has_value());
    CHECK(!hl->certified.has_value());  // library default certify=false
  }

  const PairReport* hm = find_pair(r, "High", "Mid");
  CHECK(hm != nullptr);
  if (hm) {
    CHECK(hm->kind != VerdictKind::ProvenDisjoint);
    CHECK(hm->kind == VerdictKind::ProvenOverlapping);
    CHECK(hm->witness_validated.has_value() && *hm->witness_validated);
    CHECK(subset_named(hm, "High", "Mid"));
    CHECK(!subset_named(hm, "Mid", "High"));
  }
}

void test_multi_statement_subset_is_or_of_negations() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (subset polarity)\n";
    CHECK(true);
    return;
  }
  // Tight ⊈ Loose: a jet with pt=100, |eta|=3 is in Tight and not Loose.
  // AND-of-¬unders (the old C++ query) made that UNSAT and claimed subset.
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "region Tight\n"
      "  select jets[0].pt > 50\n"
      "region Loose\n"
      "  select jets[0].pt > 40\n"
      "  select jets[0].eta < 2.5\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "tight_loose.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));

  AnalysisOptions off;
  off.solver = SolverChoice::SubprocessZ3;
  Report r = adl2::analysis::analyze_hir(hir, src, ext, off);
  const PairReport* p = find_pair(r, "Tight", "Loose");
  CHECK(p != nullptr);
  if (p) {
    CHECK(!subset_named(p, "Tight", "Loose"));
    CHECK(!subset_named(p, "Loose", "Tight"));
    CHECK(p->kind != VerdictKind::ProvenDisjoint);
  }

  AnalysisOptions on;
  on.solver = SolverChoice::SubprocessZ3;
  on.certify = true;
  Hir hir2 = analyze_str(src, "tight_loose.adl", ext);
  Report r2 = adl2::analysis::analyze_hir(hir2, src, ext, on);
  const PairReport* p2 = find_pair(r2, "Tight", "Loose");
  CHECK(p2 != nullptr);
  if (p2) {
    CHECK(!subset_named(p2, "Tight", "Loose"));
    CHECK(!subset_named(p2, "Loose", "Tight"));
  }
}

void test_no_solver_caps_overlap_at_possibly() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::NoSolver;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  const PairReport* hm = find_pair(r, "High", "Mid");
  CHECK(hm != nullptr);
  if (hm) {
    CHECK(hm->kind == VerdictKind::PossiblyOverlapping);
    CHECK(!hm->subset_a_in_b && !hm->subset_b_in_a);
  }
  const PairReport* hl = find_pair(r, "High", "Low");
  CHECK(hl != nullptr);
  if (hl) {
    CHECK(hl->kind == VerdictKind::ProvenDisjoint);
    CHECK(hl->proof_path.has_value());
  }
}

void test_certify_on_keeps_single_cut_subset() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (certified subset)\n";
    CHECK(true);
    return;
  }
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  opts.certify = true;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  const PairReport* hm = find_pair(r, "High", "Mid");
  CHECK(hm != nullptr);
  if (hm) {
    CHECK(subset_named(hm, "High", "Mid"));
    CHECK(!subset_named(hm, "Mid", "High"));
    CHECK(hm->kind == VerdictKind::ProvenOverlapping);
  }
  const PairReport* hl = find_pair(r, "High", "Low");
  CHECK(hl != nullptr);
  if (hl) {
    CHECK(hl->kind == VerdictKind::ProvenDisjoint);
    CHECK(hl->certified.has_value() && *hl->certified);
  }
}

void test_certify_interval_pair() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (certify path)\n";
    CHECK(true);
    return;
  }
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  opts.certify = true;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  CHECK(r.certification);
  const PairReport* hl = find_pair(r, "High", "Low");
  CHECK(hl != nullptr);
  if (hl) {
    CHECK(hl->kind == VerdictKind::ProvenDisjoint);
    CHECK(hl->certified.has_value() && *hl->certified);
    CHECK(hl->certificate_size.has_value());
  }
  const PairReport* hm = find_pair(r, "High", "Mid");
  CHECK(hm != nullptr);
  if (hm) {
    CHECK(hm->kind == VerdictKind::ProvenOverlapping);
  }
  std::string human = r.render_default(adl2::analysis::RenderOptions{});
  CHECK(human.find("== trust ==") != std::string::npos);
  CHECK(human.find("certification on") != std::string::npos);
  CHECK(human.find("PROVEN DISJOINT") != std::string::npos);
}

}  // namespace

int main() {
  test_solver_disjoint_and_overlap();
  test_certify_interval_pair();
  test_multi_statement_subset_is_or_of_negations();
  test_no_solver_caps_overlap_at_possibly();
  test_certify_on_keeps_single_cut_subset();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
