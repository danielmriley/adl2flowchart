#include "adl2/rdgen/inventory.hpp"

namespace adl2::rdgen {

const char* lit_class_name(LitClass c) {
  switch (c) {
    case LitClass::SectionKw:
      return "section-kw";
    case LitClass::RegionStmtKw:
      return "region-stmt-kw";
    case LitClass::ObjectStmtKw:
      return "object-stmt-kw";
    case LitClass::ExprOpWord:
      return "expr-op-word";
    case LitClass::PrimaryKw:
      return "primary-kw";
    case LitClass::OtherKw:
      return "other-kw";
    case LitClass::ContextualIdent:
      return "contextual-ident";
    case LitClass::SymbolicOp:
      return "symbolic-op";
    case LitClass::Extra:
      return "extra";
    case LitClass::Unclassified:
      return "unclassified";
  }
  return "unclassified";
}

bool build_inventory(const Grammar&, Inventory& out, std::string& error) {
  out = Inventory{};
  error = "build_inventory: not implemented (rdgen-inventory worktree)";
  return false;
}

}  // namespace adl2::rdgen
