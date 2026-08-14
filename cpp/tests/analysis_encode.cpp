#include "adl2/analysis/analysis.hpp"
#include "adl2/sema/sema.hpp"

#include <iostream>
#include <string>

using adl2::analysis::AnalysisOptions;
using adl2::analysis::PairReport;
using adl2::analysis::Report;
using adl2::analysis::SolverChoice;
using adl2::analysis::UnitEnc;
using adl2::analysis::VerdictKind;
using adl2::analysis::dump_verdicts;
using adl2::analysis::verdict_kind_human;
using adl2::formula::Formula;
using adl2::sema::ExtDecls;
using adl2::sema::Hir;
using adl2::sema::QuantityKind;
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
    "  select jets[0].pt < 50\n"
    "region Mid\n"
    "  select jets[0].pt > 30\n";

void test_interval_proves_disjoint_jets() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  CHECK(hir.regions.size() == 3);

  AnalysisOptions opts;
  opts.solver = SolverChoice::NoSolver;
  Report r = adl2::analysis::analyze_hir(hir, kSrc, ext, opts);
  CHECK(r.schema_version == adl2::analysis::SCHEMA_VERSION);
  CHECK(r.solver == "none");
  CHECK(r.pairwise.size() == 3);

  const PairReport* hl = find_pair(r, "High", "Low");
  CHECK(hl != nullptr);
  if (hl) {
    CHECK(hl->kind == VerdictKind::ProvenDisjoint);
    CHECK(hl->proof_path.has_value());
    if (hl->proof_path) {
      CHECK(*hl->proof_path == adl2::analysis::ProofPath::Interval);
    }
  }

  const PairReport* hm = find_pair(r, "High", "Mid");
  CHECK(hm != nullptr);
  if (hm) {
    CHECK(hm->kind == VerdictKind::PossiblyOverlapping);
    CHECK(hm->kind != VerdictKind::ProvenOverlapping);
    CHECK(hm->kind != VerdictKind::ProvenDisjoint);
  }

  const PairReport* lm = find_pair(r, "Low", "Mid");
  CHECK(lm != nullptr);
  if (lm) {
    CHECK(lm->kind == VerdictKind::PossiblyOverlapping);
  }

  std::string dump = dump_verdicts(r);
  CHECK(dump.find("High vs Low: PROVEN DISJOINT") != std::string::npos);
  CHECK(dump.find("High vs Mid: POSSIBLY OVERLAPPING") != std::string::npos);
  CHECK(dump.find("Low vs Mid: POSSIBLY OVERLAPPING") != std::string::npos);

  std::string js = r.to_json();
  CHECK(js.find("\"kind\": \"proven_disjoint\"") != std::string::npos);
  CHECK(js.find("\"empty\": \"unknown\"") != std::string::npos);
  CHECK(js.find("\"bin_checks\": []") != std::string::npos);
  CHECK(js.find("\"internal_diagnostics\": []") != std::string::npos);
  CHECK(js.find("\"proof_path\": \"interval\"") != std::string::npos);
  CHECK(js.find("PROVEN DISJOINT") == std::string::npos);
}

void test_encode_statement_granularity() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  UnitEnc unit = adl2::analysis::encode_unit(hir, kSrc);
  CHECK(unit.regions.size() == 3);
  for (const auto& r : unit.regions) {
    CHECK(r.stmts.size() == 1);
    CHECK(r.stmts[0].formula.is_exact());
    CHECK(!r.quantities.empty());
  }
  CHECK(unit.regions[0].stmts[0].name.value == "R0S0");
  CHECK(unit.regions[1].stmts[0].name.value == "R1S0");
}

