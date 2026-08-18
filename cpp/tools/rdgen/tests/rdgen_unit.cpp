#include "adl2/rdgen/check.hpp"
#include "adl2/rdgen/ebnf.hpp"
#include "adl2/rdgen/emit.hpp"
#include "adl2/rdgen/literals.hpp"

#include <algorithm>
#include <cstdlib>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>

#ifndef ADL2_GRAMMAR_EBNF
#error "ADL2_GRAMMAR_EBNF must be set"
#endif
#ifndef ADL2_PARSER_HPP
#error "ADL2_PARSER_HPP must be set"
#endif
#ifndef ADL2_METHOD_MAP
#error "ADL2_METHOD_MAP must be set"
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

  {
    const Grammar bad = parse_ebnf("or-expr = and-expr { (\"or\") ");
    expect(!bad.error.empty(), "unterminated production is an error");
  }

  {
    const Grammar g = parse_ebnf(
        "condition = ternary ;\n"
        "or-expr = and-expr { (\"or\" | \"||\") and-expr } ;\n"
        "unary = \"-\" unary | postfix ;\n"
        "path-token = (* comment *) ;\n"
        "ternary = or-expr [ \"?\" ternary [ \":\" ternary ] ] ;\n"
        "cmp-op = \">\" | \"<\" ;\n"
        "section = info-block | region-block ;\n");
    expect(g.error.empty(), "mini grammar parses");
    expect(g.find("or-expr") != nullptr, "find or-expr");
    expect(classify(*g.find("condition")).shape == Shape::Alias, "alias");
    expect(classify(*g.find("or-expr")).shape == Shape::LeftAssoc, "left-assoc");
    expect(classify(*g.find("unary")).shape == Shape::PrefixUnary, "prefix");
    expect(classify(*g.find("path-token")).shape == Shape::Empty, "empty");
    expect(classify(*g.find("ternary")).shape == Shape::OptionalSuffix,
           "optional suffix");
    expect(classify(*g.find("cmp-op")).shape == Shape::TokenClass, "token class");
    expect(classify(*g.find("section")).shape == Shape::Choice, "choice");
    expect(hyphen_to_underscore("or-expr") == "or_expr", "hyphen");
    expect(parse_method_name("info-block") == "parse_info_block", "method name");
  }

  const std::string ebnf_src = slurp(ADL2_GRAMMAR_EBNF);
  const std::string hpp = slurp(ADL2_PARSER_HPP);
  const std::string map_src = slurp(ADL2_METHOD_MAP);
  const Grammar g = parse_ebnf(ebnf_src);
  expect(g.error.empty(), "frozen grammar.ebnf parses");
  expect(g.prods.size() == 46, "46 productions in grammar.ebnf");
  expect(g.find("or-expr") && classify(*g.find("or-expr")).shape == Shape::LeftAssoc,
         "or-expr LeftAssoc");
  expect(g.find("and-expr") &&
             classify(*g.find("and-expr")).shape == Shape::LeftAssoc,
         "and-expr LeftAssoc");
  expect(g.find("not-expr") &&
             classify(*g.find("not-expr")).shape == Shape::PrefixUnary,
         "not-expr PrefixUnary");
  expect(g.find("unary") && classify(*g.find("unary")).shape == Shape::PrefixUnary,
         "unary PrefixUnary");
  expect(g.find("additive") &&
             classify(*g.find("additive")).shape == Shape::LeftAssoc,
         "additive LeftAssoc");
  expect(g.find("multiplicative") &&
             classify(*g.find("multiplicative")).shape == Shape::LeftAssoc,
         "multiplicative LeftAssoc");
  expect(g.find("condition") &&
             classify(*g.find("condition")).shape == Shape::Alias,
         "condition Alias");
  expect(g.find("path-token") &&
             classify(*g.find("path-token")).shape == Shape::Empty,
         "path-token Empty");
  expect(g.find("object-define") &&
             classify(*g.find("object-define")).shape == Shape::Alias,
         "object-define is define alias in EBNF (indent is a hook)");

  const MethodMap map = parse_method_map(map_src);
  expect(map.error.empty(), "method_map.txt parses");
  const CheckResult cr = check_grammar(g, map, hpp);
  for (const auto& e : cr.errors) {
    std::cerr << "check: " << e.message << "\n";
  }
  expect(cr.ok(), "frozen grammar ↔ parser.hpp ↔ method_map");

  std::string emitted;
  std::string emit_err;
  expect(emit_expr_ladder(g, map, emitted, emit_err), "emit succeeds");
  expect(emit_err.empty(), "emit has no error");
  expect(emitted.find("Parser::parse_or_expr") != std::string::npos,
         "emits parse_or_expr");
  expect(emitted.find("Parser::parse_condition") != std::string::npos,
         "emits parse_condition");
  expect(emitted.find("Parser::parse_unary") != std::string::npos,
         "emits parse_unary");
  expect(emitted.find("Parser::parse_ternary") != std::string::npos,
         "emits parse_ternary");
  expect(emitted.find("Parser::parse_reject_stmt") != std::string::npos,
         "emits parse_reject_stmt");
  expect(emitted.find("Parser::parse_trigger_stmt") != std::string::npos,
         "emits parse_trigger_stmt");
  expect(emitted.find("Parser::parse_cut_as_region") != std::string::npos,
         "emits parse_cut_as_region");
  expect(emitted.find("Parser::parse_comparison") == std::string::npos,
         "does not emit parse_comparison");
  expect(emitted.find("Parser::parse_section") == std::string::npos,
         "expr emit does not write parse_section");
  expect(emitted.find("lower_copy(kw_tok.text)") != std::string::npos,
         "cut keyword is lowercase token text");

  {
    std::string dispatched;
    std::string derr;
    expect(emit_dispatch(g, map, dispatched, derr), "dispatch emit succeeds");
    expect(derr.empty(), "dispatch has no error");
    expect(dispatched.find("Parser::parse_section") != std::string::npos,
           "emits parse_section");
    expect(dispatched.find("Parser::parse_region_stmt") != std::string::npos,
           "emits parse_region_stmt");
    expect(dispatched.find("Parser::at_section_start") != std::string::npos,
           "emits at_section_start");
    expect(dispatched.find("Parser::at_stmt_keyword") != std::string::npos,
           "emits at_stmt_keyword");
    expect(dispatched.find("Parser::is_cut_keyword") != std::string::npos,
           "emits is_cut_keyword");
    expect(dispatched.find("Parser::is_reject_keyword") != std::string::npos,
           "emits is_reject_keyword");
    expect(dispatched.find("is_ident_text(\"bins\")") != std::string::npos,
           "bins stays contextual");
    expect(dispatched.find("TokKind::KwTake") != std::string::npos,
           "take extra in region-stmt / at_stmt_keyword");
    expect(dispatched.find("iequals(peek().text, \"foo\")") == std::string::npos,
           "stock dispatch has no foo");
    expect(dispatched.find("iequals(peek().text, \"sel\")") == std::string::npos,
           "stock dispatch has no sel");
    expect(dispatched.find("TokKind::KwTrigger") != std::string::npos,
           "trigger is in generated first-sets");
  }

  {
    const Grammar extra = parse_ebnf(ebnf_src + "\nfoo-stmt = \"foo\" condition ;\n");
    expect(extra.error.empty(), "frozen + foo-stmt parses");
    const CheckResult inferred = check_grammar(extra, map, hpp);
    for (const auto& e : inferred.errors) {
      std::cerr << "inferred: " << e.message << "\n";
    }
    expect(inferred.ok(), "unmapped keywords-condition is inferred generate");
  }

  {
    std::vector<Synonym> syns;
    std::string serr;
    expect(resolve_synonyms(g, syns, serr), "stock grammar synonyms resolve");
    expect(syns.empty(), "stock grammar has no extra keyword synonyms");
  }

  {
    const std::string map_path = ADL2_METHOD_MAP;
    const auto slash = map_path.find_last_of("/\\");
    const std::string aliases_path =
        (slash == std::string::npos ? std::string(".")
                                    : map_path.substr(0, slash)) +
        "/aliases.txt";
    const std::string aliases_src = slurp(aliases_path.c_str());
    expect(!aliases_src.empty(), "aliases.txt is readable");
    std::vector<Alias> aliases;
    std::string aerr;
    expect(parse_aliases(aliases_src, aliases, aerr), "aliases.txt parses");
    expect(aliases.size() == 3, "three stock aliases");
    bool saw_or = false, saw_and = false, saw_not = false;
    for (const auto& a : aliases) {
      if (a.surface == "||" && a.canonical == "or") saw_or = true;
      if (a.surface == "&&" && a.canonical == "and") saw_and = true;
      if (a.surface == "!" && a.canonical == "not") saw_not = true;
    }
    expect(saw_or && saw_and && saw_not, "||→or &&→and !→not");
    expect(lookup_alias("||") && lookup_alias("||")->canonical == "or",
           "lookup_alias ||");
  }

  {
    std::string mutated = ebnf_src;
    const std::string from_or = "(\"or\"|\"||\")";
    const std::string to_or = "(\"or\"|\"||\"|\"xor\")";
    auto repl = [](std::string& s, const std::string& a, const std::string& b) {
      const auto pos = s.find(a);
      expect(pos != std::string::npos, "mutation needle present in grammar.ebnf");
      if (pos != std::string::npos) s.replace(pos, a.size(), b);
    };
    const std::string from_cut = "(\"select\"|\"cut\"|\"cmd\"|\"command\")";
    const std::string to_cut = "(\"select\"|\"cut\"|\"cmd\"|\"command\"|\"sel\")";
    const std::string from_ref = "| region-ref ;";
    const std::string to_ref =
        "| foo-stmt | region-ref ;\nfoo-stmt        = \"foo\" condition ;";
    repl(mutated, from_or, to_or);
    repl(mutated, from_cut, to_cut);
    repl(mutated, from_ref, to_ref);
    const Grammar mg = parse_ebnf(mutated);
    expect(mg.error.empty(), "mutated grammar parses");
    std::vector<Synonym> syns;
    std::string serr;
    expect(resolve_synonyms(mg, syns, serr), "xor as a new key resolves");
    bool xor_is_kwor = false;
    for (const auto& s : syns) {
      if (s.lit == "xor" && (s.tok == "KwOr" || s.bin == "Or")) xor_is_kwor = true;
    }
    expect(!xor_is_kwor, "xor is NOT KwOr / BinOp::Or");
    expect(syns.empty(), "xor is not a keyword synonym");
    std::string kws;
    expect(emit_keyword_synonyms(mg, kws, serr), "emit keywords after xor");
    expect(kws.find("{\"xor\", TokKind::KwOr}") == std::string::npos,
           "does not emit xor as KwOr");
    expect(kws.find("{\"xor\", TokKind::KwXor}") == std::string::npos,
           "does not emit KwXor");
    std::string xor_emitted;
    expect(emit_expr_ladder(mg, map, xor_emitted, serr), "emit mutated or-expr");
    expect(xor_emitted.find("iequals(peek().text, \"xor\")") != std::string::npos,
           "xor matches Ident text");
    expect(xor_emitted.find("bin_key") != std::string::npos, "xor sets bin_key");
    expect(xor_emitted.find("TokKind::KwXor") == std::string::npos,
           "emit does not invent KwXor");
    const CheckResult mcheck = check_grammar(mg, map, hpp);
    for (const auto& e : mcheck.errors) {
      std::cerr << "mutate-check: " << e.message << "\n";
    }
    expect(mcheck.ok(), "mutated xor/sel/foo still checks");
    std::string dmut;
    expect(emit_dispatch(mg, map, dmut, serr), "dispatch emit after sel/foo");
    expect(dmut.find("iequals(peek().text, \"sel\")") != std::string::npos,
           "sel is an Ident first-set of cut-stmt");
    expect(dmut.find("iequals(peek().text, \"foo\")") != std::string::npos,
           "foo-stmt is inlined as Ident Cut");
    expect(dmut.find("parse_foo_stmt") == std::string::npos,
           "does not invent parse_foo_stmt");
  }

  expect(emitted.find("e->bin_key") == std::string::npos,
         "stock emit has no bin_key");

  {
    const Grammar bad = parse_ebnf(
        "or-expr = and-expr { (\"or\" | \"@@\") and-expr } ;\n");
    std::vector<Synonym> syns;
    std::string serr;
    expect(!resolve_synonyms(bad, syns, serr),
           "unknown symbolic operator fails closed");
  }

  // Checker notices a missing production mapping.
  {
    MethodMap slim = map;
    slim.entries.erase(
        std::remove_if(slim.entries.begin(), slim.entries.end(),
                       [](const MapEntry& e) { return e.name == "or-expr"; }),
        slim.entries.end());
    const CheckResult bad = check_grammar(g, slim, hpp);
    expect(!bad.ok(), "missing or-expr mapping fails check");
  }

  if (g_fail) {
    std::cerr << g_fail << " failure(s)\n";
    return 1;
  }
  std::cout << "adl2_rdgen_unit: PASS\n";
  return 0;
}
