#include "adl2/rdgen/ebnf.hpp"
#include "adl2/rdgen/inventory.hpp"

#include <cstdlib>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

#ifndef ADL2_GRAMMAR_EBNF
#error "ADL2_GRAMMAR_EBNF must be set"
#endif

namespace {

int g_fail = 0;

void expect(bool cond, const char* msg) {
  if (!cond) {
    std::cerr << "FAIL: " << msg << "\n";
    ++g_fail;
  }
}

std::string slurp(const char* path) {
  std::ifstream in(path);
  std::ostringstream ss;
  ss << in.rdbuf();
  return ss.str();
}

}  // namespace

int main() {
  using namespace adl2::rdgen;

  const std::string ebnf_src = slurp(ADL2_GRAMMAR_EBNF);
  const Grammar g = parse_ebnf(ebnf_src);
  expect(g.error.empty(), "frozen grammar.ebnf parses");

  Inventory inv;
  std::string err;
  expect(build_inventory(g, inv, err), "frozen grammar inventory builds");
  expect(inv.ok(), "frozen grammar inventory ok()");
  expect(err.empty(), "frozen grammar inventory has no error string");
  if (!inv.ok()) {
    for (const auto& e : inv.errors) std::cerr << "  inventory: " << e << "\n";
  }

  {
    const LitClassRow* bins = find_lit(inv, "bins");
    expect(bins != nullptr, "bins row present");
    if (bins) {
      expect(bins->cls == LitClass::ContextualIdent, "bins is ContextualIdent");
      expect(!is_keyword_class(bins->cls), "bins is not a keyword class");
    }
  }

  {
    const LitClassRow* arrow = find_lit(inv, "->");
    expect(arrow != nullptr, "-> row present");
    if (arrow) expect(arrow->cls == LitClass::Extra, "-> is Extra");
  }

  {
    const LitClassRow* all = find_lit(inv, "all");
    const LitClassRow* none = find_lit(inv, "none");
    expect(all != nullptr, "all appears");
    expect(none != nullptr, "none appears");
    if (all) expect(all->cls == LitClass::PrimaryKw, "all is PrimaryKw");
    if (none) expect(none->cls == LitClass::PrimaryKw, "none is PrimaryKw");
  }

  {
    const LitClassRow* word_or = find_lit(inv, "or");
    const LitClassRow* sym_or = find_lit(inv, "||");
    expect(word_or != nullptr, "or row present");
    expect(sym_or != nullptr, "|| row present");
    if (word_or) {
      expect(word_or->cls == LitClass::ExprOpWord, "or is expr-op-word");
    }
    if (sym_or) {
      expect(sym_or->cls == LitClass::SymbolicOp, "|| is symbolic-op");
    }
  }

  {
    const Grammar bad = parse_ebnf("foo = \"noSuchKw123\" ;\n");
    expect(bad.error.empty(), "synthetic grammar parses");
    Inventory bad_inv;
    std::string bad_err;
    expect(!build_inventory(bad, bad_inv, bad_err),
           "unclassified literal fails closed");
    expect(!bad_inv.ok(), "synthetic inventory !ok()");
    const LitClassRow* row = find_lit(bad_inv, "noSuchKw123");
    expect(row != nullptr, "noSuchKw123 row present");
    if (row) {
      expect(row->cls == LitClass::Unclassified, "noSuchKw123 is Unclassified");
    }
    bool mentioned = false;
    for (const auto& e : bad_inv.errors) {
      if (e.find("noSuchKw123") != std::string::npos) mentioned = true;
    }
    expect(mentioned, "error names the unclassified literal");
  }

  // Multi-role spellings: one row, no error solely for the extra role.
  {
    const LitClassRow* trig = find_lit(inv, "trigger");
    expect(trig != nullptr, "trigger row present");
    if (trig) {
      expect(trig->roles.size() >= 2, "trigger lists multiple roles");
      expect(trig->note.find("region-stmt") != std::string::npos,
             "trigger note lists region-stmt role");
    }
    const LitClassRow* defn = find_lit(inv, "define");
    expect(defn != nullptr, "define row present");
    if (defn) {
      expect(defn->roles.size() >= 2, "define lists multiple roles");
    }
  }

  // path-token is a production, not a quoted literal — no invented row.
  expect(find_lit(inv, "path-token") == nullptr,
         "path-token has no literal row");

  if (g_fail) {
    std::cerr << g_fail << " failure(s)\n";
    return 1;
  }
  std::cout << "adl2_rdgen_inventory: PASS (" << inv.rows.size() << " rows)\n";
  return 0;
}
