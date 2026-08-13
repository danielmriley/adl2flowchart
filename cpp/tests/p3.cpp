#include "adl2/axioms/axioms.hpp"
#include "adl2/formula/dump.hpp"
#include "adl2/formula/encode.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <iostream>
#include <optional>
#include <set>
#include <string>

using namespace adl2::sema;
using adl2::formula::Formula;
using adl2::formula::LinAtom;
using adl2::formula::Rel;

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

void test_rat_decimal() {
  auto a = Rat::from_decimal_f64(0.3);
  CHECK(a.has_value());
  auto p = a->to_parts();
  CHECK(p.numerator == "3");
  CHECK(p.denominator == "10");
  CHECK(!p.negative);
  auto b = Rat::from_decimal_f64(0.9);
  auto c = Rat::from_decimal_f64(0.3);
  CHECK(b && c);
  auto d = *b - *c;
  auto pd = d.to_parts();
  // 0.9 - 0.3 = 6/10, reduced to 3/5 (lowest terms; dump is "3/5").
  CHECK(pd.numerator == "3" && pd.denominator == "5");
}

void test_formula_polarity() {
  auto atom = Formula::of_atom(LinAtom::single(QuantityId{0}, Rel::Gt, Rat::from_i64(1)));
  auto unk = Formula::unknown(adl2::formula::DiagId{0});
  CHECK(atom.is_exact());
  CHECK(!unk.is_exact());
  auto o = unk.over();
  auto u = unk.under();
  CHECK(o.qformula().kind == adl2::formula::QFormula::Kind::True);
  CHECK(u.qformula().kind == adl2::formula::QFormula::Kind::False);
  auto n = atom.fnot().fnot();
  CHECK(n == atom);
  auto dual = Formula::dual(Formula::ttrue(), atom, adl2::formula::DiagId{1});
  auto swapped = dual.fnot();
  CHECK(swapped.kind == Formula::Kind::Dual);
  CHECK(swapped.plus && swapped.plus->kind == Formula::Kind::Atom);  // ¬minus
  CHECK(swapped.minus && swapped.minus->kind == Formula::Kind::False);  // ¬true
}

void test_linatom_merge() {
  auto a = LinAtom::make({{Rat::from_i64(1), QuantityId{1}},
                          {Rat::from_i64(2), QuantityId{1}},
                          {Rat::from_i64(0), QuantityId{2}}},
                         Rel::Ge, Rat::from_i64(3));
  CHECK(a.terms().size() == 1);
  CHECK(a.terms()[0].second.id == 1);
  CHECK(a.terms()[0].first == Rat::from_i64(3));
}

void test_axiom_catalog() {
  CHECK(adl2::axioms::catalog_size() == adl2::axioms::AXIOM_COUNT);
  CHECK(adl2::axioms::AXIOM_COUNT == 19);
  static const char* kIds[19] = {
      "ORD",     "SZ0",     "SUB",      "UNI",     "NNEG",   "DPHI",
      "TAG",     "TWIN",    "EPRED",    "IDOM",    "SZSLICE","SZPERM",
      "COMBSIZE","TRIG",    "XSUB",     "XEQ",     "PRES",   "PDEF",
      "EPRES"};
  for (int i = 0; i < adl2::axioms::catalog_size(); ++i) {
    CHECK(std::string(adl2::axioms::axiom_id_str(adl2::axioms::catalog()[i].id)) ==
          kIds[i]);
  }
  std::set<std::string> ids;
  for (int i = 0; i < adl2::axioms::catalog_size(); ++i) {
    ids.insert(adl2::axioms::axiom_id_str(adl2::axioms::catalog()[i].id));
  }
  CHECK(ids.count("ORD"));
  CHECK(ids.count("TAG"));
  CHECK(ids.count("PDEF"));
  CHECK(ids.count("EPRES"));
  CHECK(ids.count("COMBSIZE"));
  for (int i = 0; i < adl2::axioms::catalog_size(); ++i) {
    const auto& e = adl2::axioms::catalog()[i];
    std::string st = e.statement;
    // Prohibited: mere mention of C[i] implying size(C) > i (unguarded).
    CHECK(st.find("referencing C[i] implies size(C) > i") == std::string::npos);
    CHECK(st.find("C[i] implies size") == std::string::npos);
  }
}

