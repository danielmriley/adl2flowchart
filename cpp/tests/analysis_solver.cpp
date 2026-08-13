#include "adl2/analysis/analysis.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/solver/solver.hpp"

#include <iostream>
#include <set>
#include <string>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::EmptyStatus;
using adl2::analysis::PairReport;
using adl2::analysis::Report;
using adl2::analysis::SolverChoice;
using adl2::analysis::VerdictKind;
using adl2::analysis::classify_overlap_non_sat;
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

void test_overlap_non_sat_taxonomy() {
  using adl2::solver::SatResult;
  PairReport pr;

  classify_overlap_non_sat(pr, SatResult::unknown("timeout"), SatResult::unknown("timeout"));
  CHECK(pr.kind == VerdictKind::Unknown);
  CHECK(pr.reason.find("both directions") != std::string::npos);
  CHECK(pr.kind != VerdictKind::ProvenDisjoint);
  CHECK(pr.kind != VerdictKind::ProvenOverlapping);

  classify_overlap_non_sat(pr, SatResult::unknown("timeout"), SatResult::sat());
  CHECK(pr.kind == VerdictKind::PossiblyOverlapping);
  CHECK(pr.reason.find("SAT direction") != std::string::npos);
  CHECK(pr.kind != VerdictKind::Unknown);

  classify_overlap_non_sat(pr, SatResult::unknown("timeout"), SatResult::unsat());
  CHECK(pr.kind == VerdictKind::PossiblyOverlapping);
  CHECK(pr.kind != VerdictKind::Unknown);

  classify_overlap_non_sat(pr, SatResult::unsat(), SatResult::sat());
  CHECK(pr.kind == VerdictKind::PossiblyOverlapping);
  CHECK(pr.reason.find("encoding gap") != std::string::npos);

  classify_overlap_non_sat(pr, SatResult::sat(), SatResult::sat());
  CHECK(pr.kind == VerdictKind::PossiblyOverlapping);
  CHECK(pr.kind != VerdictKind::ProvenOverlapping);
}

const char* kAngularTwins =
    "object eles\n"
    "  take Ele\n"
    "object muons\n"
    "  take Muo\n"
    "region SR_a\n"
    "  select size(eles) >= 1\n"
    "  select size(muons) >= 1\n"
    "  select dEta(eles[0], muons[0]) > 1\n"
    "region SR_b\n"
    "  select size(eles) >= 1\n"
    "  select size(muons) >= 1\n"
    "  select dEta(muons[0], eles[0]) > 1\n";

void test_oriented_twins_cap_sat_direction() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kAngularTwins, "angular_order.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));

  std::set<adl2::sema::QuantityId> qs;
  for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) {
    qs.insert(adl2::sema::QuantityId{i});
  }
  auto twins = adl2::axioms::twin_pairs(hir.table, qs);
  CHECK(!twins.empty());

  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (OPEN-2 twin cap)\n";
    CHECK(true);
    return;
  }
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  opts.certify = true;
  Report r = adl2::analysis::analyze_hir(hir, kAngularTwins, ext, opts);
  const PairReport* p = find_pair(r, "SR_a", "SR_b");
  CHECK(p != nullptr);
  if (p) {
    CHECK(p->kind != VerdictKind::ProvenDisjoint);
    CHECK(p->kind != VerdictKind::ProvenOverlapping);
    CHECK(p->kind == VerdictKind::PossiblyOverlapping);
    CHECK(p->reason.find("OPEN-2") != std::string::npos ||
          p->reason.find("twin") != std::string::npos);
  }
}

void test_integrality_only_is_candidate_not_proven() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (integrality candidate)\n";
    CHECK(true);
    return;
  }
  // size>1 ∧ size<2 is int-empty but real-feasible at 1.5. Certify-on must
  // not sell that as PROVEN (smash2 certification_tiers_*).
  const char* pair_src =
      "object jets\n"
      "  take Jet\n"
      "region RA\n"
      "  select size(jets) > 1\n"
      "region RB\n"
      "  select size(jets) < 2\n";
  const char* empty_src =
      "object jets\n"
      "  take Jet\n"
      "region DEAD\n"
      "  select size(jets) > 1\n"
      "  select size(jets) < 2\n";
  ExtDecls ext = ExtDecls::legacy();

  AnalysisOptions on;
  on.solver = SolverChoice::SubprocessZ3;
  on.certify = true;
  Hir hir = analyze_str(pair_src, "i.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  Report r = adl2::analysis::analyze_hir(hir, pair_src, ext, on);
  const PairReport* p = find_pair(r, "RA", "RB");
  CHECK(p != nullptr);
  if (p) {
    CHECK(p->kind == VerdictKind::CandidateDisjoint);
    CHECK(p->certified.has_value() && *p->certified == false);
    CHECK(p->kind != VerdictKind::ProvenDisjoint);
    CHECK(p->kind != VerdictKind::ProvenOverlapping);
  }

  AnalysisOptions off;
  off.solver = SolverChoice::SubprocessZ3;
  off.certify = false;
  Hir hir_off = analyze_str(pair_src, "i.adl", ext);
  Report r_off = adl2::analysis::analyze_hir(hir_off, pair_src, ext, off);
  const PairReport* p_off = find_pair(r_off, "RA", "RB");
  CHECK(p_off != nullptr);
  if (p_off) {
    CHECK(p_off->kind == VerdictKind::ProvenDisjoint);
    CHECK(!p_off->certified.has_value());
  }

  Hir dead = analyze_str(empty_src, "empty_i.adl", ext);
  CHECK(!adl2::sema::has_errors(dead.diags));
  Report r_dead = adl2::analysis::analyze_hir(dead, empty_src, ext, on);
  CHECK(!r_dead.regions.empty());
  if (!r_dead.regions.empty()) {
    CHECK(r_dead.regions[0].name == "DEAD");
    CHECK(r_dead.regions[0].empty == EmptyStatus::Candidate);
  }
  Hir dead_off = analyze_str(empty_src, "empty_i.adl", ext);
  Report r_dead_off = adl2::analysis::analyze_hir(dead_off, empty_src, ext, off);
  if (!r_dead_off.regions.empty()) {
    CHECK(r_dead_off.regions[0].empty == EmptyStatus::Proven);
  }
}

}  // namespace

int main() {
  test_solver_disjoint_and_overlap();
  test_certify_interval_pair();
  test_multi_statement_subset_is_or_of_negations();
  test_no_solver_caps_overlap_at_possibly();
  test_certify_on_keeps_single_cut_subset();
  test_overlap_non_sat_taxonomy();
  test_oriented_twins_cap_sat_direction();
  test_integrality_only_is_candidate_not_proven();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
