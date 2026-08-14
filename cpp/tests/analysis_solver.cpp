#include "adl2/analysis/analysis.hpp"
#include "adl2/analysis/refute.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/formula/lin.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/quantity.hpp"
#include "adl2/sema/rat.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/solver/solver.hpp"

#include <chrono>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <iterator>
#include <set>
#include <string>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::CoverageStatus;
using adl2::analysis::EmptyStatus;
using adl2::analysis::MAX_REALIZED_F;
using adl2::analysis::PairReport;
using adl2::analysis::ProofPath;
using adl2::analysis::Report;
using adl2::analysis::SolverChoice;
using adl2::analysis::WITNESS_EPS;
using adl2::analysis::VerdictKind;
using adl2::analysis::apply_interval_certify_demotion;
using adl2::analysis::classify_overlap_non_sat;
using adl2::analysis::verdict_kind_human;
using adl2::analysis::verdict_kind_json;
using adl2::analysis::verdict_kind_short;
using adl2::analysis::refined_model;
using adl2::analysis::tightened;
using adl2::formula::LinAtom;
using adl2::formula::QFormula;
using adl2::formula::Rel;
using adl2::sema::AngKind;
using adl2::sema::ElemIndexKind;
using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::Quantity;
using adl2::sema::QuantityId;
using adl2::sema::QuantityKind;
using adl2::sema::Rat;
using adl2::sema::analyze_str;
using adl2::solver::QSort;
using adl2::solver::SubprocessSolver;

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

QuantityId find_jets0_pt(const Hir& hir, const ExtDecls& ext) {
  auto jets = hir.collection_of("jets");
  std::string pt = ext.prop_canon("pt").first;
  const auto& qs = hir.table.quantities();
  for (std::uint32_t i = 0; i < qs.size(); ++i) {
    const auto& q = qs[i];
    if (q.kind == QuantityKind::ElemProp && jets && q.coll == *jets &&
        q.index.kind == ElemIndexKind::FromFront && q.index.n == 0 &&
        hir.table.prop_key(q.prop) == pt) {
      return QuantityId{i};
    }
  }
  return QuantityId{0xFFFFFFFFu};
}

QuantityId find_first_dphi(const Hir& hir) {
  const auto& qs = hir.table.quantities();
  for (std::uint32_t i = 0; i < qs.size(); ++i) {
    if (qs[i].kind == QuantityKind::AngularSep && qs[i].ang == AngKind::DPhi) {
      return QuantityId{i};
    }
  }
  return QuantityId{0xFFFFFFFFu};
}

void test_short_human_vocab() {
  CHECK(std::string(verdict_kind_short(VerdictKind::ProvenDisjoint)) == "DISJOINT");
  CHECK(std::string(verdict_kind_short(VerdictKind::ProvenOverlapping)) == "OVERLAPS");
  CHECK(std::string(verdict_kind_short(VerdictKind::CandidateOverlapping)) == "NOT PROVED");
  CHECK(std::string(verdict_kind_short(VerdictKind::CandidateDisjoint)) == "NOT PROVED");
  CHECK(std::string(verdict_kind_short(VerdictKind::PossiblyOverlapping)) == "NOT PROVED");
  CHECK(std::string(verdict_kind_short(VerdictKind::Unknown)) == "NOT PROVED");
  CHECK(std::string(verdict_kind_human(VerdictKind::ProvenDisjoint)) == "PROVEN DISJOINT");
  CHECK(std::string(verdict_kind_json(VerdictKind::ProvenDisjoint)) == "proven_disjoint");
  CHECK(std::string(verdict_kind_json(VerdictKind::PossiblyOverlapping)) ==
        "possibly_overlapping");

  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::NoSolver;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  adl2::analysis::RenderOptions full;
  std::string human = r.render_default(full);
  CHECK(human.find("PROVEN DISJOINT") != std::string::npos);
  CHECK(human.find("\n  DISJOINT") == std::string::npos);
  adl2::analysis::RenderOptions brief;
  brief.short_human = true;
  std::string short_h = r.render_default(brief);
  CHECK(short_h.find("DISJOINT") != std::string::npos);
  CHECK(short_h.find("PROVEN DISJOINT") == std::string::npos);
  std::string js = r.to_json();
  CHECK(js.find("proven_disjoint") != std::string::npos);
  CHECK(js.find("\"DISJOINT\"") == std::string::npos);
  adl2::analysis::RenderOptions expl;
  expl.short_human = true;
  std::string explained = r.render_explain(expl);
  CHECK(explained.find("PROVEN DISJOINT") != std::string::npos);
}

