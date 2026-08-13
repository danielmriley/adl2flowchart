#include "adl2/analysis/analysis.hpp"
#include "adl2/certify/bundle.hpp"
#include "adl2/sema/sema.hpp"
#include "adl2/solver/solver.hpp"

#include <algorithm>
#include <chrono>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::Report;
using adl2::analysis::SolverChoice;
using adl2::analysis::VerdictKind;
using adl2::certify::AssertSource;
using adl2::certify::CombineBundle;
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

std::filesystem::path repo_root() {
  return std::filesystem::path(__FILE__).parent_path().parent_path().parent_path();
}

AnalysisOptions combine_opts() {
  AnalysisOptions opts;
  opts.solver = SolverChoice::Auto;
  opts.timeout = std::chrono::seconds{20};
  opts.reconcile = true;
  opts.sample_gate = 64;
  opts.refute_gate = true;
  opts.certify = true;
  opts.combine = true;
  return opts;
}

Report cross_combine(const std::vector<std::pair<const char*, const char*>>& units) {
  ExtDecls ext = ExtDecls::legacy();
  std::vector<Hir> hirs;
  hirs.reserve(units.size());
  for (const auto& u : units) {
    auto h = analyze_str(u.second, u.first, ext);
    if (adl2::sema::has_errors(h.diags)) {
      std::cerr << "resolve errors in " << u.first << "\n";
    }
    hirs.push_back(std::move(h));
  }
  std::vector<const Hir*> refs;
  for (const auto& h : hirs) refs.push_back(&h);
  Hir merged = merge_hirs(refs);
  return adl2::analysis::analyze_hir(merged, "", ext, combine_opts());
}

Report cross_combine_files(const std::string& rel) {
  auto dir = repo_root() / rel;
  std::vector<std::filesystem::path> files;
  for (const auto& ent : std::filesystem::directory_iterator(dir)) {
    if (ent.path().extension() == ".adl") files.push_back(ent.path());
  }
  std::sort(files.begin(), files.end());
  CHECK(!files.empty());
  ExtDecls ext = ExtDecls::legacy();
  std::vector<Hir> hirs;
  std::vector<std::string> names;
  std::vector<std::string> srcs;
  for (const auto& p : files) {
    names.push_back(p.stem().string());
    std::ifstream in(p);
    std::ostringstream ss;
    ss << in.rdbuf();
    srcs.push_back(ss.str());
    hirs.push_back(analyze_str(srcs.back(), names.back(), ext));
  }
  std::vector<const Hir*> refs;
  for (const auto& h : hirs) refs.push_back(&h);
  Hir merged = merge_hirs(refs);
  return adl2::analysis::analyze_hir(merged, "", ext, combine_opts());
}

const char* A =
    "object jets\n  take Jet\n  select pt > 30\n  select abs(eta) < 2.4\n\n"
    "region SR\n  select size(jets) >= 3\n";
const char* B =
    "object jets\n  take Jet\n  select pt > 25\n  select abs(eta) < 2.4\n\n"
    "region CR\n  select size(jets) <= 2\n";

bool skip_no_solver() {
  if (!adl2::solver::subprocess_available("z3")) {
    std::cerr << "SKIP: no z3 (combine)\n";
    return true;
  }
  return false;
}

void test_certified_disjoint_pair_yields_replayable_bundle() {
  if (skip_no_solver()) {
    CHECK(true);
    return;
  }
  auto report = cross_combine({{"a", A}, {"b", B}});
  if (report.solver == "none") {
    CHECK(true);
    return;
  }
  CHECK(!report.pairwise.empty());
  if (report.pairwise.empty()) return;
  const auto& p = report.pairwise[0];
  CHECK(p.kind == VerdictKind::ProvenDisjoint);
  CHECK(p.certified == true);
  CHECK(report.combine_bundles.size() == 1);
  if (report.combine_bundles.empty()) return;
  const auto& bundle = report.combine_bundles[0];
  CHECK(bundle.region_a == p.a && bundle.region_b == p.b);
  CHECK(bundle.replay());

  std::string js = bundle.to_json();
  auto back = CombineBundle::from_json(js);
  CHECK(back.has_value());
  if (back) {
    CHECK(*back == bundle);
    CHECK(back->replay());
  }

  auto tampered = bundle;
  bool zeroed = false;
  auto walk = [&](auto& self, adl2::certify::CertNode& n) -> void {
    if (n.kind == adl2::certify::CertNode::Kind::Farkas) {
      for (auto& m : n.multipliers) {
        if (m.to_repr() != "0") {
          m = *adl2::certify::QRat::from_repr("0");
          zeroed = true;
          return;
        }
      }
    }
    for (auto& b : n.branches) self(self, b);
  };
  adl2::certify::CertNode root = tampered.certificate.root();
  walk(walk, root);
  tampered.certificate = adl2::certify::Certificate(std::move(root));
  CHECK(zeroed);
  CHECK(!tampered.replay());
}