void test_inherit_flattens_like_paste() {
  const char* inherit_src =
      "object jets\n"
      "  take Jet\n"
      "region Base\n"
      "  select jets[0].pt > 100\n"
      "region Child\n"
      "  Base\n"
      "region Low\n"
      "  select jets[0].pt < 50\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(inherit_src, "inh.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  AnalysisOptions opts;
  Report r = adl2::analysis::analyze_hir(hir, inherit_src, ext, opts);
  const PairReport* cl = find_pair(r, "Child", "Low");
  CHECK(cl != nullptr);
  if (cl) CHECK(cl->kind == VerdictKind::ProvenDisjoint);
  const PairReport* bl = find_pair(r, "Base", "Low");
  CHECK(bl != nullptr);
  if (bl) CHECK(bl->kind == VerdictKind::ProvenDisjoint);
}

bool formula_mentions_present(const Formula& f, const adl2::sema::QuantityTable& table) {
  switch (f.kind) {
    case Formula::Kind::Atom:
      for (const auto& t : f.atom.terms()) {
        if (table.quantity(t.second).kind == QuantityKind::Present) return true;
      }
      return false;
    case Formula::Kind::And:
    case Formula::Kind::Or:
      for (const auto& p : f.items) {
        if (formula_mentions_present(p, table)) return true;
      }
      return false;
    case Formula::Kind::Dual:
      return (f.plus && formula_mentions_present(*f.plus, table)) ||
             (f.minus && formula_mentions_present(*f.minus, table));
    default:
      return false;
  }
}

void test_size_hard_filter_presence_guard() {
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "object weird\n"
      "  take Jet\n"
      "  select bdt > 0.5\n"
      "region A\n"
      "  select size(jets) >= 1\n"
      "region B\n"
      "  select size(weird) >= 0\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "size_hard.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  UnitEnc unit = adl2::analysis::encode_unit(hir, src);
  CHECK(unit.regions.size() == 2);
  CHECK(unit.regions[0].name == "A");
  CHECK(unit.regions[1].name == "B");
  CHECK(unit.regions[0].stmts.size() == 1);
  CHECK(unit.regions[1].stmts.size() == 1);
  CHECK(unit.regions[0].stmts[0].formula.is_exact());
  CHECK(unit.regions[1].stmts[0].formula.is_exact());
  CHECK(!formula_mentions_present(unit.regions[0].stmts[0].formula, hir.table));
  CHECK(formula_mentions_present(unit.regions[1].stmts[0].formula, hir.table));
}

bool has_negate_presence_dual(const UnitEnc& unit) {
  for (const auto& r : unit.regions) {
    for (const auto& s : r.stmts) {
      for (const auto& d : s.diags.entries()) {
        if (d.reason.find("negation over an event-level datum") != std::string::npos) {
          return true;
        }
      }
    }
  }
  return false;
}

void test_mixed_filter_size_is_not_hard() {
  // In-fragment conjuncts can decide False, so size is obtainable. A
  // presence guard here would De-Morgan `p<1` into reject-OVER (Delphes
  // onZ/offZ completeness hole).
  const char* src =
      "object electrons\n"
      "  take Electron\n"
      "  select pt > 10\n"
      "  select DO < 0.5\n"
      "region A\n"
      "  select size(electrons) >= 1\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "mixed_size.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  UnitEnc unit = adl2::analysis::encode_unit(hir, src);
  CHECK(unit.regions.size() == 1);
  CHECK(unit.regions[0].stmts.size() == 1);
  CHECK(unit.regions[0].stmts[0].formula.is_exact());
  CHECK(!formula_mentions_present(unit.regions[0].stmts[0].formula, hir.table));
}

const char* kTernaryOnOffZ =
    "object leptons\n"
    "  take Electron\n"
    "  select pt > 10\n"
    "  select DO < 0.5\n"
    "composite OSdileptons\n"
    "  take leptons l1, l2\n"
    "  select l1.pdgID + l2.pdgID == 0\n"
    "region onZ\n"
    "  select size(OSdileptons) > 0 ? m(OSdileptons.l1 + OSdileptons.l2) [] 76 106\n"
    "region offZ\n"
    "  reject size(OSdileptons) > 0 ? m(OSdileptons.l1 + OSdileptons.l2) [] 76 106\n";