void test_tightened_size_and_present_stay_exact() {
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "region A\n"
      "  select jets[0].pt > 0\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "tight.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  QuantityId pt = find_jets0_pt(hir, ext);
  CHECK(pt.id != 0xFFFFFFFFu);
  if (pt.id == 0xFFFFFFFFu) return;
  auto jets = hir.collection_of("jets");
  CHECK(jets.has_value());
  if (!jets) return;
  QuantityId sz = hir.table.intern_quantity(Quantity::size(*jets));
  QuantityId pres = hir.table.intern_quantity(Quantity::present(pt));

  QFormula size_le = QFormula::of_atom(LinAtom::single(sz, Rel::Le, Rat::one()));
  CHECK(tightened(hir, size_le) == size_le);

  QFormula size_gt = QFormula::of_atom(LinAtom::single(sz, Rel::Gt, Rat::zero()));
  CHECK(tightened(hir, size_gt) == size_gt);

  QFormula pge = QFormula::of_atom(LinAtom::single(pres, Rel::Ge, Rat::one()));
  CHECK(tightened(hir, pge) == pge);

  QFormula ple = QFormula::of_atom(LinAtom::single(pres, Rel::Le, Rat::one()));
  CHECK(tightened(hir, ple) == ple);

  CHECK(tightened(hir, QFormula::ttrue()) == QFormula::ttrue());
  CHECK(tightened(hir, QFormula::ffalse()) == QFormula::ffalse());
}

void test_tightened_inequality_ne_eq() {
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "region A\n"
      "  select jets[0].pt > 0\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "tight2.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  QuantityId pt = find_jets0_pt(hir, ext);
  CHECK(pt.id != 0xFFFFFFFFu);
  if (pt.id == 0xFFFFFFFFu) return;
  auto jets = hir.collection_of("jets");
  if (!jets) return;
  QuantityId sz = hir.table.intern_quantity(Quantity::size(*jets));
  auto eps = Rat::from_decimal_f64(WITNESS_EPS);
  CHECK(eps.has_value());
  if (!eps) return;

  QFormula gt = QFormula::of_atom(LinAtom::single(pt, Rel::Gt, Rat::zero()));
  QFormula t_gt = tightened(hir, gt);
  CHECK(t_gt.kind == QFormula::Kind::Atom);
  CHECK(t_gt.atom.rel() == Rel::Gt);
  CHECK(t_gt.atom.constant() == *eps);

  QFormula le = QFormula::of_atom(LinAtom::single(pt, Rel::Le, Rat::from_i64(100)));
  QFormula t_le = tightened(hir, le);
  CHECK(t_le.kind == QFormula::Kind::Atom);
  CHECK(t_le.atom.rel() == Rel::Le);
  CHECK(t_le.atom.constant() == Rat::from_i64(100) - *eps);

  QFormula eq = QFormula::of_atom(LinAtom::single(pt, Rel::Eq, Rat::from_i64(5)));
  CHECK(tightened(hir, eq) == eq);

  QFormula ne = QFormula::of_atom(LinAtom::single(pt, Rel::Ne, Rat::from_i64(5)));
  QFormula t_ne = tightened(hir, ne);
  CHECK(t_ne.kind == QFormula::Kind::Or);
  CHECK(t_ne.items.size() == 2);
  if (t_ne.items.size() == 2) {
    CHECK(t_ne.items[0].kind == QFormula::Kind::Atom);
    CHECK(t_ne.items[0].atom.rel() == Rel::Le);
    CHECK(t_ne.items[0].atom.constant() == Rat::from_i64(5) - *eps);
    CHECK(t_ne.items[1].kind == QFormula::Kind::Atom);
    CHECK(t_ne.items[1].atom.rel() == Rel::Ge);
    CHECK(t_ne.items[1].atom.constant() == Rat::from_i64(5) + *eps);
  }

  QFormula size_le = QFormula::of_atom(LinAtom::single(sz, Rel::Le, Rat::one()));
  QFormula mixed = QFormula::of_and({gt, size_le});
  QFormula t_mixed = tightened(hir, mixed);
  CHECK(t_mixed.kind == QFormula::Kind::And);
  CHECK(t_mixed.items.size() == 2);
  if (t_mixed.items.size() == 2) {
    CHECK(t_mixed.items[0] == t_gt);
    CHECK(t_mixed.items[1] == size_le);
  }
}

