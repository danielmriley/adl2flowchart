#include "adl2/analysis/analysis.hpp"
#include "adl2/axioms/axioms.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/solver/solver.hpp"

#include <iostream>
#include <string>
#include <vector>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::PairReport;
using adl2::analysis::ReconOutcome;
using adl2::analysis::Report;
using adl2::analysis::SolverChoice;
using adl2::analysis::VerdictKind;
using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::analyze_str;
using adl2::sema::merge_hirs;

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

const PairReport* find_pair(const Report& r, const char* x, const char* y) {
  for (const auto& p : r.pairwise) {
    if ((p.a.find(x) != std::string::npos && p.b.find(y) != std::string::npos) ||
        (p.a.find(y) != std::string::npos && p.b.find(x) != std::string::npos)) {
      return &p;
    }
  }
  return nullptr;
}

Hir resolve(const char* src, const char* unit, const ExtDecls& ext) {
  auto h = analyze_str(src, unit, ext);
  if (adl2::sema::has_errors(h.diags)) {
    std::cerr << "resolve errors in " << unit << "\n";
  }
  return h;
}

Report cross(const std::vector<std::pair<const char*, const char*>>& units, bool reconcile) {
  ExtDecls ext = ExtDecls::legacy();
  std::vector<Hir> hirs;
  hirs.reserve(units.size());
  for (const auto& u : units) hirs.push_back(resolve(u.second, u.first, ext));
  std::vector<const Hir*> refs;
  for (const auto& h : hirs) refs.push_back(&h);
  Hir merged = merge_hirs(refs);
  AnalysisOptions opts;
  opts.solver = SolverChoice::Auto;
  opts.reconcile = reconcile;
  opts.certify = true;
  opts.sample_gate = 64;
  opts.refute_gate = true;
  return adl2::analysis::analyze_hir(merged, "", ext, opts);
}

void test_derived_size_le_shape() {
  using adl2::formula::QFormula;
  using adl2::formula::Rel;
  using adl2::sema::QuantityId;
  auto f = adl2::axioms::derived_size_le(QuantityId{3}, QuantityId{9});
  CHECK(f.kind == QFormula::Kind::Atom);
  CHECK(f.atom.rel() == Rel::Le);
  CHECK(f.atom.terms().size() == 2);
  CHECK(adl2::axioms::GENERIC_INDEX > adl2::sema::MAX_SOURCE_ELEM_INDEX);
}

void test_cross_shared_met() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (cross shared MET)\n";
    CHECK(true);
    return;
  }
  auto r = cross({{"hi", "region SRhi\n  select MET.pt > 200\n"},
                  {"lo", "region SRlo\n  select MET.pt > 100\n"},
                  {"veto", "region SRveto\n  select MET.pt < 50\n"}},
                 false);
  const PairReport* hi_lo = find_pair(r, "SRhi", "SRlo");
  CHECK(hi_lo != nullptr);
  if (hi_lo) {
    CHECK(hi_lo->kind == VerdictKind::ProvenOverlapping);
    bool hi_in_lo = (hi_lo->a.find("SRhi") != std::string::npos && hi_lo->subset_a_in_b) ||
                    (hi_lo->b.find("SRhi") != std::string::npos && hi_lo->subset_b_in_a);
    CHECK(hi_in_lo);
  }
  const PairReport* hi_veto = find_pair(r, "SRhi", "SRveto");
  CHECK(hi_veto != nullptr);
  if (hi_veto) CHECK(hi_veto->kind == VerdictKind::ProvenDisjoint);
}

