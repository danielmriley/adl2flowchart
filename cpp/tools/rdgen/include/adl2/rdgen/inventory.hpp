#pragma once

#include "adl2/rdgen/ebnf.hpp"

#include <string>
#include <vector>

namespace adl2::rdgen {

/// Token-class of one quoted EBNF literal or a lexer extra not in the EBNF.
enum class LitClass {
  SectionKw,
  RegionStmtKw,
  ObjectStmtKw,
  ExprOpWord,   // and / or / not
  PrimaryKw,    // all / none / true / false
  OtherKw,      // tabletype, union, …
  ContextualIdent,  // bins, …
  SymbolicOp,
  Extra,        // lexer-only (Arrow, …) or structural
  Unclassified,
};

struct LitClassRow {
  std::string lit;
  LitClass cls = LitClass::Unclassified;
  std::string note;
};

struct Inventory {
  std::vector<LitClassRow> rows;
  std::vector<std::string> errors;
  bool ok() const { return errors.empty(); }
};

/// Classify every `"…"` in the grammar plus known lexer extras.
/// Fail closed on an unclassified EBNF literal.
bool build_inventory(const Grammar& g, Inventory& out, std::string& error);

const char* lit_class_name(LitClass c);

}  // namespace adl2::rdgen
