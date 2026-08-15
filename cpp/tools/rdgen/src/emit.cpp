#include "adl2/rdgen/emit.hpp"

#include "adl2/rdgen/literals.hpp"

#include <cctype>
#include <sstream>
#include <vector>

namespace adl2::rdgen {
namespace {

std::string lower_copy(std::string s) {
  for (char& c : s) {
    c = static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
  }
  return s;
}

struct OpInfo {
  std::string tok;
  std::string bin;
  std::string un;
};

bool op_info(const std::string& lit, const std::vector<Synonym>& syns,
             OpInfo& out, std::string& error) {
  if (const LitBind* b = lookup_lit(lit)) {
    if (b->tok) out.tok = b->tok;
    if (b->bin) out.bin = b->bin;
    if (b->un) out.un = b->un;
    if (out.tok.empty()) {
      error = "literal \"" + lit + "\" has no TokKind";
      return false;
    }
    return true;
  }
  const std::string key = lower_copy(lit);
  for (const auto& s : syns) {
    if (s.lit == key || s.lit == lit) {
      out.tok = s.tok;
      out.bin = s.bin;
      out.un = s.un;
      return true;
    }
  }
  error = "unknown operator literal \"" + lit + "\"";
  return false;
}

std::string method_for(const MethodMap& map, const std::string& ebnf_name,
                       std::string& error) {
  for (const auto& e : map.entries) {
    if (e.name == ebnf_name) return e.symbol;
  }
  error = "no method_map entry for '" + ebnf_name + "'";
  return {};
}

bool same_field(const std::vector<std::string>& ops,
                const std::vector<Synonym>& syns, bool want_bin, std::string& field,
                std::string& error) {
  field.clear();
  for (const auto& op : ops) {
    OpInfo info;
    if (!op_info(op, syns, info, error)) return false;
    const std::string& v = want_bin ? info.bin : info.un;
    if (v.empty()) {
      error = std::string("literal \"") + op + "\" is not a " +
              (want_bin ? "binary" : "unary") + " operator";
      return false;
    }
    if (field.empty())
      field = v;
    else if (field != v)
      return false;
  }
  return !field.empty();
}

void emit_alias(std::ostringstream& os, const std::string& method,
                const std::string& next_method) {
  os << "std::unique_ptr<Expr> Parser::" << method << "() { return "
     << next_method << "(); }\n\n";
}

void emit_left_assoc(std::ostringstream& os, const std::string& method,
                     const std::string& next_method,
                     const std::vector<std::string>& ops,
                     const std::vector<Synonym>& syns, std::string& error) {
  std::string bin;
  const bool uniform = same_field(ops, syns, true, bin, error);
  if (!error.empty()) return;

  os << "std::unique_ptr<Expr> Parser::" << method << "() {\n";
  os << "  auto left = " << next_method << "();\n";
  if (uniform) {
    os << "  while (";
    for (std::size_t i = 0; i < ops.size(); ++i) {
      if (i) os << " || ";
      OpInfo info;
      if (!op_info(ops[i], syns, info, error)) return;
      os << "check(TokKind::" << info.tok << ")";
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
      OpInfo info;
      if (!op_info(ops[i], syns, info, error)) return;
      if (info.bin.empty()) {
        error = "unknown binary operator literal \"" + ops[i] + "\"";
        return;
      }
      os << "    ";
      if (i) os << "else ";
      os << "if (check(TokKind::" << info.tok << "))\n";
      os << "      op = BinOp::" << info.bin << ";\n";
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
                 const std::vector<std::string>& ops,
                 const std::vector<Synonym>& syns, std::string& error) {
  std::string un;
  if (!same_field(ops, syns, false, un, error)) {
    if (error.empty()) error = "mixed unary operators in " + method;
    return;
  }
  os << "std::unique_ptr<Expr> Parser::" << method << "() {\n";
  os << "  if (";
  for (std::size_t i = 0; i < ops.size(); ++i) {
    if (i) os << " || ";
    OpInfo info;
    if (!op_info(ops[i], syns, info, error)) return;
    os << "check(TokKind::" << info.tok << ")";
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

bool is_ternary_prod(const Production& p) {
  if (p.alts.size() != 1 || p.alts[0].terms.size() != 2) return false;
  const Term& head = p.alts[0].terms[0];
  const Term& opt = p.alts[0].terms[1];
  if (head.kind != TermKind::Name || opt.kind != TermKind::Optional) return false;
  if (opt.group.size() != 1 || opt.group[0].terms.size() < 2) return false;
  const Term& q = opt.group[0].terms[0];
  return q.kind == TermKind::Literal && q.text == "?";
}

void emit_ternary(std::ostringstream& os, const std::string& method,
                  const std::string& guard_method, const std::string& self_method) {
  os << "std::unique_ptr<Expr> Parser::" << method << "() {\n";
  os << "  auto guard = " << guard_method << "();\n";
  os << "  if (!match(TokKind::Question)) return guard;\n";
  os << "  auto then_e = " << self_method << "();\n";
  os << "  std::unique_ptr<Expr> else_e;\n";
  os << "  bool has_else = false;\n";
  os << "  if (match(TokKind::Colon)) {\n";
  os << "    else_e = " << self_method << "();\n";
  os << "    has_else = true;\n";
  os << "  }\n";
  os << "  auto e = std::make_unique<Expr>();\n";
  os << "  e->kind = ExprKind::Ternary;\n";
  os << "  e->span = guard->span.to(last_span_);\n";
  os << "  e->ternary_has_else = has_else;\n";
  os << "  e->guard = std::move(guard);\n";
  os << "  e->then_e = std::move(then_e);\n";
  os << "  e->else_e = std::move(else_e);\n";
  os << "  return e;\n";
  os << "}\n\n";
}

bool cond_stmt_kws(const Production& p, std::vector<std::string>& kws) {
  if (p.alts.size() != 1 || p.alts[0].terms.size() != 2) return false;
  const Term& a = p.alts[0].terms[0];
  const Term& b = p.alts[0].terms[1];
  if (b.kind != TermKind::Name || b.text != "condition") return false;
  kws.clear();
  if (a.kind == TermKind::Literal) {
    kws.push_back(a.text);
    return true;
  }
  if (a.kind != TermKind::Group) return false;
  for (const auto& alt : a.group) {
    if (alt.terms.size() != 1 || alt.terms[0].kind != TermKind::Literal) {
      return false;
    }
    kws.push_back(alt.terms[0].text);
  }
  return !kws.empty();
}

const char* region_kind_for(const std::string& first_kw) {
  if (first_kw == "reject") return "Reject";
  if (first_kw == "trigger") return "Trigger";
  if (first_kw == "select" || first_kw == "cut" || first_kw == "cmd" ||
      first_kw == "command") {
    return "Cut";
  }
  return nullptr;
}

void emit_cond_stmt(std::ostringstream& os, const std::string& method,
                    const std::vector<std::string>& kws,
                    const std::vector<Synonym>& syns, std::string& error) {
  const char* kind = region_kind_for(kws.front());
  if (!kind) {
    error = "no RegionStmt kind for keyword \"" + kws.front() + "\"";
    return;
  }
  os << "RegionStmt Parser::" << method << "() {\n";
  os << "  Token kw_tok = advance();\n";
  if (std::string(kind) == "Cut") {
    os << "  std::string kw = \"select\";\n";
    for (const auto& lit : kws) {
      OpInfo info;
      if (!op_info(lit, syns, info, error)) return;
      // Synonyms that inherit KwSelect stay "select" (default).
      if (info.tok == "KwSelect") continue;
      os << "  if (kw_tok.kind == TokKind::" << info.tok << ") kw = \"" << lit
         << "\";\n";
    }
  }
  os << "  RegionStmt st;\n";
  os << "  st.kind = RegionStmt::Kind::" << kind << ";\n";
  if (std::string(kind) == "Cut") os << "  st.keyword = kw;\n";
  os << "  st.cond = parse_condition();\n";
  os << "  st.span = kw_tok.span.to(last_span_);\n";
  os << "  return st;\n";
  os << "}\n\n";
}

}  // namespace

bool emit_generated(const Grammar& g, const MethodMap& map, std::string& out,
                    std::string& error) {
  error.clear();
  std::vector<Synonym> syns;
  if (!resolve_synonyms(g, syns, error)) return false;

  std::ostringstream os;
  os << "// Generated by adl2_rdgen from grammar.ebnf. Do not edit.\n";
  os << "// Shape-checked emit; AST inferred from EBNF (RDGEN.md).\n";
  os << "// Included from parser.cpp inside namespace adl2::syntax.\n\n";

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
      emit_left_assoc(os, method, next, sh.ops, syns, error);
      if (!error.empty()) return false;
    } else if (sh.shape == Shape::PrefixUnary) {
      const std::string next = method_for(map, sh.next, error);
      if (!error.empty()) return false;
      emit_prefix(os, method, method, next, sh.ops, syns, error);
      if (!error.empty()) return false;
    } else if (sh.shape == Shape::OptionalSuffix && is_ternary_prod(p)) {
      const std::string guard = method_for(map, sh.next, error);
      if (!error.empty()) return false;
      emit_ternary(os, method, guard, method);
    } else if (sh.shape == Shape::KeywordSeq) {
      std::vector<std::string> kws;
      if (!cond_stmt_kws(p, kws)) {
        error = "'" + p.name +
                "' is generate/KeywordSeq but not `keywords condition`";
        return false;
      }
      emit_cond_stmt(os, method, kws, syns, error);
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
