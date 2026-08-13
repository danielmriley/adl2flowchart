#include "adl2/sema/sema.hpp"

#include <iostream>
#include <string>
#include <vector>

using namespace adl2::sema;

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

Hir analyze(const char* src) { return analyze_str(src, "test.adl", ExtDecls::legacy()); }

std::vector<HNode> select_nodes(const Hir& hir, const char* region) {
  std::vector<HNode> out;
  const HirRegion* r = hir.region(region);
  if (!r) return out;
  for (const auto& s : r->stmts) {
    if (s.kind == HirRegionStmt::Kind::Select) out.push_back(s.node);
  }
  return out;
}

bool has_error_containing(const Hir& hir, const char* needle) {
  for (const auto& d : hir.diags) {
    if (d.severity == Severity::Error && d.message.find(needle) != std::string::npos) {
      return true;
    }
  }
  return false;
}

void test_pure_rename() {
  auto hir = analyze(
      "object eles\n  take Ele\n"
      "object electrons2\n  take eles\n"
      "object electrons3\n  take electrons2\n"
      "object MHT\n  take MissingET\n"
      "object MET2\n  take MHT\n");
  auto eles = hir.collection_of("eles");
  auto e2 = hir.collection_of("electrons2");
  auto e3 = hir.collection_of("electrons3");
  CHECK(eles && e2 && e3 && *eles == *e2 && *e2 == *e3);
  CHECK(eles && hir.table.collection(*eles).kind == CollectionKind::Base);
  auto mht = hir.collection_of("MHT");
  auto met2 = hir.collection_of("MET2");
  CHECK(mht && met2 && *mht == *met2);
  CHECK(mht && hir.table.collection(*mht).kind == CollectionKind::Base);
  if (mht && hir.table.collection(*mht).kind == CollectionKind::Base) {
    CHECK(hir.symbols.key(hir.table.collection(*mht).base) == "met");
  }
  for (const char* name : {"electrons2", "electrons3", "MET2"}) {
    Symbol sym;
    CHECK(hir.symbols.lookup(name, sym));
    bool found = false;
    for (const auto& o : hir.objects) {
      if (o.name == sym) {
        CHECK(o.pure_alias_of.has_value());
        found = true;
      }
    }
    CHECK(found);
  }
}

void test_filtered_distinct() {
  auto hir = analyze(
      "object jets\n  take Jet\n  select pT > 30\n"
      "object alljets\n  take Jet\n");
  auto jets = hir.collection_of("jets");
  auto alljets = hir.collection_of("alljets");
  CHECK(jets && alljets && !(*jets == *alljets));
  CHECK(jets && hir.table.collection(*jets).kind == CollectionKind::Filtered);
  if (jets && hir.table.collection(*jets).kind == CollectionKind::Filtered) {
    CHECK(hir.table.collection(*jets).parent == *alljets);
  }
}

void test_indexed_no_alias() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "region SR\n  select jets[0].pT > 100\n  select jets[1].pT > 50\n");
  int n = 0;
  const Quantity* e0 = nullptr;
  const Quantity* e1 = nullptr;
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::ElemProp) {
      if (n == 0) e0 = &q;
      if (n == 1) e1 = &q;
      ++n;
    }
  }
  CHECK(n == 2);
  if (e0 && e1) {
    CHECK(e0->coll == e1->coll);
    CHECK(e0->prop == e1->prop);
    CHECK(!(e0->index == e1->index));
  }
}

void test_property_spellings() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "region SR\n  select jets[0].pT > 100\n  select pt(jets[0]) > 100\n  "
      "select {jets[0]}Pt > 100\n");
  int n = 0;
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::ElemProp) ++n;
  }
  CHECK(n == 1);
}

void test_angular() {
  auto hir = analyze(
      "object eles\n  take Ele\n"
      "object muons\n  take Muo\n"
      "region SR\n"
      "  select dPhi(eles[0], muons[0]) > 1\n"
      "  select dPhi(muons[0], eles[0]) > 1\n"
      "  select dR(eles[0], muons[0]) > 0.4\n"
      "  select dR(muons[0], eles[0]) > 0.4\n");
  int dphis = 0, drs = 0;
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::AngularSep && q.ang == AngKind::DPhi) {
      ++dphis;
      CHECK(q.oriented);
    }
    if (q.kind == QuantityKind::AngularSep && q.ang == AngKind::DR) {
      ++drs;
      CHECK(!q.oriented);
    }
  }
  CHECK(dphis == 2);
  CHECK(drs == 1);
}

