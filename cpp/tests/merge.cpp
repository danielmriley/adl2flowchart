#include "adl2/sema/sema.hpp"

#include <iostream>
#include <optional>
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

Hir hir(const char* src, const char* unit) {
  return analyze_str(src, unit, ExtDecls::legacy());
}

std::size_t filtered_count(const Hir& m) {
  std::size_t n = 0;
  for (const auto& c : m.table.collections()) {
    if (c.kind == CollectionKind::Filtered) ++n;
  }
  return n;
}

void test_merge_doubles_regions_and_prefixes_names() {
  auto a = hir("region R\n  select MET.pt > 10\n", "a");
  auto b = hir("region S\n  select MET.pt > 20\n", "b");
  std::vector<const Hir*> units{&a, &b};
  auto m = merge_hirs(units);
  CHECK(m.regions.size() == 2);
  bool has_a = false, has_b = false;
  for (Symbol s : m.region_name_order) {
    std::string n = m.symbols.display(s);
    if (n == "a::R") has_a = true;
    if (n == "b::S") has_b = true;
  }
  CHECK(has_a);
  CHECK(has_b);
}

void test_merge_unifies_shared_event_scalar() {
  auto a = hir("region R\n  select MET.pt > 10\n", "a");
  auto b = hir("region S\n  select MET.pt > 20\n", "b");
  std::vector<const Hir*> units{&a, &b};
  auto m = merge_hirs(units);
  std::size_t met = 0;
  for (const auto& q : m.table.quantities()) {
    if (q.kind == QuantityKind::EventScalar && q.scalar.kind == ScalarSourceKind::MetProp) {
      ++met;
    }
  }
  CHECK(met == 1);
}

void test_merge_keeps_differently_cut_objects_distinct() {
  auto a = hir(
      "object goodjets\n  take Jet\n  select pt > 30\nregion R\n  select size(goodjets) >= 1\n",
      "a");
  auto b = hir(
      "object goodjets\n  take Jet\n  select pt > 100\nregion S\n  select size(goodjets) >= 1\n",
      "b");
  std::vector<const Hir*> units{&a, &b};
  auto m = merge_hirs(units);
  CHECK(filtered_count(m) == 2);

  auto b_same = hir(
      "object goodjets\n  take Jet\n  select pt > 30\nregion S\n  select size(goodjets) >= 1\n",
      "b");
  std::vector<const Hir*> same{&a, &b_same};
  auto m2 = merge_hirs(same);
  CHECK(filtered_count(m2) == 1);
}

void test_merge_never_unifies_unsupported_cuts() {
  auto a = hir(
      "object o1\n  take Jet\n  select sum(pt(Muon) + pt(Electron)) > 5\n"
      "region RA\n  select size(o1) >= 3\n",
      "a");
  auto b = hir(
      "object o2\n  take Jet\n  select sum(eta(Photon) * eta(Tau)) > 5\n"
      "region RB\n  select size(o2) <= 1\n",
      "b");
  CHECK(!a.elem_preds.empty() && !b.elem_preds.empty());
  CHECK(a.elem_preds[0].render == b.elem_preds[0].render);
  CHECK(a.elem_preds[0].node.has_unsupported());
  std::vector<const Hir*> units{&a, &b};
  auto m = merge_hirs(units);
  CHECK(filtered_count(m) == 2);
  std::vector<ElemPredId> preds;
  for (const auto& c : m.table.collections()) {
    if (c.kind == CollectionKind::Filtered) preds.push_back(c.pred);
  }
  CHECK(preds.size() == 2);
  if (preds.size() == 2) CHECK(preds[0] != preds[1]);
}

void test_merge_still_unifies_identical_supported_cuts() {
  auto a = hir("object j\n  take Jet\n  select pt > 30\nregion R\n  select size(j) >= 1\n", "a");
  auto b = hir("object k\n  take Jet\n  select pt > 30\nregion S\n  select size(k) >= 1\n", "b");
  std::vector<const Hir*> units{&a, &b};
  auto m = merge_hirs(units);
  CHECK(filtered_count(m) == 1);
}

void test_merge_of_one_preserves_region_count() {
  auto a = hir("region R\n  select MET.pt > 10\nregion S\n  select MET.pt < 5\n", "solo");
  std::vector<const Hir*> units{&a};
  auto m = merge_hirs(units);
  CHECK(m.regions.size() == a.regions.size());
}

void test_colliding_unit_labels_get_ordinal() {
  auto a = hir("region SRA\n  select MET.pt > 200\n", "atlas.adl");
  auto b = hir("region SRA\n  select MET.pt > 100\nregion SRB\n  select MET.pt < 50\n",
              "atlas.adl");
  std::vector<const Hir*> units{&a, &b};
  auto m = merge_hirs(units);
  CHECK(m.regions.size() == 3);
  std::vector<std::string> names;
  for (Symbol s : m.region_name_order) names.push_back(m.symbols.display(s));
  CHECK(names.size() == 3);
  CHECK(names[0] != names[1]);
  CHECK(names[0] != names[2]);
  CHECK(names[1] != names[2]);
}

CollectionId base_coll(QuantityTable& t, std::uint32_t s) {
  return t.intern_collection(Collection::of_base(Symbol{s}));
}
CollectionId filt_coll(QuantityTable& t, CollectionId parent, std::uint32_t p) {
  return t.intern_collection(Collection::filtered(parent, ElemPredId{p}));
}

void test_filter_chain_and_candidates() {
  QuantityTable t;
  auto jet = base_coll(t, 1);
  auto ele = base_coll(t, 2);
  auto jet_a = filt_coll(t, jet, 0);
  auto jet_b = filt_coll(t, jet, 1);
  auto ele_a = filt_coll(t, ele, 0);
  (void)base_coll(t, 3);
  auto sl = t.intern_collection(Collection::slice(jet, 0, std::nullopt));
  auto f_over_slice = filt_coll(t, sl, 5);
  Symbol base;
  std::vector<ElemPredId> preds;
  CHECK(t.filter_chain(jet, base, preds) && preds.empty() && base.id == 1);
  CHECK(t.filter_chain(jet_a, base, preds) && preds.size() == 1 && preds[0].id == 0);
  CHECK(!t.filter_chain(sl, base, preds));
  CHECK(!t.filter_chain(f_over_slice, base, preds));
  auto cands = t.reconciliation_candidates();
  CHECK(cands.size() == 1);
  if (!cands.empty()) {
    CHECK(cands[0].first == jet_a && cands[0].second == jet_b);
  }
  for (const auto& p : cands) {
    CHECK(p.first != ele_a && p.second != ele_a);
  }
}

}  // namespace

int main() {
  test_merge_doubles_regions_and_prefixes_names();
  test_merge_unifies_shared_event_scalar();
  test_merge_keeps_differently_cut_objects_distinct();
  test_merge_never_unifies_unsupported_cuts();
  test_merge_still_unifies_identical_supported_cuts();
  test_merge_of_one_preserves_region_count();
  test_colliding_unit_labels_get_ordinal();
  test_filter_chain_and_candidates();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
