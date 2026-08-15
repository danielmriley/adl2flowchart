#include "adl2/rdgen/emit.hpp"

#include <sstream>
#include <unordered_map>
#include <vector>

namespace adl2::rdgen {
namespace {

struct LitInfo {
  const char* tok = nullptr;
  const char* bin = nullptr;
  const char* un = nullptr;
};

const LitInfo* lit_info(const std::string& lit) {
  static const std::unordered_map<std::string, LitInfo> k = {
      {"or", {"KwOr", "Or", nullptr}},
      {"||", {"OrOr", "Or", nullptr}},
      {"and", {"KwAnd", "And", nullptr}},
      {"&&", {"AndAnd", "And", nullptr}},
      {"not", {"KwNot", nullptr, "Not"}},
      {"!", {"Bang", nullptr, "Not"}},
      {"+", {"Plus", "Add", nullptr}},
      {"-", {"Minus", "Sub", "Neg"}},
      {"*", {"Star", "Mul", nullptr}},
      {"/", {"Slash", "Div", nullptr}},
      {"^", {"Caret", "Pow", nullptr}},
  };
  auto it = k.find(lit);
  return it == k.end() ? nullptr : &it->second;
}

std::string method_for(const MethodMap& map, const std::string& ebnf_name,
                       std::string& error) {
  for (const auto& e : map.entries) {
    if (e.name == ebnf_name) return e.symbol;
  }
  error = "no method_map entry for '" + ebnf_name + "'";
  return {};
}

bool same_bin(const std::vector<std::string>& ops, std::string& bin,
              std::string& error) {
  bin.clear();
  for (const auto& op : ops) {
    const LitInfo* info = lit_info(op);
    if (!info || !info->bin) {
      error = "unknown binary operator literal \"" + op + "\"";
      return false;
    }
    if (bin.empty())
      bin = info->bin;
    else if (bin != info->bin)
      return false;
  }
  return !bin.empty();
}

bool same_un(const std::vector<std::string>& ops, std::string& un,
             std::string& error) {
  un.clear();
  for (const auto& op : ops) {
    const LitInfo* info = lit_info(op);
    if (!info || !info->un) {
      error = "unknown unary operator literal \"" + op + "\"";
      return false;
    }
    if (un.empty())
      un = info->un;
    else if (un != info->un)
      return false;
  }
  return !un.empty();
}

void emit_alias(std::ostringstream& os, const std::string& method,
                const std::string& next_method) {
  os << "std::unique_ptr<Expr> Parser::" << method << "() { return "
     << next_method << "(); }\n\n";
}

void emit_left_assoc(std::ostringstream& os, const std::string& method,
                     const std::string& next_method,
                     const std::vector<std::string>& ops, std::string& error) {
  std::string bin;
  const bool uniform = same_bin(ops, bin, error);
  if (!error.empty()) return;

  os << "std::unique_ptr<Expr> Parser::" << method << "() {\n";
  os << "  auto left = " << next_method << "();\n";
  if (uniform) {
    os << "  while (";
    for (std::size_t i = 0; i < ops.size(); ++i) {
      if (i) os << " || ";
      const LitInfo* info = lit_info(ops[i]);
      os << "check(TokKind::" << info->tok << ")";
    }
    os << ") {\n";
    os << "    advance();\n";
    os << "    auto right = " << next_method << "();\n";
    os << "    auto e = std::make_unique<Expr>();\n";
    os << "    e->kind = ExprKind::Binary;\n";
    os << "    e->bin_op = BinOp::" << bin << ";\n";
    os << "    e->span = left->span.to(right->span);\n";
    os << "    e->lhs = std::move(left);\n";
    os << "    e->rhs = std::move(right);\n";
    os << "    left = std::move(e);\n";
    os << "  }\n";
  } else {
    os << "  for (;;) {\n";
    os << "    BinOp op;\n";
    for (std::size_t i = 0; i < ops.size(); ++i) {
      const LitInfo* info = lit_info(ops[i]);
      if (!info || !info->bin) {
        error = "unknown binary operator literal \"" + ops[i] + "\"";
        return;
      }
      os << "    ";
      if (i) os << "else ";
      os << "if (check(TokKind::" << info->tok << "))\n";
      os << "      op = BinOp::" << info->bin << ";\n";
    }
    os << "    else\n";
    os << "      break;\n";
    os << "    advance();\n";
    os << "    auto right = " << next_method << "();\n";
    os << "    auto e = std::make_unique<Expr>();\n";
    os << "    e->kind = ExprKind::Binary;\n";
    os << "    e->bin_op = op;\n";
    os << "    e->span = left->span.to(right->span);\n";
    os << "    e->lhs = std::move(left);\n";
    os << "    e->rhs = std::move(right);\n";
    os << "    left = std::move(e);\n";
    os << "  }\n";
  }
  os << "  return left;\n";
  os << "}\n\n";
}

void emit_prefix(std::ostringstream& os, const std::string& method,
                 const std::string& self_method, const std::string& next_method,
                 const std::vector<std::string>& ops, std::string& error) {
  std::string un;
  if (!same_un(ops, un, error)) {
    if (error.empty()) error = "mixed unary operators in " + method;
    return;
  }
  os << "std::unique_ptr<Expr> Parser::" << method << "() {\n";
  os << "  if (";
  for (std::size_t i = 0; i < ops.size(); ++i) {
    if (i) os << " || ";
    const LitInfo* info = lit_info(ops[i]);
    os << "check(TokKind::" << info->tok << ")";
  }
  os << ") {\n";
  os << "    Token op = advance();\n";
  os << "    auto inner = " << self_method << "();\n";
  os << "    auto e = std::make_unique<Expr>();\n";
  os << "    e->kind = ExprKind::Unary;\n";
  os << "    e->unary_op = UnaryOp::" << un << ";\n";
  os << "    e->span = op.span.to(inner->span);\n";
  os << "    e->child = std::move(inner);\n";
  os << "    return e;\n";
  os << "  }\n";
  os << "  return " << next_method << "();\n";
  os << "}\n\n";
}

}  // namespace

bool emit_expr_ladder(const Grammar& g, const MethodMap& map, std::string& out,
                      std::string& error) {
  error.clear();
  std::ostringstream os;
  os << "// Generated by adl2_rdgen from grammar.ebnf. Do not edit.\n";
  os << "// Shape-checked Alias / LeftAssoc / PrefixUnary only (RDGEN.md).\n";
  os << "// Included from parser.cpp inside namespace adl2::syntax.\n\n";

  // Emit in grammar order so the golden is stable.
  int emitted = 0;
  for (const auto& p : g.prods) {
    const MapEntry* ent = nullptr;
    for (const auto& e : map.entries) {
      if (e.name == p.name) {
        ent = &e;
        break;
      }
    }
    if (!ent || ent->role != MapRole::Generate) continue;

    const ShapeInfo sh = classify(p);
    const std::string method = ent->symbol;
    if (sh.shape == Shape::Alias) {
      const std::string next = method_for(map, sh.next, error);
      if (!error.empty()) return false;
      emit_alias(os, method, next);
    } else if (sh.shape == Shape::LeftAssoc) {
      const std::string next = method_for(map, sh.next, error);
      if (!error.empty()) return false;
      emit_left_assoc(os, method, next, sh.ops, error);
      if (!error.empty()) return false;
    } else if (sh.shape == Shape::PrefixUnary) {
      const std::string next = method_for(map, sh.next, error);
      if (!error.empty()) return false;
      emit_prefix(os, method, method, next, sh.ops, error);
      if (!error.empty()) return false;
    } else {
      error = "'" + p.name + "' is generate but shape is " +
              std::string(shape_name(sh.shape));
      return false;
    }
    ++emitted;
  }
  if (emitted == 0) {
    error = "no role=generate productions to emit";
    return false;
  }
  out = os.str();
  return true;
}

}  // namespace adl2::rdgen
