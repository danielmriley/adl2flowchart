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

/// Surface form → canonical dump / sema key (`aliases.txt`).
/// Production membership is not meaning: sitting next to `"or"` does not
/// make a new word an alias.
struct Alias {
  std::string surface;
  std::string canonical;
};

/// Parse `aliases.txt` text (`#` comments, surface + canonical per line).
bool parse_aliases(const std::string& text, std::vector<Alias>& out,
                   std::string& error);

/// Loaded alias table (`||`→`or`, `&&`→`and`, `!`→`not` in stock).
const std::vector<Alias>& alias_table();
const Alias* lookup_alias(const std::string& lit);

/// A leftover keyword-map entry. Sibling inherit is gone: new words are
/// not synonyms and must not appear here with a borrowed TokKind/BinOp.
struct Synonym {
  std::string lit;
  std::string tok;
  std::string bin;
  std::string un;
};

/// Walk all-literal alternation groups.
/// Unknown *punctuation* fails closed. Unknown *words* are new keys
/// (lowercase lexeme) — they do not inherit a sibling's TokKind/BinOp.
bool resolve_synonyms(const Grammar& g, std::vector<Synonym>& out,
                      std::string& error);

/// Emit extra lexer-map entries for `keyword_synonyms.inc.hpp`.
/// New operator words (xor) are Ident matches, not keyword-map rows.
bool emit_keyword_synonyms(const Grammar& g, std::string& out,
                           std::string& error);

}  // namespace adl2::rdgen
