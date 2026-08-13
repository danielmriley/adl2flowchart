#include "adl2/analysis/analysis.hpp"
#include "adl2/analysis/refute.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/solver/solver.hpp"

#include <iostream>
#include <set>
#include <string>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::CoverageStatus;
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
    CHECK(!hm->witness.empty());
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

void test_validated_witness_rows_from_event() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (witness rows)\n";
    CHECK(true);
    return;
  }
  // Review F2: pT-sort can permute model indices. Displayed rows must come
  // from the validated event, so a pt>100 jet in the event makes JET[0].pt > 100.
  const char* src =
      "object bigjets\n"
      "  take Jet\n"
      "  select pt > 100\n"
      "region RA\n"
      "  select size(Jet) >= 2\n"
      "  select pT(Jet[0]) > 20\n"
      "region RB\n"
      "  select size(Jet) >= 2\n"
      "  select size(bigjets) >= 1\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "f2.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  Report r = adl2::analysis::analyze_hir(hir, src, ext, opts);
  CHECK(r.pairwise.size() == 1);
  if (r.pairwise.empty()) return;
  const PairReport& p = r.pairwise[0];
  CHECK(p.kind == VerdictKind::ProvenOverlapping);
  CHECK(p.witness_validated.has_value() && *p.witness_validated);
  CHECK(!p.witness.empty());
  bool found_lead = false;
  for (const auto& w : p.witness) {
    if (w.quantity == "JET[0].pt") {
      found_lead = true;
      CHECK(w.value > 100.0);
    }
  }
  CHECK(found_lead);
}

void test_bin_partition_and_gap() {
  const char* src =
      "object MET\n"
      "  take MissingET\n"
      "region SR_binned\n"
      "  select MET.pT > 250\n"
      "  bin MET 250 300 500\n"
      "region SR_gap\n"
      "  select MET.pT > 250\n"
      "  bin MET 300 500\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "bins_partition.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));

  AnalysisOptions none;
  none.solver = SolverChoice::NoSolver;
  Report r_none = adl2::analysis::analyze_hir(hir, src, ext, none);
  CHECK(r_none.bin_checks.size() == 2);
  // No-solver coverage is Unknown; interval can still prove adjacent bins disjoint.
  bool saw_binned = false;
  bool saw_gap = false;
  for (const auto& b : r_none.bin_checks) {
    if (b.region == "SR_binned") {
      saw_binned = true;
      CHECK(b.n_bins == 3);
      CHECK(b.disjoint_pairs_total == 3);
      CHECK(b.coverage == CoverageStatus::Unknown);
    }
    if (b.region == "SR_gap") {
      saw_gap = true;
      CHECK(b.n_bins == 2);
      CHECK(b.disjoint_pairs_total == 1);
      CHECK(b.coverage == CoverageStatus::Unknown);
    }
  }
  CHECK(saw_binned);
  CHECK(saw_gap);

  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (bin coverage)\n";
    CHECK(true);
    return;
  }
  Hir hir2 = analyze_str(src, "bins_partition.adl", ext);
  AnalysisOptions on;
  on.solver = SolverChoice::SubprocessZ3;
  Report r = adl2::analysis::analyze_hir(hir2, src, ext, on);
  const adl2::analysis::BinCheckReport* binned = nullptr;
  const adl2::analysis::BinCheckReport* gap = nullptr;
  for (const auto& b : r.bin_checks) {
    if (b.region == "SR_binned") binned = &b;
    if (b.region == "SR_gap") gap = &b;
  }
  CHECK(binned != nullptr);
  if (binned) {
    CHECK(binned->n_bins == 3);
    CHECK(binned->disjoint_pairs_proven == 3);
    CHECK(binned->disjoint_pairs_total == 3);
    CHECK(binned->coverage == CoverageStatus::Proven);
  }
  CHECK(gap != nullptr);
  if (gap) {
    CHECK(gap->n_bins == 2);
    CHECK(gap->disjoint_pairs_proven == 1);
    CHECK(gap->disjoint_pairs_total == 1);
    CHECK(gap->coverage == CoverageStatus::NotProven);
    CHECK(!gap->gap_witness.empty());
  }
  std::string explain = r.render_explain({});
  CHECK(explain.find("SR_binned [MET]: 3 bins; disjoint 3/3 pairs; coverage: proven") !=
        std::string::npos);
  CHECK(explain.find("SR_gap [MET]: 2 bins; disjoint 1/1 pairs; coverage: not proven") !=
        std::string::npos);
}

