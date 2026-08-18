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
  if (!contains(dump, "Binary op=xor")) {
    return fail("xor", "dump_ast must contain 'Binary op=xor'", r, src, dump);
  }
  if (contains(dump, "Binary op=or")) {
    return fail("xor", "dump_ast must not contain 'Binary op=or'", r, src, dump);
  }
  return 0;
}

int check_oror() {
  const std::string src = "region R\n  select a || b\n";
  const auto r = adl2::syntax::parse_source(src);
  const std::string dump = adl2::syntax::dump_ast(src, r.file);
  if (r.diags.has_errors()) {
    return fail("oror", "parse diagnostics have errors", r, src, dump);
  }
  if (error_only_tree(dump)) {
    return fail("oror", "dump is an error-only tree", r, src, dump);
  }
  if (!contains(dump, "Binary op=or")) {
    return fail("oror", "dump_ast must contain 'Binary op=or'", r, src, dump);
  }
  if (contains(dump, "Binary op=xor")) {
    return fail("oror", "dump_ast must not contain 'Binary op=xor'", r, src, dump);
  }
  return 0;
}

int check_cut_kw(const char* which, const char* src_body, const char* want,
                 const char* forbid) {
  const std::string src = std::string("region R\n  ") + src_body + "\n";
  const auto r = adl2::syntax::parse_source(src);
  const std::string dump = adl2::syntax::dump_ast(src, r.file);
  if (r.diags.has_errors()) {
    return fail(which, "parse diagnostics have errors", r, src, dump);
  }
  if (error_only_tree(dump)) {
    return fail(which, "dump is an error-only tree", r, src, dump);
  }
  if (!contains(dump, want)) {
    return fail(which, "dump_ast missing expected Cut keyword", r, src, dump);
  }
  if (forbid && contains(dump, forbid)) {
    return fail(which, "dump_ast must not inherit a sibling Cut keyword", r, src,
                dump);
  }
  return 0;
}

int check_object_sel() {
  const std::string src = "object jets\n  take Jet\n  sel pt > 20\n";
  const auto r = adl2::syntax::parse_source(src);
  const std::string dump = adl2::syntax::dump_ast(src, r.file);
  if (r.diags.has_errors()) {
    return fail("object-sel", "parse diagnostics have errors", r, src, dump);
  }
  if (!contains(dump, "Cut kw=sel")) {
    return fail("object-sel", "object-block must reach sel via is_cut_keyword",
                r, src, dump);
  }
  return 0;
}

}  // namespace

int main() {
  if (const int rc = check_xor()) return rc;
  if (const int rc = check_oror()) return rc;
  if (const int rc = check_cut_kw("sel", "sel a > 1", "Cut kw=sel",
                                  "Cut kw=select"))
    return rc;
  if (const int rc = check_cut_kw("foo", "foo a > 1", "Cut kw=foo",
                                  "Cut kw=select"))
    return rc;
  if (const int rc = check_object_sel()) return rc;
  std::cout << "mutate_parse: PASS\n";
  return 0;
}