void test_ternary_reject_has_no_negate_presence_dual() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kTernaryOnOffZ, "ternary_onz.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  UnitEnc unit = adl2::analysis::encode_unit(hir, kTernaryOnOffZ);
  CHECK(unit.regions.size() == 2);
  CHECK(!has_negate_presence_dual(unit));
}

void test_duplicate_region_names_are_disambiguated() {
  const char* src =
      "object jets\n"
      "  take Jet\n"
      "region SR\n"
      "  select jets[0].pt > 100\n"
      "region SR\n"
      "  select jets[0].pt < 50\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "dup.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  bool saw_dup = false;
  for (const auto& d : hir.diags) {
    if (d.message.find("duplicate region") != std::string::npos) saw_dup = true;
  }
  CHECK(saw_dup);
  UnitEnc unit = adl2::analysis::encode_unit(hir, src);
  CHECK(unit.regions.size() == 2);
  CHECK(unit.regions[0].name != unit.regions[1].name);
  CHECK(unit.regions[0].name.find("SR@") == 0);
  CHECK(unit.regions[1].name.find("SR@") == 0);
  AnalysisOptions opts;
  opts.solver = SolverChoice::NoSolver;
  Report r = adl2::analysis::analyze_hir(hir, src, ext, opts);
  const PairReport* p = find_pair(r, unit.regions[0].name.c_str(), unit.regions[1].name.c_str());
  CHECK(p != nullptr);
  if (p) CHECK(p->kind == VerdictKind::ProvenDisjoint);
}

void test_unknown_call_arg_names_the_identifier() {
  const char* src =
      "object electrons\n"
      "  take Electron\n"
      "  select pT > 40\n"
      "region A\n"
      "  select fMT(electrons[0], HardMETLV) > 110\n"
      "region B\n"
      "  select fMT(electrons[0], HardMETLV) < 50\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "fmt.adl", ext);
  CHECK(!adl2::sema::has_errors(hir.diags));
  UnitEnc unit = adl2::analysis::encode_unit(hir, src);
  CHECK(!unit.regions.empty());
  bool named = false;
  for (const auto& r : unit.regions) {
    for (const auto& d : r.dropped) {
      CHECK(d.second.find("element-context") == std::string::npos);
      if (d.second.find("HardMETLV") != std::string::npos &&
          d.second.find("unknown") != std::string::npos) {
        named = true;
      }
    }
  }
  CHECK(named);
}

void test_unknown_call_arg_suggests_near_name() {
  const char* src =
      "object HardMETLV\n"
      "  take MissingET\n"
      "object electrons\n"
      "  take Electron\n"
      "region A\n"
      "  select fMT(electrons[0], HardMET_LV) > 110\n";
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(src, "typo.adl", ext);
  UnitEnc unit = adl2::analysis::encode_unit(hir, src);
  bool hinted = false;
  for (const auto& r : unit.regions) {
    for (const auto& d : r.dropped) {
      if (d.second.find("HardMET_LV") != std::string::npos &&
          d.second.find("HardMETLV") != std::string::npos) {
        hinted = true;
      }
    }
  }
  CHECK(hinted);
}

void test_three_arg_entry() {
  ExtDecls ext = ExtDecls::legacy();
  Hir hir = analyze_str(kSrc, "jets.adl", ext);
  AnalysisOptions opts;
  Report r = adl2::analysis::analyze_hir(hir, ext, opts);
  CHECK(find_pair(r, "High", "Low") != nullptr);
}

}  // namespace

int main() {
  test_interval_proves_disjoint_jets();
  test_encode_statement_granularity();
  test_inherit_flattens_like_paste();
  test_size_hard_filter_presence_guard();
  test_mixed_filter_size_is_not_hard();
  test_ternary_reject_has_no_negate_presence_dual();
  test_duplicate_region_names_are_disambiguated();
  test_unknown_call_arg_names_the_identifier();
  test_unknown_call_arg_suggests_near_name();
  test_three_arg_entry();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