void test_define_inline() {
  auto hir = analyze(
      "define halfmet = MET / 2\n"
      "region SR\n  select halfmet < 10\n");
  const HirDefine* def = hir.define("halfmet");
  CHECK(def && def->kind == DefineKind::Numeric);
  auto selects = select_nodes(hir, "SR");
  CHECK(selects.size() == 1);
  if (!selects.empty() && selects[0].kind == HNode::Kind::Cmp && selects[0].a) {
    CHECK(*selects[0].a == def->body);
  }
}

void test_boolean_define() {
  auto hir = analyze(
      "define lowmet = MET < 100\n"
      "region SR\n  select lowmet\n");
  const HirDefine* def = hir.define("lowmet");
  CHECK(def && def->kind == DefineKind::Boolean);
  auto selects = select_nodes(hir, "SR");
  CHECK(selects.size() == 1);
  if (!selects.empty() && def) CHECK(selects[0] == def->body);
}

void test_define_cycle() {
  auto hir = analyze(
      "define a = b + 1\n"
      "define b = a + 1\n"
      "region SR\n  select a > 0\n");
  CHECK(has_error_containing(hir, "cycle"));
}

void test_object_take_cycle() {
  auto hir = analyze("object a\n  take b\nobject b\n  take a\n");
  CHECK(has_error_containing(hir, "cycle"));
}

void test_bare_met() {
  auto hir = analyze(
      "object MET\n  take MissingET\n"
      "region SR\n  select MET > 250\n  select MET.pT > 250\n");
  auto selects = select_nodes(hir, "SR");
  CHECK(selects.size() == 2);
  if (selects.size() == 2 && selects[0].kind == HNode::Kind::Cmp &&
      selects[1].kind == HNode::Kind::Cmp && selects[0].a && selects[1].a) {
    CHECK(selects[0].a->kind == selects[1].a->kind);
    if (selects[0].a->kind == HNode::Kind::Quantity) {
      CHECK(selects[0].a->qid == selects[1].a->qid);
    }
  }
}

void test_size_aliases() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "region SR\n  select Size(jets) > 2\n  select size(jets) > 2\n  "
      "select count(jets) > 2\n  select jets.size > 2\n");
  int n = 0;
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::Size) ++n;
  }
  CHECK(n == 1);
}

void test_union_order() {
  auto hir = analyze(
      "object eles\n  take Ele\n"
      "object muons\n  take Muo\n"
      "object lep1\n  take union(eles, muons)\n"
      "object lep2\n  take union(muons, eles)\n"
      "object lep3\n  take eles\n  take muons\n");
  auto l1 = hir.collection_of("lep1");
  auto l2 = hir.collection_of("lep2");
  auto l3 = hir.collection_of("lep3");
  CHECK(l1 && l2 && !(*l1 == *l2));
  CHECK(l1 && l3 && *l1 == *l3);
}

void test_back_index() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "region SR\n  select jets[0].pT > 100\n  select jets[-1].pT > 30\n");
  auto selects = select_nodes(hir, "SR");
  CHECK(selects.size() == 2);
  if (selects.size() == 2) CHECK(!selects[1].has_unsupported());
  int backs = 0;
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::ElemProp && q.index.kind == ElemIndexKind::FromBack &&
        q.index.n == 1) {
      ++backs;
    }
  }
  CHECK(backs == 1);
}

void test_scalar_minmax() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "region SR\n  select min(jets[0].pT, jets[1].pT) > 30\n");
  auto selects = select_nodes(hir, "SR");
  CHECK(selects.size() == 1);
  if (!selects.empty()) CHECK(!selects[0].has_unsupported());
}

void test_bare_indexed_pt() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "region SR\n  select jets[1] > 30\n  select jets[1].pT > 30\n");
  auto selects = select_nodes(hir, "SR");
  CHECK(selects.size() == 2);
  if (!selects.empty()) CHECK(!selects[0].has_unsupported());
  int n = 0;
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::ElemProp && q.index.kind == ElemIndexKind::FromFront &&
        q.index.n == 1) {
      ++n;
    }
  }
  CHECK(n == 1);
}

void test_unknown_fn_unsupported() {
  auto hir = analyze("object muons\n  take Muon\n  select D0 < 2\n  select D0(Muon) < 2\n");
  auto muons = hir.collection_of("muons");
  CHECK(muons && hir.table.collection(*muons).kind == CollectionKind::Filtered);
  if (muons && hir.table.collection(*muons).kind == CollectionKind::Filtered) {
    CHECK(hir.elem_pred(hir.table.collection(*muons).pred).node.has_unsupported());
  }
}

