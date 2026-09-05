#pragma once

#include "adl2/syntax/diag.hpp"
#include "adl2/syntax/token.hpp"

#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace adl2::syntax {

class Lexer {
 public:
  Lexer(std::string_view source, DiagSink& diags);

  /// Tokenize the entire source. NEWLINE tokens are emitted so
  /// greedy line-oriented productions can consult them later.
  std::vector<Token> tokenize();

 private:
  Token next();
  /// One lexing step: a token, or nullopt after skipping a comment or a
  /// rejected character (`next` loops; nothing recurses per byte).
  std::optional<Token> lex_one();
  void skip_spaces();  // space/tab/CR; only `\n` becomes a token
  void skip_line_comment();
  /// Byte length of the UTF-8 character starting at `k` (1 for an invalid
  /// lead or truncated sequence).
  std::size_t utf8_len_at(std::size_t k) const;
  Token make(TokKind kind, std::size_t begin, std::uint32_t line,
             std::uint32_t column, std::string text);
  TokKind keyword_or_ident(const std::string& text) const;

  std::string_view src_;
  DiagSink& diags_;
  std::size_t i_ = 0;
  std::uint32_t line_ = 1;
  std::uint32_t col_ = 1;
  /// `_<digit>` splits: full note once per file; extras summarized at EOF
  /// (matches Rust `adl-syntax` lexer; HIR dumps include these notes).
  std::uint32_t underscore_splits_ = 0;
  Span last_underscore_span_{};
};

}  // namespace adl2::syntax
