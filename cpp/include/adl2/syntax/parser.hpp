#pragma once

#include "adl2/syntax/ast.hpp"
#include "adl2/syntax/diag.hpp"
#include "adl2/syntax/token.hpp"

#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace adl2::syntax {

/// Hand-written recursive-descent parser (ADR-002 / ADR-010).
/// One `parse_<nonterminal>` per major production in grammar.ebnf.
/// Expression precedence: layered ladder in BISON_MAP.md.
class Parser {
 public:
  Parser(std::string_view src, std::vector<Token> tokens, DiagSink& diags);

  FileAst parse_file();

  // --- top-level / sections ---
  bool parse_section(Section& out);
  Section parse_info_block();
  Section parse_define_section();
  Section parse_object_block();
  Section parse_region_block();
  Section parse_table_block();
  Section parse_countsformat_block();

  // --- object / region statements ---
  ObjectStmt parse_take_stmt();
  TakeSource parse_take_source();
  ObjectStmt parse_object_define();
  ObjectStmt parse_derived_candidate();
  std::optional<RegionStmt> parse_region_stmt();
  RegionStmt parse_cut_as_region();
  RegionStmt parse_reject_stmt();
  RegionStmt parse_region_ref();
  RegionStmt parse_bin_stmt();
  RegionStmt parse_trigger_stmt();
  RegionStmt parse_histo_stmt();
  HistoArg parse_histo_arg();
  RegionStmt parse_weight_stmt();
  RegionStmt parse_print_stmt();
  RegionStmt parse_save_stmt();
  RegionStmt parse_counts_stmt();
  RegionStmt parse_sort_stmt();
  InfoLine parse_info_line();

  // --- expressions ---
  std::unique_ptr<Expr> parse_condition();
  std::unique_ptr<Expr> parse_ternary();
  std::unique_ptr<Expr> parse_or_expr();
  std::unique_ptr<Expr> parse_and_expr();
  std::unique_ptr<Expr> parse_not_expr();
  std::unique_ptr<Expr> parse_comparison();
  std::unique_ptr<Expr> parse_band_suffix(std::unique_ptr<Expr> lhs);
  std::unique_ptr<Expr> parse_additive();
  std::unique_ptr<Expr> parse_multiplicative();
  std::unique_ptr<Expr> parse_unary();
  std::unique_ptr<Expr> parse_postfix();
  std::unique_ptr<Expr> parse_primary();
  std::unique_ptr<Expr> parse_func_call(Ident name);
  std::unique_ptr<Arg> parse_arg();
  std::vector<std::unique_ptr<Arg>> parse_paren_args();
  std::vector<std::unique_ptr<Arg>> parse_arg_list_to_eol();
  NumLit parse_signed_num();
  IndexVal parse_index_val();
  std::unique_ptr<Expr> parse_index_suffix(std::unique_ptr<Expr> base);
  std::unique_ptr<Expr> extend_particle_list(std::unique_ptr<Expr> first);
  bool parse_path_token(StrLit& out);

 private:
  const Token& raw_peek() const;
  const Token& peek() const;
  const Token& peek_n(std::size_t n) const;  // significant tokens
  std::size_t sig_index() const;
  Token advance();
  bool check(TokKind k) const;
  bool match(TokKind k);
  bool match_any(std::initializer_list<TokKind> ks);
  bool expect(TokKind k, const char* what);
  bool nl_before() const;
  bool at_section_start() const;
  bool at_stmt_keyword() const;
  bool next_is_line_end() const;
  bool is_ident_text(const char* word) const;
  bool at_postfix_start() const;
  bool at_signed_num() const;
  bool at_index_val() const;
  bool at_column_one() const;
  bool rest_of_line_is_boundary_list() const;
  bool derived_candidate_ahead() const;
  void synchronize_statement();
  Ident expect_ident(const char* what);
  Ident make_ident(Token tok);
  StrLit expect_string(const char* what);
  std::optional<CmpOp> peek_cmp_op() const;
  std::unique_ptr<Expr> make_error(Span span);

  std::string_view src_;
  std::vector<Token> tokens_;
  std::size_t pos_ = 0;
  Span last_span_{};
  DiagSink& diags_;
  bool tilde_warned_ = false;
};

struct ParseResult {
  FileAst file;
  DiagSink diags;
};

ParseResult parse_source(const std::string& source);

}  // namespace adl2::syntax