void test_cross_does_not_falsely_unify_different_cuts() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (cross distinct cuts)\n";
    CHECK(true);
    return;
  }
  auto r = cross(
      {{"d",
        "object goodjets\n  take Jet\n  select pt > 30\nregion Rd\n  select size(goodjets) >= 2\n"},
       {"e",
        "object goodjets\n  take Jet\n  select pt > 100\nregion Re\n  select size(goodjets) <= 1\n"}},
      false);
  const PairReport* p = find_pair(r, "Rd", "Re");
  CHECK(p != nullptr);
  if (p) CHECK(p->kind != VerdictKind::ProvenDisjoint);

  auto r2 = cross(
      {{"d",
        "object goodjets\n  take Jet\n  select pt > 30\nregion Rd\n  select size(goodjets) >= 2\n"},
       {"f",
        "object goodjets\n  take Jet\n  select pt > 30\nregion Rf\n  select size(goodjets) <= 1\n"}},
      false);
  const PairReport* p2 = find_pair(r2, "Rd", "Rf");
  CHECK(p2 != nullptr);
  if (p2) CHECK(p2->kind == VerdictKind::ProvenDisjoint);
}

void test_reconcile_unlocks_disjoint() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (reconcile keystone)\n";
    CHECK(true);
    return;
  }
  std::vector<std::pair<const char*, const char*>> units = {
      {"a", "object jets\n  take Jet\n  select pt > 100\nregion RA\n  select size(jets) >= 3\n"},
      {"b", "object jets\n  take Jet\n  select pt > 30\nregion RB\n  select size(jets) <= 2\n"},
  };
  auto off = cross(units, false);
  const PairReport* p_off = find_pair(off, "RA", "RB");
  CHECK(p_off != nullptr);
  if (p_off) CHECK(p_off->kind == VerdictKind::PossiblyOverlapping);

  auto on = cross(units, true);
  const PairReport* p_on = find_pair(on, "RA", "RB");
  CHECK(p_on != nullptr);
  if (p_on) CHECK(p_on->kind == VerdictKind::ProvenDisjoint);
}

void test_reconcile_is_directional() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (reconcile direction)\n";
    CHECK(true);
    return;
  }
  auto r = cross(
      {{"a", "object jets\n  take Jet\n  select pt > 100\nregion RA\n  select size(jets) <= 2\n"},
       {"b", "object jets\n  take Jet\n  select pt > 30\nregion RB\n  select size(jets) >= 3\n"}},
      true);
  const PairReport* p = find_pair(r, "RA", "RB");
  CHECK(p != nullptr);
  if (p) CHECK(p->kind != VerdictKind::ProvenDisjoint);
}

void test_reconcile_opaque_superset_fails_closed() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (opaque superset)\n";
    CHECK(true);
    return;
  }
  auto r = cross({{"a",
                   "object jets\n  take Jet\n  select pt > 100\nregion RA\n  select size(jets) >= 3\n"},
                  {"b",
                   "object jets\n  take Jet\n  select pt > 30\n  select btag == 1\nregion RB\n  "
                   "select size(jets) <= 2\n"}},
                 true);
  const PairReport* p = find_pair(r, "RA", "RB");
  CHECK(p != nullptr);
  if (p) CHECK(p->kind != VerdictKind::ProvenDisjoint);
}

void test_reconcile_skips_private_base() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (private base)\n";
    CHECK(true);
    return;
  }
  auto r = cross({{"a",
                   "object cleanjets\n  take PuppiJet\n  select pt > 30\nregion SRA\n  select "
                   "size(cleanjets) >= 1\n"},
                  {"b",
                   "object hardjets\n  take PuppiJet\n  select pt > 30\n  select pt < 200\nregion "
                   "SRB\n  select size(hardjets) >= 1\n"}},
                 true);
  const PairReport* p = find_pair(r, "SRA", "SRB");
  CHECK(p != nullptr);
  if (p) {
    CHECK(!p->subset_a_in_b && !p->subset_b_in_a);
    CHECK(p->kind != VerdictKind::ProvenDisjoint);
  }
  auto r2 = cross({{"a",
                    "object cleanjets\n  take Jet\n  select pt > 30\nregion SRA\n  select "
                    "size(cleanjets) >= 1\n"},
                   {"b",
                    "object hardjets\n  take Jet\n  select pt > 30\n  select pt < 200\nregion "
                    "SRB\n  select size(hardjets) >= 1\n"}},
                  true);
  const PairReport* p2 = find_pair(r2, "SRA", "SRB");
  CHECK(p2 != nullptr);
  if (p2) CHECK(p2->subset_a_in_b || p2->subset_b_in_a);
}