void test_refined_model_dphi_zero_and_size_cap() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (refined_model)\n";
    CHECK(true);
    return;
  }
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "region A\n"
      "  select jets[0].pt > 50\n"
      "  select dPhi(jets[0], jets[1]) < 3\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "refine.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  QuantityId pt = find_jets0_pt(hir, ext);
  QuantityId dphi = find_first_dphi(hir);
  CHECK(pt.id != 0xFFFFFFFFu);
  CHECK(dphi.id != 0xFFFFFFFFu);
  if (pt.id == 0xFFFFFFFFu || dphi.id == 0xFFFFFFFFu) return;
  auto jets = hir.collection_of("jets");
  CHECK(jets.has_value());
  if (!jets) return;
  QuantityId sz = hir.table.intern_quantity(Quantity::size(*jets));

  SubprocessSolver s = SubprocessSolver::z3();
  s.declare(pt, QSort::Real);
  s.declare(dphi, QSort::Real);
  s.declare(sz, QSort::Int);
  s.assert_formula(QFormula::of_atom(LinAtom::single(pt, Rel::Gt, Rat::from_i64(50))),
                   std::nullopt);
  s.assert_formula(QFormula::of_atom(LinAtom::single(dphi, Rel::Lt, Rat::from_i64(3))),
                   std::nullopt);
  auto sat = s.check(std::chrono::milliseconds{10000});
  CHECK(sat.is_sat());
  if (!sat.is_sat()) return;

  std::set<QuantityId> mentioned{pt, dphi, sz};
  auto model = refined_model(s, hir, mentioned, {}, std::chrono::milliseconds{10000}, nullptr);
  CHECK(model.has_value());
  if (!model) return;
  auto dv = model->get(dphi);
  CHECK(dv.has_value());
  if (dv) CHECK(*dv == Rat::zero());
  auto sv = model->get(sz);
  CHECK(sv.has_value());
  if (sv) {
    auto cap = Rat::from_decimal_f64(MAX_REALIZED_F);
    CHECK(cap.has_value());
    if (cap) CHECK(*sv <= *cap);
    // jets[1] requires size > 1
    CHECK(*sv > Rat::one());
  }
}

void test_refined_overlap_still_proven() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (refined overlap)\n";
    CHECK(true);
    return;
  }
  // Presence-bearing jets[0].pt + an exact size bound. Tightening Size/Present
  // would UNSAT the interior wish; the pair must still prove overlap.
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "region High\n"
      "  select size(jets) >= 1\n"
      "  select size(jets) <= 1\n"
      "  select jets[0].pt > 100\n"
      "region Mid\n"
      "  select size(jets) >= 1\n"
      "  select jets[0].pt > 30\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "refine_ov.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  Report r = adl2::analysis::analyze_hir(hir, src, ext, opts);
  const PairReport* hm = find_pair(r, "High", "Mid");
  CHECK(hm != nullptr);
  if (hm) {
    CHECK(hm->kind == VerdictKind::ProvenOverlapping);
    CHECK(hm->witness_validated.has_value() && *hm->witness_validated);
  }
}

void test_interval_certify_demotion_policy() {
  CHECK(AnalysisOptions{}.demote_uncertified_interval == false);

  PairReport pr;
  pr.a = "High";
  pr.b = "Low";
  pr.kind = VerdictKind::ProvenDisjoint;
  pr.proof_path = ProofPath::Interval;
  pr.reason = "intervals cannot intersect";
  CHECK(!apply_interval_certify_demotion(pr, false));
  CHECK(pr.kind == VerdictKind::ProvenDisjoint);
  CHECK(!pr.certified.has_value());

  CHECK(apply_interval_certify_demotion(pr, true));
  CHECK(pr.kind == VerdictKind::CandidateDisjoint);
  CHECK(pr.certified.has_value() && *pr.certified == false);
  CHECK(pr.proof_path.has_value() && *pr.proof_path == ProofPath::Interval);
  CHECK(pr.reason.find("candidate, not a claim") != std::string::npos);
  CHECK(!apply_interval_certify_demotion(pr, true));

  PairReport certified;
  certified.kind = VerdictKind::ProvenDisjoint;
  certified.proof_path = ProofPath::Interval;
  certified.certified = true;
  CHECK(!apply_interval_certify_demotion(certified, true));
  CHECK(certified.kind == VerdictKind::ProvenDisjoint);

  PairReport solver_pd;
  solver_pd.kind = VerdictKind::ProvenDisjoint;
  solver_pd.proof_path = ProofPath::SolverCore;
  CHECK(!apply_interval_certify_demotion(solver_pd, true));
  CHECK(solver_pd.kind == VerdictKind::ProvenDisjoint);

  PairReport overlap;
  overlap.kind = VerdictKind::ProvenOverlapping;
  overlap.proof_path = ProofPath::Interval;
  CHECK(!apply_interval_certify_demotion(overlap, true));
  CHECK(overlap.kind == VerdictKind::ProvenOverlapping);
}

