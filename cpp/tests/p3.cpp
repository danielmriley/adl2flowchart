#include "adl2/axioms/axioms.hpp"
#include "adl2/formula/dump.hpp"
#include "adl2/formula/encode.hpp"
#include "adl2/formula/formula.hpp"
#include "adl2/interp/interp.hpp"
#include "adl2/sema/sema.hpp"

#include <iostream>
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
  std::set<std::string> ids;
  for (int i = 0; i < adl2::axioms::catalog_size(); ++i) {
    ids.insert(adl2::axioms::axiom_id_str(adl2::axioms::catalog()[i].id));
  }
  CHECK(ids.count("ORD"));
  CHECK(ids.count("TAG"));
  CHECK(ids.count("PDEF"));
  CHECK(ids.count("EPRES"));
  for (int i = 0; i < adl2::axioms::catalog_size(); ++i) {
    const auto& e = adl2::axioms::catalog()[i];
    std::string st = e.statement;
    // Prohibited: mere mention of C[i] implying size(C) > i (unguarded).
    CHECK(st.find("referencing C[i] implies size(C) > i") == std::string::npos);
  }
}

void test_tag_exact_name() {
  auto hir = analyze_str(
      "object jets\n  take Jet\n"
      "region SR\n  select jets[0].btagDeepB > 0.2\n  select jets[0].btag >= 0\n",
      "test.adl", ExtDecls::legacy());
  CHECK(!has_errors(hir.diags));
  std::set<QuantityId> qs;
  for (std::uint32_t i = 0; i < hir.table.quantities().size(); ++i) qs.insert(QuantityId{i});
  auto set = adl2::axioms::emit_axioms(hir, ExtDecls::legacy(), qs);
  bool saw_btag = false;
  bool saw_deep = false;
  for (const auto& inst : set.instances) {
    if (inst.id != adl2::axioms::AxiomId::Tag) continue;
    if (inst.description.find("DeepB") != std::string::npos ||
        inst.description.find("deepb") != std::string::npos)
      saw_deep = true;
    if (inst.description.find(".btag") != std::string::npos) saw_btag = true;
  }
  CHECK(saw_btag);
  CHECK(!saw_deep);
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
  test_tag_exact_name();
  test_event_pt_order();
  test_interp_tiny();
  test_encode_presence();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