void test_reconcile_xeq_and_ledger() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (XEQ ledger)\n";
    CHECK(true);
    return;
  }
  auto r = cross({{"a",
                   "object jets\n  take Jet\n  select pt > 30\n"
                   "region RA1\n  select size(jets) >= 3\nregion RA2\n  select size(jets) <= 2\n"},
                  {"b",
                   "object jets\n  take Jet\n  select pt > 20\n  select pt > 30\n"
                   "region RB1\n  select size(jets) <= 2\nregion RB2\n  select size(jets) >= 3\n"}},
                 true);
  const PairReport* p1 = find_pair(r, "RA1", "RB1");
  const PairReport* p2 = find_pair(r, "RA2", "RB2");
  CHECK(p1 != nullptr && p2 != nullptr);
  if (p1) CHECK(p1->kind == VerdictKind::ProvenDisjoint);
  if (p2) CHECK(p2->kind == VerdictKind::ProvenDisjoint);
  bool xeq = false;
  for (const auto& a : r.axioms_used) {
    if (a.id == "XEQ") {
      xeq = true;
      CHECK(a.instances == 2);
    }
  }
  CHECK(xeq);
  std::string human = r.render_default({});
  CHECK(human.find("== collection reconciliation ==") != std::string::npos);
  CHECK(human.find("cross-file:") != std::string::npos);
}

void test_jet_vs_electron_never_advised() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (jet vs electron)\n";
    CHECK(true);
    return;
  }
  auto r = cross({{"a", "object jx\n  take Jet\n  select pt > 40\nregion RA\n  select size(jx) >= 3\n"},
                  {"b",
                   "object ex\n  take Electron\n  select pt > 40\nregion RB\n  select size(ex) <= 1\n"}},
                 true);
  CHECK(r.recon_near_misses.empty());
}

void test_single_file_has_no_ledger() {
  ExtDecls ext = ExtDecls::legacy();
  const char* src =
      "object jets\n  take Jet\n  select pt > 30\nregion R\n  select size(jets) >= 1\n";
  Hir hir = analyze_str(src, "u", ext);
  AnalysisOptions opts;
  auto r = adl2::analysis::analyze_hir(hir, src, ext, opts);
  CHECK(r.reconciliations.empty());
  CHECK(r.recon_near_misses.empty());
  CHECK(r.render_default({}).find("collection reconciliation") == std::string::npos);
}

void test_abs_eta_keystone() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (abs eta)\n";
    CHECK(true);
    return;
  }
  auto r = cross(
      {{"a",
        "object jets\n  take Jet\n  select pt > 30\n  select abs(eta) < 2.4\n"
        "region RA\n  select size(jets) >= 3\n"},
       {"b",
        "object jets\n  take Jet\n  select pt > 25\n  select abs(eta) < 2.4\n"
        "region RB\n  select size(jets) <= 2\n"}},
      true);
  const PairReport* p = find_pair(r, "RA", "RB");
  CHECK(p != nullptr);
  if (p) CHECK(p->kind == VerdictKind::ProvenDisjoint);
}

}  // namespace

int main() {
  test_derived_size_le_shape();
  test_cross_shared_met();
  test_cross_does_not_falsely_unify_different_cuts();
  test_reconcile_unlocks_disjoint();
  test_reconcile_is_directional();
  test_reconcile_opaque_superset_fails_closed();
  test_reconcile_skips_private_base();
  test_reconcile_xeq_and_ledger();
  test_jet_vs_electron_never_advised();
  test_single_file_has_no_ledger();
  test_abs_eta_keystone();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
