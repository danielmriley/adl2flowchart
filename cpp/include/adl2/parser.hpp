#pragma once

#include "adl2/ast.hpp"
#include "adl2/diag.hpp"
#include "adl2/token.hpp"

#include <memory>
#include <string>
#include <vector>

namespace adl2 {

/// Hand-written recursive-descent parser.
/// One public/private parse_<nonterminal> per major production in
/// grammar.ebnf (ADR-002 / ADR-010). Expression precedence is the
/// layered ladder documented in BISON_MAP.md.
class Parser {
 public:
  Parser(std::vector<Token> tokens, DiagSink& diags);

  FileAst parse_file();

  // --- top-level / sections (grammar.ebnf) ---
  bool parse_section(Section& out);
  Section parse_info_block();
  Section parse_define();
  Section parse_object_block();
  Section parse_region_block();
  Section parse_table_block();
  Section parse_countsformat_block();

  // --- object / region statements ---
  bool parse_take_stmt();
  bool parse_take_source();
  bool parse_object_define();
  bool parse_region_stmt();
  bool parse_cut_stmt();
  bool parse_reject_stmt();
  bool parse_region_ref();
  bool parse_bin_stmt();
  bool parse_bin_body();
  bool parse_boundary_list();
  bool parse_trigger_stmt();
  bool parse_histo_stmt();
  bool parse_histo_arg();
  bool parse_weight_stmt();
  bool parse_print_stmt();
  bool parse_save_stmt();
  bool parse_counts_stmt();
  bool parse_sort_stmt();
  bool parse_info_line();

  // --- expressions (precedence ladder; entry = condition) ---
  std::unique_ptr<Expr> parse_condition();
  std::unique_ptr<Expr> parse_ternary();
  std::unique_ptr<Expr> parse_or_expr();
  std::unique_ptr<Expr> parse_and_expr();
  std::unique_ptr<Expr> parse_not_expr();
  std::unique_ptr<Expr> parse_comparison();
  std::unique_ptr<Expr> parse_additive();
  std::unique_ptr<Expr> parse_multiplicative();
  std::unique_ptr<Expr> parse_unary();
  std::unique_ptr<Expr> parse_postfix();
  std::unique_ptr<Expr> parse_primary();
  std::unique_ptr<Expr> parse_func_call();
  bool parse_arg_list();
  bool parse_arg();
  bool parse_path_token();
  bool parse_particle_list();
  bool parse_index();
  bool parse_signed_num();

 private:
  const Token& peek() const;
  const Token& peek_n(std::size_t n) const;
  Token advance();
  bool check(TokKind k) const;
  bool match(TokKind k);
  bool match_any(std::initializer_list<TokKind> ks);
  bool expect(TokKind k, const char* what);
  void skip_newlines();
  bool at_section_start() const;
  bool at_stmt_keyword() const;
  bool next_is_line_end() const;
  bool is_ident_text(const char* word) const;
  bool looks_like_postfix_boundary_list() const;
  bool at_postfix_start() const;
  void synchronize_statement();
  void not_implemented(const char* production, Span span);
  std::unique_ptr<Expr> make_unsupported(Span span, std::string reason);
  std::unique_ptr<Expr> make_leaf(ExprKind kind, Token tok);

  std::vector<Token> tokens_;
  std::size_t pos_ = 0;
  DiagSink& diags_;
};

/// Lex + parse convenience for the CLI / tests.
struct ParseResult {
  FileAst file;
  DiagSink diags;
};

ParseResult parse_source(const std::string& source);

}  // namespace adl2
