#pragma once

#include "adl2/syntax/span.hpp"

#include <string>

namespace adl2::syntax {

enum class TokKind {
  Eof,
  Newline,
  Ident,
  Int,
  Real,
  String,
  PathLike,  // bare weight-file style token (arg context only)

  // Keywords (case-insensitive). Keep in sync with SPEC_LANGUAGE §2.
  KwInfo,
  KwDefine,
  KwDef,
  KwObject,
  KwObj,
  KwComposite,
  KwTrigger,
  KwTake,
  KwUsing,
  KwSelect,
  KwCut,
  KwCmd,
  KwCommand,
  KwReject,
  KwRegion,
  KwAlgo,
  KwHistoList,
  KwBin,
  KwHisto,
  KwWeight,
  KwTable,
  KwTabletype,
  KwNvars,
  KwErrors,
  KwUnion,
  KwProcess,
  KwCounts,
  KwCountsformat,
  KwPrint,
  KwSave,
  KwSort,
  KwAll,
  KwNone,
  KwAnd,
  KwOr,
  KwNot,
  KwTrue,
  KwFalse,

  // Operators / punctuation
  Plus,
  Minus,
  Star,
  Slash,
  Caret,
  Assign,   // =
  Colon,
  Question,
  Bang,     // !
  Dot,
  Comma,
  Underscore,
  LParen,
  RParen,
  LBracket,
  RBracket,
  LBrace,
  RBrace,
  Pipe,
  Gt,
  Lt,
  Ge,
  Le,
  EqEq,
  Ne,
  TildeEq,  // ~=
  AndAnd,   // &&
  OrOr,     // ||
  BandIncl, // []
  BandExcl, // ][
  Arrow,    // ->
  PlusMinus, // +-
};

struct Token {
  TokKind kind = TokKind::Eof;
  std::string text;
  Span span;
};

const char* tok_kind_name(TokKind k);

/// Keywords are contiguous in TokKind (KwInfo..KwFalse above).
inline bool is_keyword_kind(TokKind k) {
  return k >= TokKind::KwInfo && k <= TokKind::KwFalse;
}

/// How a token is named inside a diagnostic (smash3 `Token::describe`):
/// ``identifier `x` ``, ``number `20` ``, `string literal`,
/// ``keyword `select` `` (canonical spelling), `end of file`, else the raw
/// lexeme in backticks.
std::string describe_token(const Token& t);

}  // namespace adl2::syntax
