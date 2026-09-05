#pragma once

#include "adl2/syntax/ast.hpp"
#include "adl2/syntax/diag.hpp"
#include "adl2/syntax/token.hpp"

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace adl2::syntax {

/// Hard ceiling on the depth of any expression tree the parser builds
/// (smash3 `MAX_EXPR_DEPTH`). Bounds both the parser's own recursion
/// (`((((…))))`, `- - - x`, `not not …`) and every consumer's recursion
/// over the accepted tree (`1+1+1+…` is depth-linear in the term count
/// while the parser stays flat). Untrusted ADL is a real input surface:
/// without this the process segfaults on a 200k-deep expression.
inline constexpr std::uint32_t MAX_EXPR_DEPTH = 64;

/// Hand-written recursive-descent parser.
/// One `parse_<nonterminal>` per production in grammar.ebnf.
/// Keyword dispatch is stmt_dispatch.hpp (one table, not three if-chains).
/// Flex and Bison are not the implementation. See BISON_MAP.md.
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
  std::unique_ptr<Expr> parse_postfix_inner();
  std::unique_ptr<Expr> parse_primary();
  std::unique_ptr<Expr> parse_func_call(Ident name);
  std::unique_ptr<Arg> parse_arg();
  std::vector<std::unique_ptr<Arg>> parse_paren_args(std::uint32_t* arg_depth = nullptr);
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
  /// Consume the next significant token and return it (a reference into
  /// `tokens_`, which is never mutated after construction).
  const Token& advance();
  bool check(TokKind k) const;
  bool match(TokKind k);
  bool match_any(std::initializer_list<TokKind> ks);
  bool expect(TokKind k, const char* what);
  /// A keyword slot in the table header (`tabletype`/`nvars`/`errors`).
  /// smash3's `expect_tok` compares enum discriminants, so ANY keyword
  /// satisfies a `Kw(_)` target; mirrored so recovery from a malformed
  /// table header is byte-identical. Only the table header uses it.
  bool expect_keyword_slot(const char* what);
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
  /// Backstop for an unrecognized token inside an object/region block
  /// (smash3 `recover_block_stmt`): `false` at EOF, a bare identifier or a
  /// section keyword (the block ends); otherwise warn, skip the line, `true`.
  bool recover_block_stmt(const char* ctx_label);
  /// Report `message` at the current token with the oracle's ``found `…` ``
  /// label (smash3 `error_here`); silent after a depth abort. Returns the
  /// span so callers can build an error node at the same place.
  Span error_here(std::string message);
  /// Nearest statement keyword within Levenshtein distance 2 of `word`
  /// (smash3 `suggest_keyword`), or nullptr.
  const char* suggest_keyword(const std::string& word) const;
  Ident expect_ident(const char* what);
  /// Section name after `object`/`region`/…: joins byte-adjacent `_<digit>`
  /// / `_<ident>` runs the lexer split off (`SR3L_1`, `SR_3b3j`), smash3
  /// `parse_section_name`.
  Ident parse_section_name(const char* context);
  Ident make_ident(const Token& tok);
  StrLit expect_string(const char* what);
  std::optional<CmpOp> peek_cmp_op() const;
  std::unique_ptr<Expr> make_error(Span span);

  // --- expression depth budget (smash3 parity) ---
  /// Enter one level of expression-grammar recursion; `false` means the
  /// budget is gone and the caller must return an error node at once.
  bool rec_enter();
  void rec_exit();
  /// Record a node built over children whose deepest is `child_max`.
  std::uint32_t built(std::uint32_t child_max);
  /// Record a leaf.
  std::uint32_t leaf();
  /// Report once, park the cursor on EOF so every loop unwinds.
  void abort_too_deep();

  std::string_view src_;
  std::vector<Token> tokens_;
  std::size_t pos_ = 0;
  Span last_span_{};
  DiagSink& diags_;
  bool tilde_warned_ = false;
  std::uint32_t rec_ = 0;
  std::uint32_t last_depth_ = 1;
  bool aborted_ = false;
};

struct ParseResult {
  FileAst file;
  DiagSink diags;
};

ParseResult parse_source(const std::string& source);

}  // namespace adl2::syntax
