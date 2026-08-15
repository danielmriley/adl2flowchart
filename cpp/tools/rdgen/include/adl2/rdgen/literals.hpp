#pragma once

#include "adl2/rdgen/ebnf.hpp"

#include <string>
#include <vector>

namespace adl2::rdgen {

/// Binding of an EBNF string literal to the existing C++ token / AST enums.
struct LitBind {
  const char* lit = nullptr;
  const char* tok = nullptr;  // TokKind enumerator name, or null
  const char* bin = nullptr;  // BinOp enumerator name, or null
  const char* un = nullptr;   // UnaryOp enumerator name, or null
  const char* cmp = nullptr;  // CmpOp enumerator name, or null
  bool keyword = false;       // word that belongs in the lexer map
};

const LitBind* lookup_lit(const std::string& lit);

/// A word literal that is not in the frozen catalog but sits in the same
/// alternation as a known keyword — it inherits that keyword's TokKind
/// (and BinOp / UnaryOp). Grammar authors add `"xor"` next to `"or"`;
/// they do not edit lexer.cpp or parser.cpp.
struct Synonym {
  std::string lit;
  std::string tok;
  std::string bin;
  std::string un;
};

/// Resolve new word literals in all-literal alternation groups.
/// Unknown *symbolic* operators (not words) fail closed.
bool resolve_synonyms(const Grammar& g, std::vector<Synonym>& out,
                      std::string& error);

/// Emit extra lexer-map entries for `keyword_synonyms.inc.hpp`.
bool emit_keyword_synonyms(const Grammar& g, std::string& out,
                           std::string& error);

}  // namespace adl2::rdgen