void test_region_pred() {
  auto hir = analyze(
      "region presel\n  select MET > 100\n"
      "region SR1\n  select presel\n  select MET > 200\n"
      "region SR2\n  presel\n  select MET > 300\n");
  auto s1 = select_nodes(hir, "SR1");
  CHECK(!s1.empty() && s1[0].kind == HNode::Kind::RegionPred && s1[0].region_index == 0);
  const HirRegion* sr2 = hir.region("SR2");
  CHECK(sr2 && !sr2->stmts.empty() && sr2->stmts[0].kind == HirRegionStmt::Kind::Inherit &&
        sr2->stmts[0].region == 0);
}

void test_pt_sort_alias() {
  auto hir = analyze(
      "object jets\n  take Jet\n  select pT > 30\n"
      "object sjets\n  take sort(jets, pt(jets), descend)\n");
  auto jets = hir.collection_of("jets");
  auto sjets = hir.collection_of("sjets");
  CHECK(jets && sjets && *jets == *sjets);
  bool any_sorted = false;
  for (const auto& c : hir.table.collections()) {
    if (c.kind == CollectionKind::Sorted) any_sorted = true;
  }
  CHECK(!any_sorted);
}

void test_ascend_sort_no_alias() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "object sjets\n  take sort(jets, pt(jets), ascend)\n");
  auto jets = hir.collection_of("jets");
  auto sjets = hir.collection_of("sjets");
  CHECK(jets && sjets && !(*jets == *sjets));
  CHECK(sjets && hir.table.collection(*sjets).kind == CollectionKind::Sorted);
  std::string pt = ExtDecls::legacy().prop_canon("pt").first;
  CHECK(sjets && !hir.table.pt_ordered(*sjets, pt));
}

void test_sort_over_union() {
  auto hir = analyze(
      "object eles\n  take Ele\n"
      "object muons\n  take Muo\n"
      "object leptons\n  take union(eles, muons)\n"
      "object sleptons\n  take sort(leptons, pt(leptons), descend)\n");
  auto leptons = hir.collection_of("leptons");
  auto sleptons = hir.collection_of("sleptons");
  CHECK(leptons && sleptons && !(*sleptons == *leptons));
  CHECK(sleptons && hir.table.collection(*sleptons).kind == CollectionKind::Sorted);
}

void test_sort_non_pt() {
  auto hir = analyze(
      "object jets\n  take Jet\n"
      "object sjets\n  take sort(jets, eta(jets), descend)\n");
  auto jets = hir.collection_of("jets");
  auto sjets = hir.collection_of("sjets");
  CHECK(jets && sjets && !(*sjets == *jets));
  CHECK(sjets && hir.table.collection(*sjets).kind == CollectionKind::Sorted);
}

void test_unresolved_private_base() {
  for (const char* src : {
           "object JETclean\n  take antikT(Jet, 0.4)\n  select pt > 100\n",
           "object JETclean\n  select pt > 100\n",
       }) {
    auto hir = analyze(src);
    auto coll = hir.collection_of("JETclean");
    CHECK(coll.has_value());
    if (!coll) continue;
    Symbol base;
    std::vector<ElemPredId> preds;
    CHECK(hir.table.filter_chain(*coll, base, preds));
    std::string label = hir.symbols.display(base);
    CHECK(label.find("test.adl::") == 0);
    CHECK(label.find("#unresolved") != std::string::npos);
  }
  auto hir = analyze(
      "object a\n  take b\n  select pt > 1\nobject b\n  take a\n  select pt > 2\n");
  CHECK(has_errors(hir.diags));
  for (const char* name : {"a", "b"}) {
    auto coll = hir.collection_of(name);
    CHECK(coll.has_value());
    if (!coll) continue;
    Symbol base;
    std::vector<ElemPredId> preds;
    CHECK(hir.table.filter_chain(*coll, base, preds));
    CHECK(hir.symbols.display(base).find("#unresolved") != std::string::npos);
  }
}

void test_oversized_index() {
  auto hir = analyze(
      "region SR\n  select pT(Jet[5000000000]) > 0\n  select pT(Jet[4294967295]) > 0\n");
  bool saw_clamped = false;
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::ElemProp && q.index.kind == ElemIndexKind::FromFront) {
      CHECK(q.index.n != 0xFFFFFFFFu);
      if (q.index.n == MAX_SOURCE_ELEM_INDEX) saw_clamped = true;
    }
  }
  CHECK(saw_clamped);
}