const char* kSizeHardFilter =
    "object jets\n"
    "  take Jet\n"
    "object weird\n"
    "  take Jet\n"
    "  select bdt > 0.5\n"
    "region A\n"
    "  select size(jets) >= 1\n"
    "region B\n"
    "  select size(weird) >= 0\n";

void test_size_hard_filter_is_not_a_subset_of_tautology() {
  // SOUNDNESS_PROOF §8 1b: size(weird) is a hard error (undeclared `bdt`), so
  // B is In for no event that has a jet. A ⊆ B must not be claimed.
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSizeHardFilter, "size_hard.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));

  adl2::interp::Interp interp(hir, ext);
  adl2::interp::EventError ev_err;
  auto ev = adl2::interp::parse_event(
      R"({"Jet":[{"pt":80,"eta":0,"phi":0,"m":1}],"MET":{"pt":10,"phi":0}})", ext, ev_err);
  CHECK(ev.has_value());
  if (!ev) return;
  adl2::interp::EvalError mem_err;
  auto in_a = interp.eval_region_membership_idx(0, *ev, mem_err);
  auto in_b = interp.eval_region_membership_idx(1, *ev, mem_err);
  CHECK(in_a == true);
  CHECK(!in_b.has_value());

  auto cex = adl2::analysis::search_subset_counterexample(interp, 0, 1, {*ev});
  CHECK(cex.has_value());

  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (size-hard-filter subset)\n";
    CHECK(true);
    return;
  }
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  opts.certify = true;
  opts.sample_gate = 64;
  opts.refute_gate = true;
  Report r = adl2::analysis::analyze_hir(hir, kSizeHardFilter, ext, opts);
  const PairReport* p = find_pair(r, "A", "B");
  CHECK(p != nullptr);
  if (p) {
    CHECK(!subset_named(p, "A", "B"));
    CHECK(!subset_named(p, "B", "A"));
  }
}

void test_reject_size_hard_filter_is_not_a_subset() {
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "object weird\n"
      "  take Jet\n"
      "  select bdt > 0.5\n"
      "region A\n"
      "  select size(jets) >= 1\n"
      "region B\n"
      "  reject size(weird) < 0\n";
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (reject size-hard-filter subset)\n";
    CHECK(true);
    return;
  }
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "size_hard_reject.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  opts.certify = true;
  Report r = adl2::analysis::analyze_hir(hir, src, ext, opts);
  const PairReport* p = find_pair(r, "A", "B");
  CHECK(p != nullptr);
  if (p) CHECK(!subset_named(p, "A", "B"));
}

void test_solver_core_reason_names_source_spans() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (solver core reason)\n";
    CHECK(true);
    return;
  }
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "object tight\n"
      "  take Jet\n"
      "  select pt > 100\n"
      "region A\n"
      "  select size(tight) >= 1\n"
      "region B\n"
      "  select size(jets) == 0\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "core_span.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  opts.certify = true;
  Report r = adl2::analysis::analyze_hir(hir, src, ext, opts);
  const PairReport* p = find_pair(r, "A", "B");
  CHECK(p != nullptr);
  if (!p) return;
  CHECK(p->kind == VerdictKind::ProvenDisjoint);
  CHECK(p->proof_path.has_value() && *p->proof_path == adl2::analysis::ProofPath::SolverCore);
  CHECK(p->reason.find("UNSAT core:") == 0);
  CHECK(p->reason.find("line ") != std::string::npos);
  std::string def = r.render_default({});
  CHECK(def.find("core:") != std::string::npos);
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
  test_validated_witness_rows_from_event();
  test_bin_partition_and_gap();
  test_size_hard_filter_is_not_a_subset_of_tautology();
  test_reject_size_hard_filter_is_not_a_subset();
  test_solver_core_reason_names_source_spans();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
