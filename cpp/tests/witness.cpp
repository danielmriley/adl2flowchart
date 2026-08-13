#include "adl2/analysis/analysis.hpp"
#include "adl2/interp/event.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/solver/solver.hpp"

#include <cstdint>
#include <iostream>
#include <map>
#include <set>
#include <string>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::PairReport;
using adl2::analysis::Report;
using adl2::analysis::SolverChoice;
using adl2::analysis::Validation;
using adl2::analysis::ValidationKind;
using adl2::analysis::VerdictKind;
using adl2::analysis::validate_witness;
using adl2::interp::Interp;
using adl2::sema::ElemIndexKind;
using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::Quantity;
using adl2::sema::QuantityKind;
using adl2::sema::QuantityId;
using adl2::sema::Rat;
using adl2::sema::analyze_str;
using adl2::solver::Model;

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

const char* kPtCuts =
    "object jets\n"
    "  take Jet\n"
    "region High\n"
    "  select jets[0].pt > 100\n"
    "region Mid\n"
    "  select jets[0].pt > 30\n";

const char* kOpaqueHt =
    "object jets\n"
    "  take Jet\n"
    "region A\n"
    "  select ht(jets) > 100\n"
    "region B\n"
    "  select ht(jets) > 50\n";

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

QuantityId find_size(const Hir& hir) {
  auto jets = hir.collection_of("jets");
  const auto& qs = hir.table.quantities();
  for (std::uint32_t i = 0; i < qs.size(); ++i) {
    const auto& q = qs[i];
    if (q.kind == QuantityKind::Size && jets && q.coll == *jets) return QuantityId{i};
  }
  return QuantityId{0xFFFFFFFFu};
}

const PairReport* find_pair(const Report& r, const char* a, const char* b) {
  for (const auto& p : r.pairwise) {
    if (p.a == a && p.b == b) return &p;
    if (p.a == b && p.b == a) return &p;
  }
  return nullptr;
}

void test_simple_pt_overlap_validated() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kPtCuts, "pt.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  Interp ip(hir, ext);

  QuantityId ptq = find_jets0_pt(hir, ext);
  CHECK(ptq.id != 0xFFFFFFFFu);
  if (ptq.id == 0xFFFFFFFFu) return;
  std::map<QuantityId, Rat> vals;
  vals[ptq] = Rat::from_i64(150);
  QuantityId sz = find_size(hir);
  if (sz.id != 0xFFFFFFFFu) vals[sz] = Rat::from_i64(1);
  std::set<QuantityId> mentioned;
  for (const auto& kv : vals) mentioned.insert(kv.first);
  Model model(std::move(vals));

  Validation v = validate_witness(hir, ext, ip, model, mentioned, 0, 1);
  CHECK(v.kind == ValidationKind::Validated);
  if (v.kind == ValidationKind::Validated) {
    CHECK(v.payload.find("pt") != std::string::npos);
    adl2::interp::EventError ee;
    auto ev = adl2::interp::parse_event(v.payload, ext, ee);
    CHECK(ev.has_value());
    if (ev) {
      adl2::interp::EvalError err;
      auto a = ip.eval_region_membership_idx(0, *ev, err);
      auto b = ip.eval_region_membership_idx(1, *ev, err);
      CHECK(a.has_value() && *a);
      CHECK(b.has_value() && *b);
    }
  }
}

void test_opaque_ht_candidate() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kOpaqueHt, "ht.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  Interp ip(hir, ext);
  Model model;
  std::set<QuantityId> mentioned;
  Validation v = validate_witness(hir, ext, ip, model, mentioned, 0, 1);
  CHECK(v.kind == ValidationKind::Candidate);
  CHECK(v.payload.find("opaque") != std::string::npos ||
        v.payload.find("no reference interpretation") != std::string::npos);
}

void test_realization_cap_rejected() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kPtCuts, "cap.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  Interp ip(hir, ext);
  auto jets = hir.collection_of("jets");
  CHECK(jets.has_value());
  if (!jets) return;
  QuantityId sz = find_size(hir);
  if (sz.id == 0xFFFFFFFFu) sz = hir.table.intern_quantity(Quantity::size(*jets));
  CHECK(sz.id != 0xFFFFFFFFu);
  std::map<QuantityId, Rat> vals;
  vals[sz] = Rat::from_i64(100);
  std::set<QuantityId> mentioned{sz};
  Model model(std::move(vals));
  Validation v = validate_witness(hir, ext, ip, model, mentioned, 0, 1);
  CHECK(v.kind == ValidationKind::Rejected);
  CHECK(v.payload.find("realization failed") != std::string::npos ||
        v.payload.find("realizer cap") != std::string::npos);
}

void test_interpreter_reject() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kPtCuts, "rej.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  Interp ip(hir, ext);
  QuantityId ptq = find_jets0_pt(hir, ext);
  CHECK(ptq.id != 0xFFFFFFFFu);
  if (ptq.id == 0xFFFFFFFFu) return;
  std::map<QuantityId, Rat> vals;
  vals[ptq] = Rat::from_i64(10);
  QuantityId sz = find_size(hir);
  if (sz.id != 0xFFFFFFFFu) vals[sz] = Rat::from_i64(1);
  std::set<QuantityId> mentioned;
  for (const auto& kv : vals) mentioned.insert(kv.first);
  Model model(std::move(vals));
  Validation v = validate_witness(hir, ext, ip, model, mentioned, 0, 1);
  CHECK(v.kind == ValidationKind::Rejected);
  CHECK(v.payload.find("rejects") != std::string::npos ||
        v.payload.find("realization failed") != std::string::npos);
}

void test_analyze_hir_sat_proven_overlapping() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (witness SAT path)\n";
    CHECK(true);
    return;
  }
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kPtCuts, "sat.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  Report r = adl2::analysis::analyze_hir(hir, kPtCuts, ext, opts);
  CHECK(r.solver == "smtlib-subprocess");
  const PairReport* hm = find_pair(r, "High", "Mid");
  CHECK(hm != nullptr);
  if (hm) {
    CHECK(hm->kind == VerdictKind::ProvenOverlapping);
    CHECK(hm->witness_validated.has_value() && *hm->witness_validated);
  }
}

void test_opaque_analyze_hir_candidate() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 on PATH (opaque SAT path)\n";
    CHECK(true);
    return;
  }
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kOpaqueHt, "htsat.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  opts.solver = SolverChoice::SubprocessZ3;
  Report r = adl2::analysis::analyze_hir(hir, kOpaqueHt, ext, opts);
  const PairReport* ab = find_pair(r, "A", "B");
  CHECK(ab != nullptr);
  if (ab) {
    CHECK(ab->kind != VerdictKind::ProvenOverlapping);
    CHECK(ab->kind == VerdictKind::CandidateOverlapping ||
          ab->kind == VerdictKind::PossiblyOverlapping);
    if (ab->kind == VerdictKind::CandidateOverlapping) {
      CHECK(ab->witness_validated.has_value() && !*ab->witness_validated);
    }
  }
}

}  // namespace

int main() {
  test_simple_pt_overlap_validated();
  test_opaque_ht_candidate();
  test_realization_cap_rejected();
  test_interpreter_reject();
  test_analyze_hir_sat_proven_overlapping();
  test_opaque_analyze_hir_candidate();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