void test_reconciliation_facts_travel_with_their_derivation() {
  if (skip_no_solver()) {
    CHECK(true);
    return;
  }
  auto report = cross_combine({{"a", A}, {"b", B}});
  if (report.solver == "none" || report.combine_bundles.empty()) {
    CHECK(!report.combine_bundles.empty() || report.solver == "none");
    return;
  }
  const auto& bundle = report.combine_bundles[0];
  std::vector<std::string> used;
  for (const auto& a : bundle.asserts) {
    if (a.source.kind == AssertSource::Kind::Derived) used.push_back(a.source.fact);
  }
  CHECK(!used.empty());
  for (const auto& name : used) {
    const adl2::certify::DerivedFact* fact = nullptr;
    for (const auto& f : bundle.derived_facts) {
      if (f.name == name) fact = &f;
    }
    CHECK(fact != nullptr);
    if (!fact) continue;
    CHECK(fact->axiom == "XSUB");
    CHECK(!fact->derivations.empty());
    for (const auto& d : fact->derivations) {
      CHECK(d.replay());
      CHECK(!d.premises.empty());
    }
  }
  CHECK(!bundle.quantities.empty());
  auto stripped = bundle;
  stripped.derived_facts.clear();
  CHECK(!stripped.replay());
}

void test_self_empty_region_certifies_its_pairs() {
  const char* empty =
      "object jets\n  take Jet\n  select pt > 30\n\n"
      "region VAC\n  select size(jets) >= 3\n  select size(jets) <= 2\n\n"
      "region OK\n  select MET > 100\n";
  auto report = cross_combine({{"u", empty}});
  const adl2::analysis::PairReport* p = nullptr;
  for (const auto& pr : report.pairwise) {
    if (pr.reason.compare(0, 7, "region ") == 0) {
      p = &pr;
      break;
    }
  }
  CHECK(p != nullptr);
  if (!p) return;
  CHECK(p->kind == VerdictKind::ProvenDisjoint);
  CHECK(p->certified == true);
  const CombineBundle* bundle = nullptr;
  for (const auto& b : report.combine_bundles) {
    if (b.region_a == p->a && b.region_b == p->b) bundle = &b;
  }
  CHECK(bundle != nullptr);
  if (!bundle) return;
  CHECK(bundle->replay());
  CHECK(bundle->asserts.size() == 2);
  CHECK(report.internal_diagnostics.empty());
}

void test_two_runs_produce_byte_identical_bundles() {
  if (skip_no_solver()) {
    CHECK(true);
    return;
  }
  auto one = cross_combine({{"a", A}, {"b", B}});
  auto two = cross_combine({{"a", A}, {"b", B}});
  if (one.solver == "none") {
    CHECK(true);
    return;
  }
  std::string js1, js2;
  for (const auto& b : one.combine_bundles) js1 += b.to_json();
  for (const auto& b : two.combine_bundles) js2 += b.to_json();
  CHECK(js1 == js2);
  CHECK(js1.find("timestamp") == std::string::npos);
  CHECK(js1.find("generated_at") == std::string::npos);
}

void test_every_proven_disjoint_pair_has_a_bundle() {
  if (skip_no_solver()) {
    CHECK(true);
    return;
  }
  for (const char* dir : {"examples/golden/cross/refine-disjoint",
                          "examples/golden/cross/xeq-equivalent"}) {
    auto report = cross_combine_files(dir);
    if (report.solver == "none") {
      CHECK(true);
      return;
    }
    std::size_t proven = 0;
    for (const auto& p : report.pairwise) {
      if (p.kind != VerdictKind::ProvenDisjoint) continue;
      ++proven;
      CHECK(p.certified == true);
      bool found = false;
      for (const auto& b : report.combine_bundles) {
        if (b.region_a == p.a && b.region_b == p.b) {
          found = true;
          CHECK(b.replay());
        }
      }
      CHECK(found);
    }
    CHECK(proven > 0);
    CHECK(report.combine_bundles.size() == proven);
    CHECK(report.internal_diagnostics.empty());
  }
}

void test_combine_off_produces_no_bundles() {
  ExtDecls ext = ExtDecls::legacy();
  std::vector<Hir> hirs;
  hirs.push_back(analyze_str(A, "a", ext));
  hirs.push_back(analyze_str(B, "b", ext));
  std::vector<const Hir*> refs;
  for (const auto& h : hirs) refs.push_back(&h);
  Hir merged = merge_hirs(refs);
  AnalysisOptions opts;
  opts.reconcile = true;
  auto report = adl2::analysis::analyze_hir(merged, "", ext, opts);
  CHECK(report.combine_bundles.empty());
}

}  // namespace

int main() {
  test_certified_disjoint_pair_yields_replayable_bundle();
  test_reconciliation_facts_travel_with_their_derivation();
  test_self_empty_region_certifies_its_pairs();
  test_two_runs_produce_byte_identical_bundles();
  test_every_proven_disjoint_pair_has_a_bundle();
  test_combine_off_produces_no_bundles();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