void test_unsupported_cuts_fresh() {
  auto hir = analyze(
      "object eles\n  take Ele\nobject muons\n  take Muo\n"
      "object cleanA\n  take Jet\n  reject any(dR(this, eles) < 0.2 and pt(eles) > 10)\n"
      "object cleanB\n  take Jet\n  reject any(dR(this, muons) < 0.4 and pt(muons) > 20)\n");
  auto a = hir.collection_of("cleanA");
  auto b = hir.collection_of("cleanB");
  CHECK(a && b && !(*a == *b));
  auto hir2 = analyze(
      "object x1\n  take Jet\n  select pt > 30\nobject x2\n  take Jet\n  select pt > 30\n");
  auto x1 = hir2.collection_of("x1");
  auto x2 = hir2.collection_of("x2");
  CHECK(x1 && x2 && *x1 == *x2);
}

void test_elem_context_externals() {
  auto hir = analyze(
      "object bigA\n  take Jet\n  select sqrt(pt) > 5\n"
      "object bigB\n  take Muo\n  select sqrt(pt) < 2\n");
  for (const auto& q : hir.table.quantities()) {
    if (q.kind == QuantityKind::ExternalFn) {
      for (const auto& a : q.args) {
        if (a.kind == QuantityArgKind::Opaque) {
          CHECK(a.text.find("this.") == std::string::npos);
          CHECK(a.text.find("<unsupported:") == std::string::npos);
        }
      }
    }
  }
  auto a = hir.collection_of("bigA");
  auto b = hir.collection_of("bigB");
  CHECK(a && b && !(*a == *b));
}

void test_render_injectivity() {
  struct Pair {
    const char* what;
    const char* ca;
    const char* cb;
  };
  Pair pairs[] = {
      {"literal value", "pt > 30", "pt > 31"},
      {"literal raw text", "pt > 1", "pt > 1.0"},
      {"property", "pt > 30", "eta > 30"},
      {"relation", "pt > 30", "pt >= 30"},
      {"comparison side", "pt > 30", "pt < 30"},
      {"boolean connective", "pt > 30 and eta < 2", "pt > 30 or eta < 2"},
      {"negation", "pt > 30", "not pt > 30"},
      {"band kind", "pt [] 20 30", "pt ][ 20 30"},
      {"band bound", "pt [] 20 30", "pt [] 20 31"},
      {"ternary guard", "pt > 30 ? eta < 2 : m > 5", "pt > 40 ? eta < 2 : m > 5"},
      {"ternary branch", "pt > 30 ? eta < 2 : m > 5", "pt > 30 ? eta < 1 : m > 5"},
      {"abs presence", "eta < 2", "abs(eta) < 2"},
      {"arith op", "pt + m > 30", "pt - m > 30"},
      {"arith operand", "pt + m > 30", "pt + e > 30"},
  };
  for (const auto& p : pairs) {
    std::string src = std::string("object xa\n  take Jet\n  select ") + p.ca +
                      "\nobject xb\n  take Jet\n  select " + p.cb + "\n";
    auto hir = analyze(src.c_str());
    auto a = hir.collection_of("xa");
    auto b = hir.collection_of("xb");
    bool distinct = a && b && !(*a == *b);
    if (!distinct) {
      std::cerr << "FAIL render injectivity: " << p.what << "  " << p.ca << " vs "
                << p.cb << "\n";
      ++g_fails;
    } else {
      ++g_pass;
    }
  }
}

void test_object_scoped_define() {
  auto hir = analyze(
      "object direct\n  take Jet\n  select pt / e > 0.5\n"
      "object viadefine\n  take Jet\n  define ptratio = pt / e\n  select ptratio > 0.5\n"
      "object child\n  take viadefine\n  select ptratio > 0.7\n");
  CHECK(!has_errors(hir.diags));
  auto direct = hir.collection_of("direct");
  auto via = hir.collection_of("viadefine");
  CHECK(direct && via);
  if (direct && via) {
    CHECK(hir.table.collection(*direct) == hir.table.collection(*via));
  }
  auto child = hir.collection_of("child");
  CHECK(child && hir.table.collection(*child).kind == CollectionKind::Filtered);
  if (child && hir.table.collection(*child).kind == CollectionKind::Filtered) {
    CHECK(hir.table.collection(*child).parent == *via);
  }
}

