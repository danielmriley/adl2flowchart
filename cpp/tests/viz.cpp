#include "adl2/viz/viz.hpp"
#include "adl2/sema/sema.hpp"

#include <iostream>
#include <string>

using namespace adl2::sema;
using adl2::viz::ast_dot;
using adl2::viz::flowchart_dot;

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

Hir hir_of(const char* src) { return analyze_str(src, "test.adl", ExtDecls::legacy()); }

std::string rtrim_copy(std::string s) {
  while (!s.empty() && (s.back() == '\n' || s.back() == '\r' || s.back() == ' ' ||
                        s.back() == '\t')) {
    s.pop_back();
  }
  return s;
}

std::size_t count_substr(const std::string& s, const char* needle) {
  std::size_t n = 0;
  for (std::size_t p = 0; (p = s.find(needle, p)) != std::string::npos; ++p) ++n;
  return n;
}

void test_flowchart_is_valid_digraph() {
  auto h = hir_of(
      "object jets\n  take Jet\n  select pT(Jet) > 30\nregion R\n  select pT(jets[0]) > 100\n");
  auto dot = flowchart_dot(h);
  CHECK(dot.compare(0, 19, "digraph flowchart {") == 0);
  auto t = rtrim_copy(dot);
  CHECK(!t.empty() && t.back() == '}');
  CHECK(dot.find("region0") != std::string::npos);
}

void test_ast_is_valid_digraph() {
  auto h = hir_of("region R\n  select MET.pT > 100\n");
  auto dot = ast_dot(h);
  CHECK(dot.compare(0, 13, "digraph ast {") == 0);
  auto t = rtrim_copy(dot);
  CHECK(!t.empty() && t.back() == '}');
}

void test_output_is_deterministic() {
  const char* src =
      "object jets\n  take Jet\n  select pT(Jet) > 30\nobject leptons : Union(Ele, Muo)\n"
      "region base\n  select size(jets) >= 2\nregion sr\n  base\n  select size(leptons) == 1\n";
  auto h1 = hir_of(src);
  auto h2 = hir_of(src);
  CHECK(flowchart_dot(h1) == flowchart_dot(h2));
  CHECK(ast_dot(h1) == ast_dot(h2));
}

void test_inheritance_and_take_edges_present() {
  const char* src =
      "object jets\n  take Jet\n  select pT(Jet) > 30\nregion base\n  select size(jets) >= 2\n"
      "region sr\n  base\n";
  auto h = hir_of(src);
  auto fc = flowchart_dot(h);
  CHECK(fc.find("[label=\"take\"]") != std::string::npos);
  CHECK(fc.find("[label=\"inherit\", style=dashed]") != std::string::npos);
}

void test_select_region_form_draws_the_same_inherit_edge() {
  // `select base` (region-as-predicate) inherits exactly like the bare-name
  // form and must draw the same dashed region->region edge (CORPUS gap 2).
  const char* src =
      "object jets\n  take Jet\nregion base\n  select size(jets) >= 2\nregion sr\n  "
      "select base\n  select size(jets) >= 4\n";
  auto h = hir_of(src);
  auto fc = flowchart_dot(h);
  CHECK(fc.find("region0 -> region1 [label=\"inherit\", style=dashed]") != std::string::npos);
  CHECK(count_substr(fc, "style=dashed") == 1);
}

void test_labels_are_escaped() {
  auto h = hir_of("region R\n  select pT(Jet[0]) > 100\n");
  auto dot = ast_dot(h);
  CHECK(dot.find('\t') == std::string::npos);
}

}  // namespace

int main() {
  test_flowchart_is_valid_digraph();
  test_ast_is_valid_digraph();
  test_output_is_deterministic();
  test_inheritance_and_take_edges_present();
  test_select_region_form_draws_the_same_inherit_edge();
  test_labels_are_escaped();
  std::cout << "PASS=" << g_pass << " FAIL=" << g_fails << "\n";
  return g_fails == 0 ? 0 : 1;
}