void test_interval_certify_demotion_default_keeps_pd() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));

  AnalysisOptions off;
  off.solver = SolverChoice::NoSolver;
  off.certify = true;
  CHECK(off.demote_uncertified_interval == false);
  Report r_off = adl2::analysis::analyze_hir(hir, kSrc, ext, off);
  const PairReport* hl_off = find_pair(r_off, "High", "Low");
  CHECK(hl_off != nullptr);
  if (hl_off) {
    CHECK(hl_off->kind == VerdictKind::ProvenDisjoint);
    CHECK(hl_off->proof_path.has_value() && *hl_off->proof_path == ProofPath::Interval);
    CHECK(hl_off->certified.has_value() && *hl_off->certified);
  }

  Hir hir2 = analyze_str(kSrc, "jets.adl", ext);
  AnalysisOptions on;
  on.solver = SolverChoice::NoSolver;
  on.certify = true;
  on.demote_uncertified_interval = true;
  Report r_on = adl2::analysis::analyze_hir(hir2, kSrc, ext, on);
  const PairReport* hl_on = find_pair(r_on, "High", "Low");
  CHECK(hl_on != nullptr);
  if (hl_on) {
    CHECK(hl_on->kind == VerdictKind::ProvenDisjoint);
    CHECK(hl_on->certified.has_value() && *hl_on->certified);
  }

  std::size_t interval_pd = 0;
  std::size_t would_drop = 0;
  for (const auto& p : r_off.pairwise) {
    if (p.kind != VerdictKind::ProvenDisjoint) continue;
    if (!p.proof_path || *p.proof_path != ProofPath::Interval) continue;
    ++interval_pd;
    if (p.certified != true) ++would_drop;
  }
  CHECK(interval_pd >= 1);
  CHECK(would_drop == 0);
}

void measure_interval_certify_corpus() {
  auto root = std::filesystem::path(__FILE__).parent_path().parent_path().parent_path();
  auto examples = root / "examples";
  if (!std::filesystem::exists(examples)) {
    std::cerr << "SKIP: examples/ not found for interval-certify measure\n";
    CHECK(true);
    return;
  }
  ExtDecls ext = ExtDecls::legacy();
  AnalysisOptions opts;
  opts.solver = SolverChoice::NoSolver;
  opts.certify = true;
  opts.sample_gate = 0;
  opts.refute_gate = false;
  std::size_t files = 0;
  std::size_t skipped = 0;
  std::size_t interval_pd = 0;
  std::size_t certified = 0;
  std::size_t would_drop = 0;
  for (const auto& entry : std::filesystem::recursive_directory_iterator(examples)) {
    if (!entry.is_regular_file() || entry.path().extension() != ".adl") continue;
    std::ifstream in(entry.path());
    std::string src((std::istreambuf_iterator<char>(in)), std::istreambuf_iterator<char>());
    Hir hir = analyze_str(src, entry.path().filename().string(), ext);
    if (adl2::sema::has_errors(hir.diags)) {
      ++skipped;
      continue;
    }
    ++files;
    Report r = adl2::analysis::analyze_hir(hir, src, ext, opts);
    for (const auto& p : r.pairwise) {
      if (p.kind != VerdictKind::ProvenDisjoint) continue;
      if (!p.proof_path || *p.proof_path != ProofPath::Interval) continue;
      ++interval_pd;
      if (p.certified == true) {
        ++certified;
      } else {
        ++would_drop;
        std::cerr << "interval-uncertified: " << entry.path().lexically_relative(root).string()
                  << " " << p.a << " vs " << p.b << "\n";
      }
    }
  }
  std::cerr << "interval-certify measure (examples/**/*.adl, no-solver): files=" << files
            << " skipped=" << skipped << " interval_pd=" << interval_pd
            << " certified=" << certified << " would_drop=" << would_drop << "\n";
  CHECK(files > 0);
  CHECK(would_drop == 0);
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
  test_short_human_vocab();
  test_tightened_size_and_present_stay_exact();
  test_tightened_inequality_ne_eq();
  test_refined_model_dphi_zero_and_size_cap();
  test_refined_overlap_still_proven();
  test_interval_certify_demotion_policy();
  test_interval_certify_demotion_default_keeps_pd();
  measure_interval_certify_corpus();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