void test_underscore_index_notes() {
  {
    auto hir = analyze(
        "object goodJets\n  take Jet\n"
        "region SR\n  select goodJets_0.pt > 20\n");
    int notes = 0;
    bool extra = false;
    for (const auto& d : hir.diags) {
      if (d.severity != Severity::Note) continue;
      if (d.message.find("underscore-indexing operator") != std::string::npos) {
        ++notes;
      }
      if (d.message.find("more underscore-index split") != std::string::npos) {
        extra = true;
      }
    }
    CHECK(notes == 1);
    CHECK(!extra);
  }
  {
    auto hir = analyze(
        "object goodJets\n  take Jet\n"
        "region SR\n  select goodJets_0.pt > 20\n  select goodJets_1.pt > 10\n");
    bool extra = false;
    for (const auto& d : hir.diags) {
      if (d.severity == Severity::Note &&
          d.message == "(1 more underscore-index split in this file)") {
        extra = true;
      }
    }
    CHECK(extra);
  }
}

void test_object_scoped_at_region() {
  auto hir = analyze(
      "object jets\n  take Jet\n  define ptratio = pt / e\n"
      "region r\n  select ptratio > 0.5\n");
  CHECK(!has_errors(hir.diags));
  auto nodes = select_nodes(hir, "r");
  CHECK(nodes.size() == 1);
  if (!nodes.empty()) CHECK(nodes[0].has_unsupported());
}

void test_symbols_casefold() {
  SymbolTable t;
  auto a = t.intern("MissingET");
  auto b = t.intern("missinget");
  auto c = t.intern("MISSINGET");
  CHECK(a == b && b == c);
  CHECK(t.display(a) == "MissingET");
  CHECK(t.key(a) == "missinget");
  CHECK(!(t.intern("Jet") == a));
}

void test_ext_met_family() {
  auto ext = ExtDecls::legacy();
  for (const char* s : {"MET", "MissingET", "METLV", "Delphes_MissingET", "metlv"}) {
    const std::string* c = ext.base_collection(s);
    CHECK(c && *c == "MET");
    CHECK(ext.is_met_family(s));
  }
  CHECK(!ext.is_met_family("Jet"));
  const std::string* ak4 = ext.base_collection("AK4jet");
  CHECK(ak4 && *ak4 == "JET");
  const std::string* ele = ext.base_collection("Ele");
  CHECK(ele && *ele == "ELECTRON");
  auto pt1 = ext.prop_canon("pT");
  auto pt2 = ext.prop_canon("pt");
  CHECK(pt1.first == pt2.first);
  auto btag = ext.prop_canon("btag");
  auto ctag = ext.prop_canon("ctag");
  CHECK(btag.first != ctag.first);
  CHECK(ext.is_function("dR") && ext.is_function("dr") && ext.is_function("SQRT"));
  CHECK(!ext.is_function("D0"));
}

void test_subnormal_literal_is_an_error() {
  // Smash2 lexer.rs: nonzero subnormal f64 is a lexical error (exact Rat vs
  // f64 interpreter diverge only there).
  std::string lit = "0." + std::string(320, '0') + "1";
  std::string src =
      "object jets\n  take Jet\nregion R\n  select jets[0].pt > " + lit + "\n";
  auto hir = analyze(src.c_str());
  CHECK(has_error_containing(hir, "subnormal"));
}

void test_underflow_to_zero_is_not_an_error() {
  std::string lit = "0." + std::string(400, '0') + "1";
  std::string src =
      "object jets\n  take Jet\nregion R\n  select jets[0].pt > " + lit + "\n";
  auto hir = analyze(src.c_str());
  CHECK(!has_error_containing(hir, "subnormal"));
}

}  // namespace

int main() {
  test_symbols_casefold();
  test_ext_met_family();
  test_pure_rename();
  test_filtered_distinct();
  test_indexed_no_alias();
  test_property_spellings();
  test_angular();
  test_define_inline();
  test_boolean_define();
  test_define_cycle();
  test_object_take_cycle();
  test_bare_met();
  test_size_aliases();
  test_union_order();
  test_back_index();
  test_scalar_minmax();
  test_bare_indexed_pt();
  test_unknown_fn_unsupported();
  test_region_pred();
  test_pt_sort_alias();
  test_ascend_sort_no_alias();
  test_sort_over_union();
  test_sort_non_pt();
  test_unresolved_private_base();
  test_oversized_index();
  test_unsupported_cuts_fresh();
  test_elem_context_externals();
  test_render_injectivity();
  test_object_scoped_define();
  test_object_scoped_at_region();
  test_underscore_index_notes();
  test_subnormal_literal_is_an_error();
  test_underflow_to_zero_is_not_an_error();
  std::cout << "adl2_sema identity: PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