bool unguarded_size_gt(const Hir& hir, const adl2::formula::QFormula& f) {
  using QF = adl2::formula::QFormula;
  switch (f.kind) {
    case QF::Kind::And:
      for (const auto& x : f.items) {
        if (unguarded_size_gt(hir, x)) return true;
      }
      return false;
    case QF::Kind::Or:
    case QF::Kind::True:
    case QF::Kind::False:
      return false;
    case QF::Kind::Atom: {
      if (f.atom.terms().size() != 1) return false;
      if (f.atom.terms()[0].first != Rat::from_i64(1)) return false;
      const auto& q = hir.table.quantity(f.atom.terms()[0].second);
      if (q.kind != QuantityKind::Size) return false;
      const auto& k = f.atom.constant();
      if (f.atom.rel() == Rel::Gt) return k >= Rat::from_i64(0);
      if (f.atom.rel() == Rel::Ge) return k >= Rat::from_i64(1);
      return false;
    }
  }
  return false;
}

void test_no_existence_from_mention() {
  // Port of adl-axioms `axioms_hold_on_the_empty_event_no_existence_from_mention`:
  // mentioning jets[2].pt must not emit an unguarded size(jets) > 2.
  auto hir = analyze_str(
      "object jets\n  take Jet\n  select pt > 30\n"
      "region SR\n  select jets[2].pt > 10\n",
      "mention.adl", ExtDecls::legacy());
  CHECK(!has_errors(hir.diags));
  std::set<QuantityId> qs;
  for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) qs.insert(QuantityId{i});
  auto set = adl2::axioms::emit_axioms(hir, ExtDecls::legacy(), qs);
  bool prohibited = false;
  int epred = 0;
  int epres = 0;
  for (const auto& inst : set.instances) {
    if (inst.id == adl2::axioms::AxiomId::Epred) ++epred;
    if (inst.id == adl2::axioms::AxiomId::Epres) ++epres;
    if (unguarded_size_gt(hir, inst.formula)) prohibited = true;
  }
  CHECK(!prohibited);
  CHECK(epred == 1);
  CHECK(epres == 1);
  adl2::interp::EventError err;
  auto empty = adl2::interp::parse_event(
      R"({"Jet":[],"MET":{"pt":0.0,"phi":0.0}})", ExtDecls::legacy(), err);
  CHECK(empty.has_value());
}

void test_tag_exact_name() {
  auto hir = analyze_str(
      "object jets\n  take Jet\n"
      "region SR\n"
      "  select jets[0].btagDeepB > 0.2\n"
      "  select jets[0].btagDeepFlavB > 0.2\n"
      "  select jets[0].mybtag > 0.2\n"
      "  select jets[0].btag >= 0\n",
      "test.adl", ExtDecls::legacy());
  CHECK(!has_errors(hir.diags));
  std::set<QuantityId> qs;
  for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) qs.insert(QuantityId{i});
  auto set = adl2::axioms::emit_axioms(hir, ExtDecls::legacy(), qs);
  bool saw_btag = false;
  bool saw_substring = false;
  for (const auto& inst : set.instances) {
    if (inst.id != adl2::axioms::AxiomId::Tag) continue;
    const auto& d = inst.description;
    if (d.find("DeepB") != std::string::npos || d.find("deepb") != std::string::npos ||
        d.find("DeepFlav") != std::string::npos || d.find("mybtag") != std::string::npos)
      saw_substring = true;
    if (d.find(".btag") != std::string::npos && d.find("Deep") == std::string::npos &&
        d.find("mybtag") == std::string::npos)
      saw_btag = true;
  }
  CHECK(saw_btag);
  CHECK(!saw_substring);
}

void collect_all_qs(Hir& hir, std::set<QuantityId>& qs) {
  for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) qs.insert(QuantityId{i});
}

bool comb_desc_has(const adl2::axioms::AxiomSet& set, const char* needle) {
  for (const auto& inst : set.instances) {
    if (inst.id != adl2::axioms::AxiomId::CombSize) continue;
    if (inst.description.find(needle) != std::string::npos) return true;
  }
  return false;
}

void test_combsize_cross_source() {
  auto hir = analyze_str(
      "object jets\n  take Jet\nobject eles\n  take Ele\n"
      "region SR\n  select size(jets) >= 0\n",
      "comb.adl", ExtDecls::legacy());
  CHECK(!has_errors(hir.diags));
  auto jets = hir.collection_of("jets");
  auto eles = hir.collection_of("eles");
  CHECK(jets.has_value());
  CHECK(eles.has_value());
  auto comb = hir.table.intern_collection(Collection::combination(
      {*jets, *eles}, CombKind::Cartesian, {}, std::nullopt, {}));
  hir.table.intern_quantity(Quantity::size(comb));
  std::set<QuantityId> qs;
  collect_all_qs(hir, qs);
  auto set = adl2::axioms::emit_axioms(hir, ExtDecls::legacy(), qs);
  CHECK(comb_desc_has(set, ">= 0"));
  CHECK(comb_desc_has(set, " = 0 => "));
  CHECK(comb_desc_has(set, "all parts >= 1"));
}

