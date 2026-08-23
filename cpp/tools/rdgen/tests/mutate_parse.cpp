#include "adl2/syntax/dump.hpp"
#include "adl2/syntax/parser.hpp"

#include <iostream>
#include <string>

namespace {

bool contains(const std::string& hay, const char* needle) {
  return hay.find(needle) != std::string::npos;
}

// Recovery trees wrap Error leaves and never build a real Binary/Cmp.
bool error_only_tree(const std::string& dump) {
  if (!contains(dump, "Error")) return false;
  return !contains(dump, "Binary ") && !contains(dump, "Cmp ");
}

int fail(const char* which, const char* why, const adl2::syntax::ParseResult& r,
         const std::string& src, const std::string& dump) {
  std::cerr << "mutate_parse: " << which << ": " << why << "\n";
  if (r.diags.has_errors()) {
    std::cerr << r.diags.format_all();
  }
  std::cerr << "--- dump_ast ---\n" << dump;
  if (dump.empty()) std::cerr << "(empty dump; src was)\n" << src << "\n";
  return 1;
}

int check_xor() {
  const std::string src = "region R\n  select a xor b\n";
  const auto r = adl2::syntax::parse_source(src);
  const std::string dump = adl2::syntax::dump_ast(src, r.file);
  if (r.diags.has_errors()) {
    return fail("xor", "parse diagnostics have errors", r, src, dump);
  }
  if (error_only_tree(dump)) {
    return fail("xor", "dump is an error-only tree", r, src, dump);
  }
  if (!contains(dump, "Binary op=or")) {
    return fail("xor", "dump_ast must contain 'Binary op=or'", r, src, dump);
  }
  return 0;
}

int check_sel() {
  const std::string src = "region R\n  sel a > 1\n";
  const auto r = adl2::syntax::parse_source(src);
  const std::string dump = adl2::syntax::dump_ast(src, r.file);
  if (r.diags.has_errors()) {
    return fail("sel", "parse diagnostics have errors", r, src, dump);
  }
  if (error_only_tree(dump)) {
    return fail("sel", "dump is an error-only tree", r, src, dump);
  }
  if (!contains(dump, "Cut kw=select")) {
    return fail("sel", "dump_ast must contain 'Cut kw=select'", r, src, dump);
  }
  return 0;
}

}  // namespace

int main() {
  if (const int rc = check_xor()) return rc;
  if (const int rc = check_sel()) return rc;
  std::cout << "mutate_parse: PASS\n";
  return 0;
}
