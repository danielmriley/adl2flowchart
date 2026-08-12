#pragma once

#include "adl2/diag.hpp"
#include "adl2/token.hpp"

#include <string>
#include <string_view>
#include <vector>

namespace adl2 {

class Lexer {
 public:
  Lexer(std::string_view source, DiagSink& diags);

  /// Tokenize the entire source. NEWLINE tokens are emitted so
  /// greedy line-oriented productions can consult them later.
  std::vector<Token> tokenize();

 private:
  Token next();
  void skip_spaces();  // space/tab only; newlines become tokens
  void skip_line_comment();
  Token make(TokKind kind, std::size_t begin, std::uint32_t line,
             std::uint32_t column, std::string text);
  TokKind keyword_or_ident(const std::string& text) const;

  std::string_view src_;
  DiagSink& diags_;
  std::size_t i_ = 0;
  std::uint32_t line_ = 1;
  std::uint32_t col_ = 1;
};

}  // namespace adl2