void test_combsize_same_source_omits_positive_lb() {
  auto hir = analyze_str(
      "object jets\n  take Jet\nregion SR\n  select size(jets) >= 0\n",
      "comb2.adl", ExtDecls::legacy());
  CHECK(!has_errors(hir.diags));
  auto jets = hir.collection_of("jets");
  CHECK(jets.has_value());
  auto comb = hir.table.intern_collection(Collection::combination(
      {*jets, *jets}, CombKind::Disjoint, {}, std::nullopt, {}));
  hir.table.intern_quantity(Quantity::size(comb));
  std::set<QuantityId> qs;
  collect_all_qs(hir, qs);
  auto set = adl2::axioms::emit_axioms(hir, ExtDecls::legacy(), qs);
  CHECK(comb_desc_has(set, "< 2 => "));
  CHECK(!comb_desc_has(set, "all parts >= 1"));
}

void test_event_pt_order() {
  auto ext = ExtDecls::legacy();
  adl2::interp::EventError err;
  auto ok = adl2::interp::parse_event(R"({"Jet":[{"pt":100.0},{"pt":30.0}]})", ext, err);
  CHECK(ok.has_value());
  auto bad = adl2::interp::parse_event(R"({"Jet":[{"pt":30.0},{"pt":100.0}]})", ext, err);
  CHECK(!bad.has_value());
  CHECK(err.kind == adl2::interp::EventError::Kind::NotPtDescending);
  auto neg = adl2::interp::parse_event(R"({"Electron":[{"pt":-5.0}]})", ext, err);
  CHECK(!neg.has_value());
  CHECK(err.kind == adl2::interp::EventError::Kind::AxiomDomain);
  auto disc = adl2::interp::parse_event(R"({"Jet":[{"pt":30.0,"btagDeepB":0.7}]})", ext, err);
  CHECK(disc.has_value());
}

void test_interp_tiny() {
  const char* src =
      "define nEle = size(Ele)\n"
      "region SR\n"
      "  select nEle >= 1\n"
      "  select nEle >= 2 or MET.pt > 50\n";
  auto ext = ExtDecls::legacy();
  auto hir = analyze_str(src, "tiny.adl", ext);
  CHECK(!has_errors(hir.diags));
  adl2::interp::Interp interp(hir, ext);
  adl2::interp::EventError err;
  auto e1 = adl2::interp::parse_event(
      R"({"Ele":[{"pt":40,"eta":0.1,"phi":0.0}],"MET":{"pt":60,"phi":0.0}})", ext, err);
  CHECK(e1.has_value());
  auto r1 = interp.run_event(*e1);
  CHECK(r1.size() == 1);
  CHECK(r1[0].name == "SR");
  CHECK(r1[0].pass.has_value() && *r1[0].pass);
  auto e2 = adl2::interp::parse_event(
      R"({"Ele":[],"MET":{"pt":10,"phi":0.0}})", ext, err);
  CHECK(e2.has_value());
  auto r2 = interp.run_event(*e2);
  CHECK(r2.size() == 1);
  CHECK(r2[0].pass.has_value() && !*r2[0].pass);
}

void test_encode_presence() {
  auto hir = analyze_str(
      "object jets\n  take Jet\n  select pt > 30\n"
      "region R\n  select BTag(jets[0]) >= 1\n",
      "test.adl", ExtDecls::legacy());
  CHECK(!has_errors(hir.diags));
  auto enc = adl2::formula::encode_region(hir, 0);
  auto stripped = enc.formula.without_presence(hir.table);
  CHECK(!(enc.formula == stripped));
  CHECK(stripped == stripped.without_presence(hir.table));
}

}  // namespace

int main() {
  test_rat_decimal();
  test_formula_polarity();
  test_linatom_merge();
  test_axiom_catalog();
  test_no_existence_from_mention();
  test_tag_exact_name();
  test_combsize_cross_source();
  test_combsize_same_source_omits_positive_lb();
  test_event_pt_order();
  test_interp_tiny();
  test_encode_presence();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
